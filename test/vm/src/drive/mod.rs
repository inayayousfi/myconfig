//! Runs installer commands in a guest the way a person would: in a terminal, typing the sudo
//! password and the answers to its questions. Each guest system has its own way to reach a
//! terminal, and the cycle uses them alike.

mod desktop;
mod terminal;

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use desktop::Desktop;
pub use terminal::Terminal;

use crate::answers::UnknownQuestion;
use crate::qmp::QmpError;
use crate::ssh::SshError;
use crate::vm::{Vm, VmError};

/// How long one installer command may run. A full install downloads and builds packages.
const TIMEOUT: Duration = Duration::from_secs(3 * 60 * 60);
/// The exit code of the installer when a question had no answer and something was kept.
const UNANSWERED: i32 = 3;
/// More sudo prompts than this in one command means the password is refused.
const MAX_PASSWORD_PROMPTS: u32 = 3;

/// A way to run installer commands in a guest terminal.
pub trait Driver {
    /// Runs `command`, a command line for the guest's shell, until it ends. It types the
    /// sudo password and the answers from the answer table, copies the output to `log`, and
    /// returns the output.
    fn run(&mut self, vm: &mut Vm, command: &str, log: &Path) -> Result<String, DriveError>;
}

/// A failure of one installer command. The command may have changed the guest before it
/// failed; its output is in `log` whenever the variant names one.
#[derive(Debug, thiserror::Error)]
pub enum DriveError {
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not start ssh: {0}")]
    Spawn(io::Error),
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Qmp(#[from] QmpError),
    #[error("the VM stopped during the command: {0}")]
    VmStopped(#[source] VmError),
    #[error("{question}; the output is in {log}")]
    UnknownQuestion {
        question: UnknownQuestion,
        log: PathBuf,
    },
    #[error(
        "sudo asked for the password more than {MAX_PASSWORD_PROMPTS} times; the output is in {log}"
    )]
    PasswordRefused { log: PathBuf },
    #[error("a question went unanswered and the installer kept something; the output is in {log}")]
    Unanswered { log: PathBuf },
    #[error("`{command}` failed with {status}; the output is in {log}")]
    Failed {
        command: String,
        status: String,
        log: PathBuf,
    },
    #[error("`{command}` did not end within {TIMEOUT:?}; the output so far is in {log}")]
    Timeout { command: String, log: PathBuf },
}

/// Turns the exit code of an installer command into its result.
fn check_exit(command: &str, code: Option<i32>, log: &Path) -> Result<(), DriveError> {
    match code {
        Some(0) => Ok(()),
        Some(UNANSWERED) => Err(DriveError::Unanswered {
            log: log.to_owned(),
        }),
        Some(code) => Err(DriveError::Failed {
            command: command.to_owned(),
            status: format!("exit code {code}"),
            log: log.to_owned(),
        }),
        None => Err(DriveError::Failed {
            command: command.to_owned(),
            status: "no exit code".to_owned(),
            log: log.to_owned(),
        }),
    }
}

/// The text typed for a yes or no answer.
fn answer_text(yes: bool) -> &'static str {
    if yes { "y\n" } else { "n\n" }
}
