//! Reads what the Windows scenarios see: the partitions of the Windows disk before and after an
//! install beside Windows, the places where rEFInd draws the tags of its first row, the firmware
//! boot entry that started Windows, and which system answers SSH. The scenario turns each
//! problem these return into a failed check.

use crate::ppm::Area;

/// The words of the installer's instructions when the Windows partition is BitLocker-encrypted.
pub const BITLOCKER_MESSAGE: &str = "Turn off BitLocker in Windows";
/// The words of the installer's instructions when Windows did not shut down fully.
pub const FAST_STARTUP_MESSAGE: &str = "Turn off Fast Startup in Windows";
/// The label that Windows Setup gives its firmware boot entry.
pub const WINDOWS_BOOT_MANAGER: &str = "Windows Boot Manager";

/// rEFInd's tiles of the first row are 144 pixels wide for its default 128-pixel icons, 8
/// pixels apart, and centered on the screen, so tag centers lie half a step apart from the
/// middle of the screen whatever the number of tags.
const TILE: usize = 144;
const TILE_STEP: usize = TILE + 8;
/// rEFInd's first row holds at most this many tags before it scrolls.
const MAX_TAGS: usize = 9;
/// A changed area at least this large is rEFInd's selection drawn behind a tag, not the
/// pointer.
const SELECTION_MIN: usize = 100;

/// One partition of a disk, from the listing of the partition script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    pub name: String,
    pub start: u64,
    pub bytes: u64,
    pub filesystem: String,
    pub partition_uuid: String,
    pub filesystem_uuid: String,
}

/// Reads lines of `<start> <bytes> <name> <filesystem> <partition uuid> <filesystem uuid>`,
/// where `-` stands for an empty field, in the order of their start on the disk.
pub fn partitions(listing: &str) -> Result<Vec<Partition>, String> {
    let field = |value: &str| {
        if value == "-" {
            String::new()
        } else {
            value.to_owned()
        }
    };
    let mut found = Vec::new();
    for line in listing.lines().filter(|line| !line.trim().is_empty()) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [
            start,
            bytes,
            name,
            filesystem,
            partition_uuid,
            filesystem_uuid,
        ] = fields[..]
        else {
            return Err(format!(
                "the partition line {line:?} does not have six fields"
            ));
        };
        let number = |value: &str| {
            value
                .parse()
                .map_err(|_| format!("the partition line {line:?} has no number in {value:?}"))
        };
        found.push(Partition {
            name: name.to_owned(),
            start: number(start)?,
            bytes: number(bytes)?,
            filesystem: field(filesystem),
            partition_uuid: field(partition_uuid),
            filesystem_uuid: field(filesystem_uuid),
        });
    }
    found.sort_by_key(|partition| partition.start);
    Ok(found)
}

/// The Windows partition, the largest NTFS one, and the EFI system partition, the FAT one.
fn windows_and_efi(partitions: &[Partition]) -> Option<(&Partition, &Partition)> {
    let windows = partitions
        .iter()
        .filter(|partition| partition.filesystem == "ntfs")
        .max_by_key(|partition| partition.bytes)?;
    let efi = partitions
        .iter()
        .find(|partition| partition.filesystem == "vfat")?;
    Some((windows, efi))
}

/// Compares the partitions of the Windows disk before and after an install beside Windows.
/// The Windows partition must keep its start and unique GUID and get smaller, the EFI system
/// partition must keep its filesystem, every other earlier partition must stay as it was, and
/// a Btrfs and an ext4 partition must follow Windows. Returns how much Windows shrank, or the
/// problem.
pub fn beside_windows_problem(before: &[Partition], after: &[Partition]) -> Result<String, String> {
    let (windows, efi) = windows_and_efi(before)
        .ok_or("before the install the disk has no NTFS and FAT partitions")?;
    let shrunk = after
        .iter()
        .find(|partition| partition.partition_uuid == windows.partition_uuid)
        .ok_or_else(|| {
            format!(
                "the Windows partition {} with GUID {} is gone",
                windows.name, windows.partition_uuid
            )
        })?;
    if shrunk.start != windows.start || shrunk.filesystem != "ntfs" {
        return Err(format!(
            "the Windows partition moved or lost its NTFS: it started at sector {} with NTFS, and now starts at {} with {:?}",
            windows.start, shrunk.start, shrunk.filesystem
        ));
    }
    if shrunk.bytes >= windows.bytes {
        return Err(format!(
            "the Windows partition did not shrink: {} bytes before, {} after",
            windows.bytes, shrunk.bytes
        ));
    }
    if !after.iter().any(|partition| {
        partition.filesystem == "vfat" && partition.filesystem_uuid == efi.filesystem_uuid
    }) {
        return Err(format!(
            "no FAT partition has the EFI system partition's filesystem UUID {}: it was formatted",
            efi.filesystem_uuid
        ));
    }
    for kept in before.iter().filter(|partition| *partition != windows) {
        let same = after.iter().any(|partition| {
            partition.start == kept.start
                && partition.bytes == kept.bytes
                && partition.partition_uuid == kept.partition_uuid
        });
        if !same {
            return Err(format!(
                "the partition {} with GUID {} changed",
                kept.name, kept.partition_uuid
            ));
        }
    }
    for filesystem in ["btrfs", "ext4"] {
        if !after
            .iter()
            .any(|partition| partition.filesystem == filesystem && partition.start > windows.start)
        {
            return Err(format!(
                "no {filesystem} partition follows the Windows partition"
            ));
        }
    }
    let gib = |bytes: u64| bytes as f64 / f64::from(1 << 30);
    Ok(format!(
        "Windows shrank from {:.1} GiB to {:.1} GiB",
        gib(windows.bytes),
        gib(shrunk.bytes)
    ))
}

