//! rEFInd, the boot manager, with the Black & Pink theme. On the system that an
//! installer-mode run builds, the module installs rEFInd onto the EFI system partition
//! and writes the kernel options of its Linux entries. A running system keeps the rEFInd
//! its installer set up, and gets only the theme and the managed settings, which include
//! the Windows firmware boot entry.
use std::{
    fs,
    path::{Path, PathBuf},
};

use embedded_dotfiles::REFIND_ASSETS;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    boot_config::{REFIND_LINUX_CONFIG, entry_options},
    support::{read_system_file, with_temporary_directory},
};

pub struct Refind;

impl Refind {
    pub const NAME: &'static str = "refind";
}

const CANDIDATES: [&str; 3] = [
    "/boot/EFI/refind/refind.conf",
    "/boot/efi/EFI/refind/refind.conf",
    "/efi/EFI/refind/refind.conf",
];
/// The theme comes after the managed settings, and the boot guard's file comes last, so
/// its `default_selection` wins. The boot-guard module writes that file.
const BLOCK: &str = "\n# BEGIN INAYOS REFIND\ninclude managed.conf\ninclude themes/black-pink/theme.conf\ninclude inayos-guard.conf\n# END INAYOS REFIND\n";
/// The file of the boot guard's entries, next to `refind.conf`, which BLOCK includes.
pub(crate) const GUARD_CONFIG: &str = "inayos-guard.conf";
const GUARD_CONFIG_PLACEHOLDER: &[u8] =
    b"# The InayOS boot guard writes its rEFInd entries here.\n";
/// The mount points where refind-install looks for the EFI system partition, in its order.
const ESP_MOUNTS: [&str; 3] = ["/efi", "/boot/efi", "/boot"];
/// The label of the firmware boot entry that refind-install adds.
const FIRMWARE_LABEL: &str = "rEFInd Boot Manager";
/// The label Windows gives its firmware boot entry.
const WINDOWS_LABEL: &str = "Windows Boot Manager";
/// Windows Boot Manager on the EFI system partition, relative to the partition's root.
const WINDOWS_LOADER: &str = "EFI/Microsoft/Boot/bootmgfw.efi";
/// Deletes every firmware boot entry with rEFInd's label, and succeeds when none is left.
const DELETE_FIRMWARE_ENTRIES: &str = r#"for number in $(sudo efibootmgr | sed -n 's/^Boot\([0-9A-Fa-f]\{4\}\)[* ] rEFInd Boot Manager.*/\1/p'); do
    sudo efibootmgr --quiet --bootnum "$number" --delete-bootnum || exit
done"#;
const NO_ESP: &str = "no EFI system partition is mounted at /efi, /boot/efi or /boot";
/// efibootmgr reaches the firmware's boot entries through efivarfs, which `arch-chroot`
/// mounts inside the new system when the system around it has it.
const EFIVARS: &str = "/sys/firmware/efi/efivars";

/// The rEFInd configuration file on the EFI system partition, or `None` when rEFInd is
/// not installed there.
pub(crate) fn refind_config(ctx: &Context) -> ModuleResult<Option<PathBuf>> {
    for candidate in CANDIDATES {
        if ctx.succeeds(cmd!(ctx.shell, "sudo test -f {candidate}"))? {
            return Ok(Some(PathBuf::from(candidate)));
        }
    }
    Ok(None)
}

/// The mount point of the EFI system partition, a FAT filesystem at one of `ESP_MOUNTS`.
fn efi_partition(ctx: &Context) -> ModuleResult<&'static str> {
    for mount in ESP_MOUNTS {
        let (mounted, filesystem) = ctx.read_unchecked(cmd!(
            ctx.shell,
            "findmnt --noheadings --output FSTYPE --mountpoint {mount}"
        ))?;
        // refind-install mounts the partition a second time at the same place inside
        // `arch-chroot`, so findmnt can list one line per mount.
        if mounted && filesystem.lines().any(|line| line.trim() == "vfat") {
            return Ok(mount);
        }
    }
    Err(NO_ESP.into())
}

