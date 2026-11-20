//! The steps of the rollback scenarios: a snapshot, changes after it, the rollback to it from
//! the USB stick's repair workspace with `inayos-installer rollback`, and the checks of the
//! system that boots afterwards.

use std::fs;

use super::maintenance_checks as checks;
use super::test_repository::MARKER_PACKAGE;
use super::{Scenario, StepError, sudo, user_power_off};
use crate::firmware::Firmware;
use crate::os::Boot;
use crate::ssh::Guest;

/// A file in the system that the rollback must take away.
const SYSTEM_MARKER: &str = "/inayos-rollback-marker";
/// A file in the home folder that the rollback must keep, since home is not in a snapshot.
const HOME_MARKER: &str = "inayos-rollback-home";
const HOME_MARKER_TEXT: &str = "kept across the rollback";
/// The log that `inayos-installer rollback` writes by default.
const ROLLBACK_LOG: &str = "/var/log/inayos-rollback.log";
/// Where the live system keeps the recovery key of the encrypted disk for the rollback.
const LIVE_RECOVERY_KEY: &str = "/root/inayos-recovery-key.txt";
/// Where the layer keeps the UEFI variables of the installed system while the stick boots
/// with an empty variable store.
const INSTALLED_VARS: &str = "installed-vars.fd";

/// The rollback command of the repair workspace for `snapshot`, which asks nothing. An
/// encrypted disk opens with the recovery key in `recovery_key`, because the stick boots with
/// other Secure Boot measurements than the ones the TPM sealed the keys to.
fn rollback_command(snapshot: u32, recovery_key: Option<&str>) -> String {
    let mut command = format!("inayos-installer rollback --snapshot {snapshot} --yes");
    if let Some(file) = recovery_key {
        command.push_str(&format!(" --recovery-key-file {file}"));
    }
    command
}

impl Scenario {
    /// Takes a snapshot, changes the system after it, rolls back to it from the stick, and
    /// checks that the system boots as it was while home keeps its files.
    pub(super) fn check_rollback(&mut self) -> Result<(), StepError> {
        let encrypted = self.boot == Boot::SecureUefiWithTpm;
        let user = self.boot_installed()?;
        if encrypted {
            self.check_unlocked_without_prompt("first-boot")?;
            self.check_guard_boot_ran(&user)?;
        }
        let marker = self.marker_in_guest(&user)?;
        let created = user.run(&sudo(
            "snapper --no-dbus -c root create --print-number --cleanup-algorithm number --description 'before the rollback test'",
        ))?;
        let snapshot = checks::created_number(&created).ok_or_else(|| {
            StepError::Check(format!(
                "snapper create printed no snapshot number: {created:?}"
            ))
        })?;
        println!("vm: took snapshot {snapshot}, then changing the system after it");
        user.run(&sudo(&format!(
            "sh -c 'echo changed after the snapshot > {SYSTEM_MARKER}'"
        )))?;
        user.run(&format!(
            "printf '%s\\n' '{HOME_MARKER_TEXT}' > ~/{HOME_MARKER}"
        ))?;
        user.run_logged(
            &sudo(&format!("pacman -U --noconfirm {marker}")),
            &self.log("pacman-marker"),
        )?;
        user.run(&format!("pacman -Q {MARKER_PACKAGE}"))?;
        self.power_off(&user, &user_power_off())?;

        if encrypted {
            self.empty_secure_boot_vars()?;
        }
        let live = self.boot_live()?;
        self.copy_binaries(&live)?;
        let recovery_key = if encrypted {
            let key = self.kept_recovery_key()?;
            let file = self.write("recovery-key-for-rollback.txt", &format!("{key}\n"))?;
            live.copy_to(&file, LIVE_RECOVERY_KEY)?;
            live.run(&format!("chmod 600 {LIVE_RECOVERY_KEY}"))?;
            Some(LIVE_RECOVERY_KEY)
        } else {
            None
        };
        println!("vm: rolling back to snapshot {snapshot} from the repair workspace");
        let result = live.run_logged(
            &rollback_command(snapshot, recovery_key),
            &self.log("rollback"),
        );
        if let Err(error) = live.copy_from(ROLLBACK_LOG, &self.log("rollback-file")) {
            eprintln!("vm: could not copy {ROLLBACK_LOG}: {error}");
        }
        result?;
        self.power_off(&live, "systemctl poweroff")?;
        if encrypted {
            self.restore_installed_vars()?;
        }

        let user = self.boot_installed()?;
        if encrypted {
            self.check_unlocked_without_prompt("after-rollback")?;
        }
        self.check_rolled_back(&user)
    }

    /// Checks the system after the rollback: the system marker and the package are gone, the
    /// home marker stays, `/` is the `@` subvolume, and the replaced system is kept beside it.
    fn check_rolled_back(&mut self, user: &Guest) -> Result<(), StepError> {
        println!("vm: checking the rolled back system");
        if user.run(&format!("test ! -e {SYSTEM_MARKER}")).is_err() {
            return Err(StepError::Check(format!(
                "{SYSTEM_MARKER}, written after the snapshot, is still there"
            )));
        }
        if user.run(&format!("pacman -Q {MARKER_PACKAGE}")).is_ok() {
            return Err(StepError::Check(format!(
                "{MARKER_PACKAGE}, installed after the snapshot, is still installed"
            )));
        }
        let home = user.run(&format!("cat ~/{HOME_MARKER}"))?;
        if home.trim() != HOME_MARKER_TEXT {
            return Err(StepError::Check(format!(
                "~/{HOME_MARKER} changed across the rollback: {home:?}"
            )));
        }
        let options = user.run("findmnt -no OPTIONS /")?;
        if !options
            .trim()
            .split(',')
            .any(|option| option == "subvol=/@")
        {
            return Err(StepError::Check(format!(
                "/ is not mounted from the @ subvolume: {}",
                options.trim()
            )));
        }
        let list = user.run(&sudo("btrfs subvolume list /"))?;
        self.write("subvolumes-after-rollback.txt", &list)?;
        let kept = checks::subvolumes_named(&list, "@rollback-");
        if kept.len() != 1 {
            return Err(StepError::Check(format!(
                "the disk should keep the replaced system as one @rollback-<date> subvolume, but has {kept:?}"
            )));
        }
        println!(
            "vm: the system is back at the snapshot, home kept its file, and {} holds the replaced system",
            kept[0]
        );
        Ok(())
    }

    /// Puts an empty variable store in place of the installed system's, which it keeps in the
    /// layer. The Secure Boot keys that the install enrolled refuse the unsigned stick, and an
    /// empty store puts the firmware in Setup Mode, where the stick boots.
    fn empty_secure_boot_vars(&mut self) -> Result<(), StepError> {
        let vars = self.layer.vars();
        let kept = self.layer.dir().join(INSTALLED_VARS);
        println!("vm: booting the stick with an empty UEFI variable store");
        fs::rename(&vars, &kept).map_err(|error| StepError::Io(kept.clone(), error))?;
        fs::copy(Firmware::for_boot(self.boot).vars_template, &vars)
            .map_err(|error| StepError::Io(vars.clone(), error))?;
        Ok(())
    }

    /// Puts the installed system's UEFI variables back after the stick.
    fn restore_installed_vars(&mut self) -> Result<(), StepError> {
        let vars = self.layer.vars();
        let kept = self.layer.dir().join(INSTALLED_VARS);
        fs::rename(&kept, &vars).map_err(|error| StepError::Io(vars, error))
    }
}
