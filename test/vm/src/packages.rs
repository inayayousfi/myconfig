//! Builds the InayOS pacman repository from the working tree on the host: makepkg packages
//! `packaging/inayos` with the binaries that cargo built, and `packaging/inayos-keyring`,
//! and repo-add indexes both into a repository named `inayos`, unsigned.
//!
//! While the committed keyring holds the placeholder, the keyring package carries a
//! throwaway test key made for the build, whose private part stays in the build folder.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::binary::Binary;

/// The committed fingerprint file of the keyring package, relative to the repository root.
const TRUSTED: &str = "packaging/inayos-keyring/inayos-trusted";
/// The mark of the committed keyring files while the InayOS signing key does not exist.
const PLACEHOLDER: &str = "INAYOS-PLACEHOLDER";
/// The repository's database, which names the repository `inayos`.
pub const DATABASE: &str = "inayos.db.tar.zst";
/// Each program of the `inayos` package: the binary cargo builds, and its name in the package.
const PROGRAMS: [(&str, &str); 3] = [
    ("inayos-arch-desktop", "inayos"),
    ("inayos-guard", "inayos-guard"),
    ("inayos-news", "inayos-news"),
];

#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    #[error("could not prepare {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("the built binaries lack {0}, which the inayos package holds")]
    NoBinary(&'static str),
    #[error("could not run {0}: {1}")]
    Start(&'static str, io::Error),
    #[error("`{command}` failed with {status}")]
    Failed { command: String, status: String },
    #[error("gpg made the test key but listed no fingerprint for it")]
    NoFingerprint,
}

/// A built repository.
#[derive(Debug)]
pub struct Repository {
    /// The folder with the database and the packages, which pacman reads as a `Server`.
    pub folder: PathBuf,
}

/// Builds both packages with package release `pkgrel` into a new repository under `work`,
/// which it clears first.
pub fn build_repository(
    repository: &Path,
    binaries: &[Binary],
    work: &Path,
    pkgrel: u32,
) -> Result<Repository, PackageError> {
    let io = |path: &Path| {
        let path = path.to_owned();
        move |error| PackageError::Io(path, error)
    };
    match fs::remove_dir_all(work) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(PackageError::Io(work.to_owned(), error)),
    }
    let programs = work.join("binaries");
    let build = work.join("build");
    let folder = work.join("repo");
    for path in [&programs, &build, &folder] {
        fs::create_dir_all(path).map_err(io(path))?;
    }
    for (executable, name) in PROGRAMS {
        let binary = binaries
            .iter()
            .find(|binary| binary.executable == executable)
            .ok_or(PackageError::NoBinary(executable))?;
        let staged = programs.join(name);
        fs::copy(&binary.local, &staged).map_err(io(&staged))?;
    }

    let trusted_path = repository.join(TRUSTED);
    let trusted = fs::read_to_string(&trusted_path).map_err(io(&trusted_path))?;
    let keyring = if trusted.contains(PLACEHOLDER) {
        println!(
            "vm: the committed InayOS keyring is a placeholder, so a test key signs nothing and is trusted instead"
        );
        Some(test_keyring(work)?)
    } else {
        None
    };

    let pkgrel = pkgrel.to_string();
    for package in ["inayos", "inayos-keyring"] {
        let mut makepkg = Command::new("makepkg");
        makepkg
            .args(["--nodeps", "--force", "--noconfirm"])
            .current_dir(repository.join("packaging").join(package))
            .env("BUILDDIR", &build)
            .env("SRCDEST", &build)
            .env("PKGDEST", &folder)
            .env("LOGDEST", &build)
            .env("PKGEXT", ".pkg.tar.zst")
            // The host's makepkg.conf may name another architecture than the guest's.
            .env("CARCH", "x86_64")
            .env("INAYOS_BINARY_DIR", &programs)
            .env("INAYOS_PKGREL", &pkgrel);
        if let Some(keyring) = &keyring {
            makepkg.env("INAYOS_KEYRING_DIR", keyring);
        }
        run(
            "makepkg",
            &mut makepkg,
            &format!("makepkg in packaging/{package}"),
        )?;
    }

    let mut packages: Vec<PathBuf> = fs::read_dir(&folder)
        .map_err(io(&folder))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.to_string_lossy().ends_with(".pkg.tar.zst"))
        .collect();
    packages.sort();
    let mut repo_add = Command::new("repo-add");
    repo_add
        .arg("--quiet")
        .arg(folder.join(DATABASE))
        .args(&packages);
    run("repo-add", &mut repo_add, "repo-add")?;
    // repo-add links the names pacman reads to the archives. Copies survive every way of
    // moving the folder into a guest.
    for (name, archive) in [
        ("inayos.db", DATABASE),
        ("inayos.files", "inayos.files.tar.zst"),
    ] {
        let link = folder.join(name);
        fs::remove_file(&link).map_err(io(&link))?;
        fs::copy(folder.join(archive), &link).map_err(io(&link))?;
    }
    Ok(Repository { folder })
}

