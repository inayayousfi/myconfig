//! Disk size arithmetic: sizes in GiB turned into sectors, partitions aligned to 1 MiB, and
//! the shrink target of an ext4 partition. Sector numbers are inclusive, as `sgdisk` uses them.

pub const MIB: u64 = 1 << 20;
pub const GIB: u64 = 1 << 30;

/// The EFI system partition of a wiped disk.
pub const EFI_SIZE_GIB: u64 = 2;
/// The smallest system partition the answers file accepts.
pub const MIN_SYSTEM_GIB: u64 = 20;
/// The largest system partition the answers file accepts, which keeps every product of
/// sizes far from overflowing.
pub const MAX_SYSTEM_GIB: u64 = 1 << 20;
/// The smallest home partition a wiped disk must leave for the user's files, besides the
/// hibernation swap file. The install itself fills a few GiB of it, because AUR packages
/// build in the user's cache folder.
pub const MIN_HOME_GIB: u64 = 20;

/// A run of sectors, both ends included.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub first: u64,
    pub last: u64,
}

impl Span {
    pub fn sectors(self) -> u64 {
        self.last - self.first + 1
    }
}

/// The partitions of a wiped disk. Home runs from `home_first` to the last usable sector.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WipeLayout {
    pub efi: Span,
    pub system: Span,
    pub home_first: u64,
}

/// The new end of a shrunk ext4 partition and the size its filesystem shrinks to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShrinkPlan {
    /// The shrunk partition: the same first sector and an earlier last one.
    pub home: Span,
    /// The size `resize2fs` gives the filesystem, a whole number of its blocks.
    pub filesystem_bytes: u64,
    /// The first sector of the system partition, aligned to 1 MiB.
    pub system_first: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the disk has {disk_bytes} bytes and the layout needs {needed_bytes}")]
pub struct TooSmall {
    pub needed_bytes: u64,
    pub disk_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "the files and the hibernation swap file take {used_bytes} bytes and the shrunk home \
     would hold {filesystem_bytes}, which leaves less than 10% free"
)]
pub struct HomeTooFull {
    pub used_bytes: u64,
    pub filesystem_bytes: u64,
}

pub fn gib_to_sectors(gib: u64, sector_size: u64) -> u64 {
    gib * GIB / sector_size
}

/// The first sector at or after `sector` that starts a 1 MiB boundary.
pub fn align_up(sector: u64, sector_size: u64) -> u64 {
    let per_mib = MIB / sector_size;
    sector.div_ceil(per_mib) * per_mib
}

/// The last sector at or before `sector` that starts a 1 MiB boundary.
pub fn align_down(sector: u64, sector_size: u64) -> u64 {
    let per_mib = MIB / sector_size;
    sector / per_mib * per_mib
}

/// The last sector a partition may use: GPT keeps a backup header and a 16 KiB entry array
/// at the end of the disk.
pub fn last_usable_sector(disk_sectors: u64, sector_size: u64) -> u64 {
    disk_sectors - 1 - (1 + 16 * 1024 / sector_size)
}

/// The EFI, system and home partitions of a wiped disk of `disk_sectors` sectors. Each
/// partition starts on a 1 MiB boundary, and home keeps at least `MIN_HOME_GIB` plus the
/// `swap_gib` of the hibernation swap file.
pub fn wipe_layout(
    disk_sectors: u64,
    sector_size: u64,
    system_gib: u64,
    swap_gib: u64,
) -> Result<WipeLayout, TooSmall> {
    let efi_first = align_up(1, sector_size);
    let efi = Span {
        first: efi_first,
        last: efi_first + gib_to_sectors(EFI_SIZE_GIB, sector_size) - 1,
    };
    let system_first = align_up(efi.last + 1, sector_size);
    let system = Span {
        first: system_first,
        last: system_first + gib_to_sectors(system_gib, sector_size) - 1,
    };
    let home_first = align_up(system.last + 1, sector_size);
    let needed = home_first + gib_to_sectors(MIN_HOME_GIB + swap_gib, sector_size) - 1;
    let too_small = TooSmall {
        needed_bytes: (needed + 2 + 16 * 1024 / sector_size) * sector_size,
        disk_bytes: disk_sectors * sector_size,
    };
    if disk_sectors < 64 || needed > last_usable_sector(disk_sectors, sector_size) {
        return Err(too_small);
    }
    Ok(WipeLayout {
        efi,
        system,
        home_first,
    })
}