/// The columns where a tag of rEFInd's first row can have its center on a screen `width`
/// pixels wide, from left to right. A tag's center is on the middle row of the screen.
pub fn tag_columns(width: usize) -> Vec<usize> {
    let middle = width / 2;
    let half_step = TILE_STEP / 2;
    let reach = (MAX_TAGS - 1) * half_step;
    (0..=2 * (MAX_TAGS - 1))
        .map(|step| middle + step * half_step)
        .filter_map(|shifted| shifted.checked_sub(reach))
        .filter(|&column| column >= TILE / 2 && column + TILE / 2 <= width)
        .collect()
}

/// The rows of the screen that rEFInd's first row of tags covers, on a screen `height` pixels
/// high.
pub fn tag_rows(height: usize) -> std::ops::Range<usize> {
    let middle = height / 2;
    middle.saturating_sub(TILE / 2)..middle + TILE / 2 + 1
}

/// Whether the screen area that changed when the pointer moved over a place is rEFInd's
/// selection behind a tag.
pub fn is_selection(changed: Option<Area>) -> bool {
    changed.is_some_and(|area| area.width() >= SELECTION_MIN && area.height() >= SELECTION_MIN)
}

/// The entry name and label that `boot-current.ps1` prints, such as `Boot0003` and
/// `Windows Boot Manager`.
pub fn boot_current(output: &str) -> Option<(&str, &str)> {
    let line = output
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("Boot"))?;
    let (entry, label) = line.split_once(' ')?;
    (entry.len() == 8 && entry[4..].chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| (entry, label.trim()))
}

/// The number of the firmware boot entry labelled `label` in `listing`, the output of
/// `efibootmgr`, such as `0003` for the line `Boot0003* Windows Boot Manager`.
pub fn boot_entry_number<'a>(listing: &'a str, label: &str) -> Option<&'a str> {
    listing.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("Boot")?;
        let (number, rest) = rest.split_at_checked(4)?;
        let name = rest.strip_prefix(['*', ' '])?.trim_start();
        let name = name.split('\t').next().unwrap_or(name).trim_end();
        (number.chars().all(|c| c.is_ascii_hexdigit()) && name == label).then_some(number)
    })
}

/// The system that answered SSH.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum System {
    Windows,
    InayOs,
}

