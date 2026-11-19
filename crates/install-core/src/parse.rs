//! Reads the facts the install needs out of the text that system tools and files give.
//! Every function here is pure: the caller runs the tool and hands over its output.
use serde::Deserialize;

/// One device of `lsblk --json --output PATH,TYPE,FSTYPE,LABEL,MOUNTPOINTS`, with the devices
/// inside it. A missing filesystem type or label is empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockDevice {
    pub path: String,
    pub kind: String,
    pub fstype: String,
    pub label: String,
    pub mountpoints: Vec<String>,
    pub children: Vec<BlockDevice>,
}

impl BlockDevice {
    /// This device and every device inside it, depth first.
    pub fn all(&self) -> Vec<&BlockDevice> {
        let mut found = vec![self];
        for child in &self.children {
            found.extend(child.all());
        }
        found
    }
}

#[derive(Deserialize)]
struct LsblkOutput {
    blockdevices: Vec<LsblkDevice>,
}

#[derive(Deserialize)]
struct LsblkDevice {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    fstype: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    mountpoints: Vec<Option<String>>,
    #[serde(default)]
    children: Vec<LsblkDevice>,
}

impl From<LsblkDevice> for BlockDevice {
    fn from(device: LsblkDevice) -> Self {
        BlockDevice {
            path: device.path,
            kind: device.kind,
            fstype: device.fstype.unwrap_or_default(),
            label: device.label.unwrap_or_default(),
            mountpoints: device.mountpoints.into_iter().flatten().collect(),
            children: device.children.into_iter().map(Into::into).collect(),
        }
    }
}

/// The device that `lsblk --json` printed for a single device argument. Without the NAME
/// column, lsblk prints its partitions as a flat list after it instead of as its children.
pub fn lsblk(json: &str) -> Result<BlockDevice, String> {
    let output: LsblkOutput = serde_json::from_str(json).map_err(|error| error.to_string())?;
    let mut devices = output.blockdevices.into_iter();
    match devices.next() {
        Some(mut device) => {
            let rest: Vec<LsblkDevice> = devices.collect();
            if !rest.is_empty() {
                if !device.children.is_empty() {
                    return Err("lsblk printed more than one device tree".to_owned());
                }
                device.children = rest;
            }
            Ok(device.into())
        }
        None => Err("lsblk printed no device".to_owned()),
    }
}

/// One device of `lsblk --json --bytes --output NAME,PATH,TYPE,MODEL,SIZE,START,LOG-SEC,
/// FSTYPE,LABEL,PARTTYPE,MOUNTPOINTS`, with the devices inside it. Missing values are empty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListedDevice {
    pub path: String,
    pub kind: String,
    pub model: String,
    pub size_bytes: u64,
    /// The first sector of a partition; 0 for a whole disk.
    pub start: u64,
    pub sector_size: u64,
    pub fstype: String,
    pub label: String,
    /// The GPT type GUID in lowercase, or the MBR type.
    pub parttype: String,
    pub mountpoints: Vec<String>,
    pub children: Vec<ListedDevice>,
}

#[derive(Deserialize)]
struct InventoryOutput {
    blockdevices: Vec<InventoryDevice>,
}

/// lsblk prints numbers as JSON numbers, and as strings before util-linux 2.33.
#[derive(Deserialize)]
#[serde(untagged)]
enum Number {
    Number(u64),
    Text(String),
}

impl Number {
    fn value(value: Option<Number>) -> Result<u64, String> {
        match value {
            None => Ok(0),
            Some(Number::Number(number)) => Ok(number),
            Some(Number::Text(text)) => text
                .trim()
                .parse()
                .map_err(|_| format!("lsblk printed {text:?} for a number")),
        }
    }
}

#[derive(Deserialize)]
struct InventoryDevice {
    path: String,
    #[serde(rename = "type")]
    kind: String,
    model: Option<String>,
    size: Option<Number>,
    start: Option<Number>,
    #[serde(rename = "log-sec")]
    sector_size: Option<Number>,
    fstype: Option<String>,
    label: Option<String>,
    parttype: Option<String>,
    #[serde(default)]
    mountpoints: Vec<Option<String>>,
    #[serde(default)]
    children: Vec<InventoryDevice>,
}