/// The rEFInd driver that reads `filesystem`, which holds /boot, or `None` when /boot is
/// on the FAT filesystem that the firmware reads itself.
fn driver(filesystem: &str) -> ModuleResult<Option<&'static str>> {
    Ok(match filesystem {
        "vfat" => None,
        "btrfs" => Some("btrfs"),
        "ext4" => Some("ext4"),
        "ext2" | "ext3" => Some("ext2"),
        other => {
            return Err(
                format!("rEFInd has no driver for {other}, the filesystem of /boot").into(),
            );
        }
    })
}

fn boot_driver(ctx: &Context) -> ModuleResult<Option<&'static str>> {
    let filesystem = ctx.read(cmd!(
        ctx.shell,
        "findmnt --noheadings --output FSTYPE --target /boot"
    ))?;
    driver(&filesystem)
}

/// The firmware boot entries labelled `wanted` in `listing`, the output of `efibootmgr`,
/// as their numbers and whether they are active. Entry lines read `Boot0003* <label>`, or
/// `Boot0003  <label>` for an inactive entry, and may go on with a tab and the entry's
/// device path.
fn firmware_entries_labelled(listing: &str, wanted: &str) -> Vec<(String, bool)> {
    listing
        .lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("Boot")?;
            let (number, rest) = rest.split_at_checked(4)?;
            let active = rest.starts_with('*');
            let label = rest.strip_prefix(['*', ' '])?.strip_prefix(' ')?;
            (number.chars().all(|c| c.is_ascii_hexdigit()) && label.starts_with(wanted))
                .then(|| (number.to_owned(), active))
        })
        .collect()
}

/// The numbers of the firmware boot entries labelled `wanted` in `listing`, active or not.
fn labelled_entries(listing: &str, wanted: &str) -> Vec<String> {
    firmware_entries_labelled(listing, wanted)
        .into_iter()
        .map(|(number, _)| number)
        .collect()
}

/// The numbers of the active "Windows Boot Manager" entries in `listing`. An inactive one
/// is a leftover, such as the entry of a disk that was wiped, and the firmware may skip
/// it when `BootNext` names it.
fn windows_entries(listing: &str) -> Vec<String> {
    firmware_entries_labelled(listing, WINDOWS_LABEL)
        .into_iter()
        .filter_map(|(number, active)| active.then_some(number))
        .collect()
}

fn refind_entries(listing: &str) -> Vec<String> {
    labelled_entries(listing, FIRMWARE_LABEL)
}

/// Fails unless efivarfs is mounted, so efibootmgr can add rEFInd's firmware boot entry.
fn require_efivars(ctx: &Context) -> ModuleResult {
    let (mounted, filesystem) = ctx.read_unchecked(cmd!(
        ctx.shell,
        "findmnt --noheadings --output FSTYPE --mountpoint {EFIVARS}"
    ))?;
    if !mounted || filesystem != "efivarfs" {
        return Err(format!(
            "efivarfs is not mounted at {EFIVARS}, so rEFInd would get no firmware boot entry"
        )
        .into());
    }
    Ok(())
}

fn firmware_entries(ctx: &Context) -> ModuleResult<Vec<String>> {
    let listing = ctx.read(cmd!(ctx.shell, "sudo efibootmgr"))?;
    Ok(refind_entries(&listing))
}

