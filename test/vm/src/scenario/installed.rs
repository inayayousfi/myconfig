//! The kept installed disks that some scenarios start from instead of installing again. The
//! scenario that makes one copies its target disk, UEFI variables and TPM state right after the
//! install has powered the live system off, before the installed system first boots, and the
//! copy replaces the kept one when the run passes. A scenario that starts from it boots a
//! throwaway layer over it, so every run sees the first boot of the same installed system.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::disk;
use crate::os::Boot;
use crate::paths::Layer;

/// A kept installed disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kept {
    /// The disk of `blank-disk`: no Secure Boot, no encryption.
    Plain,
    /// The disk of `encrypted`: Secure Boot on and both volumes encrypted, with the TPM state
    /// that unlocks them and the recovery key that the install made.
    Encrypted,
}

impl Kept {
    /// The firmware the disk was installed with. The UEFI variables hold the boot entry that
    /// the install wrote and, for the encrypted disk, the Secure Boot keys, so only this
    /// firmware boots the disk as installed.
    pub fn boot(self) -> Boot {
        match self {
            Kept::Plain => Boot::Uefi,
            Kept::Encrypted => Boot::SecureUefiWithTpm,
        }
    }

    /// The scenario whose passed run keeps the disk.
    pub fn maker(self) -> &'static str {
        match self {
            Kept::Plain => "blank-disk",
            Kept::Encrypted => "encrypted",
        }
    }

    /// The folder of the kept disk in the scenario folder.
    pub fn folder(self) -> &'static str {
        match self {
            Kept::Plain => "installed",
            Kept::Encrypted => "installed-encrypted",
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error(
    "no installed disk in {dir}; run the {maker} scenario first: cargo xtask vm scenario {maker}"
)]
pub struct NoInstalledDisk {
    pub dir: PathBuf,
    pub maker: &'static str,
}

/// The kept installed disk `kept` in `installed`, when a run of its scenario has passed and
/// left a complete one.
pub fn find(kept: Kept, installed: &Layer) -> Result<Layer, NoInstalledDisk> {
    let complete = disk::is_finished(installed, kept.boot())
        && (kept != Kept::Encrypted || recovery_key(installed).is_file());
    if complete {
        Ok(installed.clone())
    } else {
        Err(NoInstalledDisk {
            dir: installed.dir().to_owned(),
            maker: kept.maker(),
        })
    }
}

/// What the release file of the kept installed disk says: when the run that made it passed.
pub fn describe(installed: &Layer) -> String {
    fs::read_to_string(installed.release())
        .map(|text| text.trim().to_owned())
        .unwrap_or_default()
}

/// The file in a kept encrypted disk's folder that holds the recovery key of its volumes,
/// which the system shows only once it is unlocked.
pub fn recovery_key(installed: &Layer) -> PathBuf {
    installed.dir().join("recovery-key.txt")
}

/// Replaces `copy` with a copy of the disk and UEFI variables of `layer`, and of its TPM state
/// when `boot` keeps one.
pub fn copy(layer: &Layer, copy: &Layer, boot: Boot) -> io::Result<()> {
    remove(copy)?;
    fs::create_dir_all(copy.dir())?;
    // fs::copy clones the blocks on a filesystem that can, so the copy costs little there.
    fs::copy(layer.disk(), copy.disk())?;
    fs::copy(layer.vars(), copy.vars())?;
    if boot.keeps_tpm_state() {
        disk::copy_dir(&layer.tpm(), &copy.tpm()).map_err(io::Error::other)?;
    }
    Ok(())
}

/// Marks `copy` complete as made by a passed run of `maker`, and puts it in place of
/// `installed`. Layers made over the replaced disk can no longer boot.
pub fn promote(copy: &Layer, installed: &Layer, maker: &str) -> io::Result<()> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.as_secs())
        .unwrap_or_default();
    fs::write(
        copy.release(),
        format!(
            "installed by a {maker} run that passed at {seconds} seconds after the Unix epoch\n"
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
    fn without_a_passed_run_the_error_names_the_scenario_to_run() {
        let folder = Folder::new("missing");
        let error = find(Kept::Plain, &Layer::new(folder.0.join("installed"))).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cargo xtask vm scenario blank-disk"),
            "{error}"
        );
        let error = find(
            Kept::Encrypted,
            &Layer::new(folder.0.join("installed-encrypted")),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cargo xtask vm scenario encrypted"),
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

        copy(&work, &staged, Kept::Plain.boot()).unwrap();
        promote(&staged, &installed, Kept::Plain.maker()).unwrap();

        let found = find(Kept::Plain, &installed).unwrap();
        assert_eq!(fs::read_to_string(found.disk()).unwrap(), "new disk");
        assert_eq!(fs::read_to_string(found.vars()).unwrap(), "new vars");
        assert!(!staged.dir().exists());
    }

    #[test]
    fn the_encrypted_disk_keeps_its_tpm_state_and_counts_only_with_its_recovery_key() {
        let folder = Folder::new("encrypted");
        let work = Layer::new(folder.0.join("work"));
        fs::create_dir_all(work.tpm()).unwrap();
        fs::write(work.disk(), "disk").unwrap();
        fs::write(work.vars(), "vars").unwrap();
        fs::write(work.tpm().join("tpm2-00.permall"), "sealed").unwrap();
        let installed = Layer::new(folder.0.join("installed-encrypted"));
        let staged = Layer::new(folder.0.join("installed-encrypted.new"));

        copy(&work, &staged, Kept::Encrypted.boot()).unwrap();
        promote(&staged, &installed, Kept::Encrypted.maker()).unwrap();
        assert!(find(Kept::Encrypted, &installed).is_err());

        fs::write(recovery_key(&installed), "key").unwrap();
        let found = find(Kept::Encrypted, &installed).unwrap();
        assert_eq!(
            fs::read_to_string(found.tpm().join("tpm2-00.permall")).unwrap(),
            "sealed"
        );
    }
}
