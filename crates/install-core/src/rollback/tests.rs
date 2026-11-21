//! The rollback's command plan and the subvolumes it leaves, run against the recording fake.
use super::*;
use crate::fake::{FakeHost, FakeSink, exit, ok};

const TPM_SYSTEM: &str =
    "cryptsetup open --token-only --token-type systemd-tpm2 /dev/vda2 inayos-rollback-system";
const TPM_HOME: &str = "cryptsetup open --readonly --token-only --token-type systemd-tpm2 \
                        /dev/vda3 inayos-rollback-home";
const MKINITCPIO: &str = "arch-chroot /run/inayos-rollback/root mkinitcpio -P";
const SNAPSHOT: &str = "btrfs subvolume snapshot /run/inayos-rollback/top/@snapshots/7/snapshot \
                        /run/inayos-rollback/top/@";

fn info(number: u32, kind: &str, description: &str) -> String {
    format!(
        "<?xml version=\"1.0\"?>\n<snapshot>\n  <type>{kind}</type>\n  <num>{number}</num>\n  \
         <date>2025-11-18 09:30:00</date>\n  <description>{description}</description>\n  \
         <cleanup>number</cleanup>\n</snapshot>\n"
    )
}

fn plain() -> InstalledSystem {
    InstalledSystem {
        disk: "/dev/vda".to_owned(),
        system: "/dev/vda2".to_owned(),
        home: Some("/dev/vda3".to_owned()),
        efi: Some("/dev/vda1".to_owned()),
        encrypted: false,
    }
}

fn encrypted() -> InstalledSystem {
    InstalledSystem {
        encrypted: true,
        ..plain()
    }
}

/// The repair workspace with the system's top level as `open` mounts it: `@`, and the
/// snapshots 6 and 7 of a pacman transaction.
fn machine() -> FakeHost {
    let host = FakeHost::default();
    host.file("/run/inayos-rollback/top/@/etc/hostname", "inayos\n");
    for (number, kind) in [(6, "pre"), (7, "post")] {
        let folder = format!("/run/inayos-rollback/top/@snapshots/{number}");
        host.dir(&format!("{folder}/snapshot"), (0, 0));
        host.file(
            &format!("{folder}/info.xml"),
            &info(number, kind, "pacman -S foo &amp; bar"),
        );
    }
    // A folder that snapper left without its snapshot.
    host.file(
        "/run/inayos-rollback/top/@snapshots/8/info.xml",
        &info(8, "single", ""),
    );
    host.respond("date -u +%Y%m%d-%H%M%S", [ok("20251119-101500\n")]);
    host
}

const PRESET: &str = "default_uki=\"/efi/EFI/Linux/inayos-linux.efi\"\n";
const IMAGE: &str = "/run/inayos-rollback/root/efi/EFI/Linux/inayos-linux.efi";
const IMAGE_BEFORE: &str =
    "/run/inayos-rollback/root/efi/EFI/Linux/inayos-linux.efi.before-rollback";

/// `machine` for an encrypted system: snapshot 7 holds the InayOS preset, and the EFI
/// partition holds the current image once the chroot mounts it.
fn encrypted_machine() -> FakeHost {
    let host = machine();
    host.file(
        "/run/inayos-rollback/top/@snapshots/7/snapshot/etc/mkinitcpio.d/linux.preset",
        PRESET,
    );
    host.file(IMAGE, "current image");
    host
}

/// Gives the current system and snapshot `number` sbctl's keys.
fn with_keys(host: &FakeHost, numbers: &[u32]) {
    host.dir("/run/inayos-rollback/top/@/var/lib/sbctl/keys", (0, 0));
    for number in numbers {
        host.dir(
            &format!("/run/inayos-rollback/top/@snapshots/{number}/snapshot/var/lib/sbctl/keys"),
            (0, 0),
        );
    }
}