/// The kernel options that mount the root filesystem of `entry`, the root line of
/// `/etc/fstab` as `findmnt --raw --output SOURCE,FSTYPE,OPTIONS` prints it.
fn root_options(entry: &str) -> ModuleResult<String> {
    let fields: Vec<&str> = entry.split_whitespace().collect();
    let [source, filesystem, options] = fields[..] else {
        return Err(format!("could not read the root entry of /etc/fstab: {entry}").into());
    };
    // A device name such as /dev/sda2 can name another disk after a disk is added.
    if !source.starts_with("UUID=") && !source.starts_with("PARTUUID=") {
        return Err(
            format!("the root entry of /etc/fstab names {source}, not a UUID or PARTUUID").into(),
        );
    }
    let mut kernel = vec![format!("root={source}"), "rw".to_owned()];
    if filesystem == "btrfs"
        && let Some(subvolume) = options
            .split(',')
            .find_map(|option| option.strip_prefix("subvol="))
    {
        kernel.push(format!("rootflags=subvol={subvolume}"));
    }
    Ok(kernel.join(" "))
}

fn target_root_options(ctx: &Context) -> ModuleResult<String> {
    let entry = ctx.read(cmd!(
        ctx.shell,
        "findmnt --fstab --noheadings --raw --output SOURCE,FSTYPE,OPTIONS --mountpoint /"
    ))?;
    root_options(entry.lines().next().unwrap_or_default())
}

/// The boot entries of `refind_linux.conf`, which rEFInd offers for each kernel in /boot.
fn boot_entries(options: &str) -> String {
    format!(
        "\"Boot with standard options\"  \"{options}\"\n\"Boot to single-user mode\"    \"{options} single\"\n"
    )
}

/// Records the folders on the EFI system partition that refind-install is about to
/// create. `remove` deletes rEFInd's own folders, and a new `EFI` folder only when nothing
/// else was put in it since.
fn record_new_folders(ctx: &Context, esp: &Path) -> ModuleResult {
    let efi = esp.join("EFI");
    if !ctx.succeeds(cmd!(ctx.shell, "sudo test -e {efi}"))? {
        ctx.created_directory(&efi, true)?;
    }
    for folder in [efi.join("refind"), efi.join("tools")] {
        if !ctx.succeeds(cmd!(ctx.shell, "sudo test -e {folder}"))? {
            ctx.created(&folder, true)?;
        }
    }
    Ok(())
}

/// The EFI system partition from which `remove` would take out the rEFInd that this
/// module installed, or `None` when the module only added the theme.
fn installed_boot_manager(ctx: &Context) -> Option<&'static str> {
    ESP_MOUNTS
        .into_iter()
        .find(|esp| ctx.was_created(&Path::new(esp).join("EFI/refind")))
}

/// Whether `loader`, a path under the `EFI` folder of the EFI system partition, is a boot
/// manager other than rEFInd that the firmware can start. Windows Boot Manager,
/// systemd-boot and GRUB count when a firmware boot entry in `entries`, the output of
/// `efibootmgr -v`, names them. The fallback `BOOT/BOOTX64.EFI` counts without an entry,
/// because the firmware starts it by itself.
fn starts_another_system(loader: &str, entries: &str) -> bool {
    let lower = loader.to_ascii_lowercase();
    if lower.starts_with("refind/") {
        return false;
    }
    if lower == "boot/bootx64.efi" {
        return true;
    }
    let known = lower == "microsoft/boot/bootmgfw.efi"
        || lower == "systemd/systemd-bootx64.efi"
        || lower.ends_with("/grubx64.efi");
    let firmware_path = format!("\\efi\\{}", lower.replace('/', "\\"));
    known && entries.to_ascii_lowercase().contains(&firmware_path)
}

