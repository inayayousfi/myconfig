//! The InayOS boot guard. After each change to the boot files it checks what rEFInd will
//! start, writes rEFInd's "Previous system" entry for the last snapshot taken before a
//! pacman transaction, and makes that entry the default when a check fails. At each boot
//! it checks the kernel's lockdown mode and, on an encrypted system, binds the TPM2 slots
//! to PCR 7 again after the installer or a firmware update left a temporary slot.
//!
//! Every program it runs and every file it reads or writes goes through `System`.
#![cfg(unix)]
pub mod boot_files;
pub mod checks;
pub mod encryption;
pub mod refind_conf;
pub mod snapshots;
pub mod status;
pub mod system;
mod time;
pub mod uki;

#[cfg(test)]
mod fake;

use std::path::{Path, PathBuf};

use encryption::Volume;
use refind_conf::Loader;
use snapshots::PreviousSystem;
use status::{Check, CheckResult, Status};
use system::{System, read_text};

const CMDLINE: &str = "/proc/cmdline";
/// The kernel's id of the running boot, which changes at every start.
const BOOT_ID: &str = "/proc/sys/kernel/random/boot_id";
/// Where `rebind` keeps the recovery key while it binds the volumes: a file in memory,
/// readable by root alone, removed as soon as the volumes are bound.
const KEY_FILE: &str = "/run/inayos-guard/recovery-key";

/// The guard's own failures, which stop it before it records a result. A failed check is
/// not one of them: it is part of the result.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "no rEFInd configuration was found at EFI/refind/refind.conf under /efi, /boot/efi or /boot"
    )]
    NoRefind,
    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not read the recovery key: {source}")]
    Ask { source: std::io::Error },
    #[error("the status record is not valid: {source}")]
    Status { source: serde_json::Error },
}

/// What `check_update` did.
#[derive(Debug, PartialEq, Eq)]
pub enum UpdateOutcome {
    Checked(UpdateReport),
    /// The system started from a snapshot, so /boot and /.snapshots show the snapshot and
    /// not the files on the system subvolume. Nothing was checked, written or recorded.
    FromSnapshot,
}

/// What `check_update` found and wrote.
#[derive(Debug, PartialEq, Eq)]
pub struct UpdateReport {
    pub failures: Vec<String>,
    /// What the guard could not use or do that is not a failure of the normal entry, such
    /// as a snapshot that cannot start.
    pub notes: Vec<String>,
    /// The snapshot number of the "Previous system" entry, when there is one.
    pub previous: Option<u32>,
    /// The `default_selection` written to rEFInd's configuration.
    pub default_selection: String,
    pub config: PathBuf,
}

/// How rEFInd starts InayOS on this machine.
enum Layout {
    /// rEFInd reads the kernels in /boot on the Btrfs volume.
    Plain,
    /// The system is encrypted: rEFInd starts the unified kernel image `normal` on the EFI
    /// system partition, and the "Previous system" entry starts the image `previous`.
    Encrypted {
        volumes: Vec<Volume>,
        normal: PathBuf,
        previous: PathBuf,
        /// The kernel options the "Previous system" image starts with, changed to boot the
        /// snapshot: those of `/etc/kernel/cmdline`, or those this system started with
        /// when that file does not open an encrypted root. None when neither does.
        cmdline: Option<String>,
    },
}

/// The top of the EFI system partition, the folder above `EFI/refind`.
fn esp_root(refind_dir: &Path) -> &Path {
    refind_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or(Path::new("/"))
}

