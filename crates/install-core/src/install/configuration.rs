//! Runs `inayos install` and `inayos verify` as the user inside the new system, in installer
//! mode. With Secure Boot, it enrolls the keys of the new system between the two, once the
//! install has passed.
use std::path::Path;

use super::{Machine, TARGET, user::NewUser};
use crate::{Answers, Error, host::Command, inventory, modules::SECURE_BOOT_MODULE};

/// Where the `inayos` package puts the InayOS binary in the new system.
pub(crate) const SYSTEM_BINARY: &str = "/usr/bin/inayos";
/// Where the `inayos` package puts the boot guard in the new system.
pub(crate) const GUARD_BINARY: &str = "/usr/bin/inayos-guard";
/// Where the questions' answers wait for `inayos install`, inside the new system.
/// `arch-chroot -S` mounts an empty `/tmp`, so the file lives in `/var/tmp`.
pub(crate) const QUESTIONS: &str = "/var/tmp/inayos-answers.json";

pub(crate) fn run(
    m: &Machine,
    answers: &Answers,
    user: &NewUser,
    modules: Option<&[String]>,
) -> Result<(), Error> {
    let questions = in_target(QUESTIONS);
    let json = serde_json::to_string_pretty(&answers.questions)
        .expect("a map of strings to booleans always serializes");
    m.write(&questions, &json, 0o600)?;
    m.chown(&questions, user.uid, user.gid)?;

    m.sink.step("Installing the InayOS modules");
    let mut install =
        as_user(&user.name).args(["install", "--installer-mode", "--answers", QUESTIONS]);
    if let Some(modules) = modules {
        install = install.args(modules.iter().cloned());
    }
    m.run(install)?;

    let secure_boot = answers.security.secure_boot
        && modules.is_none_or(|kept| kept.iter().any(|name| name == SECURE_BOOT_MODULE));
    if secure_boot {
        enroll_keys(m)?;
        // The guard built its images while Secure Boot was still off, so they are unsigned
        // until it checks again with the keys enrolled.
        m.sink.step("Checking the boot files with the guard");
        m.run(Command::action("arch-chroot").args([TARGET, GUARD_BINARY, "check-update"]))?;
    }

    m.sink.step("Verifying the InayOS modules");
    let mut verify = as_user(&user.name).args(["verify", "--installer-mode"]);
    if let Some(modules) = modules {
        verify = verify.args(modules.iter().cloned());
    }
    m.run(verify)?;
    Ok(())
}

/// The firmware's built-in signature database.
const FIRMWARE_DEFAULT_DB: &str =
    "/sys/firmware/efi/efivars/dbDefault-8be4df61-93ca-11d2-aa0d-00e098032b8c";

/// Enrolls the keys that the secure-boot module created and signed every boot file with.
/// It comes after the whole install passed, because from then on the firmware refuses the
/// unsigned installer stick, so a failed install could not be retried from it.
fn enroll_keys(m: &Machine) -> Result<(), Error> {
    m.sink
        .step("Enrolling the Secure Boot keys into the firmware");
    let mut enroll =
        Command::action("arch-chroot").args([TARGET, "sbctl", "enroll-keys", "--microsoft"]);
    // Some firmware, OVMF among them, has no built-in signature database, and the flag then
    // fails.
    if m.host.exists(Path::new(FIRMWARE_DEFAULT_DB)) {
        enroll = enroll.args(["--firmware-builtin"]);
    }
    m.run(enroll)?;
    if inventory::secure_boot_setup_mode(m.host)? {
        return Err(Error::KeysNotEnrolled);
    }
    Ok(())
}

fn in_target(path: &str) -> std::path::PathBuf {
    Path::new(TARGET).join(path.trim_start_matches('/'))
}

/// `inayos` run as `user` in the new system: no login session, a cleared environment, and
/// systemd told it runs offline. `arch-chroot -S` mounts an empty `/tmp` that only root can
/// write, so temporary files go to `/var/tmp`.
fn as_user(user: &str) -> Command {
    Command::action("arch-chroot").args([
        "-S",
        "-u",
        user,
        TARGET,
        "env",
        "SYSTEMD_OFFLINE=1",
        "TMPDIR=/var/tmp",
        SYSTEM_BINARY,
    ])
}