/// Fails unless the machine keeps a boot manager once rEFInd leaves `esp`, so remove
/// never leaves it unable to start. Only loaders on this EFI system partition count:
/// the check cannot see the files of a boot manager on another disk.
fn require_other_boot_manager(ctx: &Context, esp: &str) -> ModuleResult {
    let sh = ctx.shell;
    let efi = Path::new(esp).join("EFI");
    let loaders = ctx.read(cmd!(
        sh,
        "sudo find {efi} -maxdepth 3 -type f -iname *.efi -printf %P\\n"
    ))?;
    // Without the firmware's boot entries, only the fallback loader can be confirmed.
    let (_, entries) = ctx.read_unchecked(cmd!(sh, "sudo efibootmgr -v"))?;
    let fallback_config = efi.join("BOOT/refind.conf");
    for loader in loaders.lines() {
        if !starts_another_system(loader, &entries) {
            continue;
        }
        if loader.eq_ignore_ascii_case("boot/bootx64.efi") {
            // refind-install puts its configuration next to a fallback copy of rEFInd.
            let copy = efi.join(loader);
            let shipped = "/usr/share/refind/refind_x64.efi";
            if ctx.succeeds(cmd!(sh, "sudo test -e {fallback_config}"))?
                || ctx.succeeds(cmd!(sh, "sudo cmp -s {copy} {shipped}"))?
            {
                continue;
            }
        }
        return Ok(());
    }
    Err(format!(
        "removing refind would leave this machine with no boot manager: {esp} holds no Windows Boot Manager, systemd-boot or GRUB that a firmware boot entry starts, and no fallback EFI/BOOT/BOOTX64.EFI other than rEFInd. Install another boot manager first; nothing was removed"
    )
    .into())
}

/// Installs rEFInd onto the EFI system partition of the system being built, with the
/// driver for the filesystem of /boot and a firmware boot entry.
fn install_boot_manager(ctx: &Context) -> ModuleResult {
    let sh = ctx.shell;
    // Every check comes before the first change, so a failed one leaves the disk as it was.
    let esp = Path::new(efi_partition(ctx)?);
    boot_driver(ctx)?;
    require_efivars(ctx)?;
    let options = target_root_options(ctx)?;
    // The package brings efibootmgr.
    ctx.install_packages(&[Package::Refind])?;
    let entries = firmware_entries(ctx)?;
    // Written first, so refind-install keeps it instead of copying the options of the
    // kernel that runs the installer.
    if read_system_file(ctx, REFIND_LINUX_CONFIG)?.is_none() {
        ctx.write_system_file(
            Path::new(REFIND_LINUX_CONFIG),
            boot_entries(&options).as_bytes(),
            "0644",
        )?;
    }
    record_new_folders(ctx, esp)?;
    if entries.is_empty() {
        ctx.record_undo_command(Path::new("/bin/sh"), &["-c", DELETE_FIRMWARE_ENTRIES])?;
    }
    // refind-install copies rEFInd and the one driver that reads the filesystem of /boot,
    // and adds the firmware boot entry with efibootmgr. `--yes` answers its questions and
    // skips the Shim key enrollment, which would ask for a password.
    ctx.run(cmd!(sh, "sudo refind-install --yes"))?;
    // refind-install reports a failed copy or a failed efibootmgr call only in its
    // output, and exits with success.
    verify_boot_manager(ctx)
}

/// Fails unless the system being built boots through rEFInd into its own root filesystem.
fn verify_boot_manager(ctx: &Context) -> ModuleResult {
    let directory = Path::new(efi_partition(ctx)?).join("EFI/refind");
    let mut files = vec![directory.join("refind_x64.efi")];
    if let Some(driver) = boot_driver(ctx)? {
        files.push(directory.join(format!("drivers_x64/{driver}_x64.efi")));
    }
    for file in files {
        if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {file}"))? {
            return Err(format!("{} is missing", file.display()).into());
        }
    }
    if firmware_entries(ctx)?.is_empty() {
        return Err(format!("the firmware has no \"{FIRMWARE_LABEL}\" boot entry").into());
    }
    let options = target_root_options(ctx)?;
    let config = read_system_file(ctx, REFIND_LINUX_CONFIG)?
        .ok_or_else(|| format!("{REFIND_LINUX_CONFIG} is missing"))?;
    let entries = entry_options(&config);
    let mounts_root = |entry: &String| {
        let entry: Vec<&str> = entry.split_whitespace().collect();
        options
            .split_whitespace()
            .all(|option| entry.contains(&option))
    };
    if entries.is_empty() || !entries.iter().all(mounts_root) {
        return Err(format!(
            "an entry of {REFIND_LINUX_CONFIG} does not boot the root filesystem with {options}"
        )
        .into());
    }
    Ok(())
}

