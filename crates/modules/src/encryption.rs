//! The boot of an encrypted InayOS: the initramfs opens the LUKS2 volumes with the TPM, and
//! mkinitcpio builds a unified kernel image on the EFI system partition, which rEFInd
//! starts because it cannot read the encrypted root. The InayOS installer creates the
//! volumes and writes `/etc/crypttab.initramfs` and `/etc/kernel/cmdline`; this module
//! builds the boot from them. A user unit reminds at each login that the copy of the
//! recovery key is still in the home folder.
//!
//! Each file is under `dotfiles/assets/encryption`, installed at the same path under `/`.
use std::path::{Path, PathBuf};

use embedded_dotfiles::ENCRYPTION_ASSETS;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    boot_config::{KERNEL_CMDLINE, images_without},
    support::{read_system_bytes, read_system_file, verify_packages},
};

pub struct Encryption;

impl Encryption {
    pub const NAME: &'static str = "encryption";
}

/// ukify, which builds the unified kernel image and which the boot guard uses for the
/// "Previous system" image; the TPM libraries, which the initramfs needs to open the
/// volumes with the TPM; and `notify-send` for the reminder.
const PACKAGES: &[Package] = &[Package::SystemdUkify, Package::Tpm2Tss, Package::Libnotify];
/// The volumes the initramfs opens, which the InayOS installer writes.
const CRYPTTAB: &str = "/etc/crypttab.initramfs";
/// The EFI system partition, where the InayOS installer mounts it.
const ESP: &str = "/efi";
const UKI_FOLDER: &str = "/efi/EFI/Linux";
/// The unified kernel image that the linux preset builds.
pub(crate) const UKI: &str = "/efi/EFI/Linux/inayos-linux.efi";
/// The files that change what mkinitcpio builds, so a change to them rebuilds it.
const INITRAMFS_INPUTS: [&str; 2] = [
    "/etc/mkinitcpio.conf.d/inayos-encryption.conf",
    "/etc/mkinitcpio.d/linux.preset",
];
const REMINDER: &str = "inayos-recovery-key-reminder.service";
/// The program that the `sd-encrypt` hook adds to the initramfs.
const CRYPTSETUP: &str = "usr/lib/systemd/systemd-cryptsetup";
const NO_VOLUMES: &str = "the encrypted volumes that the InayOS installer sets up, with /etc/crypttab.initramfs and /etc/kernel/cmdline,";
const NO_ESP: &str = "an EFI system partition mounted at /efi";

/// Each embedded file with the absolute path it is installed at.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile)> {
    ENCRYPTION_ASSETS
        .files()
        .into_iter()
        .map(|file| (Path::new("/").join(file.path_from_root), file))
        .collect()
}

fn is_current(ctx: &Context, path: &Path, file: &EmbeddedFile) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("an encryption path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(file.content))
}

fn exists(ctx: &Context, path: &str) -> ModuleResult<bool> {
    ctx.succeeds(cmd!(ctx.shell, "sudo test -e {path}"))
}

/// Whether the root is on the encrypted volumes that the InayOS installer set up.
fn encrypted_root(ctx: &Context) -> ModuleResult<bool> {
    Ok(exists(ctx, CRYPTTAB)? && exists(ctx, KERNEL_CMDLINE)?)
}

fn esp_mounted(ctx: &Context) -> ModuleResult<bool> {
    let (mounted, filesystem) = ctx.read_unchecked(cmd!(
        ctx.shell,
        "findmnt --noheadings --output FSTYPE --mountpoint {ESP}"
    ))?;
    Ok(mounted && filesystem.lines().any(|line| line.trim() == "vfat"))
}

/// The mapper names that `crypttab` opens.
fn volume_names(crypttab: &str) -> Vec<&str> {
    crypttab
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_whitespace().next())
        .collect()
}

/// The device that the `root=` option of `cmdline` names.
fn root_device(cmdline: &str) -> Option<&str> {
    cmdline
        .split_whitespace()
        .find_map(|option| option.strip_prefix("root="))
}