#[test]
fn the_disks_with_an_inayos_system_are_found_with_their_partitions() {
    let host = FakeHost::default();
    host.respond(
        "lsblk --json --bytes --output NAME,PATH,TYPE,MODEL,SIZE,START,LOG-SEC,FSTYPE,LABEL,PARTTYPE,MOUNTPOINTS",
        [ok(r#"{"blockdevices":[
            {"path":"/dev/vda","type":"disk","children":[
              {"path":"/dev/vda1","type":"part","fstype":"vfat","parttype":"C12A7328-F81F-11D2-BA4B-00A0C93EC93B"},
              {"path":"/dev/vda2","type":"part","fstype":"btrfs","label":"inayos-system"},
              {"path":"/dev/vda3","type":"part","fstype":"ext4","label":"inayos-home"}]},
            {"path":"/dev/vdb","type":"disk","children":[
              {"path":"/dev/vdb1","type":"part","fstype":"vfat","parttype":"c12a7328-f81f-11d2-ba4b-00a0c93ec93b"},
              {"path":"/dev/vdb2","type":"part","fstype":"crypto_LUKS","label":"inayos-system"},
              {"path":"/dev/vdb3","type":"part","fstype":"crypto_LUKS","label":"inayos-home"}]},
            {"path":"/dev/vdc","type":"disk","children":[
              {"path":"/dev/vdc1","type":"part","fstype":"ntfs","label":"Windows"}]}]}"#)],
    );
    let found = find(&host, &FakeSink::default()).unwrap();
    assert_eq!(
        found,
        [
            plain(),
            InstalledSystem {
                disk: "/dev/vdb".to_owned(),
                system: "/dev/vdb2".to_owned(),
                home: Some("/dev/vdb3".to_owned()),
                efi: Some("/dev/vdb1".to_owned()),
                encrypted: true,
            }
        ]
    );
}

#[test]
fn a_plain_system_gets_the_snapshot_as_its_system_and_keeps_the_old_one() {
    let host = machine();
    host.file(
        "/run/inayos-rollback/top/@snapshots/7/snapshot/etc/fstab",
        "UUID=1 / btrfs rw,compress=zstd:3,subvolid=256,subvol=/@ 0 0\n",
    );
    host.respond("mountpoint -q /run/inayos-rollback/root", [exit(32)]);
    let sink = FakeSink::default();
    let opened = open(&host, &sink, &plain(), Unlock::Tpm).unwrap();
    let numbers: Vec<u32> = opened
        .snapshots()
        .unwrap()
        .iter()
        .map(|snapshot| snapshot.number)
        .collect();
    assert_eq!(numbers, [6, 7]);
    let done = opened.roll_back(7).unwrap();
    assert!(opened.close().is_empty());
    assert_eq!(done.kept, "@rollback-20251119-101500");
    assert_eq!(
        host.text("/run/inayos-rollback/top/@rollback-20251119-101500/etc/hostname")
            .as_deref(),
        Some("inayos\n")
    );
    assert_eq!(
        host.text("/run/inayos-rollback/top/@/etc/fstab").as_deref(),
        Some("UUID=1\t/\tbtrfs\trw,compress=zstd:3,subvol=/@\t0\t0\n")
    );
    assert_eq!(
        host.actions(),
        [
            "mount -o subvolid=5 /dev/vda2 /run/inayos-rollback/top",
            SNAPSHOT,
            "umount /run/inayos-rollback/top",
        ]
    );
}

#[test]
fn an_encrypted_system_gets_a_signed_boot_image_of_the_snapshot() {
    let host = encrypted_machine();
    with_keys(&host, &[7]);
    let sink = FakeSink::default();
    let opened = open(&host, &sink, &encrypted(), Unlock::Tpm).unwrap();
    opened.roll_back(7).unwrap();
    assert!(!host.exists(Path::new(IMAGE_BEFORE)));
    assert_eq!(
        host.actions(),
        [
            TPM_HOME,
            "cryptsetup close inayos-rollback-home",
            TPM_SYSTEM,
            "mount -o subvolid=5 /dev/mapper/inayos-rollback-system /run/inayos-rollback/top",
            SNAPSHOT,
            "mount -o subvol=@ /dev/mapper/inayos-rollback-system /run/inayos-rollback/root",
            "mount /dev/vda1 /run/inayos-rollback/root/efi",
            MKINITCPIO,
            "arch-chroot /run/inayos-rollback/root sbctl sign-all",
        ]
    );
}

#[test]
fn a_tpm_that_refuses_asks_for_the_recovery_key_which_opens_both_volumes() {
    let host = machine();
    host.respond(TPM_HOME, [exit(1)]);
    let sink = FakeSink::default();
    let refused = open(&host, &sink, &encrypted(), Unlock::Tpm);
    assert!(matches!(
        refused,
        Err(RollbackError::TpmRefused { partition }) if partition == "/dev/vda3"
    ));
    open(
        &host,
        &sink,
        &encrypted(),
        Unlock::RecoveryKey("abcd-efgh\n"),
    )
    .unwrap();
    let inputs = host.inputs.borrow();
    assert_eq!(
        inputs
            .iter()
            .map(|(command, key)| (command.as_str(), key.as_str()))
            .collect::<Vec<_>>(),
        [
            (
                "cryptsetup open --readonly --key-file=- /dev/vda3 inayos-rollback-home",
                "abcd-efgh"
            ),
            (
                "cryptsetup open --key-file=- /dev/vda2 inayos-rollback-system",
                "abcd-efgh"
            ),
        ]
    );
}