impl InventoryDevice {
    fn into_listed(self) -> Result<ListedDevice, String> {
        Ok(ListedDevice {
            path: self.path,
            kind: self.kind,
            model: self.model.unwrap_or_default().trim().to_owned(),
            size_bytes: Number::value(self.size)?,
            start: Number::value(self.start)?,
            sector_size: Number::value(self.sector_size)?,
            fstype: self.fstype.unwrap_or_default(),
            label: self.label.unwrap_or_default(),
            parttype: self.parttype.unwrap_or_default().to_ascii_lowercase(),
            mountpoints: self.mountpoints.into_iter().flatten().collect(),
            children: self
                .children
                .into_iter()
                .map(InventoryDevice::into_listed)
                .collect::<Result<_, _>>()?,
        })
    }
}

/// Every device tree that the inventory `lsblk` printed. The NAME column makes lsblk print
/// partitions as children of their disk.
pub fn lsblk_inventory(json: &str) -> Result<Vec<ListedDevice>, String> {
    let output: InventoryOutput = serde_json::from_str(json).map_err(|error| error.to_string())?;
    output
        .blockdevices
        .into_iter()
        .map(InventoryDevice::into_listed)
        .collect()
}

/// One row of the partition table that `sgdisk --print` shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableRow {
    pub number: u32,
    pub first: u64,
    pub last: u64,
}

/// The partition rows of `sgdisk --print`, in the order shown.
pub fn partition_table(text: &str) -> Result<Vec<TableRow>, String> {
    let mut lines = text.lines();
    if !lines.any(|line| line.trim_start().starts_with("Number")) {
        return Err("sgdisk printed no partition table header".to_owned());
    }
    lines
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split_whitespace();
            let mut next = || fields.next().ok_or_else(|| format!("short row: {line}"));
            let number = next()?.parse().map_err(|_| format!("bad row: {line}"))?;
            let first = next()?.parse().map_err(|_| format!("bad row: {line}"))?;
            let last = next()?.parse().map_err(|_| format!("bad row: {line}"))?;
            Ok(TableRow {
                number,
                first,
                last,
            })
        })
        .collect()
}

/// The last sector a partition may use, from the `last usable sector is N` that
/// `sgdisk --print` prints. The table's backup sits after it.
pub fn last_usable_sector(text: &str) -> Result<u64, String> {
    text.lines()
        .find_map(|line| line.split_once("last usable sector is "))
        .and_then(|(_, rest)| rest.trim().parse().ok())
        .ok_or_else(|| "sgdisk printed no last usable sector".to_owned())
}

/// The GPT entry of one partition, from `sgdisk --info=N`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GptEntry {
    pub type_guid: String,
    pub unique_guid: String,
    pub first: u64,
    pub last: u64,
    pub name: String,
    /// The attribute flags as sgdisk prints them, 16 hexadecimal digits; all zero when
    /// sgdisk printed none.
    pub attributes: String,
}

pub fn gpt_entry(text: &str) -> Result<GptEntry, String> {
    let field = |label: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(label))
            .map(str::trim)
            .ok_or_else(|| format!("sgdisk printed no `{label}` line"))
    };
    let first_word = |label: &str| -> Result<String, String> {
        field(label)?
            .split_whitespace()
            .next()
            .map(str::to_owned)
            .ok_or_else(|| format!("sgdisk printed an empty `{label}` line"))
    };
    let sector = |label: &str| -> Result<u64, String> {
        first_word(label)?
            .parse()
            .map_err(|_| format!("sgdisk printed a bad `{label}` line"))
    };
    let name = field("Partition name:")?;
    let name = name
        .strip_prefix('\'')
        .and_then(|name| name.strip_suffix('\''))
        .ok_or("sgdisk printed a partition name without quotes")?;
    Ok(GptEntry {
        type_guid: first_word("Partition GUID code:")?,
        unique_guid: first_word("Partition unique GUID:")?,
        first: sector("First sector:")?,
        last: sector("Last sector:")?,
        name: name.to_owned(),
        attributes: first_word("Attribute flags:").unwrap_or_else(|_| "0".repeat(16)),
    })
}

/// The size of an ext4 filesystem's blocks, the bytes its files use and the bytes left free.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ext4Usage {
    pub block_size: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
}

