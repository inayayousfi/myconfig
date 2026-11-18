//! Collects the state lists of a running guest with the list scripts in `test/vm/lists/`,
//! and the state lists of the Arch WSL distribution inside the Windows guest.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::binary::ARCH_WSL_GUEST_PATH;
use crate::os::{GUEST_PASSWORD, GUEST_USER, Os, WINDOWS_TOOL_DIR};
use crate::snapshot::{ParseError, Snapshot};
use crate::ssh::{Guest, SshError};

const LINUX_SCRIPT: &str = include_str!("../lists/linux.sh");
const WINDOWS_SCRIPT: &str = include_str!("../lists/windows.ps1");
const ARCH_WSL_HOOK_SCRIPT: &str = include_str!("../lists/arch-wsl-hook.sh");

/// Where the Linux script goes in the guest: outside every folder the lists cover.
const LINUX_GUEST_PATH: &str = "/tmp/inayos-vm-lists.sh";

#[derive(Debug, thiserror::Error)]
pub enum CollectError {
    #[error("could not write {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("could not collect the state lists: {0}")]
    Ssh(#[from] SshError),
    #[error("the state lists are not valid: {0}")]
    Parse(#[from] ParseError),
    #[error(
        "the arch-wsl module did not run the Arch WSL installer hook, so no state lists of the distribution are in {0}"
    )]
    NoArchWslLists(String),
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
        Os::Ubuntu => (LINUX_SCRIPT, LINUX_GUEST_PATH.to_owned()),
        // scp takes `/` between the folders of a Windows path.
        Os::Windows => (WINDOWS_SCRIPT, windows_path.replace('\\', "/")),
    };
    let local = scratch.join(match os {
        Os::Windows => "lists.ps1",
        Os::Ubuntu => "lists.sh",
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
        Os::Ubuntu => {
            let tool = "dpkg";
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
    save(snapshot, file)
}

/// The Arch WSL installer hook in the Windows guest. The cycle names it in
/// `INAYOS_ARCH_WSL_BINARY`, so the arch-wsl module runs it instead of a release.
pub fn arch_wsl_hook() -> String {
    format!(r"{WINDOWS_TOOL_DIR}\arch-wsl-hook.sh")
}

/// Where the hook saves the state lists of the distribution, in the Windows guest.
fn arch_wsl_created() -> String {
    format!(r"{WINDOWS_TOOL_DIR}\arch-wsl-created.txt")
}

/// The Linux list script in the Windows guest, for the Arch WSL distribution.
fn arch_wsl_lists() -> String {
    format!(r"{WINDOWS_TOOL_DIR}\lists.sh")
}

/// Copies the Linux list script and the Arch WSL installer hook into the Windows guest.
/// `scratch` is a host folder for the copies.
pub fn prepare_arch_wsl(guest: &Guest, scratch: &Path) -> Result<(), CollectError> {
    let hook = ARCH_WSL_HOOK_SCRIPT
        .replace("@SAVED@", &in_wsl(&arch_wsl_created()))
        .replace("@LISTS@", &in_wsl(&arch_wsl_lists()))
        .replace("@HOME@", &format!("/home/{GUEST_USER}"))
        .replace(
            "@INSTALLER@",
            &format!("/mnt/c/Users/{GUEST_USER}/{ARCH_WSL_GUEST_PATH}"),
        );
    fs::create_dir_all(scratch).map_err(|error| CollectError::Io(scratch.to_owned(), error))?;
    guest.run(&format!(
        "if not exist {WINDOWS_TOOL_DIR} mkdir {WINDOWS_TOOL_DIR}"
    ))?;
    for (name, text, guest_path) in [
        ("lists.sh", LINUX_SCRIPT.to_owned(), arch_wsl_lists()),
        ("arch-wsl-hook.sh", hook, arch_wsl_hook()),
    ] {
        let local = scratch.join(name);
        fs::write(&local, text).map_err(|error| CollectError::Io(local.clone(), error))?;
        guest.copy_to(&local, &guest_path.replace('\\', "/"))?;
    }
    Ok(())
}

/// Deletes state lists that an earlier cycle's hook left, so that the lists read later come
/// from this cycle.
pub fn forget_arch_wsl_created(guest: &Guest) -> Result<(), CollectError> {
    let created = arch_wsl_created();
    guest.run(&format!("if exist \"{created}\" del \"{created}\""))?;
    Ok(())
}

/// Reads the state lists that the hook saved before the Arch WSL installer last ran, and saves
/// them as `file`.
pub fn arch_wsl_created_lists(guest: &Guest, file: &Path) -> Result<Snapshot, CollectError> {
    let created = arch_wsl_created();
    let text = guest.run(&format!("if exist \"{created}\" type \"{created}\""))?;
    if text.trim().is_empty() {
        return Err(CollectError::NoArchWslLists(created));
    }
    let mut snapshot = Snapshot::default();
    snapshot.read(&text)?;
    save(snapshot, file)
}

/// Collects the state lists of the default WSL distribution of the Windows guest, which the
/// arch-wsl module sets, and saves them as `file`. `prepare_arch_wsl` copies the script first.
pub fn collect_arch_wsl(guest: &Guest, file: &Path) -> Result<Snapshot, CollectError> {
    let script = in_wsl(&arch_wsl_lists());
    let mut snapshot = Snapshot::default();
    snapshot.read(&guest.run(&format!("wsl.exe --exec bash {script} user"))?)?;
    snapshot.read(&guest.run(&format!(
        "wsl.exe -u root --exec bash {script} system pacman /home/{GUEST_USER}"
    ))?)?;
    save(snapshot, file)
}

/// The path inside WSL of a path on the C: drive of the Windows guest.
fn in_wsl(windows_path: &str) -> String {
    let rest = windows_path
        .strip_prefix(r"C:\")
        .expect("the guest paths of the xtask are on C:");
    format!("/mnt/c/{}", rest.replace('\\', "/"))
}

fn save(snapshot: Snapshot, file: &Path) -> Result<Snapshot, CollectError> {
    fs::write(file, snapshot.to_string())
        .map_err(|error| CollectError::Io(file.to_owned(), error))?;
    Ok(snapshot)
}