/// The settings that let Windows start only through its own firmware boot entry, which
/// keeps rEFInd out of the boot that BitLocker measures. Each of `windows_entries`, the
/// numbers of the firmware's "Windows Boot Manager" entries, gets a stanza that sets
/// `BootNext` to it and resets the machine. `scanfor` leaves out `firmware`, so no other
/// firmware entry shows: `dont_scan_firmware` can only hide labels, and the firmware's
/// labels differ from machine to machine. `dont_scan_files` adds Windows Boot Manager to
/// the list in `refind.conf`, so rEFInd adds no entry that starts Windows itself. When
/// the EFI system partition holds Windows, `windows_partition` is its partition GUID, and
/// the partition's fallback loader, which Windows makes a copy of its boot manager, is
/// hidden too when a Windows firmware entry exists. rEFInd checks for that copy only
/// against loaders it shows, and the GUID keeps the fallback loaders of other disks, such
/// as a USB stick, in the menu. Without a Windows firmware entry, the fallback loader stays
/// in the menu as the only way into Windows.
fn firmware_settings(windows_entries: &[String], windows_partition: Option<&str>) -> String {
    let mut hidden = vec![WINDOWS_LOADER.to_owned()];
    if let Some(guid) = windows_partition.filter(|_| !windows_entries.is_empty()) {
        hidden.push(format!("{guid}:EFI/BOOT/bootx64.efi"));
    }
    let mut settings = format!(
        "scanfor internal,external,optical,manual\ndont_scan_files +,{}\n",
        hidden.join(",")
    );
    for number in windows_entries {
        let title = if windows_entries.len() == 1 {
            "Windows".to_owned()
        } else {
            format!("Windows (Boot{number})")
        };
        settings.push_str(&format!(
            "\nmenuentry \"{title}\" {{\n    icon EFI/refind/icons/os_win.png\n    firmware_bootnum {number}\n}}\n"
        ));
    }
    settings
}

/// The contents of `managed.conf` in `directory`, rEFInd's folder on the EFI system
/// partition: the shipped global settings, then the firmware settings for this machine.
fn managed_config(ctx: &Context, directory: &Path) -> ModuleResult<String> {
    let sh = ctx.shell;
    let esp = directory
        .parent()
        .and_then(Path::parent)
        .ok_or("rEFInd's folder is not inside an EFI folder")?;
    let listing = ctx.read(cmd!(sh, "sudo efibootmgr"))?;
    let windows_entries = windows_entries(&listing);
    let loader = esp.join(WINDOWS_LOADER);
    let windows_partition = if ctx.succeeds(cmd!(sh, "sudo test -f {loader}"))? {
        let guid = ctx.read(cmd!(
            sh,
            "findmnt --noheadings --output PARTUUID --mountpoint {esp}"
        ))?;
        let guid = guid.lines().next().unwrap_or_default().trim().to_owned();
        if guid.is_empty() {
            return Err(format!(
                "could not read the partition GUID of the EFI system partition at {}",
                esp.display()
            )
            .into());
        }
        Some(guid)
    } else {
        None
    };
    let global = String::from_utf8_lossy(REFIND_ASSETS.refind.global_conf.content);
    Ok(format!(
        "{}\n\n{}",
        global.trim_end(),
        firmware_settings(&windows_entries, windows_partition.as_deref())
    ))
}