/// The usage that `dumpe2fs -h` reports.
pub fn ext4_usage(text: &str) -> Result<Ext4Usage, String> {
    let number = |label: &str| -> Result<u64, String> {
        text.lines()
            .find_map(|line| line.strip_prefix(label))
            .and_then(|value| value.trim().parse().ok())
            .ok_or_else(|| format!("dumpe2fs printed no number for `{label}`"))
    };
    let blocks = number("Block count:")?;
    let free = number("Free blocks:")?;
    let block_size = number("Block size:")?;
    if free > blocks {
        return Err("dumpe2fs reported more free blocks than blocks".to_owned());
    }
    Ok(Ext4Usage {
        block_size,
        used_bytes: (blocks - free) * block_size,
        free_bytes: free * block_size,
    })
}

/// The memory size in whole GiB, rounded up, from the text of `/proc/meminfo`. The
/// hibernation swap file has this size.
pub fn memory_gib(meminfo: &str) -> Option<u64> {
    let kib: u64 = meminfo.lines().find_map(|line| {
        let value = line.strip_prefix("MemTotal:")?;
        value.trim().strip_suffix("kB")?.trim().parse().ok()
    })?;
    Some((kib * 1024).div_ceil(crate::sizes::GIB))
}

/// The file size that `debugfs -R "stat <file>"` printed. None when it printed none, as for
/// a missing file.
pub fn debugfs_size(stat: &str) -> Option<u64> {
    let mut words = stat.split_whitespace();
    while let Some(word) = words.next() {
        if word == "Size:" {
            return words.next()?.parse().ok();
        }
    }
    None
}

/// The microcode package for the processor that `/proc/cpuinfo` describes.
pub fn microcode(cpuinfo: &str) -> Option<&'static str> {
    let vendor = cpuinfo.lines().find_map(|line| {
        let (key, value) = line.split_once(':')?;
        (key.trim() == "vendor_id").then(|| value.trim())
    })?;
    match vendor {
        "AuthenticAMD" => Some("amd-ucode"),
        "GenuineIntel" => Some("intel-ucode"),
        _ => None,
    }
}

/// `locale.gen` with the line of `locale` turned on, or None when the file has no line for it.
pub fn enable_locale(locale_gen: &str, locale: &str) -> Option<String> {
    let mut found = false;
    let mut text = String::with_capacity(locale_gen.len());
    for line in locale_gen.lines() {
        // Entries read `#en_US.UTF-8 UTF-8`; the examples in the header have spaces after `#`.
        let entry = line.strip_prefix('#').unwrap_or(line);
        let names_locale = !entry.starts_with(char::is_whitespace)
            && entry.split_whitespace().next() == Some(locale)
            && entry.split_whitespace().count() == 2;
        if names_locale && !found {
            found = true;
            text.push_str(entry);
        } else {
            text.push_str(line);
        }
        text.push('\n');
    }
    found.then_some(text)
}

/// The user and group ids of `user` in the text of `/etc/passwd`.
pub fn passwd_ids(passwd: &str, user: &str) -> Option<(u32, u32)> {
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        if fields.next()? != user {
            return None;
        }
        let _password = fields.next()?;
        Some((fields.next()?.parse().ok()?, fields.next()?.parse().ok()?))
    })
}

/// Whether the text of `/etc/group` has a group with the id `gid`.
pub fn has_group_id(group: &str, gid: u32) -> bool {
    group.lines().any(|line| {
        let mut fields = line.split(':');
        fields.nth(2).and_then(|id| id.parse::<u32>().ok()) == Some(gid)
    })
}

/// The recovery key that `systemd-cryptenroll --recovery-key` printed: eight groups of eight
/// modhex characters joined by `-`. None when the text holds no such key.
pub fn recovery_key(text: &str) -> Option<String> {
    const MODHEX: &str = "cbdefghijklnrtuv";
    text.split_whitespace()
        .find(|word| {
            let groups: Vec<&str> = word.split('-').collect();
            groups.len() == 8
                && groups
                    .iter()
                    .all(|group| group.len() == 8 && group.chars().all(|c| MODHEX.contains(c)))
        })
        .map(str::to_owned)
}

/// Whether the first page of a swap area holds the signature the kernel writes when it
/// hibernates into it. The kernel puts `S1SUSPEND` where `SWAPSPACE2` stood.
pub fn holds_hibernation_signature(page: &str) -> bool {
    page.contains("S1SUSPEND")
}