/// The system whose `ver` command printed `output`: the Windows command prompt prints its
/// version, and a Linux shell has no `ver`.
pub fn system_from_ver(output: Option<&str>) -> System {
    match output {
        Some(text) if text.contains("Windows") => System::Windows,
        _ => System::InayOs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boot_entry_number_comes_from_the_line_with_the_label() {
        let listing = "BootCurrent: 0004\nBootOrder: 0004,0003\n\
                       Boot0003* Windows Boot Manager\tHD(1,GPT,...)/\\EFI\\Microsoft\n\
                       Boot0004* rEFInd Boot Manager\tHD(1,GPT,...)\n";
        assert_eq!(
            boot_entry_number(listing, WINDOWS_BOOT_MANAGER),
            Some("0003")
        );
        assert_eq!(
            boot_entry_number("Boot0001  Windows Boot Manager\n", WINDOWS_BOOT_MANAGER),
            Some("0001")
        );
        assert_eq!(boot_entry_number(listing, "UEFI Shell"), None);
        assert_eq!(boot_entry_number("BootOrder: 0003\n", "Order: 0003"), None);
    }

    const BEFORE: &str = "\
2048 314572800 sda1 vfat 1a2b-p1 9C4E-1F2A
616448 16777216 sda2 - 1a2b-p2 -
649216 274529157120 sda3 ntfs 1a2b-p3 0A1B2C3D4E5F
";

    fn after(windows_bytes: u64, efi_uuid: &str) -> String {
        format!(
            "2048 314572800 sda1 vfat 1a2b-p1 {efi_uuid}\n\
             616448 16777216 sda2 - 1a2b-p2 -\n\
             649216 {windows_bytes} sda3 ntfs 1a2b-p3 0A1B2C3D4E5F\n\
             200000000 161061273600 sda4 btrfs 1a2b-p4 aaaa\n\
             514572800 32212254720 sda5 ext4 1a2b-p5 bbbb\n"
        )
    }

    #[test]
    fn the_listing_gives_each_partition_in_disk_order_with_empty_fields() {
        let found = partitions(
            "616448 16777216 sda2 - 1a2b-p2 -\n2048 314572800 sda1 vfat 1a2b-p1 9C4E-1F2A\n",
        )
        .unwrap();
        assert_eq!(found[0].name, "sda1");
        assert_eq!(found[1].filesystem, "");
        assert_eq!(found[1].filesystem_uuid, "");
        assert!(partitions("2048 sda1 vfat\n").is_err());
    }

    #[test]
    fn a_shrunk_windows_with_the_new_partitions_after_it_passes() {
        let before = partitions(BEFORE).unwrap();
        let after = partitions(&after(100 << 30, "9C4E-1F2A")).unwrap();
        assert_eq!(
            beside_windows_problem(&before, &after),
            Ok("Windows shrank from 255.7 GiB to 100.0 GiB".to_owned())
        );
    }

    #[test]
    fn an_unshrunk_windows_or_a_formatted_efi_partition_fails() {
        let before = partitions(BEFORE).unwrap();
        let unshrunk = partitions(&after(274529157120, "9C4E-1F2A")).unwrap();
        assert!(
            beside_windows_problem(&before, &unshrunk)
                .unwrap_err()
                .contains("did not shrink")
        );
        let formatted = partitions(&after(100 << 30, "0000-0000")).unwrap();
        assert!(
            beside_windows_problem(&before, &formatted)
                .unwrap_err()
                .contains("formatted")
        );
    }

    #[test]
    fn a_windows_partition_recreated_with_another_guid_or_start_fails() {
        let before = partitions(BEFORE).unwrap();
        let other_guid =
            partitions(&after(100 << 30, "9C4E-1F2A").replace("1a2b-p3", "ffff-p3")).unwrap();
        assert!(
            beside_windows_problem(&before, &other_guid)
                .unwrap_err()
                .contains("is gone")
        );
        let moved = partitions(
            &after(100 << 30, "9C4E-1F2A").replace("649216 107374182400", "650240 107374182400"),
        )
        .unwrap();
        assert!(
            beside_windows_problem(&before, &moved)
                .unwrap_err()
                .contains("moved")
        );
    }

    #[test]
    fn the_tag_columns_hold_the_centers_of_any_number_of_tags() {
        let width = 1280;
        let columns = tag_columns(width);
        // One tag sits in the middle; two sit half a step to each side; three around the middle.
        for centers in [vec![640], vec![564, 716], vec![488, 640, 792]] {
            for center in centers {
                assert!(columns.contains(&center), "{center} in {columns:?}");
            }
        }
        assert!(columns.windows(2).all(|pair| pair[1] - pair[0] == 76));
        assert!(
            columns
                .iter()
                .all(|&column| column >= 72 && column + 72 <= width)
        );
    }

    #[test]
    fn only_a_large_changed_area_is_the_selection() {
        let area = |width: usize, height: usize| Area {
            left: 10,
            top: 10,
            right: 10 + width - 1,
            bottom: 10 + height - 1,
        };
        assert!(is_selection(Some(area(144, 144))));
        assert!(!is_selection(Some(area(16, 16))));
        assert!(!is_selection(None));
    }

    #[test]
    fn the_boot_entry_of_this_boot_has_its_number_and_label() {
        assert_eq!(
            boot_current("Boot0003 Windows Boot Manager\r\n"),
            Some(("Boot0003", WINDOWS_BOOT_MANAGER))
        );
        assert_eq!(boot_current("Exception calling Read\n"), None);
    }

    #[test]
    fn the_windows_version_tells_windows_from_inayos() {
        assert_eq!(
            system_from_ver(Some("\r\nMicrosoft Windows [Version 10.0.26200.6584]\r\n")),
            System::Windows
        );
        assert_eq!(system_from_ver(None), System::InayOs);
    }
}