/// Checks the boot files rEFInd starts, writes `inayos-guard.conf` next to rEFInd's
/// configuration, and records the result. The configuration is written before the result,
/// so a recorded result always matches the entry rEFInd starts.
///
/// The "Previous system" entry boots the newest "pre" snapshot that passes its own checks,
/// so a failure only ever makes an entry that can start the default. On an encrypted
/// system the guard builds that entry's unified kernel image first, and signs it when
/// Secure Boot is on.
pub fn check_update(system: &dyn System) -> Result<UpdateOutcome, Error> {
    let cmdline = read_text(system, Path::new(CMDLINE)).unwrap_or_default();
    if checks::booted_from_snapshot(&cmdline) {
        return Ok(UpdateOutcome::FromSnapshot);
    }
    let boot = boot_files::find(system)?;
    let esp = esp_root(&boot.refind_dir);
    let layout = match encryption::volumes(system)? {
        None => Layout::Plain,
        Some(volumes) => Layout::Encrypted {
            cmdline: [
                uki::kernel_cmdline(system).ok(),
                Some(cmdline.trim().to_owned()),
            ]
            .into_iter()
            .flatten()
            .find(|options| uki::root_failure(options, "", &volumes).is_none()),
            volumes,
            normal: esp.join(uki::NORMAL),
            previous: esp.join(uki::PREVIOUS),
        },
    };
    let normal = boot.normal_kernel();
    let normal_file = normal.map_or("vmlinuz-linux", |kernel| kernel.file.as_str());
    let normal_options = boot
        .options
        .as_ref()
        .and_then(|options| options.first())
        .map(String::as_str)
        .unwrap_or_default();
    let signed = checks::secure_boot_enforced(system);

    let mut notes = Vec::new();
    let previous_entry = snapshots::candidates(system, normal_file)
        .into_iter()
        .find_map(|candidate| {
            // On an encrypted system the snapshot's kernel goes into an image the guard
            // signs, so its own signature does not matter.
            let kernel_signed = signed && matches!(layout, Layout::Plain);
            let failures = checks::snapshot_failures(system, &candidate, kernel_signed);
            if !failures.is_empty() {
                notes.push(format!(
                    "snapshot {} cannot start, so the Previous system entry does not use it: {}",
                    candidate.number,
                    failures.join("; ")
                ));
                return None;
            }
            match previous_loader(system, &layout, &candidate, normal_options, signed, esp) {
                Ok(loader) => Some((candidate, loader)),
                Err(note) => {
                    notes.push(format!(
                        "the Previous system entry cannot start snapshot {}: {note}",
                        candidate.number
                    ));
                    None
                }
            }
        });

    let mut failures = checks::refind_failures(system, &boot);
    if signed {
        let mut files = boot.efi_binaries.clone();
        match &layout {
            Layout::Plain => files.extend(boot.kernels.iter().map(|kernel| kernel.image.clone())),
            Layout::Encrypted { normal, .. } => files.push(normal.clone()),
        }
        failures.extend(checks::signature_failures(system, &files));
    }
    failures.extend(checks::initramfs_failures(system, &boot.kernels));
    let normal_selection = match &layout {
        Layout::Plain => {
            failures.extend(checks::root_failures(system, boot.options.as_deref()));
            refind_conf::normal_selection(normal_file)
        }
        Layout::Encrypted {
            volumes, normal, ..
        } => {
            failures.extend(uki::normal_failures(system, normal, volumes));
            refind_conf::image_selection(normal)
        }
    };

    let selection = refind_conf::default_selection(
        failures.is_empty(),
        previous_entry.is_some(),
        &normal_selection,
    );
    let text = refind_conf::render(
        previous_entry
            .as_ref()
            .map(|(previous, loader)| (previous, loader)),
        &selection,
    );
    let config = boot.refind_dir.join(refind_conf::FILE_NAME);
    system
        .write_atomic(&config, text.as_bytes(), 0o644)
        .map_err(|source| Error::Write {
            path: config.clone(),
            source,
        })?;
    if let Some((previous, _)) = &previous_entry
        && let Err(note) = snapshots::keep(system, previous.number)
    {
        notes.push(note);
    }
    let mut result = CheckResult::new(system.now(), failures.clone());
    result.previous_system_default = Some(selection == refind_conf::PREVIOUS_TITLE);
    status::record(system, Check::Update, result)?;
    Ok(UpdateOutcome::Checked(UpdateReport {
        failures,
        notes,
        previous: previous_entry.map(|(previous, _)| previous.number),
        default_selection: selection,
        config,
    }))
}

/// What the "Previous system" entry starts for `candidate`: on a plain system its kernel
/// on the Btrfs volume, and on an encrypted one the unified kernel image the guard builds
/// for it. Returns why it cannot start the snapshot.
fn previous_loader(
    system: &dyn System,
    layout: &Layout,
    candidate: &PreviousSystem,
    normal_options: &str,
    signed: bool,
    esp: &Path,
) -> Result<Loader, String> {
    match layout {
        Layout::Plain => refind_conf::previous_options(candidate, normal_options)
            .map(|options| Loader::Btrfs { options })
            .ok_or_else(|| format!("{} has no root= option", boot_files::REFIND_LINUX_CONFIG)),
        Layout::Encrypted {
            previous, cmdline, ..
        } => {
            let cmdline = cmdline.as_deref().ok_or_else(|| {
                format!(
                    "neither {} nor {CMDLINE} boots a volume of {}",
                    uki::KERNEL_CMDLINE,
                    encryption::CRYPTTAB
                )
            })?;
            uki::build_previous(system, candidate, cmdline, previous, signed)?;
            let path = previous
                .strip_prefix(esp)
                .unwrap_or(previous)
                .to_string_lossy()
                .into_owned();
            Ok(Loader::Image { path })
        }
    }
}

/// Checks the system that just started and records the result: on a normal entry, the
/// kernel must not lock itself down, which would turn off hibernation. On an encrypted
/// system started from a normal entry, each volume with a temporary TPM2 slot gets a new
/// slot bound to PCR 7 in place of its earlier TPM2 slots, when Secure Boot is in the
/// state the machine was set up with and no firmware update was prepared during this
/// boot. Then every TPM2 slot is tested, and a volume that no slot opens is a failure.
/// A system started from a snapshot is not checked.
pub fn check_boot(system: &dyn System) -> Result<CheckResult, Error> {
    check_boot_after(system, encryption::TpmReport::default())
}

