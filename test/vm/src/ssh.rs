//! Runs commands in the guest and copies files to and from it over SSH, with a key that
//! the xtask generates and every image accepts.

use std::ffi::{OsStr, OsString};
use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::vm::{Vm, VmError};

const RETRY: Duration = Duration::from_secs(3);

#[derive(Debug, thiserror::Error)]
pub enum SshError {
    #[error("could not prepare {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not start {0}: {1}")]
    Spawn(&'static str, io::Error),
    #[error("ssh-keygen could not create {0}: {1}")]
    KeyGen(PathBuf, String),
    #[error("`{command}` failed in the guest with {status}: {stderr}")]
    Failed {
        command: String,
        status: ExitStatus,
        stderr: String,
    },
    #[error("the guest did not accept SSH within {0:?}")]
    Timeout(Duration),
    #[error("the VM stopped before SSH was ready: {0}")]
    VmStopped(#[source] VmError),
}

/// The private key file. The public key is the same path with `.pub` added.
#[derive(Clone, Debug)]
pub struct SshKey {
    private: PathBuf,
}

impl SshKey {
    /// Returns the key at `private`, creating a new Ed25519 key without a passphrase when
    /// none exists yet. An existing key is kept, because the images accept only that key.
    pub fn ensure(private: &Path) -> Result<SshKey, SshError> {
        let key = SshKey {
            private: private.to_owned(),
        };
        if private.is_file() && key.public_path().is_file() {
            return Ok(key);
        }
        if let Some(parent) = private.parent() {
            DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(|error| SshError::Io(parent.to_owned(), error))?;
        }
        for path in [private.to_owned(), key.public_path()] {
            match fs::remove_file(&path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => {
                    return Err(SshError::Io(path, error));
                }
                _ => {}
            }
        }
        let output = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", "inayos-vm", "-f"])
            .arg(private)
            .output()
            .map_err(|error| SshError::Spawn("ssh-keygen", error))?;
        if !output.status.success() {
            return Err(SshError::KeyGen(private.to_owned(), stderr_text(&output)));
        }
        Ok(key)
    }

    /// The public key line, as an `authorized_keys` file expects it.
    pub fn public(&self) -> Result<String, SshError> {
        let path = self.public_path();
        fs::read_to_string(&path)
            .map(|line| line.trim().to_owned())
            .map_err(|error| SshError::Io(path, error))
    }

    fn public_path(&self) -> PathBuf {
        let mut path = OsString::from(&self.private);
        path.push(".pub");
        PathBuf::from(path)
    }
}

/// An SSH account in the guest, reached through the port that QEMU forwards on the host.
#[derive(Clone, Debug)]
pub struct Guest {
    key: SshKey,
    user: String,
    port: u16,
}

impl Guest {
    pub fn new(key: SshKey, user: &str, port: u16) -> Guest {
        Guest {
            key,
            user: user.to_owned(),
            port,
        }
    }

    /// The `ssh` command line a person can paste to log in to the guest.
    pub fn login_hint(&self) -> String {
        format!(
            "ssh -i {} -p {} -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null {}@127.0.0.1",
            self.key.private.display(),
            self.port,
            self.user
        )
    }

    /// An `ssh` command that runs `remote` in the guest. The caller chooses its input and
    /// output, for example a pseudo-terminal to answer `sudo`. With `terminal`, the guest
    /// gives the command a terminal too.
    pub fn command(&self, remote: &str, terminal: bool) -> Command {
        let mut command = Command::new("ssh");
        self.add_options(&mut command);
        command.arg("-p").arg(self.port.to_string());
        if terminal {
            command.arg("-tt");
        }
        command.arg(format!("{}@127.0.0.1", self.user)).arg(remote);
        command
    }

    /// Runs `remote` in the guest and returns its output. A non-zero exit status is an error
    /// that carries the command's error output.
    pub fn run(&self, remote: &str) -> Result<String, SshError> {
        let output = self
            .command(remote, false)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| SshError::Spawn("ssh", error))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            Err(SshError::Failed {
                command: remote.to_owned(),
                status: output.status,
                stderr: stderr_text(&output),
            })
        }
    }

    /// Copies a host file to `remote`. A relative `remote` path starts in the guest user's
    /// home folder, on Linux and on Windows alike.
    pub fn copy_to(&self, local: &Path, remote: &str) -> Result<(), SshError> {
        let mut destination = OsString::from(format!("{}@127.0.0.1:", self.user));
        destination.push(remote);
        self.scp(local.as_os_str(), &destination, remote)
    }

    /// Waits until the guest accepts an SSH login, checking that the VM still runs between
    /// attempts.
    pub fn wait_until_ready(&self, vm: &mut Vm, timeout: Duration) -> Result<(), SshError> {
        let deadline = Instant::now() + timeout;
        loop {
            vm.check_running().map_err(SshError::VmStopped)?;
            // `exit 0` works in the Linux shell and in the Windows command prompt.
            let mut attempt = self.command("exit 0", false);
            attempt
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            if attempt.status().is_ok_and(|status| status.success()) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(SshError::Timeout(timeout));
            }
            thread::sleep(RETRY);
        }
    }

    fn scp(&self, source: &OsStr, destination: &OsStr, remote: &str) -> Result<(), SshError> {
        let mut command = Command::new("scp");
        self.add_options(&mut command);
        let output = command
            .arg("-P")
            .arg(self.port.to_string())
            .arg(source)
            .arg(destination)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| SshError::Spawn("scp", error))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(SshError::Failed {
                command: format!("scp {remote}"),
                status: output.status,
                stderr: stderr_text(&output),
            })
        }
    }

    fn add_options(&self, command: &mut Command) {
        // Every image has its own host key on the same local port, and the guest is reached
        // only through the loopback forward of a throwaway VM, so host keys are not recorded.
        command
            .arg("-i")
            .arg(&self.key.private)
            .args(["-o", "IdentitiesOnly=yes"])
            .args(["-o", "BatchMode=yes"])
            .args(["-o", "StrictHostKeyChecking=no"])
            .args(["-o", "UserKnownHostsFile=/dev/null"])
            .args(["-o", "LogLevel=ERROR"])
            .args(["-o", "ConnectTimeout=10"]);
        // A guest that stops answering on an open connection ends the call with an error
        // after about a minute, instead of blocking the cycle past its own time limits.
        command
            .args(["-o", "ServerAliveInterval=15"])
            .args(["-o", "ServerAliveCountMax=4"]);
    }
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}
