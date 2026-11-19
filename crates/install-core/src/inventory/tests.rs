//! The disk offers for the layouts the installer meets, read from lsblk output shaped as
//! util-linux 2.42 prints it.
use serde_json::{Value, json};

use super::{DiskInfo, Offer, Partitions, SizeLimits, disks, has_tpm2};
use crate::{
    Choice,
    fake::{FakeHost, ok},
    sizes::{GIB, MIB},
};

const LSBLK: &str = "lsblk --json --bytes --output \
                     NAME,PATH,TYPE,MODEL,SIZE,START,LOG-SEC,FSTYPE,LABEL,PARTTYPE,MOUNTPOINTS";
const EFI: &str = "c12a7328-f81f-11d2-ba4b-00a0c93ec93b";
const LINUX: &str = "0fc63daf-8483-4772-8e79-3d69d8477de4";
const MICROSOFT_DATA: &str = "ebd0a0a2-b9e5-4433-87c0-68b6b72699c7";
const MICROSOFT_RESERVED: &str = "e3c9e316-0b5c-4db8-817d-f92df00215ae";

fn disk(path: &str, model: &str, size: u64, children: Vec<Value>) -> Value {
    json!({"name": path.trim_start_matches("/dev/"), "path": path, "type": "disk",
           "model": model, "size": size, "start": null, "log-sec": 512, "fstype": null,
           "label": null, "parttype": null, "mountpoints": [null], "children": children})
}

fn part(
    path: &str,
    start: u64,
    size: u64,
    fstype: Option<&str>,
    label: Option<&str>,
    parttype: &str,
) -> Value {
    json!({"name": path.trim_start_matches("/dev/"), "path": path, "type": "part",
           "model": null, "size": size, "start": start, "log-sec": 512, "fstype": fstype,
           "label": label, "parttype": parttype.to_uppercase(), "mountpoints": [null]})
}

fn mounted(mut device: Value, point: &str) -> Value {
    device["mountpoints"] = json!([point]);
    device
}

/// The USB stick the installer booted from and a zram swap device, which every machine has.
fn live_devices() -> Vec<Value> {
    vec![
        json!({"name": "zram0", "path": "/dev/zram0", "type": "disk", "model": null,
               "size": 4 * GIB, "start": null, "log-sec": 4096, "fstype": "swap",
               "label": "zram0", "parttype": null, "mountpoints": ["[SWAP]"]}),
        disk(
            "/dev/sda",
            "USB Flash",
            16 * GIB,
            vec![
                mounted(
                    part(
                        "/dev/sda1",
                        64,
                        GIB,
                        Some("iso9660"),
                        Some("INAYOS_202611"),
                        "0x0",
                    ),
                    "/run/archiso/bootmnt",
                ),
                part(
                    "/dev/sda2",
                    2_097_216,
                    300 * MIB,
                    Some("vfat"),
                    Some("ARCHISO_EFI"),
                    "0xef",
                ),
            ],
        ),
        json!({"name": "loop0", "path": "/dev/loop0", "type": "loop", "model": null,
               "size": GIB, "start": null, "log-sec": 512, "fstype": "squashfs",
               "label": null, "parttype": null, "mountpoints": ["/run/archiso/airootfs"]}),
    ]
}

fn machine(disks: Vec<Value>) -> FakeHost {
    let mut devices = live_devices();
    devices.extend(disks);
    let host = FakeHost::default();
    host.respond(LSBLK, [ok(&json!({"blockdevices": devices}).to_string())]);
    host
}

fn offer(info: &DiskInfo, choice: Choice) -> &Offer {
    info.offers
        .iter()
        .find(|offer| offer.choice == choice)
        .unwrap()
}

fn reason(info: &DiskInfo, choice: Choice) -> String {
    offer(info, choice).available.clone().unwrap_err()
}