/// `check_boot`, with the changes and failures of a step that ran just before it.
fn check_boot_after(
    system: &dyn System,
    before: encryption::TpmReport,
) -> Result<CheckResult, Error> {
    let cmdline_path = Path::new(CMDLINE);
    let cmdline = read_text(system, cmdline_path).map_err(|source| Error::Read {
        path: cmdline_path.to_owned(),
        source,
    })?;
    let mut failures = before.failures;
    let mut changes = before.changes;
    let mut tpm = encryption::TpmReport::default();
    let mut firmware_used = false;
    if !checks::booted_from_snapshot(&cmdline) {
        // Without the file, the kernel has no lockdown support and never locks down.
        if let Ok(text) = read_text(system, Path::new(checks::LOCKDOWN)) {
            match checks::lockdown_mode(&text) {
                Some("none") => {}
                Some(mode) => failures.push(format!(
                    "the kernel runs in lockdown mode \"{mode}\", which turns off hibernation"
                )),
                None => failures.push(format!(
                    "{} does not say which lockdown mode the kernel uses",
                    checks::LOCKDOWN
                )),
            }
        }
        if let Some(volumes) = encryption::volumes(system)? {
            let mismatch = checks::secure_boot_mismatch(system);
            let prepared_now = firmware_prepared_this_boot(system);
            let hold = match (&mismatch, prepared_now) {
                (Some(reason), _) => encryption::Hold::SecureBoot(reason),
                (None, true) => encryption::Hold::FirmwareUpdate,
                (None, false) => encryption::Hold::None,
            };
            tpm = encryption::bind_to_pcr7(system, &volumes, hold);
            // A preparation from an earlier boot is over: this boot used its temporary
            // slots, or the boot's own result now says why they stay.
            firmware_used = !prepared_now;
        }
    }
    failures.extend(tpm.failures);
    changes.extend(tpm.changes);
    let mut result = CheckResult::new(system.now(), failures);
    result.tpm = tpm.state;
    result.changes = changes;
    result.notes = tpm.notes;
    let recorded = result.clone();
    status::record_with(system, |status| {
        let status = status.with(Check::Boot, recorded);
        if firmware_used {
            status.without_firmware()
        } else {
            status
        }
    })?;
    Ok(result)
}

/// The id of the running boot, or None when the kernel does not give it.
fn boot_id(system: &dyn System) -> Option<String> {
    read_text(system, Path::new(BOOT_ID))
        .ok()
        .map(|text| text.trim().to_owned())
        .filter(|id| !id.is_empty())
}

/// Whether `prepare-firmware-update` ran during this boot.
fn firmware_prepared_this_boot(system: &dyn System) -> bool {
    let Some(now) = boot_id(system) else {
        return false;
    };
    status::read(system)
        .ok()
        .flatten()
        .and_then(|status| status.firmware)
        .and_then(|firmware| firmware.boot_id)
        .is_some_and(|id| id == now)
}

/// Before a firmware or Secure Boot database update, which changes PCR 7: on an encrypted
/// system, adds a temporary TPM2 slot bound to no PCR to each volume that has none, and
/// records the result with the id of this boot. The next boot's `check-boot` binds the
/// volumes to PCR 7 again. None on a system that is not encrypted, where nothing is done
/// or recorded.
pub fn prepare_firmware_update(system: &dyn System) -> Result<Option<CheckResult>, Error> {
    let Some(volumes) = encryption::volumes(system)? else {
        return Ok(None);
    };
    let tpm = encryption::add_temporary_slots(system, &volumes);
    let mut result = CheckResult::new(system.now(), tpm.failures);
    result.tpm = tpm.state;
    result.changes = tpm.changes;
    result.boot_id = boot_id(system);
    status::record(system, Check::Firmware, result.clone())?;
    Ok(Some(result))
}

/// What `rebind` did.
#[derive(Debug, PartialEq, Eq)]
pub enum RebindOutcome {
    /// The system is not encrypted.
    NotEncrypted,
    /// Every volume opens with a TPM2 slot on this boot, so nothing was asked or changed.
    AlreadyBound,
    /// Nothing was asked or changed, for the reason given.
    Refused(String),
    /// The volumes were bound again, and `check-boot` ran after it: its recorded result.
    Rebound(CheckResult),
}

/// Binds the TPM to PCR 7 again on each volume that no TPM2 slot opens on this boot, such
/// as after the TPM was cleared: asks once for the recovery key, enrolls a slot bound to
/// PCR 7 with it on each such volume, wipes their dead TPM2 slots, then runs `check-boot`
/// and records its result. It refuses on a system started from a snapshot, and when
/// Secure Boot is not in the state the machine was set up with, because the new slots
/// would then open the volumes only in that wrong state.
pub fn rebind(system: &dyn System) -> Result<RebindOutcome, Error> {
    let Some(volumes) = encryption::volumes(system)? else {
        return Ok(RebindOutcome::NotEncrypted);
    };
    let cmdline = read_text(system, Path::new(CMDLINE)).unwrap_or_default();
    if checks::booted_from_snapshot(&cmdline) {
        return Ok(RebindOutcome::Refused(
            "this system started from a snapshot; start the normal entry first".to_owned(),
        ));
    }
    if let Some(reason) = checks::secure_boot_mismatch(system) {
        return Ok(RebindOutcome::Refused(format!(
            "{reason}; turn Secure Boot back on in the firmware settings first"
        )));
    }
    let unbound = match encryption::unbound(system, &volumes) {
        Ok(unbound) => unbound,
        Err(failures) => return Ok(RebindOutcome::Refused(failures.join("; "))),
    };
    if unbound.is_empty() {
        return Ok(RebindOutcome::AlreadyBound);
    }
    let key = system
        .ask_secret("Recovery key of the disk:")
        .map_err(|source| Error::Ask { source })?;
    let key_file = Path::new(KEY_FILE);
    let parent = key_file.parent().unwrap_or(Path::new("/run"));
    system
        .create_dir_all(parent)
        .and_then(|()| system.write_atomic(key_file, key.trim_end().as_bytes(), 0o600))
        .map_err(|source| Error::Write {
            path: key_file.to_owned(),
            source,
        })?;
    let report = encryption::rebind(system, &unbound, key_file);
    system
        .remove_file(key_file)
        .map_err(|source| Error::Write {
            path: key_file.to_owned(),
            source,
        })?;
    check_boot_after(system, report).map(RebindOutcome::Rebound)
}

