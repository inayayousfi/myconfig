//! Collects the state lists of a running guest with the list scripts in `test/vm/lists/`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::os::{GUEST_PASSWORD, GUEST_USER, Os, WINDOWS_TOOL_DIR};
use crate::snapshot::{ParseError, Snapshot};
use crate::ssh::{Guest, SshError};

const LINUX_SCRIPT: &str = include_str!("../lists/linux.sh");
const WINDOWS_SCRIPT: &str = include_str!("../lists/windows.ps1");

/// Where the Linux script goes in the guest: outside every folder the lists cover.
const LINUX_GUEST_PATH: &str = "/tmp/myconfig-vm-lists.sh";

#[derive(Debug, thiserror::Error)]
pub enum CollectError {
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not collect the state lists: {0}")]
    Ssh(#[from] SshError),
    #[error("the state lists are not valid: {0}")]
    Parse(#[from] ParseError),
}

/// Collects the state lists of the guest and saves them as `file`. `scratch` is a host folder
/// for the script copy.
pub fn collect(
    os: Os,
    guest: &Guest,
    scratch: &Path,
    file: &Path,
) -> Result<Snapshot, CollectError> {
    let windows_path = format!(r"{WINDOWS_TOOL_DIR}\lists.ps1");
    let (script, guest_path) = match os {
        Os::Cachyos | Os::Ubuntu => (LINUX_SCRIPT, LINUX_GUEST_PATH.to_owned()),
        // scp takes `/` between the folders of a Windows path.
        Os::Windows => (WINDOWS_SCRIPT, windows_path.replace('\\', "/")),
    };
    let local = scratch.join(match os {
        Os::Windows => "lists.ps1",
        Os::Cachyos | Os::Ubuntu => "lists.sh",
    });
    fs::create_dir_all(scratch).map_err(|error| CollectError::Io(scratch.to_owned(), error))?;
    fs::write(&local, script).map_err(|error| CollectError::Io(local.clone(), error))?;
    if os == Os::Windows {
        guest.run(&format!(
            "if not exist {WINDOWS_TOOL_DIR} mkdir {WINDOWS_TOOL_DIR}"
        ))?;
    }
    guest.copy_to(&local, &guest_path)?;

    let mut snapshot = Snapshot::default();
    match os {
        Os::Cachyos | Os::Ubuntu => {
            let tool = if os == Os::Cachyos { "pacman" } else { "dpkg" };
            snapshot.read(&guest.run(&format!("bash {LINUX_GUEST_PATH} user"))?)?;
            // The system lists read files that only root can read.
            snapshot.read(&guest.run(&format!(
                "printf '%s\\n' '{GUEST_PASSWORD}' | sudo -S -p '' bash {LINUX_GUEST_PATH} system {tool} /home/{GUEST_USER}"
            ))?)?;
        }
        Os::Windows => {
            snapshot.read(&guest.run(&format!(
                "powershell.exe -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File {windows_path}"
            ))?)?;
        }
    }
    fs::write(file, snapshot.to_string())
        .map_err(|error| CollectError::Io(file.to_owned(), error))?;
    Ok(snapshot)
}