/// Shrinks the ext4 partition `home` by `system_gib` and returns its new end. The freed space
/// after it starts on a 1 MiB boundary and holds at least `system_gib`. The files and the
/// hibernation swap file, `used_bytes`, must leave at least 10% of the shrunk filesystem free.
pub fn shrink_plan(
    home: Span,
    sector_size: u64,
    block_size: u64,
    used_bytes: u64,
    system_gib: u64,
) -> Result<ShrinkPlan, HomeTooFull> {
    let start_byte = home.first * sector_size;
    let partition_bytes = home.sectors() * sector_size;
    let system_bytes = system_gib * GIB;
    let too_full = |filesystem_bytes| HomeTooFull {
        used_bytes,
        filesystem_bytes,
    };
    if partition_bytes <= system_bytes {
        return Err(too_full(0));
    }
    let end_byte = (start_byte + partition_bytes - system_bytes) / MIB * MIB;
    if end_byte <= start_byte {
        return Err(too_full(0));
    }
    let filesystem_bytes = (end_byte - start_byte) / block_size * block_size;
    if used_bytes.saturating_mul(10) > filesystem_bytes.saturating_mul(9) {
        return Err(too_full(filesystem_bytes));
    }
    Ok(ShrinkPlan {
        home: Span {
            first: home.first,
            last: end_byte / sector_size - 1,
        },
        filesystem_bytes,
        system_first: end_byte / sector_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gib_become_sectors_of_either_size() {
        assert_eq!(gib_to_sectors(2, 512), 4_194_304);
        assert_eq!(gib_to_sectors(2, 4096), 524_288);
    }

    #[test]
    fn alignment_goes_to_the_nearest_mib_boundary_in_its_direction() {
        assert_eq!(align_up(1, 512), 2048);
        assert_eq!(align_up(2048, 512), 2048);
        assert_eq!(align_up(2049, 512), 4096);
        assert_eq!(align_down(4095, 512), 2048);
        assert_eq!(align_up(1, 4096), 256);
    }

    #[test]
    fn a_wiped_disk_gets_2_gib_efi_then_the_system_then_home_to_the_end() {
        // 64 GiB of 512-byte sectors.
        let disk = 64 * GIB / 512;
        let layout = wipe_layout(disk, 512, 40, 0).unwrap();
        assert_eq!(
            layout.efi,
            Span {
                first: 2048,
                last: 2048 + 4_194_304 - 1
            }
        );
        assert_eq!(layout.system.first, 2048 + 4_194_304);
        assert_eq!(layout.system.sectors() * 512, 40 * GIB);
        assert_eq!(layout.home_first, layout.system.last + 1);
        assert_eq!(layout.home_first % 2048, 0);
    }

    #[test]
    fn a_disk_without_room_for_home_is_too_small() {
        let disk = (2 + 40) * GIB / 512 + 2048;
        assert!(wipe_layout(disk, 512, 40, 0).is_err());
        let disk = (2 + 40 + 19) * GIB / 512;
        assert!(wipe_layout(disk, 512, 40, 0).is_err());
        let disk = (2 + 40 + 21) * GIB / 512;
        assert!(wipe_layout(disk, 512, 40, 0).is_ok());
    }

    #[test]
    fn home_keeps_room_for_the_swap_file_besides_the_files() {
        let disk = (2 + 40 + 21 + 8) * GIB / 512;
        assert!(wipe_layout(disk, 512, 40, 8).is_ok());
        assert!(wipe_layout(disk, 512, 40, 9).is_err());
    }

    #[test]
    fn a_shrunk_home_keeps_its_start_and_frees_the_system_size_after_it() {
        // A 100 GiB partition from sector 2048, with 30 GiB of files, shrunk by 40 GiB.
        let home = Span {
            first: 2048,
            last: 2048 + 100 * GIB / 512 - 1,
        };
        let plan = shrink_plan(home, 512, 4096, 30 * GIB, 40).unwrap();
        assert_eq!(plan.home.first, 2048);
        assert_eq!(plan.filesystem_bytes, 60 * GIB);
        assert_eq!(plan.system_first, plan.home.last + 1);
        assert_eq!((home.last + 1 - plan.system_first) * 512, 40 * GIB);
    }

    #[test]
    fn an_unaligned_start_still_frees_an_aligned_system_start() {
        let home = Span {
            first: 63,
            last: 63 + 100 * GIB / 512 - 1,
        };
        let plan = shrink_plan(home, 512, 4096, GIB, 40).unwrap();
        assert_eq!(plan.system_first % 2048, 0);
        assert!((home.last + 1 - plan.system_first) * 512 >= 40 * GIB);
        assert_eq!(plan.filesystem_bytes % 4096, 0);
        assert_eq!(
            plan.filesystem_bytes,
            (plan.home.sectors() * 512) / 4096 * 4096
        );
    }

    #[test]
    fn files_that_leave_less_than_10_percent_free_refuse_the_shrink() {
        let home = Span {
            first: 2048,
            last: 2048 + 100 * GIB / 512 - 1,
        };
        // 60 GiB remain: 54 GiB of files leaves exactly 10% free, a byte more does not.
        assert!(shrink_plan(home, 512, 4096, 54 * GIB, 40).is_ok());
        assert_eq!(
            shrink_plan(home, 512, 4096, 54 * GIB + 1, 40),
            Err(HomeTooFull {
                used_bytes: 54 * GIB + 1,
                filesystem_bytes: 60 * GIB
            })
        );
        assert!(shrink_plan(home, 512, 4096, 0, 100).is_err());
    }
}
