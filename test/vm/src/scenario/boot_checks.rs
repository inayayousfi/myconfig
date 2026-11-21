//! Reads what the boot chain scenarios see in the installed system: the Secure Boot firmware
//! variable, `sbctl verify`, the guard's status file, the rEFInd file the guard writes,
//! snapper's snapshot list and the kernel command line. The scenario turns each problem these
//! return into a failed check.

use serde_json::Value;

/// The title of the rEFInd entry that the guard writes for the newest pre-update snapshot.
pub const PREVIOUS_SYSTEM: &str = "Previous system";

/// Whether a UEFI variable printed by `od -An -tu1` is on: its last byte is 1. The first four
/// bytes of a file in efivarfs are the variable's attributes.
pub fn efivar_on(od: &str) -> Option<bool> {
    let bytes: Vec<u8> = od
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    if bytes.len() < 5 {
        return None;
    }
    bytes.last().map(|&byte| byte == 1)
}

/// The files that `sbctl verify` reports as signed and as not signed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Verified {
    pub signed: Vec<String>,
    pub unsigned: Vec<String>,
}

/// Reads the lines of `sbctl verify`, which end with "is signed" or "is not signed" after the
/// file path, behind a check or cross mark.
pub fn verified(output: &str) -> Verified {
    let mut verified = Verified::default();
    for line in output.lines() {
        let line = line.trim();
        let path = |suffix: &str| {
            line.strip_suffix(suffix).map(|rest| {
                rest.trim_start_matches(|character: char| !character.is_ascii() || character == ' ')
                    .trim()
                    .to_owned()
            })
        };
        if let Some(file) = path(" is not signed") {
            verified.unsigned.push(file);
        } else if let Some(file) = path(" is signed") {
            verified.signed.push(file);
        }
    }
    verified
}

/// The result that `inayos-guard` records in its status file.
#[derive(Debug, PartialEq, Eq)]
pub struct GuardStatus {
    pub passed: bool,
    pub failures: Vec<String>,
}

/// Reads the guard's status file: an object with `passed` and the `failures` in plain words.
pub fn guard_status(json: &str) -> Result<GuardStatus, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| format!("the guard's status file is not JSON: {error}"))?;
    let passed = value
        .get("passed")
        .and_then(Value::as_bool)
        .ok_or("the guard's status file has no passed field")?;
    let failures = value
        .get("failures")
        .and_then(Value::as_array)
        .map(|failures| {
            failures
                .iter()
                .map(|failure| match failure {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(GuardStatus { passed, failures })
}

/// One `menuentry` stanza of a rEFInd configuration file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Entry {
    pub title: String,
    pub volume: Option<String>,
    pub loader: Option<String>,
    pub initrds: Vec<String>,
    pub options: Option<String>,
}

/// The parts of the guard's rEFInd file that the scenarios check.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GuardConf {
    pub default_selection: Option<String>,
    pub entries: Vec<Entry>,
}

impl GuardConf {
    /// Reads a rEFInd configuration file: one token per line, then its value, which may be
    /// quoted; `menuentry "<title>" {` opens a stanza and `}` closes it.
    pub fn parse(text: &str) -> GuardConf {
        let mut conf = GuardConf::default();
        let mut entry: Option<Entry> = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "}" {
                conf.entries.extend(entry.take());
                continue;
            }
            let (token, value) = line
                .split_once(char::is_whitespace)
                .map_or((line, ""), |(token, value)| (token, value.trim()));
            let value = unquote(value);
            match (token, entry.as_mut()) {
                ("menuentry", _) => {
                    conf.entries.extend(entry.take());
                    entry = Some(Entry {
                        title: unquote(value.trim_end_matches('{').trim()),
                        ..Entry::default()
                    });
                }
                ("default_selection", None) => conf.default_selection = Some(value),
                ("volume", Some(entry)) => entry.volume = Some(value),
                ("loader", Some(entry)) => entry.loader = Some(value),
                ("initrd", Some(entry)) => entry.initrds.push(value),
                ("options", Some(entry)) => entry.options = Some(value),
                _ => {}
            }
        }
        conf.entries.extend(entry);
        conf
    }

    pub fn entry(&self, title: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.title == title)
    }
}

fn unquote(value: &str) -> String {
    let value = value.trim();
    value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(value)
        .to_owned()
}

