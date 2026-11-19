//! Starts and stops the processes of one running VM: QEMU, swtpm for a guest with a TPM,
//! and the SPICE viewer when the user watches.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::machine::{Machine, Sockets};
use crate::os::Boot;
use crate::qmp::{Qmp, QmpError};

const SOCKET_TIMEOUT: Duration = Duration::from_secs(15);
const POLL: Duration = Duration::from_millis(100);

#[derive(Debug, thiserror::Error)]
pub enum VmError {
    #[error("could not prepare {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not start {0}: {1}")]
    Spawn(&'static str, io::Error),
    #[error("{program} stopped with {status}; its output is in {log}")]
    Exited {
        program: &'static str,
        status: ExitStatus,
        log: PathBuf,
    },
    #[error("{program} did not open {socket} within {SOCKET_TIMEOUT:?}; its output is in {log}")]
    NoSocket {
        program: &'static str,
        socket: PathBuf,
        log: PathBuf,
    },
    #[error("could not wait for {0}: {1}")]
    Wait(&'static str, io::Error),
}

/// A running VM. Dropping it stops every process it started, so an error never leaves
/// QEMU, swtpm or the viewer running.
pub struct Vm {
    qemu: Child,
    swtpm: Option<Child>,
    viewer: Option<Child>,
    sockets: Sockets,
    log: PathBuf,
}

impl Vm {
    /// Starts the machine. `runtime` receives the sockets named in `machine.sockets`
    /// and the process logs. With `watch`, a SPICE viewer window opens on the display.
    pub fn start(machine: &Machine, runtime: &Path, watch: bool) -> Result<Vm, VmError> {
        fs::create_dir_all(runtime).map_err(|error| VmError::Io(runtime.to_owned(), error))?;
        let sockets = &machine.sockets;
        for socket in [&sockets.qmp, &sockets.spice, &sockets.tpm] {
            // A socket left by an earlier VM would make the waits below succeed too early.
            // The caller holds the VM lock, so no running VM owns these sockets.
            match fs::remove_file(socket) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => {
                    return Err(VmError::Io(socket.clone(), error));
                }
                _ => {}
            }
        }

        let swtpm = if machine.boot == Boot::SecureUefiWithTpm {
            Some(start_swtpm(machine, runtime)?)
        } else {
            None
        };

        let log = runtime.join("qemu.log");
        let qemu = log_file(&log).and_then(|(stdout, stderr)| {
            Command::new("qemu-system-x86_64")
                .args(machine.qemu_args())
                .stdin(Stdio::null())
                .stdout(stdout)
                .stderr(stderr)
                .spawn()
                .map_err(|error| VmError::Spawn("qemu-system-x86_64", error))
        });
        let qemu = match qemu {
            Ok(qemu) => qemu,
            Err(error) => {
                if let Some(mut swtpm) = swtpm {
                    stop(&mut swtpm);
                }
                return Err(error);
            }
        };
        let mut vm = Vm {
            qemu,
            swtpm,
            viewer: None,
            sockets: sockets.clone(),
            log,
        };
        wait_for_socket(&mut vm.qemu, "qemu-system-x86_64", &vm.sockets.qmp, &vm.log)?;

        if watch {
            let mut uri = std::ffi::OsString::from("spice+unix://");
            uri.push(&vm.sockets.spice);
            let viewer = Command::new("remote-viewer")
                .arg(uri)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| VmError::Spawn("remote-viewer", error))?;
            vm.viewer = Some(viewer);
        }
        Ok(vm)
    }

    /// Opens a new QMP session with the running QEMU.
    pub fn qmp(&self) -> Result<Qmp, QmpError> {
        Qmp::connect(&self.sockets.qmp)
    }

    /// Whether QEMU still runs. When it has stopped, the error names its exit status and log.
    pub fn check_running(&mut self) -> Result<(), VmError> {
        match self.qemu.try_wait() {
            Ok(None) => Ok(()),
            Ok(Some(status)) => Err(VmError::Exited {
                program: "qemu-system-x86_64",
                status,
                log: self.log.clone(),
            }),
            Err(error) => Err(VmError::Wait("qemu-system-x86_64", error)),
        }
    }

    /// Waits until QEMU stops, which happens when the guest shuts down.
    pub fn wait(&mut self) -> Result<ExitStatus, VmError> {
        self.qemu
            .wait()
            .map_err(|error| VmError::Wait("qemu-system-x86_64", error))
    }

    /// Asks the guest to shut down and waits up to `timeout`. A guest that is still running
    /// after that is stopped at once, like a power cut, when this value is dropped.
    pub fn shut_down(mut self, timeout: Duration) {
        // A failed request leaves the guest running, and the power cut still stops it.
        let requested = self.qmp().and_then(|mut qmp| qmp.powerdown()).is_ok();
        let deadline = Instant::now() + timeout;
        while requested && Instant::now() < deadline {
            if !matches!(self.qemu.try_wait(), Ok(None)) {
                return;
            }
            thread::sleep(POLL * 10);
        }
    }
}

impl Drop for Vm {
    fn drop(&mut self) {
        let children = [
            Some(&mut self.qemu),
            self.swtpm.as_mut(),
            self.viewer.as_mut(),
        ];
        for child in children.into_iter().flatten() {
            stop(child);
        }
    }
}

fn start_swtpm(machine: &Machine, runtime: &Path) -> Result<Child, VmError> {
    let state = machine.layer.tpm();
    fs::create_dir_all(&state).map_err(|error| VmError::Io(state.clone(), error))?;
    let log = runtime.join("swtpm.log");
    let mut state_arg = std::ffi::OsString::from("dir=");
    state_arg.push(&state);
    let mut control = std::ffi::OsString::from("type=unixio,path=");
    control.push(&machine.sockets.tpm);
    // --terminate ends swtpm when QEMU closes the connection.
    let (stdout, stderr) = log_file(&log)?;
    let mut swtpm = Command::new("swtpm")
        .args(["socket", "--tpm2", "--terminate", "--tpmstate"])
        .arg(state_arg)
        .arg("--ctrl")
        .arg(control)
        .stdin(Stdio::null())
        .stdout(stdout)
        .stderr(stderr)
        .spawn()
        .map_err(|error| VmError::Spawn("swtpm", error))?;
    if let Err(error) = wait_for_socket(&mut swtpm, "swtpm", &machine.sockets.tpm, &log) {
        stop(&mut swtpm);
        return Err(error);
    }
    Ok(swtpm)
}

/// Stops a process that may already have stopped by itself.
fn stop(child: &mut Child) {
    if let Ok(None) = child.try_wait() {
        // Kill and wait can only fail when the process already stopped.
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Empties the log file of one process and returns it twice, for its output and its errors.
fn log_file(path: &Path) -> Result<(File, File), VmError> {
    let io_error = |error| VmError::Io(path.to_owned(), error);
    let file = File::create(path).map_err(io_error)?;
    let copy = file.try_clone().map_err(io_error)?;
    Ok((file, copy))
}

fn wait_for_socket(
    child: &mut Child,
    program: &'static str,
    socket: &Path,
    log: &Path,
) -> Result<(), VmError> {
    let deadline = Instant::now() + SOCKET_TIMEOUT;
    loop {
        if socket.exists() {
            return Ok(());
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|error| VmError::Wait(program, error))?
        {
            return Err(VmError::Exited {
                program,
                status,
                log: log.to_owned(),
            });
        }
        if Instant::now() >= deadline {
            return Err(VmError::NoSocket {
                program,
                socket: socket.to_owned(),
                log: log.to_owned(),
            });
        }
        thread::sleep(POLL);
    }
}
