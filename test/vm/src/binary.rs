//! The fresh installer binaries: built from the working tree and copied into a guest.

use std::env;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use serde_json::Value;

use crate::collect;
use crate::os::{GUEST_USER, Os};
use crate::ssh::{Guest, SshError};

/// A static Linux binary, which runs on any Linux guest.
const MUSL_TARGET: &str = "x86_64-unknown-linux-musl";
/// A Linux binary linked to the system C library. The Iced interface of the InayOS installer
/// needs it, because winit loads the Wayland libraries at run time, which a static binary
/// cannot do.
const GNU_TARGET: &str = "x86_64-unknown-linux-gnu";
const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

#[derive(Debug, thiserror::Error)]
pub enum BinaryError {
    #[error("could not start cargo for {0}: {1}")]
    Spawn(&'static str, io::Error),
    #[error("could not read the cargo output for {0}: {1}")]
    Read(&'static str, io::Error),
    #[error("building {package} failed with {status}")]
    Failed {
        package: &'static str,
        status: ExitStatus,
    },
    #[error("cargo built {0} but named no executable for it")]
    NoExecutable(&'static str),
    #[error("could not copy {package} into the guest: {error}")]
    Copy {
        package: &'static str,
        #[source]
        error: SshError,
    },
    #[error("could not run readelf on {package}: {error}")]
    Readelf {
        package: &'static str,
        error: io::Error,
    },
    #[error("readelf could not read {package}: {stderr}")]
    ReadelfFailed {
        package: &'static str,
        stderr: String,
    },
    #[error("could not read the C library version of the guest: {0}")]
    GuestGlibc(#[source] SshError),
    #[error("the guest's `ldd --version` names no C library version: {0:?}")]
    NoGuestGlibc(String),
    #[error(
        "{package} needs glibc {needs}, but the guest has glibc {guest}; build it on a host whose glibc is not newer than the guest's"
    )]
    GlibcTooNew {
        package: &'static str,
        needs: GlibcVersion,
        guest: GlibcVersion,
    },
}

/// One installer to build, and where it goes in the guest.
#[derive(Clone, Copy, Debug)]
struct Recipe {
    package: &'static str,
    target: &'static str,
    /// Relative to the guest user's home folder, with `/` between folders.
    guest_path: &'static str,
    launch: Launch,
}

/// How the guest starts an installer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Launch {
    /// From the shell of a Linux guest.
    Linux,
    /// From the command prompt of the Windows guest.
    Windows,
    /// Inside the default WSL distribution, from the command prompt of the Windows guest.
    /// The Arch WSL module of the Windows installer makes its distribution the default.
    Wsl,
}

/// A built installer on the host and its place in the guest.
#[derive(Clone, Debug)]
pub struct Binary {
    pub package: &'static str,
    pub local: PathBuf,
    pub target: &'static str,
    /// Relative to the guest user's home folder, with `/` between folders.
    pub guest_path: &'static str,
    pub launch: Launch,
}

/// Where the Windows guest keeps the Arch WSL installer, relative to the user's home folder.
pub const ARCH_WSL_GUEST_PATH: &str = "inayos-arch-wsl";

impl Binary {
    /// The command line that runs this installer with `arguments` in the guest's shell.
    pub fn command(&self, arguments: &str) -> String {
        match self.launch {
            Launch::Linux => format!("~/{} {arguments}", self.guest_path),
            Launch::Windows => {
                let path = self.guest_path.replace('/', "\\");
                // The Arch WSL module runs the hook, which runs the fresh Linux installer,
                // instead of a release.
                let hook = collect::arch_wsl_hook();
                format!(
                    "set \"INAYOS_ARCH_WSL_BINARY={hook}\" && \"%USERPROFILE%\\{path}\" {arguments}"
                )
            }
            Launch::Wsl => {
                // A copy inside the distribution runs whatever the mount options of C: are.
                let copy = "/tmp/inayos-arch-wsl";
                format!(
                    "wsl.exe --exec bash -c \"install -m 0755 /mnt/c/Users/{GUEST_USER}/{} {copy} && \
                     exec {copy} {arguments}\"",
                    self.guest_path
                )
            }
        }
    }
}

/// The installers each guest runs. Linux guests keep the binary where `install.sh` puts it,
/// and Windows keeps it where `install.ps1` puts it. A Windows guest also receives the Arch
/// WSL installer. The arch-wsl module of the Windows installer installs with it, and the cycle
/// runs it inside WSL to list and remove its modules.
fn recipes(os: Os) -> &'static [Recipe] {
    match os {
        Os::Ubuntu => &[Recipe {
            package: "inayos-ubuntu-devbox",
            target: MUSL_TARGET,
            guest_path: ".local/bin/inayos",
            launch: Launch::Linux,
        }],
        // The Arch guest builds the USB stick, which carries the installer and the InayOS
        // desktop installer that it places in the new system. The installer's screens need
        // the system C library of the live system.
        Os::Arch => &[
            Recipe {
                package: "inayos-installer",
                target: GNU_TARGET,
                guest_path: "inayos-installer",
                launch: Launch::Linux,
            },
            Recipe {
                package: "inayos-arch-desktop",
                target: MUSL_TARGET,
                guest_path: "inayos-arch-desktop",
                launch: Launch::Linux,
            },
        ],
        Os::Windows => &[
            Recipe {
                package: "inayos-windows",
                target: WINDOWS_TARGET,
                guest_path: "AppData/Local/inayos/inayos.exe",
                launch: Launch::Windows,
            },
            Recipe {
                package: "inayos-arch-wsl",
                target: MUSL_TARGET,
                guest_path: ARCH_WSL_GUEST_PATH,
                launch: Launch::Wsl,
            },
        ],
    }
}