pub(crate) fn generate_images(ctx: &Context, directory: &Path) -> ModuleResult {
    let sh = ctx.shell;
    let tool = ctx
        .find_program("magick")
        .or_else(|_| ctx.find_program("convert"))?;
    let pink = "#ff4ead";
    // ImageMagick writes the creation time into a PNG unless told not to, which would make
    // every install rewrite the files with new content.
    let no_dates = "png:exclude-chunks=date,time";
    let banner = directory.join("banner.png");
    let bottom = "rectangle 0,1076 1920,1080";
    ctx.run(cmd!(
        sh,
        "{tool} -size 1920x1080 xc:#000000 -fill {pink} -draw {bottom} -define {no_dates} {banner}"
    ))?;
    for (name, size, rect, stroke) in [
        (
            "selection_big.png",
            "144x144",
            "roundrectangle 2,2 142,142 12,12",
            "3",
        ),
        (
            "selection_small.png",
            "64x64",
            "roundrectangle 1,1 63,63 6,6",
            "2",
        ),
    ] {
        let output = directory.join(name);
        let transparent = "xc:none";
        let pink_fill = "rgba(255,78,173,0.16)";
        ctx.run(cmd!(sh, "{tool} -size {size} {transparent} -fill {pink_fill} -draw {rect} -stroke {pink} -strokewidth {stroke} -fill none -draw {rect} -define {no_dates} {output}"))?;
    }
    Ok(())
}

/// The config with one managed block that includes the theme, replacing the block a rerun finds.
fn updated_config(original: &str) -> String {
    let mut new = String::new();
    let mut managed = false;
    for line in original.lines() {
        match line {
            "# BEGIN INAYOS REFIND" => managed = true,
            "# END INAYOS REFIND" => managed = false,
            _ if !managed => {
                new.push_str(line);
                new.push('\n');
            }
            _ => {}
        }
    }
    // Trailing blank lines are dropped so repeated runs leave the file unchanged.
    let mut new = new.trim_end().to_owned();
    new.push('\n');
    new.push_str(BLOCK);
    new
}

