//! The installed disk that the boot chain scenarios start from. A `blank-disk` run copies its
//! target disk and UEFI variables right after the install has powered the live system off,
//! before the installed system first boots, and the copy replaces the kept one when the run
//! passes. A scenario that starts from it boots a throwaway layer over it, so every run sees
//! the first boot of the same installed system.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::disk;
use crate::os::Boot;
use crate::paths::Layer;

/// The firmware the installed disk was installed with. The UEFI variables hold the boot entry
/// that the install wrote, so only this firmware boots the disk.
pub const BOOT: Boot = Boot::Uefi;

#[derive(Debug, thiserror::Error)]
#[error(
    "no installed disk in {0}; run the blank-disk scenario first: cargo xtask vm scenario blank-disk"
)]
pub struct NoInstalledDisk(pub PathBuf);

/// The kept installed disk, when a `blank-disk` run has passed and left a complete one.
pub fn find(installed: &Layer) -> Result<Layer, NoInstalledDisk> {
    if disk::is_finished(installed, BOOT) {
        Ok(installed.clone())
    } else {
        Err(NoInstalledDisk(installed.dir().to_owned()))
    }
}

/// What the release file of the kept installed disk says: when the run that made it passed.
pub fn describe(installed: &Layer) -> String {
    fs::read_to_string(installed.release())
        .map(|text| text.trim().to_owned())
        .unwrap_or_default()
}

/// Replaces `copy` with a copy of the disk and UEFI variables of `layer`.
pub fn copy(layer: &Layer, copy: &Layer) -> io::Result<()> {
    remove(copy)?;
    fs::create_dir_all(copy.dir())?;
    // fs::copy clones the blocks on a filesystem that can, so the copy costs little there.
    fs::copy(layer.disk(), copy.disk())?;
    fs::copy(layer.vars(), copy.vars())?;
    Ok(())
}

/// Marks `copy` complete and puts it in place of `installed`. Layers made over the replaced
/// disk can no longer boot.
pub fn promote(copy: &Layer, installed: &Layer) -> io::Result<()> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or_default();
    fs::write(
        copy.release(),
        format!(
            "installed by a blank-disk run that passed at {seconds} seconds after the Unix epoch\n"
        ),
    )?;
    remove(installed)?;
    fs::rename(copy.dir(), installed.dir())
}

/// Removes a layer folder. A missing one is already removed.
pub fn remove(layer: &Layer) -> io::Result<()> {
    match fs::remove_dir_all(layer.dir()) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A new empty folder for one test, removed when the value is dropped.
    struct Folder(PathBuf);

    impl Folder {
        fn new(test: &str) -> Folder {
            let path = std::env::temp_dir().join(format!(
                "inayos-xtask-installed-{test}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Folder(path)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn without_a_passed_blank_disk_run_the_error_says_to_run_it() {
        let folder = Folder::new("missing");
        let error = find(&Layer::new(folder.0.join("installed"))).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cargo xtask vm scenario blank-disk"),
            "{error}"
        );
    }

    #[test]
    fn a_promoted_copy_replaces_the_kept_disk_and_is_found() {
        let folder = Folder::new("promoted");
        let work = Layer::new(folder.0.join("work"));
        fs::create_dir_all(work.dir()).unwrap();
        fs::write(work.disk(), "new disk").unwrap();
        fs::write(work.vars(), "new vars").unwrap();
        let installed = Layer::new(folder.0.join("installed"));
        fs::create_dir_all(installed.dir()).unwrap();
        fs::write(installed.disk(), "old disk").unwrap();
        let staged = Layer::new(folder.0.join("installed.new"));

        copy(&work, &staged).unwrap();
        promote(&staged, &installed).unwrap();

        let found = find(&installed).unwrap();
        assert_eq!(fs::read_to_string(found.disk()).unwrap(), "new disk");
        assert_eq!(fs::read_to_string(found.vars()).unwrap(), "new vars");
        assert!(!staged.dir().exists());
    }
}
