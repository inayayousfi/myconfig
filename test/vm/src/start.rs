//! Starts a guest with the fresh installers: builds them from the working tree, boots the
//! guest on a new throwaway layer, on the existing throwaway layer or on the kept failed
//! layer, and copies them in once the guest accepts SSH.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::binary::{self, Binary, BinaryError};
use crate::disk::{self, DiskError};
use crate::host::{self, HostError};
use crate::lock::{LockError, VmLock};
use crate::machine::{Machine, Media, SSH_PORT, Sockets};
use crate::os::{GUEST_USER, Os};
use crate::paths::{Layer, Paths, PathsError};
use crate::ssh::{Guest, SshError, SshKey};
use crate::vm::{Vm, VmError};

/// The layer a guest starts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Disk {
    /// A new throwaway layer over the clean image, which replaces the previous one.
    Throwaway,
    /// The existing throwaway layer, with what the previous run left on it.
    Work,
    /// The kept layer of the last failed cycle.
    Failed,
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub os: Os,
    pub disk: Disk,
    /// 1, or 2 for a guest that supports a second display.
    pub displays: u8,
    /// Opens a SPICE viewer window on the guest display.
    pub watch: bool,
}

/// A running guest that may not accept SSH yet. It holds the VM lock until it is dropped.
pub struct Started {
    pub paths: Paths,
    pub vm: Vm,
    pub guest: Guest,
    pub binaries: Vec<Binary>,
    pub layer: Layer,
    pub spice: PathBuf,
    _lock: VmLock,
}

/// A failure before the guest runs. After a `Vm` failure, the new throwaway layer stays until
/// the next run or cycle of that system replaces it.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    Paths(#[from] PathsError),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error("{0} supports only one display")]
    Displays(Os),
    #[error("no kept failed disk in {0}")]
    NoFailedDisk(PathBuf),
    #[error("no throwaway disk in {0}")]
    NoWorkDisk(PathBuf),
    #[error(transparent)]
    Disk(#[from] DiskError),
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Binary(#[from] BinaryError),
    #[error(transparent)]
    Vm(#[from] VmError),
}

/// A failure while the guest runs, before the installers are in it.
#[derive(Debug, thiserror::Error)]
pub enum ReadyError {
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Binary(#[from] BinaryError),
}

/// Builds the installers of `options.os` from the working tree at `repository` and boots the
/// guest. The layer is checked before the build, so a missing image or kept disk does not
/// cost a build.
pub fn start(options: Options, repository: &Path) -> Result<Started, StartError> {
    let Options {
        os,
        disk,
        displays,
        watch,
    } = options;
    if displays > 1 && !os.supports_second_display() {
        return Err(StartError::Displays(os));
    }
    let paths = Paths::from_env()?;
    host::check(os)?;
    let lock = VmLock::acquire(&paths.lock())?;

    let image = paths.image(os);
    let layer = match disk {
        Disk::Throwaway if !disk::is_finished(&image, os.boot()) => {
            return Err(DiskError::NoImage(image.dir().to_owned()).into());
        }
        Disk::Throwaway => paths.work(os),
        Disk::Work => {
            let layer = paths.work(os);
            if !disk::is_bootable(&layer, os.boot()) {
                return Err(StartError::NoWorkDisk(layer.dir().to_owned()));
            }
            layer
        }
        Disk::Failed => {
            let layer = paths.failed(os);
            if !disk::is_bootable(&layer, os.boot()) {
                return Err(StartError::NoFailedDisk(layer.dir().to_owned()));
            }
            layer
        }
    };

    let key = SshKey::ensure(&paths.ssh_key())?;
    println!(
        "vm: building the {os} installers from {}",
        repository.display()
    );
    let binaries = binary::build(os, repository)?;
    if disk == Disk::Throwaway {
        println!("vm: creating a throwaway disk over the clean {os} image");
        disk::layer_over(&image, &layer, os.boot())?;
    }

    let runtime = paths.runtime();
    let sockets = Sockets::in_dir(&runtime);
    let spice = sockets.spice.clone();
    let machine = Machine {
        os,
        layer: layer.clone(),
        sockets,
        media: Media::default(),
        displays,
        serial: None,
    };
    println!("vm: starting {os} from {}", layer.dir().display());
    let vm = Vm::start(&machine, &runtime, watch)?;
    Ok(Started {
        paths,
        vm,
        guest: Guest::new(key, GUEST_USER, SSH_PORT),
        binaries,
        layer,
        spice,
        _lock: lock,
    })
}

/// Waits until the guest accepts SSH, then copies the installers into it.
pub fn install_binaries(
    os: Os,
    vm: &mut Vm,
    guest: &Guest,
    binaries: &[Binary],
) -> Result<(), ReadyError> {
    println!("vm: waiting for SSH");
    guest.wait_until_ready(vm, Duration::from_secs(os.ssh_timeout_secs()))?;
    binary::install(os, guest, binaries)?;
    Ok(())
}
