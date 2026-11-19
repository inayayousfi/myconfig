//! Builds the InayOS USB stick image: boots a throwaway layer over the clean Arch image,
//! builds the installer, the InayOS desktop installer and the boot guard from the working tree, assembles the
//! mkarchiso profile from archiso's releng profile and the overlay in `test/usb/profile/`, runs
//! mkarchiso in the guest, and copies the image to the state folder.
//!
//! The installer links to the system C library, so the build checks that it needs no newer
//! glibc than the image carries. The new image replaces the previous one only when it is
//! complete. A failed build keeps its
//! layer, with the build log and the last screen, as the failed Arch disk.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::binary::{self, Binary, BinaryError};
use crate::disk::{self, DiskError};
use crate::os::{GUEST_PASSWORD, GUEST_USER, Os};
use crate::release;
use crate::ssh::{Guest, SshError};
use crate::start::{self, Disk, Options, ReadyError, StartError};

/// The overlay on releng, relative to the repository root.
const OVERLAY: &str = "test/usb/profile";
/// The latest release of the on-screen keyboard that the live system runs.
const OSK_URL: &str = "https://github.com/axide-dev/axidev-osk/releases/latest/download/axidev-osk-linux-x86_64.tar.gz";
const SCRIPT: &str = include_str!("../usb/build-iso.sh");
/// Where the guest writes the finished image, outside the build folder that the script clears.
const GUEST_ISO: &str = "/var/tmp/inayos.iso";
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(3 * 60);

#[derive(Debug, thiserror::Error)]
pub enum UsbError {
    #[error("no mkarchiso profile overlay in {0}")]
    NoOverlay(PathBuf),
    #[error(transparent)]
    Start(#[from] StartError),
    #[error(transparent)]
    Disk(#[from] DiskError),
    #[error(
        "the USB stick build failed: {error}\nthe disk, build log and last screen are kept in {kept}"
    )]
    Failed {
        error: Box<StepError>,
        kept: PathBuf,
    },
    #[error(
        "the USB stick build failed: {error}\nkeeping the failed disk failed too, so it is still in {work}: {keep}"
    )]
    NotKept {
        error: Box<StepError>,
        work: PathBuf,
        keep: io::Error,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error(transparent)]
    Ready(#[from] ReadyError),
    #[error(transparent)]
    Ssh(#[from] SshError),
    #[error(transparent)]
    Binary(#[from] BinaryError),
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
}

/// Builds the USB stick image and saves it as `usb/inayos.iso` in the state folder.
pub fn build(watch: bool, repository: &Path) -> Result<(), UsbError> {
    let os = Os::Arch;
    let overlay = repository.join(OVERLAY);
    if !overlay.is_dir() {
        return Err(UsbError::NoOverlay(overlay));
    }
    let mut started = start::start(
        Options {
            os,
            disk: Disk::Throwaway,
            displays: 1,
            watch,
        },
        repository,
    )?;
    println!("vm: {}", release::check(os, &started.paths));
    let failed = started.paths.failed(os);
    disk::discard(&failed)?;
    let iso = started.paths.usb_iso();
    let work = started.layer.clone();
    let folder = work.dir().to_owned();

    let result = (|| -> Result<(), StepError> {
        start::install_binaries(os, &mut started.vm, &started.guest, &started.binaries)?;
        build_in_guest(&started.guest, &started.binaries, &overlay, &folder)?;
        // The build upgraded the guest from the same mirrors as the image's packages, so
        // its glibc is the one the live system has.
        binary::check_glibc(&started.guest, &started.binaries)?;
        save_iso(&started.guest, &iso)
    })();
    let vm = started.vm;
    match result {
        Ok(()) => {
            vm.shut_down(SHUTDOWN_TIMEOUT);
            disk::discard(&work)?;
            println!("vm: the USB stick image is ready in {}", iso.display());
            Ok(())
        }
        Err(error) => {
            let screen = folder.join("screen.png");
            if let Err(screen_error) = vm.qmp().and_then(|mut qmp| qmp.screendump(&screen)) {
                eprintln!("vm: could not save the last screen: {screen_error}");
            }
            vm.shut_down(SHUTDOWN_TIMEOUT);
            match fs::rename(work.dir(), failed.dir()) {
                Ok(()) => Err(UsbError::Failed {
                    error: Box::new(error),
                    kept: failed.dir().to_owned(),
                }),
                Err(keep) => Err(UsbError::NotKept {
                    error: Box::new(error),
                    work: folder,
                    keep,
                }),
            }
        }
    }
}

/// Copies the overlay and the build script into the guest and runs the script as root.
fn build_in_guest(
    guest: &Guest,
    binaries: &[Binary],
    overlay: &Path,
    folder: &Path,
) -> Result<(), StepError> {
    println!("vm: copying the profile overlay into the guest");
    guest.copy_dir_to(overlay, "usb-profile")?;
    let script = folder.join("build-iso.sh");
    fs::write(&script, SCRIPT).map_err(|error| StepError::Io(script.clone(), error))?;
    guest.copy_to(&script, "build-iso.sh")?;
    println!("vm: building the USB stick image in the guest");
    let home = format!("/home/{GUEST_USER}");
    // `start::install_binaries` copied the binaries into the home folder.
    let binary = |executable: &str| {
        let binary = binaries
            .iter()
            .find(|binary| binary.executable == executable)
            .expect("the Arch guest receives the installer, the desktop installer and the guard");
        format!("{home}/{}", binary.guest_path)
    };
    let command = format!(
        "printf '%s\\n' '{GUEST_PASSWORD}' | sudo -S -p '' bash {home}/build-iso.sh \
         {home}/usb-profile {} {} {} '{OSK_URL}' {GUEST_ISO}",
        binary("inayos-installer"),
        binary("inayos-arch-desktop"),
        binary("inayos-guard"),
    );
    guest.run_logged(&command, &folder.join("logs/build-iso.log"))?;
    Ok(())
}

/// Copies the image from the guest next to `iso`, then moves it into place, so `iso` is
/// always a complete image.
fn save_iso(guest: &Guest, iso: &Path) -> Result<(), StepError> {
    let folder = iso.parent().expect("the image path has a folder");
    fs::create_dir_all(folder).map_err(|error| StepError::Io(folder.to_owned(), error))?;
    let partial = iso.with_extension("iso.part");
    println!("vm: copying the image to {}", iso.display());
    guest.copy_from(GUEST_ISO, &partial)?;
    fs::rename(&partial, iso).map_err(|error| StepError::Io(iso.to_owned(), error))
}
