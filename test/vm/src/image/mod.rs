//! Builds the clean image of a guest system: finds the current release at its source,
//! downloads and verifies it, installs it unattended, checks that the installed guest accepts
//! the xtask's SSH key, and records the release it was built from.
//!
//! An image is built in its own folder and replaces the clean image only when it is complete,
//! so a failed build leaves the previous image as it was.

mod ubuntu;
mod windows;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::disk::{self, DiskError};
use crate::download::{self, DownloadError};
use crate::firmware::Firmware;
use crate::host::{self, HostError};
use crate::lock::{LockError, VmLock};
use crate::machine::{Machine, Media, SSH_PORT, Sockets};
use crate::os::{GUEST_PASSWORD, GUEST_USER, Os};
use crate::paths::{Layer, Paths, PathsError};
use crate::qmp::QmpError;
use crate::release::{Release, ReleaseError};
use crate::seed::{self, SeedError, SeedFile};
use crate::ssh::{Guest, SshError, SshKey};
use crate::vm::{Vm, VmError};

/// The size of a new guest disk. The qcow2 file grows only as the guest writes.
const DISK_SIZE: &str = "100G";
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const POLL: Duration = Duration::from_secs(5);
/// An installer script writes this line to the first serial port when it fails.
const FAILED_MARKER: &str = "inayos-image: failed";

/// The current release at a system's source, and the file to install it from.
#[derive(Clone, Debug)]
pub struct Source {
    /// The identifier the source publishes for this release, saved with the image.
    pub release: String,
    pub sha256: String,
    pub origin: Origin,
    pub kind: SourceKind,
}

/// Where the file of a release comes from.
#[derive(Clone, Debug)]
pub enum Origin {
    /// A link that the xtask downloads into `file_name` in the download folder.
    Link { url: String, file_name: String },
    /// The source refused to give a link to this host, so the user saves the file from
    /// `page` into the download folder by hand.
    ByHand { page: String },
}

/// What the downloaded file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// An installer disc, started on an empty disk.
    BootDisc,
    /// A ready disk image, which becomes the guest disk.
    Disk,
}

