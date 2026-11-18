//! The disks an interface can offer and, for each one, which disk choices fit it and why the
//! others do not. It only reads the machine.
use std::path::Path;

use crate::{
    Choice, Error,
    host::{Command, Host},
    install::{checks::LIVE_MEDIUM, machine::Machine},
    parse::{self, ListedDevice},
    sink::Silent,
    sizes::{self, GIB},
};

/// The label prefix of the InayOS USB stick's filesystem, which stays listed as a disk when
/// archiso copied it to memory and unmounted it.
const LIVE_LABEL_PREFIX: &str = "INAYOS_";
/// The GPT type of an EFI system partition.
const EFI_TYPE: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
const SYSTEM_LABEL: &str = "inayos-system";
const HOME_LABEL: &str = "inayos-home";
const TPM_VERSION: &str = "/sys/class/tpm/tpm0/tpm_version_major";

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
    let lsblk = Command::query("lsblk").args([
        "--json",
        "--bytes",
        "--output",
        "NAME,PATH,TYPE,MODEL,SIZE,START,LOG-SEC,FSTYPE,LABEL,PARTTYPE,MOUNTPOINTS",
    ]);
    let shown = lsblk.shown();
    let devices =
        parse::lsblk_inventory(&m.query(lsblk)?).map_err(|problem| Error::UnexpectedOutput {
            command: shown,
            problem,
        })?;
    Ok(devices
        .iter()
        .filter(|device| is_installable(device))
        .map(|disk| DiskInfo {
            path: disk.path.clone(),
            model: disk.model.clone(),
            size_bytes: disk.size_bytes,
            offers: offers(&m, disk),
        })
        .collect())
}

/// Whether the machine has a TPM 2.0 chip, which encryption needs.
pub fn has_tpm2(host: &dyn Host) -> bool {
    host.read(Path::new(TPM_VERSION))
        .is_ok_and(|version| version.trim() == "2")
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

fn offers(m: &Machine, disk: &ListedDevice) -> Vec<Offer> {
    let in_use = disk.all().into_iter().find_map(|part| {
        part.mountpoints
            .first()
            .map(|point| format!("{} is in use at {point}", part.path))
    });
    Choice::ALL
        .into_iter()
        .map(|choice| match &in_use {
            Some(reason) if choice != Choice::BesideWindows => unavailable(choice, reason.clone()),
            _ => offer(m, disk, choice),
        })
        .collect()
}

fn unavailable(choice: Choice, reason: impl Into<String>) -> Offer {
    Offer {
        choice,
        available: Err(reason.into()),
        system_size: None,
    }
}

fn offer(m: &Machine, disk: &ListedDevice, choice: Choice) -> Offer {
    match choice {
        Choice::WipeDisk => wipe(disk),
        Choice::ReinstallKeepHome => reinstall(disk),
        Choice::ShrinkExt4KeepHome => shrink(m, disk),
        Choice::BesideWindows => unavailable(choice, "comes in a later version"),
    }
}

/// The largest whole number of GiB from `min` up to `max` for which `fits` holds, when it
/// holds for `min`. `fits` must hold for every size below one that fits.
fn largest_fitting(min: u64, max: u64, fits: impl Fn(u64) -> bool) -> Option<u64> {
    if max < min || !fits(min) {
        return None;
    }
    let (mut low, mut high) = (min, max);
    while low < high {
        let middle = low + (high - low).div_ceil(2);
        if fits(middle) {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    Some(low)
}

fn wipe(disk: &ListedDevice) -> Offer {
    let sectors = disk.size_bytes / disk.sector_size;
    let fits = |gib| sizes::wipe_layout(sectors, disk.sector_size, gib).is_ok();
    let max = sizes::MAX_SYSTEM_GIB.min(disk.size_bytes / GIB);
    match largest_fitting(sizes::MIN_SYSTEM_GIB, max, fits) {
        Some(max_gib) => Offer {
            choice: Choice::WipeDisk,
            available: Ok(Partitions::default()),
            system_size: Some(SizeLimits {
                min_gib: sizes::MIN_SYSTEM_GIB,
                max_gib,
            }),
        },
        None => {
            let needed = sizes::wipe_layout(sectors, disk.sector_size, sizes::MIN_SYSTEM_GIB)
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
        .find(|part| part.parttype == EFI_TYPE && part.fstype == "vfat")
}

fn reinstall(disk: &ListedDevice) -> Offer {
    let labeled = |label| disk.children.iter().find(|part| part.label == label);
    let available = match (labeled(SYSTEM_LABEL), labeled(HOME_LABEL)) {
        (Some(system), Some(home)) => match efi_partition(disk) {
            None => Err("the disk has no EFI partition".to_owned()),
            Some(_) if home.fstype != "ext4" => Err(format!(
                "the InayOS home partition {} does not hold ext4",
                home.path
            )),
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
    }
}

fn shrink(m: &Machine, disk: &ListedDevice) -> Offer {
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
    let fits =
        |gib| sizes::shrink_plan(span, sector, usage.block_size, usage.used_bytes, gib).is_ok();
    let max = sizes::MAX_SYSTEM_GIB.min(home.size_bytes / GIB);
    match largest_fitting(sizes::MIN_SYSTEM_GIB, max, fits) {
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
        },
        None => unavailable(
            choice,
            format!(
                "the files on {} leave less than {} GiB to free",
                home.path,
                sizes::MIN_SYSTEM_GIB
            ),
        ),
    }
}

#[cfg(test)]
mod tests;