/// The number that `debugfs -R "bmap <file> 0"` printed: the first block of the file. None
/// when debugfs printed none, as for a missing file.
pub fn first_block(bmap: &str) -> Option<u64> {
    bmap.trim().parse().ok().filter(|block| *block > 0)
}

/// The sizes that `ntfsresize --info` reports for an NTFS filesystem.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NtfsSizes {
    /// The size of the filesystem now.
    pub volume_bytes: u64,
    /// The smallest size the filesystem can shrink to.
    pub smallest_bytes: u64,
}

/// The sizes in the output of `ntfsresize --info`: its `Current volume size: N bytes` line,
/// and its `You might resize at N bytes` advice. Without the advice, the filesystem cannot
/// shrink.
pub fn ntfs_sizes(text: &str) -> Result<NtfsSizes, String> {
    let bytes_after = |label: &str| {
        text.lines().find_map(|line| {
            let rest = line.trim_start().strip_prefix(label)?;
            rest.split_whitespace().next()?.parse::<u64>().ok()
        })
    };
    let volume_bytes =
        bytes_after("Current volume size:").ok_or("ntfsresize printed no current volume size")?;
    let smallest_bytes = bytes_after("You might resize at").unwrap_or(volume_bytes);
    Ok(NtfsSizes {
        volume_bytes,
        smallest_bytes: smallest_bytes.min(volume_bytes),
    })
}

/// The bytes available that `df --block-size=1 --output=avail` printed under its header.
pub fn df_available(text: &str) -> Result<u64, String> {
    text.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .and_then(|line| line.parse().ok())
        .ok_or_else(|| format!("df printed no number of bytes: {:?}", text.trim()))
}

/// The signature at byte 3 of a partition that BitLocker encrypts, where an NTFS boot sector
/// holds `NTFS    `.
pub const BITLOCKER_SIGNATURE: &str = "-FVE-FS-";

/// Whether the 8 bytes from byte 3 of a partition are BitLocker's signature.
pub fn is_bitlocker_signature(bytes: &str) -> bool {
    bytes.starts_with(BITLOCKER_SIGNATURE)
}