/// A failure to learn the current release from its source.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error(transparent)]
    Fetch(#[from] DownloadError),
    #[error("{url} no longer has {expected}; its format changed")]
    Format { url: String, expected: &'static str },
    #[error("could not read {0}: {1}")]
    Io(PathBuf, io::Error),
}

/// A failure of `image build`. The previous clean image is unchanged after every failure,
/// except `Replace`, which names what is left.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error(transparent)]
    Paths(#[from] PathsError),
    #[error(transparent)]
    Host(#[from] HostError),
    #[error(transparent)]
    Lock(#[from] LockError),
    #[error(transparent)]
    Source(#[from] SourceError),
    #[error(transparent)]
    Download(#[from] DownloadError),
    #[error(
        "{page} refused to give this host a download link for {os} {release}; save the file from that page into {folder} by hand and build again; the build uses the file there whose SHA-256 is {sha256}"
    )]
    ByHand {
        os: Os,
        release: String,
        page: String,
        folder: PathBuf,
        sha256: String,
    },
    #[error(transparent)]
    Seed(#[from] SeedError),
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Vm(#[from] VmError),
    #[error(transparent)]
    Qmp(#[from] QmpError),
    #[error(transparent)]
    Release(#[from] ReleaseError),
    #[error(transparent)]
    Disk(#[from] DiskError),
    #[error("could not change {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not start qemu-img: {0}")]
    QemuImgSpawn(io::Error),
    #[error("qemu-img could not prepare {0}: {1}")]
    QemuImg(PathBuf, String),
    #[error("the installation failed; see {serial} and the last screen in {screen}")]
    InstallFailed { serial: PathBuf, screen: PathBuf },
    #[error(
        "the installation did not finish within {timeout:?}; see {serial} and the last screen in {screen}"
    )]
    InstallTimeout {
        timeout: Duration,
        serial: PathBuf,
        screen: PathBuf,
    },
    #[error("the installed guest did not shut down within {0:?}")]
    NoShutdown(Duration),
    #[error("`ver` in the guest printed no Windows build number: {0}")]
    NoBuildNumber(String),
    #[error("the new image is complete in {built}, but replacing the old one failed: {error}")]
    Replace {
        built: PathBuf,
        #[source]
        error: Box<ImageError>,
    },
}

/// The release the source of `os` lists now.
pub fn latest_release(os: Os) -> Result<String, SourceError> {
    match os {
        Os::Windows => windows::latest_release(),
        Os::Ubuntu => ubuntu::latest_release(),
    }
}

fn source(os: Os) -> Result<Source, SourceError> {
    match os {
        Os::Windows => windows::source(),
        Os::Ubuntu => ubuntu::source(),
    }
}

/// Builds the clean image of `os` from its current release and replaces the previous one.
/// The throwaway and kept failed disks of `os` depend on the previous image, so they are
/// removed with it. With `watch`, a viewer window shows the guest display.
pub fn build(os: Os, watch: bool) -> Result<(), ImageError> {
    let paths = Paths::from_env()?;
    host::check(os)?;
    let _lock = VmLock::acquire(&paths.lock())?;
    let key = SshKey::ensure(&paths.ssh_key())?;

    println!("vm: looking up the current {os} release");
    let source = source(os)?;
    println!("vm: the current {os} release is {}", source.release);
    let downloads = paths.cache().join(os.name());
    let downloaded = match &source.origin {
        Origin::Link { url, file_name } => {
            download::fetch_verified(url, &source.sha256, &downloads, file_name)?
        }
        Origin::ByHand { page } => download::find_verified(&source.sha256, &downloads)?
            .ok_or_else(|| ImageError::ByHand {
                os,
                release: source.release.clone(),
                page: page.clone(),
                folder: downloads.clone(),
                sha256: source.sha256.clone(),
            })?,
    };

    // The build folder of an earlier failed build holds only its logs.
    let folder = paths.image_build(os);
    remove_dir(&folder)?;
    let layer = Layer::new(folder.join("layer"));
    fs::create_dir_all(layer.dir()).map_err(|error| ImageError::Io(layer.dir().into(), error))?;

    let build = Build {
        os,
        watch,
        folder: folder.clone(),
        layer: layer.clone(),
        runtime: paths.runtime(),
    };
    build.create_disk(&source, &downloaded)?;
    build.install(&source, &downloaded, &key.public()?)?;
    let built_from = build.verify(&source, &downloaded, key)?;
    built_from.write(&layer.release())?;

    replace_image(os, &paths, &layer).map_err(|error| ImageError::Replace {
        built: layer.dir().to_owned(),
        error: Box::new(error),
    })?;
    remove_dir(&folder)?;
    println!("vm: the {os} image is ready, built from {built_from}");
    Ok(())
}

/// One image build in progress, with its own folder for the new layer, the seed and the logs.
struct Build {
    os: Os,
    watch: bool,
    folder: PathBuf,
    layer: Layer,
    runtime: PathBuf,
}

impl Build {
    fn create_disk(&self, source: &Source, downloaded: &Path) -> Result<(), ImageError> {
        let disk = self.layer.disk();
        match source.kind {
            SourceKind::BootDisc => qemu_img(&disk, |command| {
                command
                    .args(["create", "-q", "-f", "qcow2"])
                    .arg(&disk)
                    .arg(DISK_SIZE);
            })?,
            SourceKind::Disk => {
                qemu_img(&disk, |command| {
                    command
                        .args(["convert", "-q", "-O", "qcow2"])
                        .arg(downloaded)
                        .arg(&disk);
                })?;
                qemu_img(&disk, |command| {
                    command.args(["resize", "-q"]).arg(&disk).arg(DISK_SIZE);
                })?;
            }
        }
        let vars = Firmware::for_boot(self.os.boot()).vars_template;
        fs::copy(&vars, self.layer.vars())
            .map_err(|error| ImageError::Io(self.layer.vars(), error))?;
        Ok(())
    }

    /// Boots the installer with the seed disc and waits until the guest powers itself off.
    fn install(
        &self,
        source: &Source,
        downloaded: &Path,
        public_key: &str,
    ) -> Result<(), ImageError> {
        let (label, files, timeout): (&str, Vec<SeedFile>, Duration) = match self.os {
            Os::Windows => (
                windows::SEED_LABEL,
                windows::seed_files(public_key),
                windows::INSTALL_TIMEOUT,
            ),
            Os::Ubuntu => (
                ubuntu::SEED_LABEL,
                ubuntu::seed_files(public_key),
                ubuntu::INSTALL_TIMEOUT,
            ),
        };
        let seed_iso = self.folder.join("seed.iso");
        seed::build(label, &files, &self.folder.join("seed"), &seed_iso)?;

        let serial = self.folder.join("install-serial.log");
        let media = Media {
            boot: (source.kind == SourceKind::BootDisc).then(|| downloaded.to_owned()),
            attached: vec![seed_iso],
        };
        println!(
            "vm: installing {os} unattended; this takes up to {timeout:?}",
            os = self.os
        );
        let mut vm = Vm::start(&self.machine(media, &serial), &self.runtime, self.watch)?;
        match self.os {
            Os::Windows => windows::start_install(&mut vm)?,
            Os::Ubuntu => {}
        }

        let deadline = Instant::now() + timeout;
        loop {
            if !is_running(&mut vm)? {
                break;
            }
            if serial_contains(&serial, FAILED_MARKER)? {
                let screen = self.screenshot(&vm);
                return Err(ImageError::InstallFailed { serial, screen });
            }
            if Instant::now() >= deadline {
                let screen = self.screenshot(&vm);
                return Err(ImageError::InstallTimeout {
                    timeout,
                    serial,
                    screen,
                });
            }
            thread::sleep(POLL);
        }
        Ok(())
    }

    /// Boots the installed guest without discs, logs in with the key, reads what only the
    /// guest knows about its release, and shuts it down.
    fn verify(
        &self,
        source: &Source,
        downloaded: &Path,
        key: SshKey,
    ) -> Result<Release, ImageError> {
        println!("vm: booting the installed {} to check SSH", self.os);
        let serial = self.folder.join("verify-serial.log");
        let mut vm = Vm::start(
            &self.machine(Media::default(), &serial),
            &self.runtime,
            self.watch,
        )?;
        let guest = Guest::new(key, GUEST_USER, SSH_PORT);
        guest.wait_until_ready(&mut vm, Duration::from_secs(self.os.ssh_timeout_secs()))?;
        let build = match self.os {
            Os::Windows => Some(windows::build_number(&guest)?),
            Os::Ubuntu => None,
        };

        let shutdown = match self.os {
            Os::Windows => "shutdown /s /t 0".to_owned(),
            Os::Ubuntu => {
                format!("printf '%s\\n' '{GUEST_PASSWORD}' | sudo -S -p '' systemctl poweroff")
            }
        };
        // The connection may drop while the guest goes down, so only the shutdown counts.
        let _ = guest.run(&shutdown);
        let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
        while is_running(&mut vm)? {
            if Instant::now() >= deadline {
                return Err(ImageError::NoShutdown(SHUTDOWN_TIMEOUT));
            }
            thread::sleep(POLL);
        }
        Ok(Release {
            release: source.release.clone(),
            build,
            source: downloaded
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })
    }

    fn machine(&self, media: Media, serial: &Path) -> Machine {
        Machine {
            os: self.os,
            layer: self.layer.clone(),
            sockets: Sockets::in_dir(&self.runtime),
            media,
            displays: 1,
            serial: Some(serial.to_owned()),
        }
    }

    /// Saves the guest screen into the build folder and returns its path. A screen that cannot
    /// be saved is reported and does not hide the failure it would illustrate.
    fn screenshot(&self, vm: &Vm) -> PathBuf {
        let screen = self.folder.join("screen.png");
        if let Err(error) = vm.qmp().and_then(|mut qmp| qmp.screendump(&screen)) {
            eprintln!("vm: could not save the guest screen: {error}");
        }
        screen
    }
}

/// Whether QEMU still runs. A stopped QEMU is the expected end of an installation, so its
/// exit is not an error here.
fn is_running(vm: &mut Vm) -> Result<bool, ImageError> {
    match vm.check_running() {
        Ok(()) => Ok(true),
        Err(VmError::Exited { .. }) => Ok(false),
        Err(error) => Err(error.into()),
    }
}

/// Whether the serial log holds `marker`. A log that QEMU has not created yet holds nothing.
fn serial_contains(serial: &Path, marker: &str) -> Result<bool, ImageError> {
    match fs::read(serial) {
        Ok(bytes) => Ok(String::from_utf8_lossy(&bytes).contains(marker)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ImageError::Io(serial.to_owned(), error)),
    }
}

/// Moves the new layer into the place of the clean image. The throwaway and failed disks of
/// `os` read from the old image disk, so they go first, then the old image.
fn replace_image(os: Os, paths: &Paths, built: &Layer) -> Result<(), ImageError> {
    for layer in [paths.work(os), paths.failed(os), paths.image(os)] {
        disk::discard(&layer)?;
    }
    let image = paths.image(os);
    fs::rename(built.dir(), image.dir())
        .map_err(|error| ImageError::Io(image.dir().to_owned(), error))
}

fn remove_dir(folder: &Path) -> Result<(), ImageError> {
    match fs::remove_dir_all(folder) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(ImageError::Io(folder.to_owned(), error))
        }
        _ => Ok(()),
    }
}

fn qemu_img(target: &Path, arguments: impl FnOnce(&mut Command)) -> Result<(), ImageError> {
    let mut command = Command::new("qemu-img");
    arguments(&mut command);
    let output = command.output().map_err(ImageError::QemuImgSpawn)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ImageError::QemuImg(
            target.to_owned(),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}