#[test]
fn a_blank_disk_offers_only_the_wipe_and_never_lists_the_live_devices() {
    let host = machine(vec![disk("/dev/vda", "QEMU HARDDISK ", 64 * GIB, vec![])]);
    let found = disks(&host).unwrap();
    assert_eq!(found.len(), 1);
    let vda = &found[0];
    assert_eq!(
        (vda.path.as_str(), vda.model.as_str(), vda.size_bytes),
        ("/dev/vda", "QEMU HARDDISK", 64 * GIB)
    );
    assert_eq!(
        vda.offers
            .iter()
            .map(|offer| offer.choice)
            .collect::<Vec<_>>(),
        Choice::ALL
    );
    // 64 GiB less 1 MiB before the EFI partition, 2 GiB of EFI, at least 20 GiB of home and
    // the backup GPT leaves 41 whole GiB for the system.
    assert_eq!(
        offer(vda, Choice::WipeDisk),
        &Offer {
            choice: Choice::WipeDisk,
            available: Ok(Partitions::default()),
            system_size: Some(SizeLimits {
                min_gib: 20,
                max_gib: 41
            }),
            windows: None,
        }
    );
    assert_eq!(
        reason(vda, Choice::ReinstallKeepHome),
        "the disk holds no earlier InayOS"
    );
    assert_eq!(
        reason(vda, Choice::ShrinkExt4KeepHome),
        "the disk has no ext4 partition"
    );
    assert_eq!(
        reason(vda, Choice::BesideWindows),
        "the disk has no EFI partition"
    );
}

#[test]
fn a_disk_too_small_for_the_smallest_layout_offers_nothing() {
    let host = machine(vec![disk("/dev/vda", "", 16 * GIB, vec![])]);
    let vda = &disks(&host).unwrap()[0];
    assert!(reason(vda, Choice::WipeDisk).starts_with("the disk is too small"));
    assert!(vda.offers.iter().all(|offer| offer.available.is_err()));
}

/// A 64 GiB disk with an InayOS whose system and home partitions hold `system` and `home`.
fn inayos_disk(system: &str, home: &str) -> Value {
    disk(
        "/dev/vda",
        "",
        64 * GIB,
        vec![
            part("/dev/vda1", 2048, 2 * GIB, Some("vfat"), Some("EFI"), EFI),
            part(
                "/dev/vda2",
                4_196_352,
                40 * GIB,
                Some(system),
                Some("inayos-system"),
                LINUX,
            ),
            part(
                "/dev/vda3",
                88_082_432,
                22 * GIB,
                Some(home),
                Some("inayos-home"),
                LINUX,
            ),
        ],
    )
}

#[test]
fn an_inayos_disk_offers_the_reinstall_with_its_three_partitions() {
    let host = machine(vec![inayos_disk("btrfs", "ext4")]);
    let vda = &disks(&host).unwrap()[0];
    assert_eq!(
        offer(vda, Choice::ReinstallKeepHome),
        &Offer {
            choice: Choice::ReinstallKeepHome,
            available: Ok(Partitions {
                efi: Some("/dev/vda1".into()),
                system: Some("/dev/vda2".into()),
                home: Some("/dev/vda3".into()),
            }),
            system_size: None,
            windows: None,
        }
    );
    assert!(offer(vda, Choice::WipeDisk).available.is_ok());
    // The InayOS home has no `home` folder at its top level, which the shrink needs.
    assert_eq!(
        reason(vda, Choice::ShrinkExt4KeepHome),
        "the disk holds InayOS; reinstalling keeps its home partition"
    );
}

#[test]
fn every_offer_leaves_home_room_for_a_swap_file_as_large_as_the_memory() {
    let host = machine(vec![inayos_disk("btrfs", "ext4")]);
    host.file("/proc/meminfo", "MemTotal:        8000000 kB\n");
    // 7 GiB free on the kept home besides no swap file.
    host.respond(
        "dumpe2fs -h /dev/vda3",
        [ok(&format!(
            "Block count: {}\nFree blocks: {}\nBlock size: 4096\n",
            22 * GIB / 4096,
            7 * GIB / 4096
        ))],
    );
    let vda = &disks(&host).unwrap()[0];
    assert_eq!(
        reason(vda, Choice::ReinstallKeepHome),
        "the InayOS home partition /dev/vda3 has no room for the 8 GiB hibernation swap \
         file, the size of the memory"
    );
    // 64 GiB less 2 GiB of EFI, 20 GiB of home and 8 GiB of swap file leaves 33 GiB of
    // system once the partitions are aligned and the GPT has its room.
    assert_eq!(
        offer(vda, Choice::WipeDisk)
            .system_size
            .map(|limits| limits.max_gib),
        Some(33)
    );
}