#[test]
fn a_hibernated_machine_is_refused_before_anything_opens() {
    let host = machine();
    host.file(hibernation::HIBERNATE_LOCATION, "");
    let sink = FakeSink::default();
    let refused = open(&host, &sink, &encrypted(), Unlock::Tpm);
    assert!(matches!(refused, Err(RollbackError::Hibernated)));
    assert!(host.actions().is_empty());
}

#[test]
fn a_failed_boot_image_puts_the_old_system_and_its_image_back() {
    let host = encrypted_machine();
    host.respond(MKINITCPIO, [exit(1)]);
    host.writes_when_run(MKINITCPIO, IMAGE, "half-written image");
    let sink = FakeSink::default();
    let opened = open(&host, &sink, &encrypted(), Unlock::Tpm).unwrap();
    let failed = opened.roll_back(7);
    assert!(matches!(failed, Err(RollbackError::Undone(_))));
    assert_eq!(
        host.text("/run/inayos-rollback/top/@/etc/hostname")
            .as_deref(),
        Some("inayos\n")
    );
    assert!(!host.exists(Path::new(
        "/run/inayos-rollback/top/@rollback-20251119-101500"
    )));
    assert_eq!(host.text(IMAGE).as_deref(), Some("current image"));
    assert!(!host.exists(Path::new(IMAGE_BEFORE)));
    let actions = host.actions();
    assert_eq!(
        &actions[actions.len() - 2..],
        [
            "umount -R /run/inayos-rollback/root",
            "btrfs subvolume delete /run/inayos-rollback/top/@",
        ]
    );
}

#[test]
fn a_snapshot_from_before_secure_boot_is_refused_on_a_signed_system() {
    let host = machine();
    with_keys(&host, &[6]);
    let sink = FakeSink::default();
    let opened = open(&host, &sink, &plain(), Unlock::Tpm).unwrap();
    assert!(matches!(
        opened.roll_back(7),
        Err(RollbackError::Unsuitable { number: 7, .. })
    ));
    assert!(host.exists(Path::new("/run/inayos-rollback/top/@/etc/hostname")));
    assert_eq!(
        host.actions(),
        ["mount -o subvolid=5 /dev/vda2 /run/inayos-rollback/top"]
    );
}

#[test]
fn an_encrypted_snapshot_from_before_its_boot_image_preset_is_refused() {
    let host = encrypted_machine();
    let sink = FakeSink::default();
    let opened = open(&host, &sink, &encrypted(), Unlock::Tpm).unwrap();
    assert!(matches!(
        opened.roll_back(6),
        Err(RollbackError::Unsuitable { number: 6, .. })
    ));
    assert!(host.exists(Path::new("/run/inayos-rollback/top/@/etc/hostname")));
    assert!(
        !host
            .actions()
            .iter()
            .any(|action| action.starts_with("btrfs"))
    );
}

#[test]
fn the_fstab_loses_its_subvolume_ids_and_keeps_the_rest() {
    let fstab = "# /dev/vda2\n\
        UUID=1\t/\tbtrfs\trw,subvolid=256,subvol=/@\t0 0\n\
        UUID=1 /.snapshots btrfs rw,subvolid=257,subvol=/@snapshots 0 0\n\
        UUID=2 /home ext4 rw,relatime 0 2\n";
    assert_eq!(
        without_subvolume_ids(fstab),
        "# /dev/vda2\n\
         UUID=1\t/\tbtrfs\trw,subvol=/@\t0\t0\n\
         UUID=1\t/.snapshots\tbtrfs\trw,subvol=/@snapshots\t0\t0\n\
         UUID=2 /home ext4 rw,relatime 0 2\n"
    );
}

#[test]
fn a_missing_snapshot_changes_nothing() {
    let host = machine();
    let sink = FakeSink::default();
    let opened = open(&host, &sink, &plain(), Unlock::Tpm).unwrap();
    assert!(matches!(
        opened.roll_back(8),
        Err(RollbackError::NoSnapshot { number: 8 })
    ));
    assert!(host.exists(Path::new("/run/inayos-rollback/top/@/etc/hostname")));
}

#[test]
fn snappers_description_reads_with_its_entities_decoded() {
    assert_eq!(
        snapshot_info(&info(12, "pre", "pacman -S foo &amp; bar")),
        Some(Snapshot {
            number: 12,
            kind: "pre".to_owned(),
            date: "2025-11-18 09:30:00".to_owned(),
            description: "pacman -S foo & bar".to_owned(),
        })
    );
    assert_eq!(snapshot_info("<snapshot><type>pre</type></snapshot>"), None);
}