/// Makes a throwaway key in `work/gnupg` and writes the keyring package's three files for it
/// into `work/keyring`, which it returns.
fn test_keyring(work: &Path) -> Result<PathBuf, PackageError> {
    let home = work.join("gnupg");
    let keyring = work.join("keyring");
    for path in [&home, &keyring] {
        fs::create_dir_all(path).map_err(|error| PackageError::Io(path.clone(), error))?;
    }
    // gpg refuses a home folder that others can read.
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&home, fs::Permissions::from_mode(0o700))
        .map_err(|error| PackageError::Io(home.clone(), error))?;
    let gpg = || {
        let mut command = Command::new("gpg");
        command.arg("--homedir").arg(&home).arg("--batch");
        command
    };
    run(
        "gpg",
        gpg().args([
            "--passphrase",
            "",
            "--quick-gen-key",
            "InayOS VM test key <test@inayos.invalid>",
            "ed25519",
            "sign",
            "never",
        ]),
        "gpg --quick-gen-key",
    )?;
    let listed = gpg()
        .args(["--with-colons", "--list-keys"])
        .output()
        .map_err(|error| PackageError::Start("gpg", error))?;
    let fingerprint = first_fingerprint(&String::from_utf8_lossy(&listed.stdout))
        .ok_or(PackageError::NoFingerprint)?;
    let exported = gpg()
        .args(["--armor", "--export", &fingerprint])
        .output()
        .map_err(|error| PackageError::Start("gpg", error))?;
    let write = |name: &str, content: &[u8]| {
        let path = keyring.join(name);
        fs::write(&path, content).map_err(|error| PackageError::Io(path, error))
    };
    write("inayos.gpg", &exported.stdout)?;
    write("inayos-trusted", format!("{fingerprint}:4:\n").as_bytes())?;
    write("inayos-revoked", b"")?;
    Ok(keyring)
}

/// The fingerprint of the first key in `gpg --with-colons` output: the `fpr` line right
/// after the first `pub` line.
fn first_fingerprint(listing: &str) -> Option<String> {
    let mut lines = listing.lines().skip_while(|line| !line.starts_with("pub:"));
    lines.next()?;
    let line = lines.next()?;
    let fields: Vec<&str> = line.split(':').collect();
    (fields.first() == Some(&"fpr"))
        .then(|| fields.get(9).map(|field| (*field).to_owned()))
        .flatten()
        .filter(|fingerprint| !fingerprint.is_empty())
}

fn run(program: &'static str, command: &mut Command, shown: &str) -> Result<(), PackageError> {
    let status = command
        .status()
        .map_err(|error| PackageError::Start(program, error))?;
    if status.success() {
        Ok(())
    } else {
        Err(PackageError::Failed {
            command: shown.to_owned(),
            status: status.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::first_fingerprint;

    #[test]
    fn the_fingerprint_is_the_one_of_the_primary_key() {
        let listing = "tru::1:1700000000:0:3:1:5\n\
                       pub:u:255:22:1111222233334444:1700000000:::u:::scESC:::::ed25519:::0:\n\
                       fpr:::::::::AAAABBBBCCCCDDDDEEEEFFFF1111222233334444:\n\
                       uid:u::::1700000000::HASH::InayOS VM test key <test@inayos.invalid>::::::::::0:\n\
                       sub:u:255:18:5555666677778888:1700000000::::::e:::::cv25519::\n\
                       fpr:::::::::9999888877776666555544443333222211110000:\n";
        assert_eq!(
            first_fingerprint(listing).as_deref(),
            Some("AAAABBBBCCCCDDDDEEEEFFFF1111222233334444")
        );
        assert_eq!(first_fingerprint("tru::1:1700000000:0:3:1:5\n"), None);
    }
}
