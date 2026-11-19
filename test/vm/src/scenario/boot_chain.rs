//! The steps of the boot chain scenarios: the checks of Secure Boot and of the guard on an
//! installed system, the pacman transaction that rebuilds the boot files broken, and the boot
//! of the "Previous system" entry that the guard makes the default after it.

use std::thread;
use std::time::Duration;

use super::boot_checks::{self, GuardConf, GuardStatus, PREVIOUS_SYSTEM};
use super::{Scenario, StepError, sudo};
use crate::ssh::{Guest, SshError};

const SECURE_BOOT_VARIABLE: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
const GUARD: &str = "/usr/local/bin/inayos-guard";
const GUARD_STATUS: &str = "/var/lib/inayos-guard/status.json";
const GUARD_FAILED: &str = "/var/lib/inayos-guard/failed";
const GUARD_CONF: &str = "inayos-guard.conf";
const GUARD_INCLUDE: &str = "include inayos-guard.conf";
/// The folders where rEFInd's configuration can be, as the refind module looks for it.
const REFIND_FOLDERS: [&str; 3] = [
    "/efi/EFI/refind",
    "/boot/efi/EFI/refind",
    "/boot/EFI/refind",
];
const BREAK_SCRIPT: &str = include_str!("../../scenario/break-boot.sh");
const BREAK_DROP_IN: &str = "/etc/mkinitcpio.conf.d/zz-test-break.conf";
/// rEFInd waits 20 seconds with its default configuration, after the firmware's few seconds.
/// The screenshots cover that time.
const REFIND_SHOTS: u32 = 8;
const GUARD_BOOT_UNIT: &str = "inayos-guard-boot.service";
const GUARD_BOOT_WAITS: u32 = 30;
const GUARD_BOOT_WAIT: Duration = Duration::from_secs(2);
const REFIND_SHOT_INTERVAL: Duration = Duration::from_secs(3);

impl Scenario {
    /// Checks that the firmware runs with Secure Boot on and that `sbctl verify` finds no
    /// unsigned file.
    pub(super) fn check_secure_boot(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: checking that Secure Boot is on");
        let od = user.run(&format!("od -An -tu1 {SECURE_BOOT_VARIABLE}"))?;
        if boot_checks::efivar_on(&od) != Some(true) {
            return Err(StepError::Check(format!(
                "the SecureBoot firmware variable is not 1: {:?}",
                od.trim()
            )));
        }
        println!("vm: checking the signatures with sbctl verify");
        // sbctl's exit status does not count: the file list below decides, and an empty list fails.
        let output = user.run(&format!("{} || true", sudo("sbctl verify 2>&1")))?;
        self.write("sbctl-verify.txt", &output)?;
        let verified = boot_checks::verified(&output);
        if !verified.unsigned.is_empty() {
            return Err(StepError::Check(format!(
                "sbctl verify lists unsigned files: {}",
                verified.unsigned.join(", ")
            )));
        }
        if verified.signed.is_empty() {
            return Err(StepError::Check(format!(
                "sbctl verify lists no signed file:\n{output}"
            )));
        }
        println!(
            "vm: Secure Boot is on, and sbctl verify lists {} signed files",
            verified.signed.len()
        );
        Ok(())
    }

    /// Checks the guard on a new installed system: its last result passed, rEFInd includes its
    /// file, and that file has a "Previous system" entry, not the default, that boots a pre
    /// snapshot snapper lists and whose kernel exists.
    pub(super) fn check_guard_after_install(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: checking the guard's result and its rEFInd file");
        let status = self.guard_status(user, "install")?;
        if !status.passed {
            return Err(StepError::Check(format!(
                "the guard's last result failed: {}",
                status.failures.join("; ")
            )));
        }
        let conf = self.guard_conf(user, "install")?;
        let snapshots = self.snapshots(user)?;
        let snapshot =
            boot_checks::previous_entry_snapshot(&conf, &snapshots).map_err(StepError::Check)?;
        let loader = conf
            .entry(PREVIOUS_SYSTEM)
            .and_then(|entry| entry.loader.as_deref())
            .unwrap_or_default();
        let kernel = loader.replacen("/@snapshots/", "/.snapshots/", 1);
        if user.run(&sudo(&format!("test -f {kernel}"))).is_err() {
            return Err(StepError::Check(format!(
                "the {PREVIOUS_SYSTEM:?} entry's kernel {kernel} does not exist"
            )));
        }
        if conf.default_selection.as_deref() == Some(PREVIOUS_SYSTEM) {
            return Err(StepError::Check(format!(
                "the guard passed, but {PREVIOUS_SYSTEM:?} is rEFInd's default"
            )));
        }
        println!("vm: the guard passed, and {PREVIOUS_SYSTEM:?} boots snapshot {snapshot}");
        Ok(())
    }

