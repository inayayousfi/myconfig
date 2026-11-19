//! The files rEFInd starts on this machine: its own EFI binaries on the EFI system
//! partition, and each kernel in /boot with its boot images and the kernel options of
//! /boot/refind_linux.conf.
use std::path::{Path, PathBuf};

use crate::{
    Error,
    system::{System, read_text},
};

/// The mount points where the EFI system partition can be, in the order refind-install
/// looks at them.
pub const ESP_MOUNTS: [&str; 3] = ["/efi", "/boot/efi", "/boot"];
pub const BOOT: &str = "/boot";
pub const REFIND_LINUX_CONFIG: &str = "/boot/refind_linux.conf";

/// A kernel rEFInd finds in /boot, and the boot images it starts it with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kernel {
    /// The kernel's file name, such as `vmlinuz-linux`.
    pub file: String,
    pub image: PathBuf,
    /// The boot images of this kernel that exist: `initramfs-<name>.img` and its fallback.
    pub initramfs: Vec<PathBuf>,
    /// The `initramfs-<name>.img` that rEFInd passes by default.
    pub main_initramfs: PathBuf,
}

impl Kernel {
    /// The part of the file name after `vmlinuz-`, such as `linux`.
    pub fn name(&self) -> &str {
        self.file.strip_prefix("vmlinuz-").unwrap_or(&self.file)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BootFiles {
    /// The folder of rEFInd on the EFI system partition, such as `/efi/EFI/refind`.
    pub refind_dir: PathBuf,
    /// rEFInd, its drivers and the EFI tools it shows.
    pub efi_binaries: Vec<PathBuf>,
    pub kernels: Vec<Kernel>,
    /// The kernel options of each entry of /boot/refind_linux.conf, or None when the file
    /// is missing.
    pub options: Option<Vec<String>>,
}

impl BootFiles {
    /// The kernel that rEFInd's normal InayOS entry starts: `vmlinuz-linux`, or the first
    /// kernel in /boot when the machine has another one.
    pub fn normal_kernel(&self) -> Option<&Kernel> {
        self.kernels
            .iter()
            .find(|kernel| kernel.file == "vmlinuz-linux")
            .or_else(|| self.kernels.first())
    }
}

/// The rEFInd folder on the EFI system partition, the first of `ESP_MOUNTS` that holds
/// `EFI/refind/refind.conf`.
pub fn refind_dir(system: &dyn System) -> Result<PathBuf, Error> {
    ESP_MOUNTS
        .iter()
        .map(|mount| Path::new(mount).join("EFI/refind"))
        .find(|folder| system.exists(&folder.join("refind.conf")))
        .ok_or(Error::NoRefind)
}

/// The `.efi` files in `folder`, or none when the folder is missing.
fn efi_files(system: &dyn System, folder: &Path) -> Vec<PathBuf> {
    system
        .list(folder)
        .unwrap_or_default()
        .into_iter()
        .filter(|name| name.to_ascii_lowercase().ends_with(".efi"))
        .map(|name| folder.join(name))
        .collect()
}

pub fn find(system: &dyn System) -> Result<BootFiles, Error> {
    let refind_dir = refind_dir(system)?;
    let mut efi_binaries = vec![refind_dir.join("refind_x64.efi")];
    efi_binaries.extend(efi_files(system, &refind_dir.join("drivers_x64")));
    if let Some(efi) = refind_dir.parent() {
        efi_binaries.extend(efi_files(system, &efi.join("tools")));
    }
    let boot = Path::new(BOOT);
    let names = system.list(boot).map_err(|source| Error::Read {
        path: boot.to_owned(),
        source,
    })?;
    let kernels = names
        .iter()
        .filter(|name| name.starts_with("vmlinuz-"))
        .map(|file| {
            let name = file.trim_start_matches("vmlinuz-");
            let main_initramfs = boot.join(format!("initramfs-{name}.img"));
            let fallback = boot.join(format!("initramfs-{name}-fallback.img"));
            let initramfs = [&main_initramfs, &fallback]
                .into_iter()
                .filter(|path| system.exists(path))
                .cloned()
                .collect();
            Kernel {
                file: file.clone(),
                image: boot.join(file),
                initramfs,
                main_initramfs,
            }
        })
        .collect();
    let options = read_text(system, Path::new(REFIND_LINUX_CONFIG))
        .ok()
        .map(|config| entry_options(&config));
    Ok(BootFiles {
        refind_dir,
        efi_binaries,
        kernels,
        options,
    })
}

/// The kernel options of each entry of `refind_linux.conf`: lines with a quoted title and
/// quoted options, comments left out.
pub fn entry_options(config: &str) -> Vec<String> {
    config
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('"').collect();
            (fields.len() >= 5).then(|| fields[3].to_owned())
        })
        .collect()
}

/// The value of the kernel option `name` in `options`, such as `UUID=1234` for `root`.
pub fn option_value<'a>(options: &'a str, name: &str) -> Option<&'a str> {
    options
        .split_whitespace()
        .find_map(|option| option.strip_prefix(name)?.strip_prefix('='))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_options_reads_each_quoted_entry_and_skips_comments() {
        let config = "# \"a\" \"comment\"\n\"Boot with standard options\"  \"root=UUID=1 rw quiet\"\n\"Boot to single-user mode\"    \"root=UUID=1 rw single\"\n";
        assert_eq!(
            entry_options(config),
            ["root=UUID=1 rw quiet", "root=UUID=1 rw single"]
        );
    }
}
