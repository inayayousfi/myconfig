//! The fresh installer binaries: built from the working tree and copied into a guest.

use std::env;
use std::ffi::OsString;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use serde_json::Value;

use crate::os::{GUEST_USER, Os};
use crate::ssh::{Guest, SshError};

const LINUX_TARGET: &str = "x86_64-unknown-linux-musl";
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
}

/// One installer to build, and where it goes in the guest.
#[derive(Clone, Copy, Debug)]
struct Recipe {
    package: &'static str,
    windows: bool,
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
    /// Relative to the guest user's home folder, with `/` between folders.
    pub guest_path: &'static str,
    pub launch: Launch,
}

/// Where the Windows guest keeps the Arch WSL installer, relative to the user's home folder.
const ARCH_WSL_GUEST_PATH: &str = "myconfig-arch-wsl";

impl Binary {
    /// The command line that runs this installer with `arguments` in the guest's shell.
    pub fn command(&self, arguments: &str) -> String {
        match self.launch {
            Launch::Linux => format!("~/{} {arguments}", self.guest_path),
            Launch::Windows => {
                let path = self.guest_path.replace('/', "\\");
                // The Arch WSL module installs this fresh Linux installer instead of a release.
                format!(
                    "set \"MYCONFIG_ARCH_WSL_BINARY=%USERPROFILE%\\{ARCH_WSL_GUEST_PATH}\" && \"%USERPROFILE%\\{path}\" {arguments}"
                )
            }
            Launch::Wsl => {
                // A copy inside the distribution runs whatever the mount options of C: are.
                // MYCONFIG_WINDOWS_SSH is what the Arch WSL module passes to this installer.
                let copy = "/tmp/myconfig-arch-wsl";
                format!(
                    "wsl.exe --exec bash -c \"install -m 0755 /mnt/c/Users/{GUEST_USER}/{} {copy} && \
                     exec env MYCONFIG_WINDOWS_SSH=/mnt/c/Windows/System32/OpenSSH/ssh.exe {copy} {arguments}\"",
                    self.guest_path
                )
            }
        }
    }
}

/// The installers each guest runs. Linux guests keep the binary where `install.sh` puts it,
/// and Windows keeps it where `install.ps1` puts it. A Windows guest also receives the Arch
/// WSL installer, which runs inside WSL.
fn recipes(os: Os) -> &'static [Recipe] {
    match os {
        Os::Cachyos => &[Recipe {
            package: "myconfig-cachyos",
            windows: false,
            guest_path: ".local/bin/myconfig",
            launch: Launch::Linux,
        }],
        Os::Ubuntu => &[Recipe {
            package: "myconfig-ubuntu-devbox",
            windows: false,
            guest_path: ".local/bin/myconfig",
            launch: Launch::Linux,
        }],
        Os::Windows => &[
            Recipe {
                package: "myconfig-windows",
                windows: true,
                guest_path: "AppData/Local/myconfig/myconfig.exe",
                launch: Launch::Windows,
            },
            Recipe {
                package: "myconfig-arch-wsl",
                windows: false,
                guest_path: ARCH_WSL_GUEST_PATH,
                launch: Launch::Wsl,
            },
        ],
    }
}

/// The Rust targets that the installers of `os` build for.
pub fn targets(os: Os) -> Vec<&'static str> {
    let mut targets: Vec<&'static str> = recipes(os).iter().map(Recipe::target).collect();
    targets.sort_unstable();
    targets.dedup();
    targets
}

/// Whether the installers of `os` build with `cargo xwin`.
pub fn needs_xwin(os: Os) -> bool {
    recipes(os).iter().any(|recipe| recipe.windows)
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
                Os::Cachyos | Os::Ubuntu => format!("mkdir -p '{folder}'"),
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
    fn target(&self) -> &'static str {
        if self.windows {
            WINDOWS_TARGET
        } else {
            LINUX_TARGET
        }
    }
}

fn build_one(recipe: &Recipe, repository: &Path) -> Result<Binary, BinaryError> {
    let package = recipe.package;
    let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(cargo);
    if recipe.windows {
        command.arg("xwin");
    }
    command
        .args([
            "build",
            "--release",
            "--locked",
            "--target",
            recipe.target(),
        ])
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
        guest_path: recipe.guest_path,
        launch: recipe.launch,
    })
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
