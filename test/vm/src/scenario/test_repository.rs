//! A test copy of the InayOS pacman repository, signed with a throwaway GPG key made per run,
//! and a small marker package that the maintenance and rollback scenarios build on the host
//! with makepkg.
//!
//! The `inayos` packages that the test repository serves come from `crate::packages`, which
//! packages the binaries the xtask built. The marker package, written by the scenario, gives a
//! pacman transaction that needs no Arch mirror.

use std::fs::{self, DirBuilder, File};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The repository name, which names its database files.
pub const REPOSITORY: &str = "inayos";
/// The package of the InayOS desktop installer, the guard and their pacman hooks.
pub const INAYOS_PACKAGE: &str = "inayos";
/// The architecture that the guest's pacman installs. The host's makepkg.conf may name
/// another, so the build sets it.
const ARCH: &str = "x86_64";
const KEY_USER: &str = "InayOS test repository <test-repository@inayos.invalid>";
/// The small package that a scenario installs, reinstalls or rolls back.
pub const MARKER_PACKAGE: &str = "inayos-test-marker";
const MARKER_PKGBUILD: &str = r#"pkgname=inayos-test-marker
pkgver=1
pkgrel=1
pkgdesc='A file that the InayOS VM scenarios install and remove'
arch=(any)
license=(MIT)

package() {
    install -D -m 644 /dev/null "$pkgdir/usr/share/inayos-test-marker/marker"
}
"#;

#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not start {0}: {1}")]
    Spawn(&'static str, io::Error),
    #[error("{command} failed; its output is in {log}")]
    Failed { command: String, log: PathBuf },
    #[error("makepkg built no {package} package in {folder}")]
    NoPackage { package: String, folder: PathBuf },
    #[error("gpg made a key but lists no fingerprint for it")]
    NoFingerprint,
}

/// A built package file and the version that `pacman -Q` shows for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub file: PathBuf,
    /// `pkgver-pkgrel`, with the epoch when the package has one.
    pub version: String,
}

/// The test repository: the folder that the file server gives the guest, and the GPG home with
/// the throwaway key that signs its packages and database.
pub struct TestRepository {
    folder: PathBuf,
    gnupg: PathBuf,
    logs: PathBuf,
    fingerprint: String,
    public_key: PathBuf,
}

impl TestRepository {
    /// Makes the empty repository folder and a new signing key in `dir`. Command output goes to
    /// `logs`.
    pub fn create(dir: &Path, logs: &Path) -> Result<TestRepository, RepositoryError> {
        let folder = dir.join("repository");
        let gnupg = dir.join("gnupg");
        for path in [&folder, &gnupg] {
            let _ = fs::remove_dir_all(path);
        }
        fs::create_dir_all(&folder).map_err(|error| RepositoryError::Io(folder.clone(), error))?;
        fs::create_dir_all(logs).map_err(|error| RepositoryError::Io(logs.to_owned(), error))?;
        // gpg refuses a home folder that others can read.
        DirBuilder::new()
            .mode(0o700)
            .create(&gnupg)
            .map_err(|error| RepositoryError::Io(gnupg.clone(), error))?;
        let mut repository = TestRepository {
            folder,
            public_key: dir.join("test-repository-key.asc"),
            gnupg,
            logs: logs.to_owned(),
            fingerprint: String::new(),
        };
        repository.gpg(
            "gpg-key",
            &[
                "--batch",
                "--pinentry-mode",
                "loopback",
                "--passphrase",
                "",
                "--quick-generate-key",
                KEY_USER,
                "ed25519",
                "sign",
                "never",
            ],
        )?;
        let colons = repository.gpg_output(&["--with-colons", "--list-secret-keys"])?;
        repository.fingerprint = fingerprint(&colons).ok_or(RepositoryError::NoFingerprint)?;
        let armored = repository.gpg_output(&["--armor", "--export", &repository.fingerprint])?;
        fs::write(&repository.public_key, armored)
            .map_err(|error| RepositoryError::Io(repository.public_key.clone(), error))?;
        println!(
            "vm: the test repository signs with the throwaway key {}",
            repository.fingerprint
        );
        Ok(repository)
    }