impl Module for Refind {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("terminal-tools")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Imagemagick, Package::Refind],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.on_new_system(|| install_boot_manager(ctx))?;
        let Some(config) = refind_config(ctx)? else {
            return ctx.missing_part("a rEFInd installation", "the rEFInd theme");
        };
        let directory = config
            .parent()
            .ok_or("rEFInd configuration has no parent directory")?;
        let theme = directory.join("themes/black-pink");
        with_temporary_directory("refind", |temporary| {
            generate_images(ctx, temporary)?;
            let theme_conf = REFIND_ASSETS.refind.themes.black_pink.theme_conf.content;
            ctx.write_system_file(&theme.join("theme.conf"), theme_conf, "0644")?;
            for name in ["banner.png", "selection_big.png", "selection_small.png"] {
                ctx.write_system_file(&theme.join(name), &fs::read(temporary.join(name))?, "0644")?;
            }
            let managed = managed_config(ctx, directory)?;
            ctx.write_system_file(&directory.join("managed.conf"), managed.as_bytes(), "0644")?;
            // The block includes the guard's file, so it always exists: without the
            // boot-guard module it holds only this comment, and the guard replaces it.
            let guard = directory.join(GUARD_CONFIG);
            if !ctx.succeeds(cmd!(sh, "sudo test -e {guard}"))? {
                ctx.write_system_file(&guard, GUARD_CONFIG_PLACEHOLDER, "0644")?;
            }
            let original = ctx.read(cmd!(sh, "sudo cat {config}"))?;
            ctx.write_system_file(&config, updated_config(&original).as_bytes(), "0644")
        })
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        ctx.on_new_system(|| verify_boot_manager(ctx))?;
        let Some(config) = refind_config(ctx)? else {
            return ctx.missing_part("a rEFInd installation", "the rEFInd theme check");
        };
        let contents = ctx.read(cmd!(ctx.shell, "sudo cat {config}"))?;
        if !format!("{contents}\n").ends_with(BLOCK) {
            return Err("rEFInd does not include the managed theme block".into());
        }
        let directory = config
            .parent()
            .ok_or("rEFInd configuration has no parent directory")?;
        let managed = directory.join("managed.conf");
        let (_, contents) = ctx.read_unchecked(cmd!(ctx.shell, "sudo cat {managed}"))?;
        if format!("{contents}\n") != managed_config(ctx, directory)? {
            return Err(format!(
                "{} does not hold the managed settings for this machine's firmware boot entries",
                managed.display()
            )
            .into());
        }
        for path in [
            GUARD_CONFIG,
            "themes/black-pink/theme.conf",
            "themes/black-pink/banner.png",
        ] {
            let path = directory.join(path);
            if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {path}"))? {
                return Err(format!("{} is missing", path.display()).into());
            }
        }
        Ok(())
    }

    /// Refuses before any undo when it would take out the machine's only boot manager.
    fn remove(&self, ctx: &Context) -> ModuleResult {
        match installed_boot_manager(ctx) {
            Some(esp) => require_other_boot_manager(ctx, esp),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_managed_block_is_added_once() {
        let original = "timeout 5\n";
        assert_eq!(updated_config(original), format!("timeout 5\n{BLOCK}"));
        assert_eq!(
            updated_config(&updated_config(original)),
            updated_config(original)
        );
    }

    #[test]
    fn only_the_windows_boot_manager_entries_are_read_from_the_firmware() {
        let listing = "BootCurrent: 0003\nBootOrder: 0003,0001,0000,0004\nBoot0000* UiApp\tFvVol(7cb8bdc9)\nBoot0001* Windows Boot Manager\tHD(1,GPT,2f5a)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi\nBoot0002* UEFI PXEv4 (MAC:525400123456)\nBoot0003* rEFInd Boot Manager\nBoot0004  Windows Boot Manager\nBootNext: 0001\n";
        assert_eq!(labelled_entries(listing, WINDOWS_LABEL), ["0001", "0004"]);
        assert_eq!(windows_entries(listing), ["0001"]);
        assert_eq!(refind_entries(listing), ["0003"]);
    }

    #[test]
    fn windows_on_the_partition_without_a_firmware_entry_keeps_its_fallback_loader() {
        let guid = "2f5a0c1e-0000-4000-8000-000000000001";
        assert_eq!(
            firmware_settings(&[], Some(guid)),
            firmware_settings(&[], None)
        );
    }

    #[test]
    fn a_machine_without_windows_gets_no_firmware_entry() {
        assert_eq!(
            firmware_settings(&[], None),
            "scanfor internal,external,optical,manual\ndont_scan_files +,EFI/Microsoft/Boot/bootmgfw.efi\n"
        );
    }

    #[test]
    fn windows_starts_only_through_its_firmware_entry() {
        let guid = "2f5a0c1e-0000-4000-8000-000000000001";
        let settings = firmware_settings(&["0001".to_owned()], Some(guid));
        assert_eq!(
            settings,
            format!(
                "scanfor internal,external,optical,manual\n\
                 dont_scan_files +,EFI/Microsoft/Boot/bootmgfw.efi,{guid}:EFI/BOOT/bootx64.efi\n\
                 \n\
                 menuentry \"Windows\" {{\n    icon EFI/refind/icons/os_win.png\n    firmware_bootnum 0001\n}}\n"
            )
        );
    }

    #[test]
    fn several_windows_entries_get_titles_that_tell_them_apart() {
        let settings = firmware_settings(&["0001".to_owned(), "0004".to_owned()], None);
        for number in ["0001", "0004"] {
            assert!(
                settings.contains(&format!(
                    "menuentry \"Windows (Boot{number})\" {{\n    icon EFI/refind/icons/os_win.png\n    firmware_bootnum {number}\n}}"
                )),
                "{settings}"
            );
        }
    }
}
