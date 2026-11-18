//! The install from start to finish: checks, disk, base system, user, InayOS modules, cleanup.
pub(crate) mod checks;
mod configuration;
mod disk;
pub(crate) mod machine;
mod system;
mod user;

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use machine::Machine;

use crate::{Answers, Error, Failure, Sink, Stage, host::Command, host::Host};

/// Where the new system is mounted while it is built.
pub(crate) const TARGET: &str = "/mnt";

/// Where the new system keeps the copy of the install log.
pub const INSTALLED_LOG: &str = "/var/log/inayos-installer.log";

/// What the installer brings besides the answers.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The InayOS binary of the profile the new system runs, copied into it.
    pub system_binary: PathBuf,
    /// The install log, copied into the new system at the end.
    pub log_file: Option<PathBuf>,
}

impl Settings {
    /// Where the USB stick keeps the InayOS binary.
    pub const SYSTEM_BINARY: &str = "/usr/local/lib/inayos/inayos";
}

/// Installs InayOS on the disk the answers name. The machine does not reboot: the caller
/// reboots or powers off. A failure says at which stage it happened, which says what it left
/// on the disk.
pub fn install(
    answers: &Answers,
    settings: &Settings,
    host: &dyn Host,
    sink: &dyn Sink,
) -> Result<(), Failure> {
    let m = Machine { host, sink };
    let at = |stage| {
        move |error| Failure {
            stage,
            error,
            cleanup: Vec::new(),
        }
    };
    sink.step("Checking the machine and the answers");
    let checked = checks::check(&m, answers, settings).map_err(at(Stage::Checks))?;

    let result = (|| {
        disk::prepare(&m, &checked.disk, &answers.user.username).map_err(at(Stage::Disk))?;
        system::install(&m, answers).map_err(at(Stage::System))?;
        let user = user::create(&m, answers, checked.kept_owner).map_err(at(Stage::User))?;
        configuration::run(&m, answers, settings, &user, checked.modules.as_deref())
            .map_err(at(Stage::Configuration))
    })();

    let cleanup = finish(&m, settings);
    match result {
        Ok(()) if cleanup.is_empty() => {
            sink.step("InayOS is installed");
            Ok(())
        }
        Ok(()) => {
            let mut cleanup = cleanup.into_iter();
            let error = cleanup.next().expect("the cleanup has an error");
            Err(Failure {
                stage: Stage::Finish,
                error,
                cleanup: cleanup.collect(),
            })
        }
        Err(mut failure) => {
            failure.cleanup = cleanup;
            Err(failure)
        }
    }
}

/// Removes what only the install needed, copies the log into the new system and unmounts it.
/// Runs after every install that got past the checks, and returns every step that failed.
fn finish(m: &Machine, settings: &Settings) -> Vec<Error> {
    m.sink.step("Cleaning up and unmounting the new system");
    let mut errors = Vec::new();
    let target = Path::new(TARGET);
    for leftover in [user::INSTALLER_RULE, configuration::QUESTIONS] {
        let path = target.join(leftover.trim_start_matches('/'));
        if m.exists(&path)
            && let Err(error) = m.remove_file(&path)
        {
            errors.push(error);
        }
    }
    if let Some(log) = &settings.log_file {
        let log_folder = target.join("var/log");
        if m.is_dir(&log_folder)
            && let Err(error) = m.copy(
                log,
                &target.join(INSTALLED_LOG.trim_start_matches('/')),
                0o600,
            )
        {
            errors.push(error);
        }
    }
    match m.run_any(&Command::query("mountpoint").args(["-q", TARGET])) {
        Ok(output) if output.success() => {
            if let Err(error) = m.run(Command::action("umount").args(["-R", TARGET])) {
                errors.push(error);
            }
        }
        Ok(_) => {}
        Err(error) => errors.push(error),
    }
    errors
}
