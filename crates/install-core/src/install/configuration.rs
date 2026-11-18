//! Runs `inayos install` and `inayos verify` as the user inside the new system, in installer
//! mode.
use std::path::Path;

use super::{Machine, Settings, TARGET, user::NewUser};
use crate::{Answers, Error, host::Command};

/// Where the new system keeps the InayOS binary.
pub(crate) const SYSTEM_BINARY: &str = "/usr/local/bin/inayos";
/// Where the questions' answers wait for `inayos install`, inside the new system.
/// `arch-chroot -S` mounts an empty `/tmp`, so the file lives in `/var/tmp`.
pub(crate) const QUESTIONS: &str = "/var/tmp/inayos-answers.json";

pub(crate) fn run(
    m: &Machine,
    answers: &Answers,
    settings: &Settings,
    user: &NewUser,
    modules: Option<&[String]>,
) -> Result<(), Error> {
    m.sink.step("Copying InayOS into the new system");
    let binary = in_target(SYSTEM_BINARY);
    m.create_dir(
        binary.parent().expect("the binary path has a folder"),
        0o755,
    )?;
    m.copy(&settings.system_binary, &binary, 0o755)?;

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

    m.sink.step("Verifying the InayOS modules");
    let mut verify = as_user(&user.name).args(["verify", "--installer-mode"]);
    if let Some(modules) = modules {
        verify = verify.args(modules.iter().cloned());
    }
    m.run(verify)?;
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
