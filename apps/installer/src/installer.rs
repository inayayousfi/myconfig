//! The `install` command, which the installer core carries out.

use std::{path::PathBuf, process::ExitCode};

use crate::console;
use clap::{Parser, Subcommand};
use inayos_install_core::{Answers, RealHost, Settings, install};

#[derive(Parser)]
#[command(about = "Installs InayOS from the live USB system")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Installs InayOS as the answers file describes. Erases what the disk choice erases.
    /// Does not reboot.
    Install {
        /// The TOML answers file.
        #[arg(long)]
        answers: PathBuf,
        /// The install log, also copied into the new system.
        #[arg(long, default_value = "/var/log/inayos-installer.log")]
        log: PathBuf,
        /// The InayOS binary that the new system runs.
        #[arg(long, default_value = Settings::SYSTEM_BINARY)]
        system_binary: PathBuf,
    },
}

pub fn main() -> ExitCode {
    let Command::Install {
        answers,
        log,
        system_binary,
    } = Arguments::parse().command;
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