/// What is wrong with the "Previous system" entry for booting `snapshot`, or none when it
/// starts that snapshot's kernel with its root on systemd's overlay.
pub fn previous_entry_problem(conf: &GuardConf, snapshot: u32) -> Option<String> {
    let Some(entry) = conf.entry(PREVIOUS_SYSTEM) else {
        return Some(format!(
            "the guard's rEFInd file has no {PREVIOUS_SYSTEM:?} entry"
        ));
    };
    let folder = format!("/@snapshots/{snapshot}/snapshot/boot/");
    if !entry
        .loader
        .as_deref()
        .is_some_and(|loader| loader.starts_with(&folder))
    {
        return Some(format!(
            "the {PREVIOUS_SYSTEM:?} entry's loader {:?} is not in {folder}",
            entry.loader
        ));
    }
    let options = entry.options.as_deref().unwrap_or_default();
    if booted_snapshot(options) != Some(snapshot) || !volatile_overlay(options) {
        return Some(format!(
            "the {PREVIOUS_SYSTEM:?} entry's options {options:?} do not start snapshot {snapshot} with systemd.volatile=overlay"
        ));
    }
    None
}

/// The snapshot that the "Previous system" entry starts, when it is a pre snapshot in
/// snapper's list and the entry boots it with its kernel and its root on systemd's overlay.
/// Package installs that do not touch the boot files take newer snapshots without running the
/// guard, so the entry may name any listed pre snapshot, not only the newest.
pub fn previous_entry_snapshot(conf: &GuardConf, snapshots: &[Snapshot]) -> Result<u32, String> {
    let options = conf
        .entry(PREVIOUS_SYSTEM)
        .and_then(|entry| entry.options.as_deref())
        .unwrap_or_default();
    let Some(number) = booted_snapshot(options) else {
        return Err(previous_entry_problem(conf, 0).unwrap_or_else(|| {
            format!("the {PREVIOUS_SYSTEM:?} entry's options {options:?} name no snapshot")
        }));
    };
    if !snapshots
        .iter()
        .any(|snapshot| snapshot.number == number && snapshot.kind == "pre")
    {
        return Err(format!(
            "the {PREVIOUS_SYSTEM:?} entry boots snapshot {number}, which snapper does not list as a pre snapshot"
        ));
    }
    match previous_entry_problem(conf, number) {
        Some(problem) => Err(problem),
        None => Ok(number),
    }
}

/// The snapshot number that `rootflags=subvol=@snapshots/<n>/snapshot` on a kernel command
/// line names, with or without a leading slash.
pub fn booted_snapshot(cmdline: &str) -> Option<u32> {
    cmdline
        .split_whitespace()
        .filter_map(|word| word.strip_prefix("rootflags="))
        .flat_map(|flags| flags.split(','))
        .filter_map(|flag| flag.strip_prefix("subvol="))
        .find_map(|subvolume| {
            subvolume
                .trim_start_matches('/')
                .strip_prefix("@snapshots/")?
                .strip_suffix("/snapshot")?
                .parse()
                .ok()
        })
}

/// Whether a kernel command line puts the root on systemd's overlay.
pub fn volatile_overlay(cmdline: &str) -> bool {
    cmdline
        .split_whitespace()
        .any(|word| word == "systemd.volatile=overlay")
}

/// What `systemctl show -p LoadState,ActiveState,Result,ExecMainStartTimestampMonotonic,ExecMainStatus`
/// says about a oneshot unit that must have run and succeeded in this boot.
#[derive(Debug, PartialEq, Eq)]
pub enum UnitRun {
    /// It ran in this boot and exited with status 0.
    Succeeded,
    /// It is still running.
    Running,
    /// It is missing, did not run, or failed; the text says which.
    Problem(String),
}

/// Reads the `key=value` lines of `systemctl show` for a oneshot unit.
pub fn unit_run(show: &str) -> UnitRun {
    let field = |key: &str| {
        show.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
            .unwrap_or_default()
            .trim()
    };
    if field("LoadState") != "loaded" {
        return UnitRun::Problem(format!("the unit is not loaded: {:?}", field("LoadState")));
    }
    if field("ActiveState") == "activating" {
        return UnitRun::Running;
    }
    if field("ExecMainStartTimestampMonotonic")
        .parse::<u64>()
        .unwrap_or(0)
        == 0
    {
        return UnitRun::Problem("the unit did not run in this boot".to_owned());
    }
    if field("Result") != "success" || field("ExecMainStatus") != "0" {
        return UnitRun::Problem(format!(
            "the unit ended with result {:?} and exit status {:?}",
            field("Result"),
            field("ExecMainStatus")
        ));
    }
    UnitRun::Succeeded
}

