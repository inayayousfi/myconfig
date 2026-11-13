//! Builds the small read-only disc that carries the files an unattended installation reads,
//! such as answer files, scripts and the SSH public key.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum SeedError {
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not start xorriso: {0}")]
    Spawn(io::Error),
    #[error("xorriso could not build {0}: {1}")]
    Xorriso(PathBuf, String),
}

/// One file on the disc.
pub struct SeedFile {
    pub name: &'static str,
    pub content: String,
    pub executable: bool,
}

/// Writes `files` into the folder `staging` and builds the ISO 9660 disc `iso` from it, with
/// the volume label `label`. Rock Ridge keeps the executable bit, and Joliet keeps the long
/// names that Windows reads.
pub fn build(label: &str, files: &[SeedFile], staging: &Path, iso: &Path) -> Result<(), SeedError> {
    let io_error = |path: &Path| {
        let path = path.to_owned();
        move |error| SeedError::Io(path, error)
    };
    match fs::remove_dir_all(staging) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => {
            return Err(SeedError::Io(staging.to_owned(), error));
        }
        _ => {}
    }
    fs::create_dir_all(staging).map_err(io_error(staging))?;
    for file in files {
        let path = staging.join(file.name);
        fs::write(&path, &file.content).map_err(io_error(&path))?;
        let mode = if file.executable { 0o755 } else { 0o644 };
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).map_err(io_error(&path))?;
    }

    let output = Command::new("xorriso")
        .args([
            "-as",
            "mkisofs",
            "-quiet",
            "-rational-rock",
            "-joliet",
            "-volid",
        ])
        .arg(label)
        .arg("-output")
        .arg(iso)
        .arg(staging)
        .output()
        .map_err(SeedError::Spawn)?;
    if !output.status.success() {
        return Err(SeedError::Xorriso(
            iso.to_owned(),
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(())
}