/// Whether a file at the top of an EFI partition is a kernel, initramfs or microcode image of
/// a system that kept `/boot` there.
pub fn is_old_boot_file(name: &str) -> bool {
    name.starts_with("vmlinuz-")
        || (name.starts_with("initramfs-") && name.ends_with(".img"))
        || name.ends_with("-ucode.img")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsblk_gives_the_tree_with_its_mount_points() {
        let json = r#"{"blockdevices":[{"path":"/dev/vda","type":"disk","mountpoints":[null],
            "children":[{"path":"/dev/vda1","type":"part","mountpoints":["/boot","/efi"]},
                        {"path":"/dev/vda2","type":"part","mountpoints":[null]}]}]}"#;
        let disk = lsblk(json).unwrap();
        assert_eq!(disk.kind, "disk");
        assert!(disk.mountpoints.is_empty());
        let all: Vec<_> = disk.all().iter().map(|d| d.path.as_str()).collect();
        assert_eq!(all, ["/dev/vda", "/dev/vda1", "/dev/vda2"]);
        assert_eq!(disk.children[0].mountpoints, ["/boot", "/efi"]);
    }

    #[test]
    fn lsblk_without_the_name_column_lists_the_partitions_after_the_disk() {
        let json = r#"{"blockdevices":[{"path":"/dev/vda","type":"disk","mountpoints":[null]},
            {"path":"/dev/vda1","type":"part","mountpoints":["/mnt/efi"]},
            {"path":"/dev/vda2","type":"part","mountpoints":[null]}]}"#;
        let disk = lsblk(json).unwrap();
        let all: Vec<_> = disk.all().iter().map(|d| d.path.as_str()).collect();
        assert_eq!(all, ["/dev/vda", "/dev/vda1", "/dev/vda2"]);
        assert_eq!(disk.children[0].mountpoints, ["/mnt/efi"]);
    }

    #[test]
    fn the_sgdisk_table_gives_each_partition_span() {
        let text = "Disk /dev/vda: 134217728 sectors, 64.0 GiB\n\
            Sector size (logical/physical): 512/512 bytes\n\
            Number  Start (sector)    End (sector)  Size       Code  Name\n\
            \x20  1            2048         1050623   512.0 MiB   EF00  EFI system partition\n\
            \x20  2         1050624       134215679   63.5 GiB    8300  \n";
        assert_eq!(
            partition_table(text).unwrap(),
            [
                TableRow {
                    number: 1,
                    first: 2048,
                    last: 1_050_623
                },
                TableRow {
                    number: 2,
                    first: 1_050_624,
                    last: 134_215_679
                },
            ]
        );
    }

    #[test]
    fn the_sgdisk_entry_keeps_guids_sectors_and_the_quoted_name() {
        let text = "Partition GUID code: 0FC63DAF-8483-4772-8E79-3D69D8477DE4 (Linux filesystem)\n\
            Partition unique GUID: 5D4C9E1A-7B0E-4D71-9F5B-2E8A1C3D4E5F\n\
            First sector: 1050624 (at 513.0 MiB)\n\
            Last sector: 134215679 (at 64.0 GiB)\n\
            Partition size: 133165056 sectors (63.5 GiB)\n\
            Attribute flags: 0000000000000000\n\
            Partition name: 'my root'\n";
        assert_eq!(
            gpt_entry(text).unwrap(),
            GptEntry {
                type_guid: "0FC63DAF-8483-4772-8E79-3D69D8477DE4".into(),
                unique_guid: "5D4C9E1A-7B0E-4D71-9F5B-2E8A1C3D4E5F".into(),
                first: 1_050_624,
                last: 134_215_679,
                name: "my root".into(),
                attributes: "0000000000000000".into(),
            }
        );
    }

    #[test]
    fn sgdisk_print_gives_the_last_usable_sector() {
        let text = "Disk /dev/vda: 268435456 sectors, 128.0 GiB\n\
            Sector size (logical/physical): 512/512 bytes\n\
            Partition table holds up to 128 entries\n\
            First usable sector is 34, last usable sector is 268435422\n\
            Partitions will be aligned on 2048-sector boundaries\n";
        assert_eq!(last_usable_sector(text), Ok(268_435_422));
        assert!(last_usable_sector("Number  Start (sector)\n").is_err());
    }

    #[test]
    fn ntfsresize_gives_the_volume_size_and_the_smallest_size() {
        let text = "ntfsresize v2022.10.3 (libntfs-3g)\nDevice name        : /dev/vda3\n\
            NTFS volume version: 3.1\nCluster size       : 4096 bytes\n\
            Current volume size: 85371367936 bytes (85372 MB)\n\
            Current device size: 85371371520 bytes (85372 MB)\n\
            Checking filesystem consistency ...\nAccounting clusters ...\n\
            Space in use       : 24578 MB (28.8%)\nCollecting resizing constraints ...\n\
            You might resize at 24577306624 bytes or 24578 MB (freeing 60794 MB).\n\
            Please make a test run using both the -n and -s options before real resizing!\n";
        assert_eq!(
            ntfs_sizes(text).unwrap(),
            NtfsSizes {
                volume_bytes: 85_371_367_936,
                smallest_bytes: 24_577_306_624,
            }
        );
        // Without the advice the filesystem cannot shrink.
        let full = "Current volume size: 1000 bytes (0 MB)\n";
        assert_eq!(ntfs_sizes(full).unwrap().smallest_bytes, 1000);
        assert!(ntfs_sizes("ERROR: the partition is hibernated\n").is_err());
    }

    #[test]
    fn df_gives_the_available_bytes_under_its_header() {
        assert_eq!(df_available("    Avail\n70254592\n"), Ok(70_254_592));
        assert!(df_available("    Avail\n").is_err());
    }

    #[test]
    fn dumpe2fs_gives_the_used_bytes() {
        let text = "Block count:              1000\nReserved block count:     50\n\
            Free blocks:              400\nBlock size:               4096\n";
        assert_eq!(
            ext4_usage(text).unwrap(),
            Ext4Usage {
                block_size: 4096,
                used_bytes: 600 * 4096,
                free_bytes: 400 * 4096,
            }
        );
    }

    #[test]
    fn meminfo_gives_the_memory_rounded_up_to_a_whole_gib() {
        assert_eq!(
            memory_gib("MemTotal:        8040956 kB\nMemFree: 1 kB\n"),
            Some(8)
        );
        assert_eq!(memory_gib("MemTotal:        8388608 kB\n"), Some(8));
        assert_eq!(memory_gib("MemTotal:        8388609 kB\n"), Some(9));
        assert_eq!(memory_gib("MemFree: 1 kB\n"), None);
    }

    #[test]
    fn debugfs_stat_gives_the_file_size() {
        let stat = "Inode: 12   Type: regular    Mode:  0600   Flags: 0x80000\n\
            Generation: 1    Version: 0x00000000:00000001\n\
            User:     0   Group:     0   Project:     0   Size: 8589934592\n";
        assert_eq!(debugfs_size(stat), Some(8 * crate::sizes::GIB));
        assert_eq!(
            debugfs_size("/swapfile: File not found by ext2_lookup\n"),
            None
        );
    }

    #[test]
    fn the_cpu_vendor_picks_the_microcode() {
        assert_eq!(
            microcode("processor\t: 0\nvendor_id\t: AuthenticAMD\n"),
            Some("amd-ucode")
        );
        assert_eq!(
            microcode("vendor_id\t: GenuineIntel\n"),
            Some("intel-ucode")
        );
        assert_eq!(microcode("vendor_id\t: HygonGenuine\n"), None);
    }

    #[test]
    fn the_locale_line_is_turned_on_and_the_rest_kept() {
        let text =
            "#  en_US.UTF-8 UTF-8\n#en_GB.UTF-8 UTF-8  \n#en_US ISO-8859-1\n#en_US.UTF-8 UTF-8  \n";
        assert_eq!(
            enable_locale(text, "en_US.UTF-8").unwrap(),
            "#  en_US.UTF-8 UTF-8\n#en_GB.UTF-8 UTF-8  \n#en_US ISO-8859-1\nen_US.UTF-8 UTF-8  \n"
        );
        assert_eq!(enable_locale(text, "xx_XX.UTF-8"), None);
    }

    #[test]
    fn group_ids_are_found_in_the_third_field() {
        let text = "root:x:0:root\nusers:x:100:\nwheel:x:998:tester\n";
        assert!(has_group_id(text, 100));
        assert!(!has_group_id(text, 1000));
    }

    #[test]
    fn passwd_gives_the_ids_of_the_named_user() {
        let text =
            "root:x:0:0::/root:/bin/bash\ntester:x:1000:1000:Tester:/home/tester:/bin/bash\n";
        assert_eq!(passwd_ids(text, "tester"), Some((1000, 1000)));
        assert_eq!(passwd_ids(text, "test"), None);
    }

    #[test]
    fn only_old_kernels_initramfs_and_microcode_count_as_boot_files() {
        for name in [
            "vmlinuz-linux",
            "initramfs-linux.img",
            "initramfs-linux-fallback.img",
            "amd-ucode.img",
        ] {
            assert!(is_old_boot_file(name), "{name}");
        }
        for name in [
            "EFI",
            "refind_linux.conf",
            "loader",
            "initramfs-linux.img.bak",
        ] {
            assert!(!is_old_boot_file(name), "{name}");
        }
    }

    #[test]
    fn the_recovery_key_is_the_modhex_word_of_the_output() {
        let key = "fkhjdulc-hbinrcig-ntvcnejb-uifjlvbn-cerbtnkg-ivnbvjil-tggkjhel-nedrlkrj";
        assert_eq!(recovery_key(&format!("{key}\n")).as_deref(), Some(key));
        assert_eq!(
            recovery_key(&format!("\u{1b}[0;1;39m{key}")).as_deref(),
            None
        );
        assert_eq!(recovery_key("fkhjdulc-hbinrcig"), None);
        assert_eq!(recovery_key(&key.replace('f', "a")), None);
        assert_eq!(recovery_key(""), None);
    }

    #[test]
    fn a_hibernated_swap_page_holds_the_suspend_signature_and_a_file_has_a_first_block() {
        let swap = format!("{}SWAPSPACE2", "\0".repeat(4086));
        assert!(!holds_hibernation_signature(&swap));
        assert!(holds_hibernation_signature(
            &swap.replace("SWAPSPACE2", "S1SUSPEND\0")
        ));
        assert_eq!(first_block("34816\n"), Some(34816));
        assert_eq!(first_block(""), None);
        assert_eq!(first_block("0\n"), None);
    }
}