    /// Puts the guard built from the working tree in place of the installed one, and checks
    /// that it passes on the system before the broken transaction.
    pub(super) fn put_fresh_guard(&mut self, user: &Guest) -> Result<(), StepError> {
        let guard = self
            .binaries
            .iter()
            .find(|binary| binary.executable == "inayos-guard")
            .expect("the scenarios build the guard");
        println!("vm: putting the guard built from the working tree in {GUARD}");
        user.copy_to(&guard.local, "/tmp/inayos-guard")?;
        user.run(&sudo(&format!("install -m 755 /tmp/inayos-guard {GUARD}")))?;
        user.run_logged(
            &sudo(&format!("{GUARD} check-update")),
            &self.log("guard-before"),
        )?;
        let status = self.guard_status(user, "before")?;
        if !status.passed {
            return Err(StepError::Check(format!(
                "the guard fails before the boot files are broken: {}",
                status.failures.join("; ")
            )));
        }
        Ok(())
    }

    /// Runs the pacman transaction that rebuilds the boot files without kernel modules, and
    /// returns the number of the pre snapshot that snap-pac took of it.
    pub(super) fn break_boot_files(&mut self, user: &Guest) -> Result<u32, StepError> {
        let before = self
            .snapshots(user)?
            .iter()
            .map(|snapshot| snapshot.number)
            .max()
            .unwrap_or_default();
        println!("vm: reinstalling linux with an initramfs that leaves the kernel modules out");
        let script = self.write("break-boot.sh", BREAK_SCRIPT)?;
        user.copy_to(&script, "/tmp/break-boot.sh")?;
        user.run_logged(
            &sudo(&format!("bash /tmp/break-boot.sh {BREAK_DROP_IN}")),
            &self.log("break-boot"),
        )?;
        let snapshots = self.snapshots(user)?;
        let Some(pre) = boot_checks::newest_pre_after(&snapshots, before) else {
            return Err(StepError::Check(format!(
                "snap-pac took no pre snapshot of the transaction: snapper lists none after {before}"
            )));
        };
        if !pre.description.contains("linux") {
            return Err(StepError::Check(format!(
                "the pre snapshot {} describes {:?}, not the linux transaction",
                pre.number, pre.description
            )));
        }
        println!("vm: snap-pac took the pre snapshot {}", pre.number);
        Ok(pre.number)
    }

    /// Checks that the guard caught the broken boot files: its result failed, its failed file
    /// exists for the notification, and "Previous system" boots `snapshot` and is the default.
    pub(super) fn check_guard_caught(
        &mut self,
        user: &Guest,
        snapshot: u32,
    ) -> Result<(), StepError> {
        println!("vm: checking that the guard caught the broken boot files");
        let status = self.guard_status(user, "broken")?;
        if status.passed || status.failures.is_empty() {
            return Err(StepError::Check(format!(
                "the guard did not report the initramfs without modules: {status:?}"
            )));
        }
        if user.run(&sudo(&format!("test -e {GUARD_FAILED}"))).is_err() {
            return Err(StepError::Check(format!(
                "the guard failed but left no {GUARD_FAILED} for the notification"
            )));
        }
        let conf = self.guard_conf(user, "broken")?;
        if let Some(problem) = boot_checks::previous_entry_problem(&conf, snapshot) {
            return Err(StepError::Check(problem));
        }
        if conf.default_selection.as_deref() != Some(PREVIOUS_SYSTEM) {
            return Err(StepError::Check(format!(
                "the guard failed, but rEFInd's default is {:?}, not {PREVIOUS_SYSTEM:?}",
                conf.default_selection
            )));
        }
        println!(
            "vm: the guard reported {}, and {PREVIOUS_SYSTEM:?} is the default",
            status.failures.join("; ")
        );
        Ok(())
    }

    /// Boots the installed disk, saves screenshots while rEFInd shows its menu, and waits until
    /// the new user accepts SSH.
    pub(super) fn boot_installed_showing_refind(&mut self) -> Result<Guest, StepError> {
        println!("vm: booting the installed system and saving rEFInd's menu");
        self.start(crate::machine::Media::default())?;
        for shot in 1..=REFIND_SHOTS {
            thread::sleep(REFIND_SHOT_INTERVAL);
            self.screenshot(&format!("refind-{shot}.png"))?;
        }
        let guest = Guest::new(
            self.key.clone(),
            crate::os::GUEST_USER,
            crate::machine::SSH_PORT,
        );
        self.wait_for_ssh(&guest)?;
        Ok(guest)
    }