/// Fails unless the kernel options mount the root from a volume that the initramfs opens.
fn check_root(ctx: &Context) -> ModuleResult {
    let crypttab = read_system_file(ctx, CRYPTTAB)?.unwrap_or_default();
    let cmdline = read_system_file(ctx, KERNEL_CMDLINE)?.unwrap_or_default();
    let names = volume_names(&crypttab);
    match root_device(&cmdline).and_then(|device| device.strip_prefix("/dev/mapper/")) {
        Some(name) if names.contains(&name) => Ok(()),
        _ => Err(format!(
            "the root= of {KERNEL_CMDLINE} names no volume that {CRYPTTAB} opens ({})",
            names.join(", ")
        )
        .into()),
    }
}

impl Module for Encryption {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        if !encrypted_root(ctx)? {
            return ctx.missing_part(NO_VOLUMES, "the encrypted boot");
        }
        if !esp_mounted(ctx)? {
            return ctx.missing_part(NO_ESP, "the encrypted boot");
        }
        check_root(ctx)?;
        ctx.install_packages(PACKAGES)?;
        let mut inputs_changed = false;
        for (path, file) in files() {
            let input = INITRAMFS_INPUTS
                .iter()
                .any(|input| path == Path::new(input));
            if input && !is_current(ctx, &path, file)? {
                if !inputs_changed {
                    // Recorded before the files, so remove rebuilds once they are back.
                    ctx.record_undo_command(Path::new("sudo"), &["mkinitcpio", "-P"])?;
                }
                inputs_changed = true;
            }
            ctx.write_system_file(&path, file.content, "0644")?;
        }
        if !exists(ctx, UKI_FOLDER)? {
            ctx.created_directory(Path::new(UKI_FOLDER), true)?;
            ctx.run(cmd!(sh, "sudo install -d {UKI_FOLDER}"))?;
        }
        // Read from the boot files rather than from the drop-in, so a run that stopped
        // before the rebuild rebuilds on the next one.
        if inputs_changed || !exists(ctx, UKI)? || !images_without(ctx, CRYPTSETUP)?.is_empty() {
            ctx.run(cmd!(sh, "sudo mkinitcpio -P"))?;
        }
        ctx.enable_service(REMINDER, ServiceScope::User)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if !encrypted_root(ctx)? {
            return ctx.missing_part(NO_VOLUMES, "the encrypted boot check");
        }
        verify_packages(ctx, PACKAGES)?;
        for (path, file) in files() {
            if !is_current(ctx, &path, file)? {
                return Err(format!(
                    "{} does not hold the InayOS encryption settings",
                    path.display()
                )
                .into());
            }
        }
        check_root(ctx)?;
        if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {UKI}"))? {
            return Err(format!(
                "{UKI} is missing, so rEFInd has nothing to start the encrypted system with"
            )
            .into());
        }
        let missing = images_without(ctx, CRYPTSETUP)?;
        if !missing.is_empty() {
            return Err(format!(
                "these initramfs images lack the sd-encrypt hook, so they cannot open the encrypted volumes: {}",
                missing.join(", ")
            )
            .into());
        }
        if !ctx.service_state(REMINDER, ServiceScope::User)?.enabled {
            return Err(format!("{REMINDER} is not enabled").into());
        }
        Ok(())
    }

    /// Refuses while the root is encrypted: without the hook and the preset, the next
    /// kernel update would build no image that opens the volumes.
    fn remove(&self, ctx: &Context) -> ModuleResult {
        if exists(ctx, CRYPTTAB)? {
            return Err(format!(
                "removing encryption while {CRYPTTAB} opens the root would leave the next kernel update without an image that opens it; nothing was removed"
            )
            .into());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_names_a_volume_of_the_crypttab() {
        let crypttab = "# opened in the initramfs\nsystem UUID=1 none tpm2-device=auto,discard\nhome UUID=2 none tpm2-device=auto,discard\n";
        assert_eq!(volume_names(crypttab), ["system", "home"]);
        assert_eq!(
            root_device("root=/dev/mapper/system rootflags=subvol=@ rw quiet"),
            Some("/dev/mapper/system")
        );
        assert_eq!(root_device("quiet"), None);
    }
}
