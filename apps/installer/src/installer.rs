//! The command line: no subcommand opens the screens, `install` installs from an answers
//! file without them, and `gui --save-answers` runs the screens and writes the answers file.
use std::{path::PathBuf, process::ExitCode, sync::Arc};

use crate::{app, console};
use clap::{Args, Parser, Subcommand};
use inayos_install_core::{Answers, RealHost, Settings, install};

#[derive(Parser)]
#[command(about = "Installs InayOS from the live USB system")]
struct Arguments {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Installs InayOS as the answers file describes, without the screens. Erases what the
    /// disk choice erases. Does not reboot.
    Install {
        /// The TOML answers file.
        #[arg(long)]
        answers: PathBuf,
        #[command(flatten)]
        paths: Paths,
    },
    /// Opens the screens. With --save-answers, the summary writes the answers file instead
    /// of installing.
    Gui {
        /// Where to write the answers file. It holds the passwords and gets mode 600.
        #[arg(long)]
        save_answers: Option<PathBuf>,
        #[command(flatten)]
        paths: Paths,
    },
}

#[derive(Args)]
struct Paths {
    /// The install log, also copied into the new system.
    #[arg(long, default_value = "/var/log/inayos-installer.log")]
    log: PathBuf,
    /// The InayOS binary that the new system runs.
    #[arg(long, default_value = Settings::SYSTEM_BINARY)]
    system_binary: PathBuf,
}

impl Default for Paths {
    fn default() -> Self {
        Paths {
            log: PathBuf::from("/var/log/inayos-installer.log"),
            system_binary: PathBuf::from(Settings::SYSTEM_BINARY),
        }
    }
}

pub fn main() -> ExitCode {
    match Arguments::parse().command {
        Some(Command::Install { answers, paths }) => install_from_file(answers, paths),
        Some(Command::Gui {
            save_answers,
            paths,
        }) => screens(save_answers, paths),
        None => screens(None, Paths::default()),
    }
}

fn screens(save_answers: Option<PathBuf>, paths: Paths) -> ExitCode {
    let options = app::Options {
        save_answers,
        system_binary: paths.system_binary,
        log: paths.log,
        root: PathBuf::from("/"),
        in_sway: std::env::var_os("SWAYSOCK").is_some(),
    };
    match app::run(options, Arc::new(RealHost)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: the screens could not open: {error}");
            ExitCode::FAILURE
        }
    }
}

fn install_from_file(answers: PathBuf, paths: Paths) -> ExitCode {
    let Paths { log, system_binary } = paths;
    let console = match console::Console::open(&log) {
        Ok(console) => console,
        Err(error) => {
            eprintln!("error: could not open the log {}: {error}", log.display());
            return ExitCode::FAILURE;
        }
    };
    let answers = match Answers::read(&answers) {
        Ok(answers) => answers,
        Err(error) => {
            console.error(&error.to_string());
            return ExitCode::FAILURE;
        }
    };
    let settings = Settings {
        system_binary,
        log_file: Some(log),
    };
    match install(&answers, &settings, &RealHost, &console) {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            console.error(&failure.to_string());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_consistent() {
        super::Arguments::command().debug_assert();
    }
}