/// A laptop with a 512 MiB EFI partition and one 100 GiB ext4 partition holding 30 GiB.
fn laptop(after_home: Vec<Value>) -> FakeHost {
    let mut parts = vec![
        part("/dev/nvme0n1p1", 2048, 512 * MIB, Some("vfat"), None, EFI),
        part(
            "/dev/nvme0n1p2",
            1_050_624,
            100 * GIB,
            Some("ext4"),
            None,
            LINUX,
        ),
    ];
    parts.extend(after_home);
    let host = machine(vec![disk(
        "/dev/nvme0n1",
        "SAMSUNG MZAL81T0HFLB",
        120 * GIB,
        parts,
    )]);
    host.respond(
        "dumpe2fs -h /dev/nvme0n1p2",
        [ok(&format!(
            "Block count:              {}\nFree blocks:              {}\nBlock size:               4096\n",
            100 * GIB / 4096,
            70 * GIB / 4096
        ))],
    );
    host
}

#[test]
fn a_laptop_offers_the_shrink_with_a_size_that_leaves_its_files_room() {
    let nvme = &disks(&laptop(vec![])).unwrap()[0];
    // 30 GiB of files need a home of at least 33.4 GiB to keep 10% free, so at most 66 of
    // the 100 GiB can go to the system.
    assert_eq!(
        offer(nvme, Choice::ShrinkExt4KeepHome),
        &Offer {
            choice: Choice::ShrinkExt4KeepHome,
            available: Ok(Partitions {
                efi: Some("/dev/nvme0n1p1".into()),
                system: None,
                home: Some("/dev/nvme0n1p2".into()),
            }),
            system_size: Some(SizeLimits {
                min_gib: 20,
                max_gib: 66
            }),
            windows: None,
        }
    );
    assert_eq!(
        SizeLimits {
            min_gib: 20,
            max_gib: 66
        }
        .clamp(150),
        66
    );
    assert_eq!(
        reason(nvme, Choice::ReinstallKeepHome),
        "the disk holds no earlier InayOS"
    );
}

#[test]
fn a_laptop_after_a_shrink_install_offers_the_reinstall_of_its_kept_home() {
    // The shrink labels the kept ext4 partition inayos-home and puts the system after it.
    let host = machine(vec![disk(
        "/dev/nvme0n1",
        "SAMSUNG MZAL81T0HFLB",
        120 * GIB,
        vec![
            part("/dev/nvme0n1p1", 2048, 512 * MIB, Some("vfat"), None, EFI),
            part(
                "/dev/nvme0n1p2",
                1_050_624,
                60 * GIB,
                Some("ext4"),
                Some("inayos-home"),
                LINUX,
            ),
            part(
                "/dev/nvme0n1p3",
                1_050_624 + 60 * GIB / 512,
                40 * GIB,
                Some("btrfs"),
                Some("inayos-system"),
                LINUX,
            ),
        ],
    )]);
    let nvme = &disks(&host).unwrap()[0];
    assert_eq!(
        offer(nvme, Choice::ReinstallKeepHome).available,
        Ok(Partitions {
            efi: Some("/dev/nvme0n1p1".into()),
            system: Some("/dev/nvme0n1p3".into()),
            home: Some("/dev/nvme0n1p2".into()),
        })
    );
    assert_eq!(
        reason(nvme, Choice::ShrinkExt4KeepHome),
        "the disk holds InayOS; reinstalling keeps its home partition"
    );
}

#[test]
fn a_partition_after_the_ext4_partition_refuses_the_shrink() {
    let swap = part(
        "/dev/nvme0n1p3",
        1_050_624 + 100 * GIB / 512,
        8 * GIB,
        Some("swap"),
        None,
        LINUX,
    );
    let nvme = &disks(&laptop(vec![swap])).unwrap()[0];
    assert_eq!(
        reason(nvme, Choice::ShrinkExt4KeepHome),
        "a partition follows /dev/nvme0n1p2, so the system has no room after it"
    );
}

#[test]
fn a_mounted_partition_refuses_every_choice_that_changes_the_disk() {
    let host = machine(vec![disk(
        "/dev/vda",
        "",
        64 * GIB,
        vec![mounted(
            part("/dev/vda1", 2048, 60 * GIB, Some("ext4"), None, LINUX),
            "/mnt",
        )],
    )]);
    let vda = &disks(&host).unwrap()[0];
    assert_eq!(reason(vda, Choice::WipeDisk), "/dev/vda1 is in use at /mnt");
    assert_eq!(
        reason(vda, Choice::ShrinkExt4KeepHome),
        "/dev/vda1 is in use at /mnt"
    );
}