/// One snapshot of snapper's `root` configuration.
#[derive(Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub number: u32,
    pub kind: String,
    pub description: String,
    /// The cleanup algorithm that may delete the snapshot, such as `number`, or empty.
    pub cleanup: String,
    /// Whether its userdata marks it important, which snapper's number cleanup counts apart.
    pub important: bool,
    /// The pre snapshot of a post snapshot.
    pub pre_number: Option<u32>,
}

/// Reads `snapper --jsonout -c root list`: an object with one list of snapshots under the
/// configuration's name.
pub fn snapshots(json: &str) -> Result<Vec<Snapshot>, String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| format!("snapper's list is not JSON: {error}"))?;
    let list = value
        .as_object()
        .and_then(|configs| configs.values().find_map(Value::as_array))
        .ok_or("snapper's list holds no configuration")?;
    list.iter()
        .map(|snapshot| {
            let number = snapshot
                .get("number")
                .and_then(Value::as_u64)
                .and_then(|number| u32::try_from(number).ok())
                .ok_or_else(|| format!("a snapshot in snapper's list has no number: {snapshot}"))?;
            let text = |field: &str| {
                snapshot
                    .get(field)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            Ok(Snapshot {
                number,
                kind: text("type"),
                description: text("description"),
                cleanup: text("cleanup"),
                important: snapshot
                    .get("userdata")
                    .and_then(|userdata| userdata.get("important"))
                    .and_then(Value::as_str)
                    == Some("yes"),
                pre_number: snapshot
                    .get("pre-number")
                    .and_then(Value::as_u64)
                    .and_then(|number| u32::try_from(number).ok())
                    .filter(|number| *number != 0),
            })
        })
        .collect()
}

