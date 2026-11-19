//! Whether InayOS can go beside the Windows on a disk, and the facts the layout needs. The
//! inventory and the install's checks both ask here, so the screens and the install refuse
//! with the same words. It only reads: the EFI partition is mounted read-only and unmounted
//! before it returns.
use std::path::Path;

use crate::{
    Error,
    disk_names::partition_number,
    host::Command,
    install::machine::Machine,
    parse::{self, ListedDevice},
    sizes::{self, BesideWindows, MIB},
};

/// The GPT type of an EFI system partition.
pub(crate) const EFI_TYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
/// The Windows Boot Manager, which shows that Windows boots from the EFI partition.
const BOOT_MANAGER: &str = "EFI/Microsoft/Boot/bootmgfw.efi";
/// The free space the EFI partition needs for rEFInd, its drivers, icons and theme, and the
/// signed copies the secure-boot module adds, with room to spare.
pub const ESP_FREE_MIB: u64 = 50;
/// The `ntfs-3g.probe` exit codes for a filesystem that Windows hibernated, which Fast
/// Startup does at every shutdown, and for one that Windows did not close cleanly.
const PROBE_HIBERNATED: i32 = 14;
const PROBE_UNCLEAN: i32 = 15;

/// What to do when BitLocker encrypts the Windows partition.
pub const BITLOCKER: &str = "Turn off BitLocker in Windows (Settings, Privacy and security, \
                             Device encryption, or Control Panel, BitLocker Drive Encryption), \
                             wait until decryption finishes, then start the installer again";
/// What to do when Windows did not shut down fully.
pub const FAST_STARTUP: &str = "Turn off Fast Startup in Windows (Control Panel, Power \
                                Options, Choose what the power buttons do, untick Turn on fast \
                                startup), then shut Windows down with Shut down, not Restart, \
                                and start the installer again";

/// The facts of a disk that InayOS can go beside Windows on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WindowsDisk {
    /// The EFI partition, which the install reuses without formatting it.
    pub esp: String,
    /// The Windows partition.
    pub windows: String,
    pub windows_number: u32,
    /// The partition numbers after every existing one, for the system and home.
    pub system_number: u32,
    pub home_number: u32,
    /// The Windows partition, the free space after it and the size of its files.
    pub room: BesideWindows,
    /// The size of the Windows filesystem now.
    pub volume_bytes: u64,
}

