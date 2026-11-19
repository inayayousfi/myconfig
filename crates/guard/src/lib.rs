//! The InayOS boot guard. After each change to the boot files it checks what rEFInd will
//! start, writes rEFInd's "Previous system" entry for the last snapshot taken before a
//! pacman transaction, and makes that entry the default when a check fails. At each boot
//! it checks the kernel's lockdown mode.
//!
//! Every program it runs and every file it reads or writes goes through `System`.
#![cfg(unix)]
pub mod boot_files;
pub mod checks;
pub mod refind_conf;
pub mod snapshots;
pub mod status;
pub mod system;
mod time;

#[cfg(test)]
mod fake;

use std::path::{Path, PathBuf};

use status::{Check, CheckResult, Status};
use system::{System, read_text};

const CMDLINE: &str = "/proc/cmdline";

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

/// Checks the boot files rEFInd starts, writes `inayos-guard.conf` next to rEFInd's
/// configuration, and records the result. The configuration is written before the result,
/// so a recorded result always matches the entry rEFInd starts.
///
/// The "Previous system" entry boots the newest "pre" snapshot that passes its own checks,
/// so a failure only ever makes an entry that can start the default.
pub fn check_update(system: &dyn System) -> Result<UpdateOutcome, Error> {
    let cmdline = read_text(system, Path::new(CMDLINE)).unwrap_or_default();
    if checks::booted_from_snapshot(&cmdline) {
        return Ok(UpdateOutcome::FromSnapshot);
    }
    let boot = boot_files::find(system)?;
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
            let failures = checks::snapshot_failures(system, &candidate, signed);
            if !failures.is_empty() {
                notes.push(format!(
                    "snapshot {} cannot start, so the Previous system entry does not use it: {}",
                    candidate.number,
                    failures.join("; ")
                ));
                return None;
            }
            let options = refind_conf::previous_options(&candidate, normal_options)?;
            Some((candidate, options))
        });

    let mut failures = checks::refind_failures(system, &boot);
    if signed {
        let mut files = boot.efi_binaries.clone();
        files.extend(boot.kernels.iter().map(|kernel| kernel.image.clone()));
        failures.extend(checks::signature_failures(system, &files));
    }
    failures.extend(checks::initramfs_failures(system, &boot.kernels));
    failures.extend(checks::root_failures(system, boot.options.as_deref()));

    let selection = refind_conf::default_selection(
        failures.is_empty(),
        previous_entry.is_some(),
        &refind_conf::normal_selection(normal_file),
    );
    let text = refind_conf::render(
        previous_entry
            .as_ref()
            .map(|(previous, options)| (previous, options.as_str())),
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

/// Checks the system that just started and records the result: on a normal entry, the
/// kernel must not lock itself down, which would turn off hibernation. A system started
/// from a snapshot is not checked.
pub fn check_boot(system: &dyn System) -> Result<CheckResult, Error> {
    let cmdline_path = Path::new(CMDLINE);
    let cmdline = read_text(system, cmdline_path).map_err(|source| Error::Read {
        path: cmdline_path.to_owned(),
        source,
    })?;
    let mut failures = Vec::new();
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
    }
    let result = CheckResult::new(system.now(), failures);
    status::record(system, Check::Boot, result.clone())?;
    Ok(result)
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
}