/// The newest pre snapshot numbered above `after`.
pub fn newest_pre_after(snapshots: &[Snapshot], after: u32) -> Option<&Snapshot> {
    snapshots
        .iter()
        .filter(|snapshot| snapshot.kind == "pre" && snapshot.number > after)
        .max_by_key(|snapshot| snapshot.number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_secure_boot_variable_is_on_when_its_last_byte_is_one() {
        assert_eq!(efivar_on("   6   0   0   0   1\n"), Some(true));
        assert_eq!(efivar_on("   6   0   0   0   0\n"), Some(false));
        assert_eq!(efivar_on(""), None);
    }

    #[test]
    fn sbctl_verify_lines_sort_into_signed_and_unsigned_files() {
        let output = "Verifying file database and EFI images in /efi...\n\
                      ✓ /efi/EFI/refind/refind_x64.efi is signed\n\
                      ✗ /boot/vmlinuz-linux is not signed\n\
                      ✓ /efi/EFI/refind/drivers_x64/btrfs_x64.efi is signed\n";
        assert_eq!(
            verified(output),
            Verified {
                signed: vec![
                    "/efi/EFI/refind/refind_x64.efi".to_owned(),
                    "/efi/EFI/refind/drivers_x64/btrfs_x64.efi".to_owned(),
                ],
                unsigned: vec!["/boot/vmlinuz-linux".to_owned()],
            }
        );
    }

    #[test]
    fn the_guard_status_gives_the_result_and_its_failures() {
        let status = guard_status(
            r#"{"time":"2026-11-19T10:00:00Z","passed":false,"failures":["the initramfs has no modules"]}"#,
        )
        .unwrap();
        assert_eq!(
            status,
            GuardStatus {
                passed: false,
                failures: vec!["the initramfs has no modules".to_owned()],
            }
        );
        assert!(guard_status(r#"{"failures":[]}"#).is_err());
    }

    const GUARD_CONF: &str = r#"# Written by inayos-guard
default_selection "Previous system"

menuentry "Previous system" {
    volume inayos-system
    loader /@snapshots/12/snapshot/boot/vmlinuz-linux
    initrd /@snapshots/12/snapshot/boot/intel-ucode.img
    initrd /@snapshots/12/snapshot/boot/initramfs-linux.img
    options "root=UUID=1234 rootflags=subvol=@snapshots/12/snapshot systemd.volatile=overlay rw quiet"
}
"#;

    #[test]
    fn the_guard_file_gives_the_default_and_the_previous_system_entry() {
        let conf = GuardConf::parse(GUARD_CONF);
        assert_eq!(conf.default_selection.as_deref(), Some(PREVIOUS_SYSTEM));
        let entry = conf.entry(PREVIOUS_SYSTEM).unwrap();
        assert_eq!(entry.volume.as_deref(), Some("inayos-system"));
        assert_eq!(entry.initrds.len(), 2);
        assert_eq!(previous_entry_problem(&conf, 12), None);
    }

    #[test]
    fn a_previous_system_entry_for_another_snapshot_is_a_problem() {
        let conf = GuardConf::parse(GUARD_CONF);
        assert!(previous_entry_problem(&conf, 13).is_some());
        assert!(previous_entry_problem(&GuardConf::parse("default_selection 1\n"), 12).is_some());
    }

    #[test]
    fn the_previous_system_entry_may_boot_an_older_listed_pre_snapshot() {
        let conf = GuardConf::parse(GUARD_CONF);
        let snapshot = |number: u32, kind: &str| Snapshot {
            number,
            kind: kind.to_owned(),
            description: String::new(),
            cleanup: "number".to_owned(),
            important: false,
            pre_number: None,
        };
        let listed = [
            snapshot(12, "pre"),
            snapshot(13, "post"),
            snapshot(20, "pre"),
        ];
        assert_eq!(previous_entry_snapshot(&conf, &listed), Ok(12));
        let removed = [snapshot(20, "pre")];
        assert!(previous_entry_snapshot(&conf, &removed).is_err());
        let post = [snapshot(12, "post")];
        assert!(previous_entry_snapshot(&conf, &post).is_err());
        let no_entry = GuardConf::parse("default_selection 1\n");
        assert!(previous_entry_snapshot(&no_entry, &listed).is_err());
    }

    #[test]
    fn a_oneshot_unit_counts_only_when_it_ran_and_exited_with_zero() {
        let show = |active: &str, result: &str, start: &str, status: &str| {
            format!(
                "LoadState=loaded\nActiveState={active}\nResult={result}\n\
                 ExecMainStartTimestampMonotonic={start}\nExecMainStatus={status}\n"
            )
        };
        assert_eq!(
            unit_run(&show("inactive", "success", "8123456", "0")),
            UnitRun::Succeeded
        );
        assert_eq!(
            unit_run(&show("activating", "success", "8123456", "0")),
            UnitRun::Running
        );
        assert!(matches!(
            unit_run(&show("inactive", "success", "0", "0")),
            UnitRun::Problem(_)
        ));
        assert!(matches!(
            unit_run(&show("failed", "exit-code", "8123456", "1")),
            UnitRun::Problem(_)
        ));
        assert!(matches!(
            unit_run("LoadState=not-found\nActiveState=inactive\n"),
            UnitRun::Problem(_)
        ));
    }

    #[test]
    fn the_command_line_names_the_booted_snapshot_and_the_overlay() {
        let snapshot = "initrd=\\boot\\initramfs-linux.img root=UUID=1234 \
                        rootflags=subvol=@snapshots/12/snapshot systemd.volatile=overlay rw";
        assert_eq!(booted_snapshot(snapshot), Some(12));
        assert!(volatile_overlay(snapshot));
        assert_eq!(
            booted_snapshot("rootflags=compress=zstd,subvol=/@snapshots/7/snapshot"),
            Some(7)
        );
        let normal = "root=UUID=1234 rootflags=subvol=@ rw quiet";
        assert_eq!(booted_snapshot(normal), None);
        assert!(!volatile_overlay(normal));
    }

    #[test]
    fn the_newest_pre_snapshot_after_a_number_comes_from_snappers_list() {
        let list = snapshots(
            r#"{"root":[
                {"number":0,"type":"single","description":"current"},
                {"number":5,"type":"pre","description":"pacman -S foo"},
                {"number":6,"type":"post","description":"foo"},
                {"number":7,"type":"pre","description":"pacman -U linux-7.3.4.arch1-2-x86_64.pkg.tar.zst"},
                {"number":8,"type":"post","description":"linux"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(newest_pre_after(&list, 6).map(|pre| pre.number), Some(7));
        assert_eq!(newest_pre_after(&list, 7), None);
    }
}