/// The Rust targets that the installers of `os` build for.
pub fn targets(os: Os) -> Vec<&'static str> {
    let mut targets: Vec<&'static str> = recipes(os).iter().map(|recipe| recipe.target).collect();
    targets.sort_unstable();
    targets.dedup();
    targets
}

/// Whether the installers of `os` build with `cargo xwin`.
pub fn needs_xwin(os: Os) -> bool {
    recipes(os).iter().any(Recipe::uses_xwin)
}

/// Whether an installer of `os` links to the system C library, whose version the guest must
/// meet, which `readelf` reads.
pub fn needs_readelf(os: Os) -> bool {
    recipes(os).iter().any(|recipe| recipe.target == GNU_TARGET)
}

/// Builds the release installers for `os` from the working tree at `repository`.
/// Cargo's progress goes to the terminal.
pub fn build(os: Os, repository: &Path) -> Result<Vec<Binary>, BinaryError> {
    recipes(os)
        .iter()
        .map(|recipe| build_one(recipe, repository))
        .collect()
}

/// Copies the built installers into the guest, creating their folders first.
pub fn install(os: Os, guest: &Guest, binaries: &[Binary]) -> Result<(), BinaryError> {
    for binary in binaries {
        let copy_error = |error| BinaryError::Copy {
            package: binary.package,
            error,
        };
        if let Some((folder, _)) = binary.guest_path.rsplit_once('/') {
            let command = match os {
                Os::Windows => {
                    // The OpenSSH server of Windows runs commands in the command prompt.
                    let folder = folder.replace('/', "\\");
                    format!("if not exist \"{folder}\" mkdir \"{folder}\"")
                }
                Os::Ubuntu | Os::Arch => format!("mkdir -p '{folder}'"),
            };
            guest.run(&command).map_err(copy_error)?;
        }
        guest
            .copy_to(&binary.local, binary.guest_path)
            .map_err(copy_error)?;
    }
    Ok(())
}

impl Recipe {
    fn uses_xwin(&self) -> bool {
        self.target == WINDOWS_TARGET
    }
}

fn build_one(recipe: &Recipe, repository: &Path) -> Result<Binary, BinaryError> {
    let package = recipe.package;
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    if recipe.uses_xwin() {
        command.arg("xwin");
    }
    command
        .args(["build", "--release", "--locked", "--target", recipe.target])
        .args(["-p", package, "--message-format=json-render-diagnostics"])
        .current_dir(repository)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = command
        .spawn()
        .map_err(|error| BinaryError::Spawn(package, error))?;

    let stdout = child.stdout.take().expect("stdout was piped above");
    let mut executable = None;
    for line in BufReader::new(stdout).lines() {
        let line = line.map_err(|error| BinaryError::Read(package, error))?;
        if let Some(path) = artifact_executable(&line, package) {
            executable = Some(path);
        }
    }
    let status = child
        .wait()
        .map_err(|error| BinaryError::Read(package, error))?;
    if !status.success() {
        return Err(BinaryError::Failed { package, status });
    }
    Ok(Binary {
        package,
        local: executable.ok_or(BinaryError::NoExecutable(package))?,
        target: recipe.target,
        guest_path: recipe.guest_path,
        launch: recipe.launch,
    })
}

