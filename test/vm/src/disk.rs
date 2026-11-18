//! Creates throwaway layers over a clean image, so a guest never writes to the image itself,
//! and blank layers for guests that install onto an empty disk.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::firmware::Firmware;
use crate::os::Boot;
use crate::paths::Layer;

#[derive(Debug, thiserror::Error)]
pub enum DiskError {
    #[error("no finished image in {0}; build it with: cargo xtask vm image build <os>")]
    NoImage(PathBuf),
    #[error("could not change {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not start qemu-img: {0}")]
    Spawn(io::Error),
    #[error("qemu-img could not create {0}: {1}")]
    QemuImg(PathBuf, String),
    #[error("{error}; the half-made layer {leftover} could not be removed either: {cleanup}")]
    LeftBehind {
        error: Box<DiskError>,
        leftover: PathBuf,
        cleanup: io::Error,
    },
}

/// Whether `layer` holds everything a guest with `boot` starts from.
pub fn is_bootable(layer: &Layer, boot: Boot) -> bool {
    layer.disk().is_file()
        && layer.vars().is_file()
        && (boot != Boot::SecureUefiWithTpm || layer.tpm().is_dir())
}

/// Whether `image` is bootable and has the release file that the image builder writes last.
pub fn is_finished(image: &Layer, boot: Boot) -> bool {
    image.release().is_file() && is_bootable(image, boot)
}

/// Replaces `target` with a new layer over `image`: a qcow2 disk that reads unchanged
/// blocks from the image disk, and copies of the UEFI variables and the TPM state.
/// On failure the target folder is removed, so no half-made layer is left behind.
pub fn layer_over(image: &Layer, target: &Layer, boot: Boot) -> Result<(), DiskError> {
    if !is_finished(image, boot) {
        return Err(DiskError::NoImage(image.dir().to_owned()));
    }
    discard(target)?;
    build_layer(image, target, boot).map_err(|error| match fs::remove_dir_all(target.dir()) {
        Ok(()) => error,
        Err(cleanup) => DiskError::LeftBehind {
            error: Box::new(error),
            leftover: target.dir().to_owned(),
            cleanup,
        },
    })
}

/// Replaces `target` with a new layer that holds an empty qcow2 disk of `size`, in the
/// `qemu-img` size notation, and fresh UEFI variables. On failure the target folder is removed.
pub fn blank(target: &Layer, size: &str, boot: Boot) -> Result<(), DiskError> {
    discard(target)?;
    build_blank(target, size, boot).map_err(|error| match fs::remove_dir_all(target.dir()) {
        Ok(()) => error,
        Err(cleanup) => DiskError::LeftBehind {
            error: Box::new(error),
            leftover: target.dir().to_owned(),
            cleanup,
        },
    })
}

fn build_blank(target: &Layer, size: &str, boot: Boot) -> Result<(), DiskError> {
    fs::create_dir_all(target.dir())
        .map_err(|error| DiskError::Io(target.dir().to_owned(), error))?;
    let output = Command::new("qemu-img")
        .args(["create", "-q", "-f", "qcow2"])
        .arg(target.disk())
        .arg(size)
        .output()
        .map_err(DiskError::Spawn)?;
    if !output.status.success() {
        return Err(DiskError::QemuImg(
            target.disk(),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let vars = Firmware::for_boot(boot).vars_template;
    fs::copy(&vars, target.vars()).map_err(|error| DiskError::Io(target.vars(), error))?;
    if boot == Boot::SecureUefiWithTpm {
        fs::create_dir_all(target.tpm()).map_err(|error| DiskError::Io(target.tpm(), error))?;
    }
    Ok(())
}

/// Removes a layer and everything in it. A missing layer is already discarded.
pub fn discard(layer: &Layer) -> Result<(), DiskError> {
    match fs::remove_dir_all(layer.dir()) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            Err(DiskError::Io(layer.dir().to_owned(), error))
        }
        _ => Ok(()),
    }
}

fn build_layer(image: &Layer, target: &Layer, boot: Boot) -> Result<(), DiskError> {
    fs::create_dir_all(target.dir())
        .map_err(|error| DiskError::Io(target.dir().to_owned(), error))?;

    let backing =
        fs::canonicalize(image.disk()).map_err(|error| DiskError::Io(image.disk(), error))?;
    let output = Command::new("qemu-img")
        .args(["create", "-q", "-f", "qcow2", "-F", "qcow2", "-b"])
        .arg(&backing)
        .arg(target.disk())
        .output()
        .map_err(DiskError::Spawn)?;
    if !output.status.success() {
        return Err(DiskError::QemuImg(
            target.disk(),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }

    fs::copy(image.vars(), target.vars()).map_err(|error| DiskError::Io(target.vars(), error))?;
    if boot == Boot::SecureUefiWithTpm {
        copy_dir(&image.tpm(), &target.tpm())?;
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), DiskError> {
    let io_error = |path: &Path| {
        let path = path.to_owned();
        move |error| DiskError::Io(path, error)
    };
    fs::create_dir_all(to).map_err(io_error(to))?;
    for entry in fs::read_dir(from).map_err(io_error(from))? {
        let entry = entry.map_err(io_error(from))?;
        let source = entry.path();
        let destination = to.join(entry.file_name());
        if entry.file_type().map_err(io_error(&source))?.is_dir() {
            copy_dir(&source, &destination)?;
        } else {
            fs::copy(&source, &destination).map_err(io_error(&destination))?;
        }
    }
    Ok(())
}