/// The last recorded status, or None when no check has run yet.
pub fn status(system: &dyn System) -> Result<Option<Status>, Error> {
    status::read(system)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::{FakeSystem, exit, kernel_image, ok};

    const VERSION: &str = "7.3.4-arch1-2";
    const OPTIONS: &str = "root=UUID=abc rw rootflags=subvol=@ quiet";

    fn listing(with_modules: bool) -> String {
        let mut text =
            format!("usr/lib/modules/{VERSION}/\nusr/lib/modules/{VERSION}/modules.dep\n");
        if with_modules {
            text.push_str(&format!(
                "usr/lib/modules/{VERSION}/kernel/drivers/block/virtio_blk.ko.zst\n"
            ));
        }
        text
    }

    fn verified(file: &str, signed: i8) -> String {
        format!("[\n  {{\n    \"file_name\": \"{file}\",\n    \"is_signed\": {signed}\n  }}\n]\n")
    }

    /// An installed InayOS: rEFInd on /efi with its Btrfs driver, one kernel, a pre
    /// snapshot taken before the last pacman transaction, and Secure Boot off.
    fn machine() -> FakeSystem {
        let system = FakeSystem::default();
        system.file("/efi/EFI/refind/refind.conf", b"timeout 5\n");
        system.file("/efi/EFI/refind/refind_x64.efi", b"MZ");
        system.file("/efi/EFI/refind/drivers_x64/btrfs_x64.efi", b"MZ");
        system.file("/boot/vmlinuz-linux", &kernel_image(VERSION));
        system.file("/boot/initramfs-linux.img", b"cpio");
        system.file("/boot/intel-ucode.img", b"cpio");
        system.file(
            "/boot/refind_linux.conf",
            format!("\"Boot with standard options\"  \"{OPTIONS}\"\n\"Boot to single-user mode\"    \"{OPTIONS} single\"\n").as_bytes(),
        );
        system.file("/dev/disk/by-uuid/abc", b"");
        snapshot(&system, 7, true);
        system.file(checks::SECURE_BOOT_VARIABLE, &[6, 0, 0, 0, 0]);
        system.respond("lsinitcpio /boot/initramfs-linux.img", ok(&listing(true)));
        system
    }

    /// A pre snapshot whose boot image holds the kernel's modules, or not.
    fn snapshot(system: &FakeSystem, number: u32, with_modules: bool) {
        system.file(
            &format!("/.snapshots/{number}/info.xml"),
            format!("<snapshot><type>pre</type><num>{number}</num><description>pacman -Syu</description></snapshot>").as_bytes(),
        );
        let boot = format!("/.snapshots/{number}/snapshot/boot");
        system.file(&format!("{boot}/vmlinuz-linux"), &kernel_image(VERSION));
        system.file(&format!("{boot}/initramfs-linux.img"), b"cpio");
        system.respond(
            &format!("lsinitcpio {boot}/initramfs-linux.img"),
            ok(&listing(with_modules)),
        );
        system.respond(
            &format!("snapper --no-dbus -c root modify --userdata important=yes {number}"),
            ok(""),
        );
    }

    fn checked(system: &FakeSystem) -> UpdateReport {
        match check_update(system).expect("check-update runs") {
            UpdateOutcome::Checked(report) => report,
            UpdateOutcome::FromSnapshot => panic!("the machine did not start from a snapshot"),
        }
    }

    fn signed(system: &FakeSystem, file: &str, signed: i8) {
        system.respond(
            &format!("sbctl verify --json --disable-landlock {file}"),
            ok(&verified(file, signed)),
        );
    }

    #[test]
    fn a_healthy_update_keeps_the_normal_entry_and_offers_the_previous_system() {
        let system = machine();
        system.file(status::FAILED_FILE, b"");
        let report = checked(&system);
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(report.previous, Some(7));
        let config = system
            .text("/efi/EFI/refind/inayos-guard.conf")
            .expect("the rEFInd file");
        assert!(config.contains("menuentry \"Previous system\""));
        assert!(config.contains(
            "options \"root=UUID=abc rootflags=subvol=@snapshots/7/snapshot systemd.volatile=overlay rw quiet\""
        ));
        assert!(config.ends_with("default_selection \"vmlinuz-linux from\"\n"));
        let status = status(&system).expect("readable").expect("recorded");
        assert!(status.passed);
        assert_eq!(
            status.update.expect("update").previous_system_default,
            Some(false)
        );
        assert!(!system.exists(Path::new(status::FAILED_FILE)));
        // The snapshot the entry boots is kept from snapper's number cleanup.
        assert!(
            system.log.borrow().contains(
                &"snapper --no-dbus -c root modify --userdata important=yes 7".to_owned()
            )
        );
    }

    #[test]
    fn a_boot_image_without_modules_makes_the_previous_system_the_default() {
        let system = machine();
        system.respond("lsinitcpio /boot/initramfs-linux.img", ok(&listing(false)));
        let report = checked(&system);
        assert_eq!(
            report.failures,
            [format!(
                "/boot/initramfs-linux.img holds no kernel modules of {VERSION}, the version of /boot/vmlinuz-linux"
            )]
        );
        let config = system
            .text("/efi/EFI/refind/inayos-guard.conf")
            .expect("written");
        assert!(config.ends_with("default_selection \"Previous system\"\n"));
        let status = status(&system).expect("readable").expect("recorded");
        assert!(!status.passed);
        assert_eq!(status.failures, report.failures);
        assert_eq!(
            status.update.expect("update").previous_system_default,
            Some(true)
        );
        assert!(system.exists(Path::new(status::FAILED_FILE)));
    }

    #[test]
    fn a_snapshot_that_cannot_start_is_never_the_previous_system() {
        // A repair attempt after a broken update: snap-pac took snapshot 9 of the broken
        // system, so the entry keeps booting snapshot 7.
        let system = machine();
        snapshot(&system, 9, false);
        system.respond("lsinitcpio /boot/initramfs-linux.img", ok(&listing(false)));
        let report = checked(&system);
        assert_eq!(report.previous, Some(7));
        assert_eq!(report.default_selection, "Previous system");
        assert_eq!(report.failures.len(), 1);
        assert!(report.notes[0].starts_with("snapshot 9 cannot start"));
    }

    #[test]
    fn with_secure_boot_on_an_unsigned_kernel_fails_the_update() {
        let system = machine();
        system.file(checks::SECURE_BOOT_VARIABLE, &[6, 0, 0, 0, 1]);
        for file in [
            "/efi/EFI/refind/refind_x64.efi",
            "/efi/EFI/refind/drivers_x64/btrfs_x64.efi",
            "/.snapshots/7/snapshot/boot/vmlinuz-linux",
        ] {
            signed(&system, file, 1);
        }
        signed(&system, "/boot/vmlinuz-linux", 0);
        let report = checked(&system);
        assert_eq!(
            report.failures,
            ["/boot/vmlinuz-linux is not signed, so Secure Boot refuses to start it"]
        );
        assert_eq!(report.default_selection, "Previous system");
    }

    #[test]
    fn with_secure_boot_on_an_unsigned_snapshot_kernel_only_rules_out_that_snapshot() {
        // The snapshot snap-pac took before sbctl signed the kernel holds an unsigned copy.
        let system = machine();
        system.file(checks::SECURE_BOOT_VARIABLE, &[6, 0, 0, 0, 1]);
        snapshot(&system, 5, true);
        for file in [
            "/efi/EFI/refind/refind_x64.efi",
            "/efi/EFI/refind/drivers_x64/btrfs_x64.efi",
            "/boot/vmlinuz-linux",
            "/.snapshots/5/snapshot/boot/vmlinuz-linux",
        ] {
            signed(&system, file, 1);
        }
        signed(&system, "/.snapshots/7/snapshot/boot/vmlinuz-linux", 0);
        let report = checked(&system);
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(report.previous, Some(5));
        assert_eq!(report.default_selection, "vmlinuz-linux from");
        assert!(report.notes[0].starts_with("snapshot 7 cannot start"));
    }

    #[test]
    fn check_update_leaves_everything_alone_on_a_system_started_from_a_snapshot() {
        let system = machine();
        system.file(
            "/efi/EFI/refind/inayos-guard.conf",
            b"default_selection \"Previous system\"\n",
        );
        system.file(status::FAILED_FILE, b"");
        system.file(
            "/proc/cmdline",
            b"root=UUID=abc rootflags=subvol=@snapshots/7/snapshot systemd.volatile=overlay rw\n",
        );
        assert_eq!(
            check_update(&system).expect("check-update runs"),
            UpdateOutcome::FromSnapshot
        );
        assert_eq!(
            system.text("/efi/EFI/refind/inayos-guard.conf").as_deref(),
            Some("default_selection \"Previous system\"\n")
        );
        assert!(system.exists(Path::new(status::FAILED_FILE)));
        assert!(status(&system).expect("readable").is_none());
    }

    #[test]
    fn a_root_disk_that_does_not_exist_fails_the_update() {
        let system = machine();
        system
            .files
            .borrow_mut()
            .remove(Path::new("/dev/disk/by-uuid/abc"));
        system.respond("blkid -U abc", exit(2));
        let report = checked(&system);
        assert_eq!(
            report.failures,
            ["/boot/refind_linux.conf boots root=UUID=abc, a disk that does not exist"]
        );
    }

    #[test]
    fn without_a_snapshot_a_failure_keeps_the_normal_entry() {
        let system = machine();
        system
            .files
            .borrow_mut()
            .retain(|path, _| !path.starts_with("/.snapshots"));
        system.respond("lsinitcpio /boot/initramfs-linux.img", ok(""));
        let report = checked(&system);
        assert_eq!(report.previous, None);
        let config = system
            .text("/efi/EFI/refind/inayos-guard.conf")
            .expect("written");
        assert!(!config.contains("menuentry"));
        assert!(config.ends_with("default_selection \"vmlinuz-linux from\"\n"));
        let status = status(&system).expect("readable").expect("recorded");
        assert_eq!(
            status.update.expect("update").previous_system_default,
            Some(false)
        );
    }

    #[test]
    fn check_boot_fails_on_lockdown_except_from_a_snapshot() {
        let system = machine();
        system.file(checks::LOCKDOWN, b"none [integrity] confidentiality\n");
        system.file(
            "/proc/cmdline",
            format!("initrd=\\boot\\initramfs-linux.img {OPTIONS}\n").as_bytes(),
        );
        let result = check_boot(&system).expect("check-boot runs");
        assert!(!result.passed);
        let status = status(&system).expect("readable").expect("recorded");
        assert_eq!(status.boot, Some(result));

        system.file(
            "/proc/cmdline",
            b"root=UUID=abc rootflags=subvol=@snapshots/7/snapshot systemd.volatile=overlay rw\n",
        );
        assert!(check_boot(&system).expect("check-boot runs").passed);
    }

    const ENCRYPTED_CMDLINE: &str = "root=/dev/mapper/system rootflags=subvol=@ rw quiet";
    const SYSTEM: &str = "/dev/disk/by-uuid/1111";
    const HOME: &str = "/dev/disk/by-uuid/2222";

    fn dump(device: &str) -> String {
        format!("cryptsetup luksDump --dump-json-metadata {device}")
    }

    /// An installed encrypted InayOS: the machine above, with the normal unified kernel
    /// image on the EFI system partition, both volumes in crypttab.initramfs, and each
    /// volume bound to PCR 7.
    fn encrypted() -> FakeSystem {
        let system = machine();
        system
            .files
            .borrow_mut()
            .remove(Path::new("/boot/refind_linux.conf"));
        system.file(
            encryption::CRYPTTAB,
            b"system UUID=1111 none tpm2-device=auto,discard\nhome UUID=2222 none tpm2-device=auto,discard\n",
        );
        system.file(
            uki::KERNEL_CMDLINE,
            format!("{ENCRYPTED_CMDLINE}\n").as_bytes(),
        );
        system.file(
            "/efi/EFI/Linux/inayos-linux.efi",
            &uki::tests::image(&[
                (".cmdline", ENCRYPTED_CMDLINE.as_bytes()),
                (".linux", &kernel_image(VERSION)),
                (".initrd", b"cpio"),
            ]),
        );
        system.respond(
            "lsinitcpio /run/inayos-guard/initrd.img",
            ok(&listing(true)),
        );
        let build = format!(
            "ukify build --linux=/.snapshots/7/snapshot/boot/vmlinuz-linux --initrd=/.snapshots/7/snapshot/boot/initramfs-linux.img --cmdline=root=/dev/mapper/system rootflags=subvol=@snapshots/7/snapshot systemd.volatile=overlay rw quiet --output={PREVIOUS_NEW}"
        );
        system.respond(&build, ok(""));
        system.creates(&build, PREVIOUS_NEW);
        for device in [SYSTEM, HOME] {
            system.respond(
                &dump(device),
                ok(&encryption::tests::header(&[(1, 1, "7")])),
            );
        }
        system
    }

    const PREVIOUS_NEW: &str = "/efi/EFI/inayos/previous.efi.new";

    #[test]
    fn an_encrypted_update_builds_the_previous_image_and_keeps_the_normal_image() {
        let system = encrypted();
        let report = checked(&system);
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(report.notes, Vec::<String>::new());
        assert_eq!(report.previous, Some(7));
        assert_eq!(report.default_selection, "inayos-linux.efi");
        assert!(system.exists(Path::new("/efi/EFI/inayos/previous.efi")));
        assert!(!system.exists(Path::new(PREVIOUS_NEW)));
        let config = system
            .text("/efi/EFI/refind/inayos-guard.conf")
            .expect("written");
        assert!(
            config.contains(
                "menuentry \"Previous system\" {\n    loader /EFI/inayos/previous.efi\n}\n"
            )
        );
        assert!(config.ends_with("default_selection \"inayos-linux.efi\"\n"));
    }

    #[test]
    fn with_secure_boot_on_the_encrypted_update_signs_the_previous_image() {
        let system = encrypted();
        system.file(checks::SECURE_BOOT_VARIABLE, &[6, 0, 0, 0, 1]);
        for file in [
            "/efi/EFI/refind/refind_x64.efi",
            "/efi/EFI/refind/drivers_x64/btrfs_x64.efi",
            "/efi/EFI/Linux/inayos-linux.efi",
            PREVIOUS_NEW,
        ] {
            signed(&system, file, 1);
        }
        system.respond(&format!("sbctl sign {PREVIOUS_NEW}"), ok(""));
        let report = checked(&system);
        assert_eq!(report.failures, Vec::<String>::new());
        assert_eq!(report.previous, Some(7));
        assert!(system.ran(&format!("sbctl sign {PREVIOUS_NEW}")));
        // The snapshot's own kernel is not checked: rEFInd starts the signed image.
        assert!(!system.ran(
            "sbctl verify --json --disable-landlock /.snapshots/7/snapshot/boot/vmlinuz-linux"
        ));
        assert!(!system.ran("sbctl verify --json --disable-landlock /boot/vmlinuz-linux"));
    }

    #[test]
    fn an_encrypted_root_that_crypttab_does_not_open_fails_the_update() {
        let system = encrypted();
        system.file(uki::KERNEL_CMDLINE, b"root=/dev/mapper/root rw\n");
        system.file("/proc/cmdline", format!("{ENCRYPTED_CMDLINE}\n").as_bytes());
        let report = checked(&system);
        assert_eq!(
            report.failures,
            [
                "/etc/kernel/cmdline boots root=/dev/mapper/root, which is not a volume that /etc/crypttab.initramfs opens"
            ]
        );
        // The snapshot's image takes the options this system started with.
        assert_eq!(report.default_selection, "Previous system");
    }

    #[test]
    fn a_previous_image_that_cannot_be_built_leaves_no_entry() {
        let system = encrypted();
        system.respond(
            &format!(
                "ukify build --linux=/.snapshots/7/snapshot/boot/vmlinuz-linux --initrd=/.snapshots/7/snapshot/boot/initramfs-linux.img --cmdline=root=/dev/mapper/system rootflags=subvol=@snapshots/7/snapshot systemd.volatile=overlay rw quiet --output={PREVIOUS_NEW}"
            ),
            exit(1),
        );
        let report = checked(&system);
        assert_eq!(report.previous, None);
        assert!(report.notes[0].starts_with(
            "the Previous system entry cannot start snapshot 7: ukify could not build it"
        ));
        let config = system
            .text("/efi/EFI/refind/inayos-guard.conf")
            .expect("written");
        assert!(!config.contains("menuentry"));
    }

    fn first_boot(system: &FakeSystem) {
        system.file("/proc/cmdline", format!("{ENCRYPTED_CMDLINE}\n").as_bytes());
        for device in [SYSTEM, HOME] {
            system.respond_in_turn(
                &dump(device),
                vec![
                    ok(&encryption::tests::header(&[(1, 1, "")])),
                    ok(&encryption::tests::header(&[(1, 1, ""), (2, 2, "7")])),
                    ok(&encryption::tests::header(&[(2, 2, "7")])),
                ],
            );
            for command in [
                format!(
                    "systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs=7 {device}"
                ),
                format!("cryptsetup open --test-passphrase --token-only --token-id 2 {device}"),
                format!("systemd-cryptenroll --wipe-slot=1 {device}"),
            ] {
                system.respond(&command, ok(""));
            }
        }
    }

    #[test]
    fn the_first_encrypted_boot_binds_both_volumes_to_pcr7_and_records_it() {
        let system = encrypted();
        first_boot(&system);
        let result = check_boot(&system).expect("check-boot runs");
        assert!(result.passed, "{:?}", result.failures);
        assert_eq!(
            result.changes,
            [
                "system: enrolled TPM2 slot 2 bound to PCR 7",
                "system: wiped the replaced TPM2 slots 1",
                "home: enrolled TPM2 slot 2 bound to PCR 7",
                "home: wiped the replaced TPM2 slots 1",
            ]
        );
        assert_eq!(
            result.tpm,
            [
                "system: TPM2 slot 2 bound to PCR 7",
                "home: TPM2 slot 2 bound to PCR 7"
            ]
        );
        let status = status(&system).expect("readable").expect("recorded");
        assert_eq!(status.boot, Some(result));
    }

    #[test]
    fn with_sbctl_keys_and_secure_boot_off_the_temporary_slot_stays() {
        let system = encrypted();
        first_boot(&system);
        system.respond(
            "sbctl status --json",
            ok("{\"installed\":true,\"setup_mode\":true,\"secure_boot\":false}"),
        );
        for device in [SYSTEM, HOME] {
            opens(&system, device, 1, true);
        }
        let result = check_boot(&system).expect("check-boot runs");
        assert!(!result.passed);
        assert_eq!(result.failures.len(), 2);
        assert!(
            result.failures[0]
                .contains("Secure Boot is off although sbctl holds the machine's keys")
        );
        assert!(
            !system
                .log
                .borrow()
                .iter()
                .any(|line| line.starts_with("systemd-cryptenroll"))
        );
    }

    #[test]
    fn a_boot_from_a_snapshot_leaves_the_tpm_slots_alone() {
        let system = encrypted();
        first_boot(&system);
        system.file(
            "/proc/cmdline",
            b"root=/dev/mapper/system rootflags=subvol=@snapshots/7/snapshot systemd.volatile=overlay rw\n",
        );
        let result = check_boot(&system).expect("check-boot runs");
        assert!(result.passed);
        assert!(result.tpm.is_empty());
        assert!(system.log.borrow().is_empty());
    }

    #[test]
    fn prepare_firmware_update_does_nothing_on_a_plain_system() {
        let system = machine();
        assert_eq!(prepare_firmware_update(&system).expect("runs"), None);
        assert!(status(&system).expect("readable").is_none());
    }

    #[test]
    fn prepare_firmware_update_adds_and_records_a_temporary_slot_on_each_volume() {
        let system = encrypted();
        for device in [SYSTEM, HOME] {
            system.respond_in_turn(
                &dump(device),
                vec![
                    ok(&encryption::tests::header(&[(1, 1, "7")])),
                    ok(&encryption::tests::header(&[(1, 1, "7"), (2, 2, "")])),
                ],
            );
            system.respond(
                &format!("systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs= {device}"),
                ok(""),
            );
        }
        let result = prepare_firmware_update(&system)
            .expect("runs")
            .expect("encrypted");
        assert!(result.passed);
        assert_eq!(result.changes.len(), 2);
        let status = status(&system).expect("readable").expect("recorded");
        assert_eq!(status.firmware, Some(result));
    }

    const BOOT_ID_FILE: &str = "/proc/sys/kernel/random/boot_id";

    fn opens(system: &FakeSystem, device: &str, token: u32, works: bool) {
        system.respond(
            &format!("cryptsetup open --test-passphrase --token-only --token-id {token} {device}"),
            if works { ok("") } else { exit(1) },
        );
    }

    #[test]
    fn a_boot_after_the_tpm_was_cleared_fails_and_names_rebind() {
        let system = encrypted();
        system.file("/proc/cmdline", format!("{ENCRYPTED_CMDLINE}\n").as_bytes());
        for device in [SYSTEM, HOME] {
            opens(&system, device, 1, false);
        }
        let result = check_boot(&system).expect("check-boot runs");
        assert!(!result.passed);
        assert_eq!(result.failures.len(), 2);
        assert!(
            result
                .failures
                .iter()
                .all(|f| f.contains("sudo inayos-guard rebind"))
        );
        assert_eq!(
            result.tpm[0],
            "system: TPM2 slot 1 bound to PCR 7, does not open the volume"
        );
        // The desktop notification watches this file.
        assert!(system.exists(Path::new(status::FAILED_FILE)));
    }

    #[test]
    fn rebind_asks_once_binds_both_volumes_and_records_a_passing_boot() {
        let system = encrypted();
        system.file("/proc/cmdline", format!("{ENCRYPTED_CMDLINE}\n").as_bytes());
        *system.secret.borrow_mut() = Some("aaaa-bbbb\n".to_owned());
        for device in [SYSTEM, HOME] {
            system.respond_in_turn(
                &dump(device),
                vec![
                    ok(&encryption::tests::header(&[(1, 1, "7")])),
                    ok(&encryption::tests::header(&[(2, 2, "7")])),
                ],
            );
            opens(&system, device, 1, false);
            opens(&system, device, 2, true);
            system.respond(
                &format!("systemd-cryptenroll --unlock-key-file=/run/inayos-guard/recovery-key --tpm2-device=auto --tpm2-pcrs=7 --wipe-slot=1 {device}"),
                ok(""),
            );
        }
        let outcome = rebind(&system).expect("rebind runs");
        let RebindOutcome::Rebound(result) = outcome else {
            panic!("not rebound: {outcome:?}");
        };
        assert!(result.passed, "{:?}", result.failures);
        assert_eq!(result.changes.len(), 4);
        assert_eq!(
            result.tpm,
            [
                "system: TPM2 slot 2 bound to PCR 7",
                "home: TPM2 slot 2 bound to PCR 7"
            ]
        );
        let asked = system
            .log
            .borrow()
            .iter()
            .filter(|line| line.starts_with("ask "))
            .count();
        assert_eq!(asked, 1);
        assert!(!system.exists(Path::new(KEY_FILE)));
        assert!(!system.exists(Path::new(status::FAILED_FILE)));
    }

    #[test]
    fn rebind_with_secure_boot_off_asks_nothing() {
        let system = encrypted();
        system.respond(
            "sbctl status --json",
            ok("{\"installed\":true,\"setup_mode\":false,\"secure_boot\":false}"),
        );
        let outcome = rebind(&system).expect("rebind runs");
        assert!(matches!(outcome, RebindOutcome::Refused(_)), "{outcome:?}");
        assert!(
            !system
                .log
                .borrow()
                .iter()
                .any(|line| line.starts_with("ask "))
        );
    }

    #[test]
    fn check_boot_keeps_the_temporary_slots_of_a_firmware_update_prepared_this_boot() {
        let system = encrypted();
        system.file("/proc/cmdline", format!("{ENCRYPTED_CMDLINE}\n").as_bytes());
        system.file(BOOT_ID_FILE, b"boot-a\n");
        for device in [SYSTEM, HOME] {
            system.respond_in_turn(
                &dump(device),
                vec![
                    ok(&encryption::tests::header(&[(1, 1, "7")])),
                    ok(&encryption::tests::header(&[(1, 1, "7"), (2, 2, "")])),
                ],
            );
            system.respond(
                &format!("systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs= {device}"),
                ok(""),
            );
            opens(&system, device, 1, true);
            opens(&system, device, 2, true);
        }
        let prepared = prepare_firmware_update(&system)
            .expect("runs")
            .expect("encrypted");
        assert_eq!(prepared.boot_id.as_deref(), Some("boot-a"));

        // The boot service runs after the preparation, during the same boot.
        let result = check_boot(&system).expect("check-boot runs");
        assert!(result.passed, "{:?}", result.failures);
        assert!(result.changes.is_empty());
        assert_eq!(result.notes.len(), 2);
        assert!(
            !system
                .log
                .borrow()
                .iter()
                .any(|line| line.contains("--tpm2-pcrs=7"))
        );
        let recorded = status(&system).expect("readable").expect("recorded");
        assert!(recorded.firmware.is_some());

        // At the next boot the preparation is over, so its result goes.
        system.file(BOOT_ID_FILE, b"boot-b\n");
        for device in [SYSTEM, HOME] {
            system.respond_in_turn(
                &dump(device),
                vec![
                    ok(&encryption::tests::header(&[(1, 1, "7"), (2, 2, "")])),
                    ok(&encryption::tests::header(&[
                        (1, 1, "7"),
                        (2, 2, ""),
                        (3, 3, "7"),
                    ])),
                    ok(&encryption::tests::header(&[(3, 3, "7")])),
                ],
            );
            system.respond(
                &format!("systemd-cryptenroll --unlock-tpm2-device=auto --tpm2-device=auto --tpm2-pcrs=7 {device}"),
                ok(""),
            );
            opens(&system, device, 3, true);
            system.respond(
                &format!("systemd-cryptenroll --wipe-slot=1,2 {device}"),
                ok(""),
            );
        }
        let result = check_boot(&system).expect("check-boot runs");
        assert!(result.passed, "{:?}", result.failures);
        let recorded = status(&system).expect("readable").expect("recorded");
        assert!(recorded.firmware.is_none());
    }
}
