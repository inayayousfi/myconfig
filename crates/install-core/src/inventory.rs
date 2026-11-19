//! The disks an interface can offer and, for each one, which disk choices fit it and why the
//! others do not. It only reads the machine.
use std::path::Path;

use crate::{
    Choice, Error, hibernation,
    host::{Command, Host},
    install::{checks::LIVE_MEDIUM, machine::Machine},
    parse::{self, ListedDevice},
    sink::Silent,
    sizes::{self, BesideWindows, GIB},
    windows,
};

/// The label prefix of the InayOS USB stick's filesystem, which stays listed as a disk when
/// archiso copied it to memory and unmounted it.
const LIVE_LABEL_PREFIX: &str = "INAYOS_";
/// The lsblk columns the inventory reads, which `parse::lsblk_inventory` parses.
pub(crate) const COLUMNS: &str =
    "NAME,PATH,TYPE,MODEL,SIZE,START,LOG-SEC,FSTYPE,LABEL,PARTTYPE,MOUNTPOINTS";
/// Where the inventory mounts an EFI partition read-only for a moment, to look for the
/// Windows Boot Manager.
const ESP_LOOK: &str = "/run/inayos-installer/esp";
const SYSTEM_LABEL: &str = "inayos-system";
pub(crate) const HOME_LABEL: &str = "inayos-home";
const TPM_VERSION: &str = "/sys/class/tpm/tpm0/tpm_version_major";
/// The firmware variable that says whether the firmware is in Secure Boot Setup Mode.
const SETUP_MODE: &str = "/sys/firmware/efi/efivars/SetupMode-8be4df61-93ca-11d2-aa0d-00e098032b8c";

/// One whole disk the install can use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskInfo {
    /// The device path, such as `/dev/nvme0n1`.
    pub path: String,
    /// The model the disk reports; empty when it reports none.
    pub model: String,
    pub size_bytes: u64,
    /// One offer per choice, in the order of `Choice::ALL`.
    pub offers: Vec<Offer>,
}

/// Whether one disk choice fits the disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub choice: Choice,
    /// The partitions the answers name for this choice, or in plain words why the choice
    /// does not fit the disk.
    pub available: Result<Partitions, String>,
    /// The system sizes the answers may give, for an available choice that takes a size.
    pub system_size: Option<SizeLimits>,
    /// How the Windows partition shrinks, for an available `beside-windows`.
    pub windows: Option<WindowsShrink>,
}

/// The Windows partition that installing beside Windows shrinks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsShrink {
    pub partition: String,
    /// The size of the partition now.
    pub size_bytes: u64,
    room: BesideWindows,
    swap_gib: u64,
}

impl WindowsShrink {
    /// The Windows partition at `partition`, laid out as `room`, beside which home keeps
    /// room for a swap file of `swap_gib`.
    pub fn new(partition: impl Into<String>, room: BesideWindows, swap_gib: u64) -> Self {
        WindowsShrink {
            partition: partition.into(),
            size_bytes: room.windows.sectors() * room.sector_size,
            room,
            swap_gib,
        }
    }

    /// The size of the Windows partition after an install with a system of `system_gib`;
    /// None when that system does not fit.
    pub fn size_after(&self, system_gib: u64) -> Option<u64> {
        sizes::beside_windows_layout(self.room, system_gib, self.swap_gib)
            .ok()
            .map(|layout| layout.windows_bytes(self.room.sector_size))
    }
}

/// The existing partitions a choice reuses. A wiped disk has none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Partitions {
    pub efi: Option<String>,
    pub system: Option<String>,
    pub home: Option<String>,
}

/// The smallest and largest system size in GiB that the install accepts on this disk, both
/// included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SizeLimits {
    pub min_gib: u64,
    pub max_gib: u64,
}

impl SizeLimits {
    /// `gib` moved into the limits.
    pub fn clamp(self, gib: u64) -> u64 {
        gib.clamp(self.min_gib, self.max_gib)
    }
}