/// Checks that every binary linked to the system C library runs with the C library of
/// `guest`: no symbol version it needs is newer than the guest's glibc. A binary built on a
/// host with a newer glibc would fail to start in the guest.
pub fn check_glibc(guest: &Guest, binaries: &[Binary]) -> Result<(), BinaryError> {
    let linked: Vec<&Binary> = binaries
        .iter()
        .filter(|binary| binary.target == GNU_TARGET)
        .collect();
    if linked.is_empty() {
        return Ok(());
    }
    let ldd = guest
        .run("ldd --version")
        .map_err(BinaryError::GuestGlibc)?;
    let guest_version = ldd_version(&ldd).ok_or_else(|| BinaryError::NoGuestGlibc(ldd.clone()))?;
    for binary in linked {
        let package = binary.package;
        let output = Command::new("readelf")
            .args(["--version-info", "--wide"])
            .arg(&binary.local)
            .stdin(Stdio::null())
            .output()
            .map_err(|error| BinaryError::Readelf { package, error })?;
        if !output.status.success() {
            return Err(BinaryError::ReadelfFailed {
                package,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            });
        }
        if let Some(needs) = needed_glibc(&String::from_utf8_lossy(&output.stdout))
            && needs > guest_version
        {
            return Err(BinaryError::GlibcTooNew {
                package,
                needs,
                guest: guest_version,
            });
        }
        println!("vm: {package} runs with the guest's glibc {guest_version}");
    }
    Ok(())
}

/// A glibc version, such as 2.41. Versions compare by number, so 2.9 comes before 2.34.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GlibcVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

impl std::fmt::Display for GlibcVersion {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)?;
        if self.patch != 0 {
            write!(formatter, ".{}", self.patch)?;
        }
        Ok(())
    }
}

impl GlibcVersion {
    /// Reads `2.41` or `2.2.5`.
    fn parse(text: &str) -> Option<GlibcVersion> {
        let mut parts = text.split('.').map(str::parse::<u32>);
        let major = parts.next()?.ok()?;
        let minor = parts.next()?.ok()?;
        let patch = match parts.next() {
            None => 0,
            Some(patch) => patch.ok()?,
        };
        if parts.next().is_some() {
            return None;
        }
        Some(GlibcVersion {
            major,
            minor,
            patch,
        })
    }
}

/// The newest `GLIBC_` symbol version that `readelf --version-info` lists, or none when the
/// binary needs no glibc symbol version.
fn needed_glibc(readelf: &str) -> Option<GlibcVersion> {
    readelf
        .split(|character: char| !(character.is_ascii_alphanumeric() || "._".contains(character)))
        .filter_map(|word| word.strip_prefix("GLIBC_"))
        .filter_map(GlibcVersion::parse)
        .max()
}

/// The glibc version on the first line of `ldd --version`, such as `ldd (GNU libc) 2.41`.
fn ldd_version(ldd: &str) -> Option<GlibcVersion> {
    ldd.lines()
        .next()?
        .split_whitespace()
        .next_back()
        .and_then(GlibcVersion::parse)
}

/// The executable path in one cargo JSON message, when the message reports the binary of `package`.
fn artifact_executable(line: &str, package: &str) -> Option<PathBuf> {
    let message: Value = serde_json::from_str(line).ok()?;
    if message.get("reason")?.as_str()? != "compiler-artifact" {
        return None;
    }
    if message.get("target")?.get("name")?.as_str()? != package {
        return None;
    }
    message.get("executable")?.as_str().map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    const READELF: &str = "\
Version needs section '.gnu.version_r' contains 2 entries:
 Addr: 0x0000000000001234  Offset: 0x001234  Link: 7 (.dynstr)
  000000: Version: 1  File: ld-linux-x86-64.so.2  Cnt: 1
  0x0010:   Name: GLIBC_2.3  Flags: none  Version: 9
  0x0020: Version: 1  File: libc.so.6  Cnt: 4
  0x0030:   Name: GLIBC_2.9  Flags: none  Version: 8
  0x0040:   Name: GLIBC_2.34  Flags: none  Version: 7
  0x0050:   Name: GLIBC_2.2.5  Flags: none  Version: 6
  0x0060:   Name: GLIBC_PRIVATE  Flags: none  Version: 5
";

    fn version(major: u32, minor: u32) -> GlibcVersion {
        GlibcVersion {
            major,
            minor,
            patch: 0,
        }
    }

    #[test]
    fn the_needed_glibc_is_the_newest_version_by_number() {
        // 2.34 is newer than 2.9, which a text comparison would get wrong.
        assert_eq!(needed_glibc(READELF), Some(version(2, 34)));
    }

    #[test]
    fn a_binary_without_glibc_versions_needs_none() {
        assert_eq!(
            needed_glibc("No version information found in this file.\n"),
            None
        );
    }

    #[test]
    fn the_guest_glibc_comes_from_the_first_line_of_ldd() {
        let ldd = "ldd (GNU libc) 2.41\nCopyright (C) 2025 Free Software Foundation, Inc.\n";
        assert_eq!(ldd_version(ldd), Some(version(2, 41)));
    }
}