/// A 256 GiB disk as Windows 11 sets it up: a 100 MiB EFI partition, the Microsoft reserved
/// partition, Windows with `windows_fstype` on 250 GiB, and the recovery partition after it.
fn windows_disk(windows_fstype: &str) -> Value {
    disk(
        "/dev/nvme0n1",
        "WD Blue",
        256 * GIB,
        vec![
            part("/dev/nvme0n1p1", 2048, 100 * MIB, Some("vfat"), None, EFI),
            part(
                "/dev/nvme0n1p2",
                206_848,
                16 * MIB,
                None,
                None,
                MICROSOFT_RESERVED,
            ),
            part(
                "/dev/nvme0n1p3",
                239_616,
                250 * GIB,
                Some(windows_fstype),
                None,
                MICROSOFT_DATA,
            ),
            part(
                "/dev/nvme0n1p4",
                524_527_616,
                700 * MIB,
                Some("ntfs"),
                None,
                "de94bba4-06d1-4d40-a16a-bfd50179d6ac",
            ),
        ],
    )
}

const ESP_LOOK: &str = "/run/inayos-installer/esp";
const DF: &str = "df --block-size=1 --output=avail /run/inayos-installer/esp";
const PROBE: &str = "ntfs-3g.probe --readwrite /dev/nvme0n1p3";
const NTFSRESIZE: &str = "ntfsresize --info --force --no-action --no-progress-bar /dev/nvme0n1p3";

/// The machine with that disk, whose EFI partition holds the Windows Boot Manager and has
/// 70 MiB free, and whose Windows files take 30 GiB.
fn windows_machine(windows_fstype: &str) -> FakeHost {
    let host = machine(vec![windows_disk(windows_fstype)]);
    host.file(
        &format!("{ESP_LOOK}/EFI/Microsoft/Boot/bootmgfw.efi"),
        "boot manager",
    );
    host.respond(DF, [ok(&format!("Avail\n{}\n", 70 * MIB))]);
    host.respond(
        NTFSRESIZE,
        [ok(&format!(
            "Current volume size: {} bytes (268435 MB)\n\
             You might resize at {} bytes or 32212 MB (freeing 236223 MB).\n",
            250 * GIB - 4096,
            30 * GIB
        ))],
    );
    host
}

#[test]
fn a_windows_disk_offers_to_go_beside_windows_and_names_why_the_others_do_not_fit() {
    let host = windows_machine("ntfs");
    let nvme = &disks(&host).unwrap()[0];
    assert!(offer(nvme, Choice::WipeDisk).available.is_ok());
    assert_eq!(
        reason(nvme, Choice::ReinstallKeepHome),
        "the disk holds no earlier InayOS"
    );
    assert_eq!(
        reason(nvme, Choice::ShrinkExt4KeepHome),
        "the disk has no ext4 partition"
    );
    let beside = offer(nvme, Choice::BesideWindows);
    assert_eq!(
        beside.available,
        Ok(Partitions {
            efi: Some("/dev/nvme0n1p1".into()),
            system: None,
            home: None,
        })
    );
    // Windows keeps its 30 GiB of files and 20 GiB, home at least 20 GiB, and the system
    // and home start on 1 MiB boundaries: 179 whole GiB are left for the system.
    assert_eq!(
        beside.system_size,
        Some(SizeLimits {
            min_gib: 20,
            max_gib: 179
        })
    );
    let windows = beside.windows.as_ref().unwrap();
    assert_eq!(windows.partition, "/dev/nvme0n1p3");
    assert_eq!(windows.size_bytes, 250 * GIB);
    // With a 50 GiB system, Windows and home share the 200 GiB left evenly.
    assert_eq!(windows.size_after(50), Some(100 * GIB - MIB));
    assert_eq!(windows.size_after(150), Some(50 * GIB));
    assert_eq!(windows.size_after(180), None);
    // The EFI partition was mounted read-only and unmounted again.
    let actions = host.actions();
    assert_eq!(
        actions,
        [
            "mount -o ro /dev/nvme0n1p1 /run/inayos-installer/esp",
            "umount /run/inayos-installer/esp"
        ]
    );
}