/// Every whole disk of the machine except the USB stick the installer runs from, with its
/// offers. Fails when lsblk fails or prints something unexpected.
pub fn disks(host: &dyn Host) -> Result<Vec<DiskInfo>, Error> {
    let m = Machine {
        host,
        sink: &Silent,
    };
    let lsblk = Command::query("lsblk").args(["--json", "--bytes", "--output", COLUMNS]);
    let shown = lsblk.shown();
    let devices =
        parse::lsblk_inventory(&m.query(lsblk)?).map_err(|problem| Error::UnexpectedOutput {
            command: shown,
            problem,
        })?;
    // The firmware variable does not say which disk the hibernated system is on.
    let firmware_hibernated = hibernation::firmware_hibernated(&m);
    // An unreadable memory size leaves no room aside here; the install's checks read it
    // again and stop on the error.
    let swap_gib = hibernation::swap_gib(&m).unwrap_or(0);
    Ok(devices
        .iter()
        .filter(|device| is_installable(device))
        .map(|disk| DiskInfo {
            path: disk.path.clone(),
            model: disk.model.clone(),
            size_bytes: disk.size_bytes,
            offers: offers(&m, disk, firmware_hibernated, swap_gib),
        })
        .collect())
}

/// Whether the machine has a TPM 2.0 chip, which encryption needs.
pub fn has_tpm2(host: &dyn Host) -> bool {
    host.read(Path::new(TPM_VERSION))
        .is_ok_and(|version| version.trim() == "2")
}

/// Whether the firmware is in Setup Mode, where it holds no platform key and accepts the
/// keys that the secure-boot module enrolls. efivarfs gives the variable's 4 attribute
/// bytes, then its value byte, which is 1 in Setup Mode. Fails when the variable cannot be
/// read, such as on a machine that did not start in UEFI mode.
pub fn secure_boot_setup_mode(host: &dyn Host) -> Result<bool, Error> {
    let variable = host
        .read(Path::new(SETUP_MODE))
        .map_err(|error| Error::Io {
            action: "read",
            path: SETUP_MODE.into(),
            error,
        })?;
    Ok(variable.as_bytes().last() == Some(&1))
}

/// A whole disk with room on it that is not the live medium. zram devices count as disks
/// for lsblk.
fn is_installable(device: &ListedDevice) -> bool {
    let name = device.path.rsplit('/').next().unwrap_or_default();
    device.kind == "disk"
        && !name.starts_with("zram")
        && device.size_bytes > 0
        && device.sector_size > 0
        && !device.all().iter().any(|part| {
            part.mountpoints.iter().any(|point| point == LIVE_MEDIUM)
                || part.label.starts_with(LIVE_LABEL_PREFIX)
        })
}

impl ListedDevice {
    fn all(&self) -> Vec<&ListedDevice> {
        let mut found = vec![self];
        for child in &self.children {
            found.extend(child.all());
        }
        found
    }
}

/// Whether an unmounted InayOS home partition of `disk` holds a swap file that a system
/// hibernated into. A partition that cannot be read counts as not hibernated here; the
/// install's checks read it again and stop on the error.
fn holds_hibernated_swap_file(m: &Machine, disk: &ListedDevice) -> bool {
    disk.all().into_iter().any(|part| {
        part.label == HOME_LABEL
            && part.fstype == "ext4"
            && part.mountpoints.is_empty()
            && hibernation::swap_file_hibernated(m, &part.path).unwrap_or(false)
    })
}

/// The offers of `disk`. Every home they leave has room for the hibernation swap file of
/// `swap_gib`.
fn offers(
    m: &Machine,
    disk: &ListedDevice,
    firmware_hibernated: bool,
    swap_gib: u64,
) -> Vec<Offer> {
    if firmware_hibernated || holds_hibernated_swap_file(m, disk) {
        return Choice::ALL
            .into_iter()
            .map(|choice| unavailable(choice, hibernation::HIBERNATED))
            .collect();
    }
    let in_use = disk.all().into_iter().find_map(|part| {
        part.mountpoints
            .first()
            .map(|point| format!("{} is in use at {point}", part.path))
    });
    Choice::ALL
        .into_iter()
        .map(|choice| match &in_use {
            Some(reason) => unavailable(choice, reason.clone()),
            _ => offer(m, disk, choice, swap_gib),
        })
        .collect()
}

