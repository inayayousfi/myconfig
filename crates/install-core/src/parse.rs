//! Reads the facts the install needs out of the text that system tools and files give.
//! Every function here is pure: the caller runs the tool and hands over its output.
use serde::Deserialize;

/// One device of `lsblk --json --output PATH,TYPE,MOUNTPOINTS`, with the devices inside it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockDevice {
    pub path: String,
    pub kind: String,
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
    mountpoints: Vec<Option<String>>,
    #[serde(default)]
    children: Vec<LsblkDevice>,
}

impl From<LsblkDevice> for BlockDevice {
    fn from(device: LsblkDevice) -> Self {
        BlockDevice {
            path: device.path,
            kind: device.kind,
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

/// The GPT entry of one partition, from `sgdisk --info=N`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GptEntry {
    pub type_guid: String,
    pub unique_guid: String,
    pub first: u64,
    pub last: u64,
    pub name: String,
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
    })
}

/// The size of an ext4 filesystem's blocks and the bytes its files use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ext4Usage {
    pub block_size: u64,
    pub used_bytes: u64,
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
    })
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
            }
        );
    }

    #[test]
    fn dumpe2fs_gives_the_used_bytes() {
        let text = "Block count:              1000\nReserved block count:     50\n\
            Free blocks:              400\nBlock size:               4096\n";
        assert_eq!(
            ext4_usage(text).unwrap(),
            Ext4Usage {
                block_size: 4096,
                used_bytes: 600 * 4096
            }
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
}
