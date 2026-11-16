//! The hands-on mode: boots a guest with the fresh installer in it and keeps it running
//! until the guest shuts down.

use std::path::Path;

use crate::os::Os;
use crate::start::{self, Disk, Options, ReadyError, StartError};
use crate::vm::VmError;

#[derive(Clone, Copy, Debug)]
pub struct RunOptions {
    pub os: Os,
    /// The layer the guest boots from.
    pub disk: Disk,
    pub displays: u8,
    /// Opens a SPICE viewer window on the guest display.
    pub watch: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Start(#[from] StartError),
    #[error(transparent)]
    Ready(#[from] ReadyError),
    #[error(transparent)]
    Vm(#[from] VmError),
}

/// Boots the guest, copies the fresh installer into it, prints how to reach it, and waits
/// until the guest shuts down. The throwaway disk stays until a cycle or a run on a new
/// throwaway disk replaces it.
pub fn run(options: RunOptions, repository: &Path) -> Result<(), RunError> {
    let RunOptions {
        os,
        disk,
        displays,
        watch,
    } = options;
    let mut started = start::start(
        Options {
            os,
            disk,
            displays,
            watch,
        },
        repository,
    )?;
    start::install_binaries(os, &mut started.vm, &started.guest, &started.binaries)?;

    for binary in &started.binaries {
        println!(
            "vm: {} is at ~/{} in the guest",
            binary.package, binary.guest_path
        );
    }
    println!("vm: log in with: {}", started.guest.login_hint());
    if !watch {
        println!(
            "vm: watch the display with: remote-viewer spice+unix://{}",
            started.spice.display()
        );
    }
    if displays > 1 {
        println!("vm: turn on the second display in the viewer under View > Displays");
    }
    println!("vm: the VM runs until the guest shuts down; Ctrl+C stops it at once");
    started.vm.wait()?;
    Ok(())
}