    /// Checks that the machine runs `snapshot` on systemd's overlay, with `/home` and
    /// `/var/log` mounted and the guard's boot check run and passed.
    pub(super) fn check_booted_snapshot(
        &mut self,
        user: &Guest,
        snapshot: u32,
    ) -> Result<(), StepError> {
        println!("vm: checking that the machine booted snapshot {snapshot}");
        let cmdline = user.run("cat /proc/cmdline")?;
        if boot_checks::booted_snapshot(&cmdline) != Some(snapshot)
            || !boot_checks::volatile_overlay(&cmdline)
        {
            return Err(StepError::Check(format!(
                "the kernel command line does not boot snapshot {snapshot} on the overlay: {}",
                cmdline.trim()
            )));
        }
        let mounts = user.run(
            "findmnt -no FSTYPE / && findmnt -no FSTYPE /home && findmnt -no FSTYPE /var/log",
        )?;
        if mounts != "overlay\next4\nbtrfs\n" {
            return Err(StepError::Check(format!(
                "/, /home and /var/log are {mounts:?}, not overlay, ext4 and Btrfs"
            )));
        }
        self.check_guard_boot_ran(user)?;
        println!(
            "vm: the machine runs snapshot {snapshot} on the overlay, with /home and /var/log mounted"
        );
        Ok(())
    }

    /// Waits until inayos-guard-boot.service has run in this boot, and checks that it exited
    /// with status 0.
    fn check_guard_boot_ran(&self, user: &Guest) -> Result<(), StepError> {
        let command = format!(
            "systemctl show -p LoadState,ActiveState,Result,ExecMainStartTimestampMonotonic,ExecMainStatus {GUARD_BOOT_UNIT}"
        );
        for _ in 0..GUARD_BOOT_WAITS {
            match boot_checks::unit_run(&user.run(&command)?) {
                boot_checks::UnitRun::Succeeded => return Ok(()),
                boot_checks::UnitRun::Running => thread::sleep(GUARD_BOOT_WAIT),
                boot_checks::UnitRun::Problem(problem) => {
                    return Err(StepError::Check(format!(
                        "{GUARD_BOOT_UNIT} on the snapshot: {problem}"
                    )));
                }
            }
        }
        Err(StepError::Check(format!(
            "{GUARD_BOOT_UNIT} still runs after {} seconds",
            GUARD_BOOT_WAITS as u64 * GUARD_BOOT_WAIT.as_secs()
        )))
    }

    /// Reads the guard's status file and keeps a copy named after `label` in the layer.
    fn guard_status(&self, user: &Guest, label: &str) -> Result<GuardStatus, StepError> {
        let json = user.run(&sudo(&format!("cat {GUARD_STATUS}")))?;
        self.write(&format!("guard-status-{label}.json"), &json)?;
        boot_checks::guard_status(&json).map_err(StepError::Check)
    }

    /// Checks that rEFInd's configuration includes the guard's file, reads that file, and keeps
    /// copies of both named after `label` in the layer.
    fn guard_conf(&self, user: &Guest, label: &str) -> Result<GuardConf, StepError> {
        let folders = REFIND_FOLDERS.join(" ");
        let folder = user.run(&sudo(&format!(
            "sh -c 'for folder in {folders}; do if [ -f \"$folder/refind.conf\" ]; then echo \"$folder\"; break; fi; done'"
        )))?;
        let folder = folder.trim();
        if folder.is_empty() {
            return Err(StepError::Check(format!(
                "no refind.conf in {}",
                REFIND_FOLDERS.join(", ")
            )));
        }
        let refind = user.run(&sudo(&format!("cat {folder}/refind.conf")))?;
        self.write(&format!("refind-{label}.conf"), &refind)?;
        if !refind.lines().any(|line| line.trim() == GUARD_INCLUDE) {
            return Err(StepError::Check(format!(
                "{folder}/refind.conf has no line {GUARD_INCLUDE:?}"
            )));
        }
        let text = match user.run(&sudo(&format!("cat {folder}/{GUARD_CONF}"))) {
            Ok(text) => text,
            Err(SshError::Failed { stderr, .. }) => {
                return Err(StepError::Check(format!(
                    "the guard wrote no {folder}/{GUARD_CONF}: {stderr}"
                )));
            }
            Err(error) => return Err(error.into()),
        };
        self.write(&format!("inayos-guard-{label}.conf"), &text)?;
        Ok(GuardConf::parse(&text))
    }

    fn snapshots(&self, user: &Guest) -> Result<Vec<boot_checks::Snapshot>, StepError> {
        let json = user.run(&sudo("snapper --no-dbus --jsonout -c root list"))?;
        boot_checks::snapshots(&json).map_err(StepError::Check)
    }
}