/// Examines `disk`, a device tree as the inventory's lsblk prints it, mounting its EFI
/// partition read-only at `esp_mount` for a moment. Gives the facts, or in plain words why
/// InayOS cannot go beside the Windows on it. Fails when a tool cannot run or prints
/// something unexpected.
pub(crate) fn examine(
    m: &Machine,
    disk: &ListedDevice,
    esp_mount: &Path,
) -> Result<Result<WindowsDisk, String>, Error> {
    let Some(esp) = disk
        .children
        .iter()
        .find(|part| part.parttype == EFI_TYPE && part.fstype == "vfat")
    else {
        return Ok(Err("the disk has no EFI partition".to_owned()));
    };
    let Some(windows) = disk
        .children
        .iter()
        .filter(|part| part.fstype == "ntfs" || part.fstype == "BitLocker")
        .max_by_key(|part| part.size_bytes)
    else {
        return Ok(Err("the disk has no Windows partition".to_owned()));
    };
    if windows.fstype == "BitLocker" || bitlocker_signature(m, &windows.path)? {
        return Ok(Err(format!(
            "BitLocker encrypts the Windows partition {}. {BITLOCKER}",
            windows.path
        )));
    }
    let esp_free = match look_into_esp(m, &esp.path, esp_mount)? {
        Ok(free) => free,
        Err(reason) => return Ok(Err(reason)),
    };
    if esp_free < ESP_FREE_MIB * MIB {
        return Ok(Err(format!(
            "the EFI partition {} has {} MiB free, and rEFInd needs {ESP_FREE_MIB} MiB",
            esp.path,
            esp_free / MIB
        )));
    }
    let probe = m.run_any(&Command::query("ntfs-3g.probe").args(["--readwrite", &windows.path]))?;
    match probe.code {
        Some(0) => {}
        Some(PROBE_HIBERNATED | PROBE_UNCLEAN) => {
            return Ok(Err(format!(
                "Windows did not shut down fully, so its partition {} cannot change. \
                 {FAST_STARTUP}",
                windows.path
            )));
        }
        _ => {
            return Ok(Err(format!(
                "the Windows partition {} cannot be opened for writing: `ntfs-3g.probe` \
                 ended with {}",
                windows.path,
                probe.status()
            )));
        }
    }
    let info = Command::query("ntfsresize").args([
        "--info",
        "--force",
        "--no-action",
        "--no-progress-bar",
        &windows.path,
    ]);
    let shown = info.shown();
    let sizes = parse::ntfs_sizes(&m.query(info)?).map_err(|problem| Error::UnexpectedOutput {
        command: shown,
        problem,
    })?;
    let sector = disk.sector_size;
    let disk_sectors = disk.size_bytes / sector;
    // lsblk gives partition starts in 512-byte units, whatever the disk's sector size.
    let first_sector = |part: &ListedDevice| part.start * 512 / sector;
    let windows_first = first_sector(windows);
    let room_last = disk
        .children
        .iter()
        .map(first_sector)
        .filter(|first| *first > windows_first)
        .map(|first| first - 1)
        .min()
        .unwrap_or_else(|| sizes::last_usable_sector(disk_sectors, sector));
    let numbers: Vec<u32> = disk
        .children
        .iter()
        .filter_map(|part| partition_number(&disk.path, &part.path))
        .collect();
    let last_number = numbers.iter().copied().max().unwrap_or(0);
    let Some(windows_number) = partition_number(&disk.path, &windows.path) else {
        return Err(Error::MissingPartition {
            partition: windows.path.clone(),
        });
    };
    Ok(Ok(WindowsDisk {
        esp: esp.path.clone(),
        windows: windows.path.clone(),
        windows_number,
        system_number: last_number + 1,
        home_number: last_number + 2,
        room: BesideWindows {
            windows: sizes::Span {
                first: windows_first,
                last: windows_first + windows.size_bytes / sector - 1,
            },
            room_last,
            sector_size: sector,
            windows_files_bytes: sizes.smallest_bytes,
        },
        volume_bytes: sizes.volume_bytes,
    }))
}

/// Whether the partition starts with BitLocker's signature, which blkid may not know.
fn bitlocker_signature(m: &Machine, partition: &str) -> Result<bool, Error> {
    // The bytes are binary data, which stays out of the log.
    let read = m.run_any(
        &Command::query("dd")
            .args([
                format!("if={partition}"),
                "bs=1".to_owned(),
                "skip=3".to_owned(),
                "count=8".to_owned(),
                "status=none".to_owned(),
            ])
            .hidden_output(),
    )?;
    Ok(read.success() && parse::is_bitlocker_signature(&read.stdout))
}

/// Mounts the EFI partition read-only at `point`, and gives its free bytes when it holds the
/// Windows Boot Manager. Unmounts it before returning.
fn look_into_esp(m: &Machine, esp: &str, point: &Path) -> Result<Result<u64, String>, Error> {
    let shown = point.display().to_string();
    m.create_dir(point, 0o755)?;
    m.run(Command::action("mount").args(["-o", "ro", esp, &shown]))?;
    let found = m.exists(&point.join(BOOT_MANAGER));
    let free = Command::query("df").args(["--block-size=1", "--output=avail", &shown]);
    let free_shown = free.shown();
    let free = m.query(free).and_then(|text| {
        parse::df_available(&text).map_err(|problem| Error::UnexpectedOutput {
            command: free_shown,
            problem,
        })
    });
    m.run(Command::action("umount").arg(&shown))?;
    if !found {
        return Ok(Err(format!(
            "the EFI partition {esp} holds no Windows Boot Manager"
        )));
    }
    Ok(Ok(free?))
}