    pub fn folder(&self) -> &Path {
        &self.folder
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// The armored public key, which the guest's pacman keyring signs locally.
    pub fn public_key(&self) -> &Path {
        &self.public_key
    }

    /// Signs `package`, copies it with its signature into the repository, and adds it to the
    /// signed database, replacing an older version of the same package.
    pub fn add(&self, package: &Package) -> Result<(), RepositoryError> {
        let name = package.file.file_name().expect("a package file has a name");
        let placed = self.folder.join(name);
        fs::copy(&package.file, &placed)
            .map_err(|error| RepositoryError::Io(placed.clone(), error))?;
        let signature = format!("{}.sig", placed.display());
        let _ = fs::remove_file(&signature);
        self.gpg(
            "gpg-sign",
            &[
                "--batch",
                "--local-user",
                &self.fingerprint,
                "--detach-sign",
                "--no-armor",
                "--output",
                &signature,
                &placed.to_string_lossy(),
            ],
        )?;
        let database = self.folder.join(format!("{REPOSITORY}.db.tar.zst"));
        let mut command = Command::new("repo-add");
        command
            .args(["--sign", "--key", &self.fingerprint, "--remove"])
            .arg(&database)
            .arg(&placed)
            .env("GNUPGHOME", &self.gnupg);
        run(command, "repo-add", &self.logs.join("repo-add.log"))
    }

    fn gpg(&self, label: &str, arguments: &[&str]) -> Result<(), RepositoryError> {
        let mut command = Command::new("gpg");
        command.args(arguments).env("GNUPGHOME", &self.gnupg);
        run(command, "gpg", &self.logs.join(format!("{label}.log")))
    }

    fn gpg_output(&self, arguments: &[&str]) -> Result<String, RepositoryError> {
        let output = Command::new("gpg")
            .args(["--batch"])
            .args(arguments)
            .env("GNUPGHOME", &self.gnupg)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| RepositoryError::Spawn("gpg", error))?;
        if !output.status.success() {
            let log = self.logs.join("gpg-output.log");
            let _ = fs::write(&log, &output.stderr);
            return Err(RepositoryError::Failed {
                command: format!("gpg {}", arguments.join(" ")),
                log,
            });
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

impl Drop for TestRepository {
    /// Stops the gpg agent that the key's commands started for the run's GPG home.
    fn drop(&mut self) {
        let _ = Command::new("gpgconf")
            .args(["--kill", "gpg-agent"])
            .env("GNUPGHOME", &self.gnupg)
            .stdin(Stdio::null())
            .status();
    }
}

/// Builds the marker package from the PKGBUILD this file holds, into `dir`.
pub fn build_marker(dir: &Path, logs: &Path) -> Result<Package, RepositoryError> {
    println!("vm: building the {MARKER_PACKAGE} package");
    let source = dir.join(format!("{MARKER_PACKAGE}-source"));
    fs::create_dir_all(&source).map_err(|error| RepositoryError::Io(source.clone(), error))?;
    fs::create_dir_all(logs).map_err(|error| RepositoryError::Io(logs.to_owned(), error))?;
    let pkgbuild = source.join("PKGBUILD");
    fs::write(&pkgbuild, MARKER_PKGBUILD).map_err(|error| RepositoryError::Io(pkgbuild, error))?;
    let out = dir.join(MARKER_PACKAGE);
    let command = makepkg(&source, &out)?;
    run(
        command,
        "makepkg",
        &logs.join(format!("makepkg-{MARKER_PACKAGE}.log")),
    )?;
    find_package(&out.join("packages"), MARKER_PACKAGE)
}

/// A makepkg command for the PKGBUILD in `source` that writes the package and its build
/// folders under `out`. It skips the dependency checks, which read the host's installed
/// packages, not the guest's.
fn makepkg(source: &Path, out: &Path) -> Result<Command, RepositoryError> {
    let _ = fs::remove_dir_all(out);
    let packages = out.join("packages");
    let build = out.join("build");
    for folder in [&packages, &build] {
        fs::create_dir_all(folder).map_err(|error| RepositoryError::Io(folder.clone(), error))?;
    }
    let mut command = Command::new("makepkg");
    command
        .args(["--force", "--nodeps", "--noconfirm", "--cleanbuild"])
        .current_dir(source)
        .env("PKGDEST", &packages)
        .env("BUILDDIR", &build)
        .env("SRCDEST", &build)
        .env("LOGDEST", &build)
        .env("PKGEXT", ".pkg.tar.zst")
        .env("CARCH", ARCH);
    Ok(command)
}

/// The package file of `package` in the folder `packages`.
pub fn find_package(packages: &Path, package: &str) -> Result<Package, RepositoryError> {
    let names: Vec<String> = fs::read_dir(packages)
        .map_err(|error| RepositoryError::Io(packages.to_owned(), error))?
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    package_file(&names, package)
        .map(|(name, version)| Package {
            file: packages.join(name),
            version,
        })
        .ok_or_else(|| RepositoryError::NoPackage {
            package: package.to_owned(),
            folder: packages.to_owned(),
        })
}

/// Runs `command` with its output and error output in `log`.
fn run(mut command: Command, program: &'static str, log: &Path) -> Result<(), RepositoryError> {
    let file = File::create(log).map_err(|error| RepositoryError::Io(log.to_owned(), error))?;
    let errors = file
        .try_clone()
        .map_err(|error| RepositoryError::Io(log.to_owned(), error))?;
    let status = command
        .stdin(Stdio::null())
        .stdout(file)
        .stderr(errors)
        .status()
        .map_err(|error| RepositoryError::Spawn(program, error))?;
    if status.success() {
        Ok(())
    } else {
        Err(RepositoryError::Failed {
            command: format!("{program} ({status})"),
            log: log.to_owned(),
        })
    }
}

/// The file among `names` that holds the package `package`, with its version. A package file
/// is named `<name>-<pkgver>-<pkgrel>-<arch>.pkg.tar.<compression>`; a name can hold dashes,
/// so `inayos-keyring` and `inayos-debug` files are not `inayos`.
pub fn package_file(names: &[String], package: &str) -> Option<(String, String)> {
    names.iter().find_map(|name| {
        let stem = name.split_once(".pkg.tar.")?.0;
        if name.ends_with(".sig") {
            return None;
        }
        let mut parts = stem.rsplitn(4, '-');
        let (_arch, pkgrel, pkgver, found) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        (found == package).then(|| (name.clone(), format!("{pkgver}-{pkgrel}")))
    })
}

/// The fingerprint of the first key in `gpg --with-colons` output.
pub fn fingerprint(colons: &str) -> Option<String> {
    colons.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        (fields.first() == Some(&"fpr"))
            .then(|| fields.get(9).copied())
            .flatten()
            .filter(|fingerprint| !fingerprint.is_empty())
            .map(str::to_owned)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_package_file_is_the_one_whose_whole_name_matches() {
        let names: Vec<String> = [
            "inayos-debug-0.9.0-1001-x86_64.pkg.tar.zst",
            "inayos-keyring-20261119-1-any.pkg.tar.zst",
            "inayos-0.9.0-1001-x86_64.pkg.tar.zst.sig",
            "inayos-0.9.0-1001-x86_64.pkg.tar.zst",
        ]
        .map(str::to_owned)
        .to_vec();
        assert_eq!(
            package_file(&names, "inayos"),
            Some((
                "inayos-0.9.0-1001-x86_64.pkg.tar.zst".to_owned(),
                "0.9.0-1001".to_owned()
            ))
        );
        assert_eq!(
            package_file(&names, "inayos-keyring").map(|(_, version)| version),
            Some("20261119-1".to_owned())
        );
        assert_eq!(package_file(&names, "inayos-news"), None);
    }

    #[test]
    fn the_fingerprint_comes_from_the_first_fpr_line() {
        let colons = "sec:u:255:22:1234ABCD1234ABCD:1763550000:::u:::scSC:::+:::ed25519:::0:\n\
                      fpr:::::::::0123456789ABCDEF0123456789ABCDEF01234567:\n\
                      grp:::::::::AAAA:\n\
                      uid:u::::1763550000::HASH::InayOS test repository <x@inayos.invalid>::::::::::0:\n";
        assert_eq!(
            fingerprint(colons).as_deref(),
            Some("0123456789ABCDEF0123456789ABCDEF01234567")
        );
        assert_eq!(fingerprint("sec:u:255:22:1234:\n"), None);
    }
}
