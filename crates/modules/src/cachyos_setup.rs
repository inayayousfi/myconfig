//! Replaces CachyOS defaults with the tools this setup uses instead, and leaves the boot
//! as a plain Arch installation shows it.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    boot_config::{
        MKINITCPIO_CONFIG, REFIND_LINUX_CONFIG, has_hook, remove_initramfs_hook, without_options,
    },
    support::{edit_system_file, read_system_file},
};

pub struct CachyosSetup;

const INSTALLED: &[Package] = &[
    Package::CachyosKernelManager,
    Package::LinuxCachyos,
    Package::NotoFontsCjk,
];

const UNWANTED: &[Package] = &[
    Package::Konsole,
    Package::Alacritty,
    Package::CachyosHello,
    Package::CachyosZshConfig,
    Package::Vim,
    Package::Fish,
    Package::CachyosFishConfig,
    Package::FishAutopair,
    Package::FishPurePrompt,
    Package::Fisher,
    Package::Firefox,
    Package::FirefoxI18nFr,
    Package::MesloFont,
    Package::CachyosEmeraldKdeTheme,
    Package::CachyosIridescentKde,
    Package::CachyosNordKdeTheme,
    Package::Kate,
    Package::Micro,
    Package::CachyosMicroSettings,
    Package::Nano,
    Package::NanoSyntaxHighlighting,
    Package::Meld,
    Package::Glances,
    Package::Duf,
    Package::Tealdeer,
    Package::Filelight,
    Package::Pavucontrol,
    Package::Kcalc,
    Package::Shelly,
    Package::CachyosPackageinstaller,
    Package::Expac,
    Package::CachyosWallpapers,
    Package::Hwdetect,
    Package::Qtscrcpy,
    Package::Fzf,
    Package::Bat,
    Package::Eza,
    Package::Btop,
    Package::Haruna,
    Package::Mpvqt,
    Package::Mpv,
    Package::Kdeconnect,
    Package::Partitionmanager,
    Package::Gwenview,
    Package::PlasmaBrowserIntegration,
    Package::CachyosKdeSettings,
    Package::CharWhite,
    Package::OsProber,
    Package::Efitools,
    Package::Mtools,
    Package::Pkgfile,
    Package::Hwinfo,
    Package::Pv,
    Package::Plocate,
    Package::SNail,
    Package::AwesomeTerminalFonts,
    Package::CantarellFonts,
    Package::TtfOpensans,
    Package::Ffmpegthumbnailer,
    Package::PythonDefusedxml,
    Package::PythonPackaging,
    Package::Libgsf,
    Package::Fsarchiver,
    Package::NfsUtils,
    Package::Hdparm,
    Package::Sg3Utils,
    Package::Lsscsi,
    Package::Sysfsutils,
    Package::Iwd,
    Package::Netctl,
    Package::Xl2tpd,
    Package::NetworkmanagerOpenvpn,
    Package::Xf86VideoAmdgpu,
];

/// Tools for a disk format or layout, removed only when no block device uses it. The
/// CachyOS installer formats with Btrfs by default and can add LVM or RAID, so each tool
/// is matched against `lsblk` before it goes. A filesystem type matches the FSTYPE
/// column, and a device type the TYPE column.
const DISK_TOOLS: &[(Package, &[&str])] = &[
    (Package::BtrfsProgs, &["btrfs"]),
    (Package::F2fsTools, &["f2fs"]),
    (Package::Jfsutils, &["jfs"]),
    (Package::NilfsUtils, &["nilfs2"]),
    (Package::Xfsprogs, &["xfs"]),
    (Package::Lvm2, &["LVM2_member", "lvm"]),
    (Package::Dmraid, &["dmraid"]),
];

/// The disk tools whose format appears nowhere in `lsblk -rno FSTYPE,TYPE` output. A
/// BIOS RAID member, such as `isw_raid_member`, counts as `dmraid`; a Linux software
/// RAID member belongs to mdadm, which stays.
fn unused_disk_tools(devices: &str) -> Vec<Package> {
    let used: Vec<&str> = devices
        .split_whitespace()
        .map(|field| {
            if field.ends_with("_raid_member") && field != "linux_raid_member" {
                "dmraid"
            } else {
                field
            }
        })
        .collect();
    DISK_TOOLS
        .iter()
        .filter(|(_, formats)| !formats.iter().any(|format| used.contains(format)))
        .map(|(package, _)| *package)
        .collect()
}

fn disk_tools_to_remove(ctx: &Context) -> ModuleResult<Vec<Package>> {
    let devices = ctx.read(cmd!(ctx.shell, "lsblk -rno FSTYPE,TYPE"))?;
    Ok(unused_disk_tools(&devices))
}

/// Removed after the plymouth hook leaves mkinitcpio, so no initramfs build looks for a
/// missing hook.
const SPLASH: &[Package] = &[
    Package::PlymouthKcm,
    Package::CachyosPlymouthBootanimation,
    Package::CachyosPlymouthTheme,
    Package::Plymouth,
];

/// Kernel options that the CachyOS installer adds to a plain Arch boot.
const SPLASH_OPTIONS: &[&str] = &["quiet", "nowatchdog", "splash"];

fn remove_boot_splash(ctx: &Context) -> ModuleResult {
    let hooks_changed = remove_initramfs_hook(ctx, "plymouth")?;
    ctx.uninstall_packages(SPLASH)?;
    if edit_system_file(ctx, REFIND_LINUX_CONFIG, |config| {
        without_options(config, SPLASH_OPTIONS)
    })?
    .is_none()
    {
        ctx.note(&format!(
            "Skipping the kernel options because {REFIND_LINUX_CONFIG} was not found"
        ));
    }
    if hooks_changed {
        ctx.run(cmd!(ctx.shell, "sudo mkinitcpio -P"))?;
    }
    Ok(())
}

impl Module for CachyosSetup {
    fn name(&self) -> &'static str {
        "cachyos-setup"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("plasma-version"), Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: INSTALLED.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(INSTALLED)?;
        ctx.uninstall_packages(UNWANTED)?;
        ctx.uninstall_packages(&disk_tools_to_remove(ctx)?)?;
        remove_boot_splash(ctx)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        crate::support::verify_packages(ctx, INSTALLED)?;
        crate::support::verify_absent(ctx, UNWANTED)?;
        crate::support::verify_absent(ctx, &disk_tools_to_remove(ctx)?)?;
        crate::support::verify_absent(ctx, SPLASH)?;
        let config = read_system_file(ctx, MKINITCPIO_CONFIG)?
            .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?;
        if has_hook(&config, "plymouth") {
            return Err(format!("{MKINITCPIO_CONFIG} still runs the plymouth hook").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Package, unused_disk_tools};

    #[test]
    fn disk_tools_stay_for_the_formats_and_layouts_the_machine_uses() {
        let plain = " disk\nvfat part\next4 part\nswap disk\n";
        assert_eq!(unused_disk_tools(plain).len(), 7);
        let btrfs_on_lvm = " disk\nvfat part\nLVM2_member part\nbtrfs lvm\n";
        let unused = unused_disk_tools(btrfs_on_lvm);
        assert!(!unused.contains(&Package::BtrfsProgs));
        assert!(!unused.contains(&Package::Lvm2));
        assert!(unused.contains(&Package::Xfsprogs));
        let bios_raid = "isw_raid_member disk\nlinux_raid_member disk\n";
        assert!(!unused_disk_tools(bios_raid).contains(&Package::Dmraid));
        assert!(unused_disk_tools("linux_raid_member disk\n").contains(&Package::Dmraid));
    }
}