fn unavailable(choice: Choice, reason: impl Into<String>) -> Offer {
    Offer {
        choice,
        available: Err(reason.into()),
        system_size: None,
        windows: None,
    }
}

fn offer(m: &Machine, disk: &ListedDevice, choice: Choice, swap_gib: u64) -> Offer {
    match choice {
        Choice::WipeDisk => wipe(disk, swap_gib),
        Choice::ReinstallKeepHome => reinstall(m, disk, swap_gib),
        Choice::ShrinkExt4KeepHome => shrink(m, disk, swap_gib),
        Choice::BesideWindows => beside_windows(m, disk, swap_gib),
    }
}

/// The system sizes that fit beside Windows in `room`, or why none does.
pub(crate) fn beside_windows_limits(
    room: BesideWindows,
    swap_gib: u64,
) -> Result<SizeLimits, String> {
    let fits = |gib| sizes::beside_windows_layout(room, gib, swap_gib).is_ok();
    let room_bytes = (room.room_last + 1 - room.windows.first) * room.sector_size;
    let max = sizes::MAX_SYSTEM_GIB.min(room_bytes / GIB);
    match sizes::largest_fitting(sizes::MIN_SYSTEM_GIB, max, fits) {
        Some(max_gib) => Ok(SizeLimits {
            min_gib: sizes::MIN_SYSTEM_GIB,
            max_gib,
        }),
        None => Err(
            sizes::beside_windows_layout(room, sizes::MIN_SYSTEM_GIB, swap_gib)
                .err()
                .map_or_else(String::new, |no_room| no_room.to_string()),
        ),
    }
}

fn beside_windows(m: &Machine, disk: &ListedDevice, swap_gib: u64) -> Offer {
    let choice = Choice::BesideWindows;
    let found = match windows::examine(m, disk, Path::new(ESP_LOOK)) {
        Ok(Ok(found)) => found,
        Ok(Err(reason)) => return unavailable(choice, reason),
        Err(error) => return unavailable(choice, format!("Windows cannot be read: {error}")),
    };
    match beside_windows_limits(found.room, swap_gib) {
        Ok(limits) => Offer {
            choice,
            available: Ok(Partitions {
                efi: Some(found.esp),
                system: None,
                home: None,
            }),
            system_size: Some(limits),
            windows: Some(WindowsShrink::new(found.windows, found.room, swap_gib)),
        },
        Err(reason) => unavailable(choice, reason),
    }
}

fn wipe(disk: &ListedDevice, swap_gib: u64) -> Offer {
    let sectors = disk.size_bytes / disk.sector_size;
    let fits = |gib| sizes::wipe_layout(sectors, disk.sector_size, gib, swap_gib).is_ok();
    let max = sizes::MAX_SYSTEM_GIB.min(disk.size_bytes / GIB);
    match sizes::largest_fitting(sizes::MIN_SYSTEM_GIB, max, fits) {
        Some(max_gib) => Offer {
            choice: Choice::WipeDisk,
            available: Ok(Partitions::default()),
            system_size: Some(SizeLimits {
                min_gib: sizes::MIN_SYSTEM_GIB,
                max_gib,
            }),
            windows: None,
        },
        None => {
            let needed =
                sizes::wipe_layout(sectors, disk.sector_size, sizes::MIN_SYSTEM_GIB, swap_gib)
                    .err()
                    .map_or(0, |too_small| too_small.needed_bytes);
            unavailable(
                Choice::WipeDisk,
                format!(
                    "the disk is too small: InayOS needs at least {:.1} GiB",
                    needed as f64 / GIB as f64
                ),
            )
        }
    }
}

fn efi_partition(disk: &ListedDevice) -> Option<&ListedDevice> {
    disk.children
        .iter()
        .find(|part| part.parttype == windows::EFI_TYPE && part.fstype == "vfat")
}