#[test]
fn going_beside_windows_is_refused_with_instructions_when_windows_cannot_shrink_safely() {
    let beside = |host: &FakeHost| reason(&disks(host).unwrap()[0], Choice::BesideWindows);
    let bitlocker = "BitLocker encrypts the Windows partition /dev/nvme0n1p3. Turn off \
                     BitLocker in Windows (Settings, Privacy and security, Device encryption, \
                     or Control Panel, BitLocker Drive Encryption), wait until decryption \
                     finishes, then start the installer again";
    assert_eq!(beside(&windows_machine("BitLocker")), bitlocker);
    // A BitLocker partition that blkid does not name still has the signature.
    let host = windows_machine("ntfs");
    host.respond(
        "dd if=/dev/nvme0n1p3 bs=1 skip=3 count=8 status=none",
        [ok("-FVE-FS-")],
    );
    assert_eq!(beside(&host), bitlocker);

    let fast_startup = "Windows did not shut down fully, so its partition /dev/nvme0n1p3 \
                        cannot change. Turn off Fast Startup in Windows (Control Panel, Power \
                        Options, Choose what the power buttons do, untick Turn on fast \
                        startup), then shut Windows down with Shut down, not Restart, and start \
                        the installer again";
    for code in [14, 15] {
        let host = windows_machine("ntfs");
        host.respond(PROBE, [crate::fake::exit(code)]);
        assert_eq!(beside(&host), fast_startup);
    }

    let host = windows_machine("ntfs");
    host.respond(DF, [ok(&format!("Avail\n{}\n", 30 * MIB))]);
    assert_eq!(
        beside(&host),
        "the EFI partition /dev/nvme0n1p1 has 30 MiB free, and rEFInd needs 50 MiB"
    );

    let host = machine(vec![windows_disk("ntfs")]);
    assert_eq!(
        beside(&host),
        "the EFI partition /dev/nvme0n1p1 holds no Windows Boot Manager"
    );

    // 215 GiB of files leave Windows 235 GiB, and InayOS needs 40 GiB and the alignment.
    let host = windows_machine("ntfs");
    host.respond(
        NTFSRESIZE,
        [ok(&format!(
            "Current volume size: {} bytes\nYou might resize at {} bytes\n",
            250 * GIB,
            215 * GIB
        ))],
    );
    assert_eq!(
        beside(&host),
        "Windows keeps its files and 20 GiB free, which leaves 15.0 GiB, and InayOS needs \
         40.0 GiB: the system, 20 GiB of home and the 0 GiB hibernation swap file"
    );
}

#[test]
fn only_a_version_2_tpm_counts() {
    let host = FakeHost::default();
    assert!(!has_tpm2(&host));
    host.file("/sys/class/tpm/tpm0/tpm_version_major", "1\n");
    assert!(!has_tpm2(&host));
    host.file("/sys/class/tpm/tpm0/tpm_version_major", "2\n");
    assert!(has_tpm2(&host));
}

const HIBERNATED: &str =
    "This disk holds a hibernated InayOS: start it and shut it down properly first";

#[test]
fn a_hibernated_inayos_makes_every_offer_of_its_disk_unavailable() {
    let every_offer_refused = |info: &DiskInfo| {
        Choice::ALL
            .into_iter()
            .all(|choice| reason(info, choice) == HIBERNATED)
    };
    // The firmware variable does not say which disk, so it refuses every disk.
    let host = machine(vec![inayos_disk("btrfs", "ext4")]);
    host.file(
        "/sys/firmware/efi/efivars/HibernateLocation-8cf2644b-4b0b-428f-9387-6d876050dc67",
        "{}",
    );
    assert!(disks(&host).unwrap().iter().all(every_offer_refused));

    // The swap file in the InayOS home of one disk refuses only that disk.
    let host = machine(vec![inayos_disk("btrfs", "ext4")]);
    host.respond("debugfs -R 'bmap /swapfile 0' /dev/vda3", [ok("1000\n")]);
    host.respond(
        "dumpe2fs -h /dev/vda3",
        [ok(
            "Block count: 5000000\nFree blocks: 4000000\nBlock size: 4096\n",
        )],
    );
    let dd = "dd if=/dev/vda3 iflag=skip_bytes,count_bytes skip=4096000 count=4096 status=none";
    host.respond(dd, [ok(&format!("{}S1SUSPEND", "\0".repeat(4086)))]);
    assert!(every_offer_refused(&disks(&host).unwrap()[0]));

    host.respond(dd, [ok(&format!("{}SWAPSPACE2", "\0".repeat(4086)))]);
    let vda = &disks(&host).unwrap()[0];
    assert!(offer(vda, Choice::ReinstallKeepHome).available.is_ok());
}

#[test]
fn an_encrypted_inayos_offers_the_wipe_but_not_the_reinstall() {
    let host = machine(vec![inayos_disk("crypto_LUKS", "crypto_LUKS")]);
    let vda = &disks(&host).unwrap()[0];
    assert!(offer(vda, Choice::WipeDisk).available.is_ok());
    assert!(reason(vda, Choice::ReinstallKeepHome).contains("encrypted"));
}