/// The GiB free on the kept home `partition` for the swap file, counting the earlier
/// system's swap file, which the install deletes. None when the filesystem cannot be read:
/// the install's checks read it again and stop on the error.
fn swap_room_gib(m: &Machine, partition: &str) -> Option<f64> {
    let dump = Command::query("dumpe2fs").args(["-h", partition]);
    let usage = parse::ext4_usage(&m.query(dump).ok()?).ok()?;
    let old = hibernation::swap_file_bytes(m, partition).ok()?;
    Some((usage.free_bytes + old) as f64 / GIB as f64)
}

fn reinstall(m: &Machine, disk: &ListedDevice, swap_gib: u64) -> Offer {
    let labeled = |label| disk.children.iter().find(|part| part.label == label);
    let available = match (labeled(SYSTEM_LABEL), labeled(HOME_LABEL)) {
        (Some(system), Some(home)) => match efi_partition(disk) {
            None => Err("the disk has no EFI partition".to_owned()),
            Some(_) if home.fstype == "crypto_LUKS" => Err(
                "the InayOS on this disk is encrypted, and reinstalling keeps only a home \
                 partition that is not"
                    .to_owned(),
            ),
            Some(_) if home.fstype != "ext4" => Err(format!(
                "the InayOS home partition {} does not hold ext4",
                home.path
            )),
            Some(_) if swap_room_gib(m, &home.path).is_some_and(|free| free < swap_gib as f64) => {
                Err(format!(
                    "the InayOS home partition {} has no room for the {swap_gib} GiB \
                     hibernation swap file, the size of the memory",
                    home.path
                ))
            }
            Some(efi) => Ok(Partitions {
                efi: Some(efi.path.clone()),
                system: Some(system.path.clone()),
                home: Some(home.path.clone()),
            }),
        },
        _ => Err("the disk holds no earlier InayOS".to_owned()),
    };
    Offer {
        choice: Choice::ReinstallKeepHome,
        available,
        system_size: None,
        windows: None,
    }
}

fn shrink(m: &Machine, disk: &ListedDevice, swap_gib: u64) -> Offer {
    let choice = Choice::ShrinkExt4KeepHome;
    let Some(home) = disk
        .children
        .iter()
        .filter(|part| part.fstype == "ext4")
        .max_by_key(|part| part.start)
    else {
        return unavailable(choice, "the disk has no ext4 partition");
    };
    if home.label == HOME_LABEL {
        return unavailable(
            choice,
            "the disk holds InayOS; reinstalling keeps its home partition",
        );
    }
    if disk.children.iter().any(|part| part.start > home.start) {
        return unavailable(
            choice,
            format!(
                "a partition follows {}, so the system has no room after it",
                home.path
            ),
        );
    }
    let Some(efi) = efi_partition(disk) else {
        return unavailable(choice, "the disk has no EFI partition");
    };
    let dump = Command::query("dumpe2fs").args(["-h", home.path.as_str()]);
    let Some(usage) = m
        .query(dump)
        .ok()
        .and_then(|text| parse::ext4_usage(&text).ok())
    else {
        return unavailable(
            choice,
            format!("the ext4 filesystem on {} cannot be read", home.path),
        );
    };
    let sector = disk.sector_size;
    let span = sizes::Span {
        first: home.start,
        last: home.start + home.size_bytes / sector - 1,
    };
    let needed = usage.used_bytes + swap_gib * GIB;
    let fits = |gib| sizes::shrink_plan(span, sector, usage.block_size, needed, gib).is_ok();
    let max = sizes::MAX_SYSTEM_GIB.min(home.size_bytes / GIB);
    match sizes::largest_fitting(sizes::MIN_SYSTEM_GIB, max, fits) {
        Some(max_gib) => Offer {
            choice,
            available: Ok(Partitions {
                efi: Some(efi.path.clone()),
                system: None,
                home: Some(home.path.clone()),
            }),
            system_size: Some(SizeLimits {
                min_gib: sizes::MIN_SYSTEM_GIB,
                max_gib,
            }),
            windows: None,
        },
        None => unavailable(
            choice,
            format!(
                "the files on {} and the {swap_gib} GiB hibernation swap file leave less than \
                 {} GiB to free",
                home.path,
                sizes::MIN_SYSTEM_GIB
            ),
        ),
    }
}

#[cfg(test)]
mod tests;
