//! The install's command plan and the files it leaves, run against the recording fake.
use crate::{
    Answers, Error, Stage,
    fake::{FakeHost, FakeSink, Node, exit, ok},
    install,
    install::Settings,
    sizes::GIB,
};

const PASSWORD: &str = "s3cret-pass";
const WIFI_PASSWORD: &str = "w1fi-pass";
const INSTALL: &str = "arch-chroot -S -u tester /mnt env SYSTEMD_OFFLINE=1 TMPDIR=/var/tmp /usr/local/bin/inayos \
                       install --installer-mode --answers /var/tmp/inayos-answers.json";
const VERIFY: &str = "arch-chroot -S -u tester /mnt env SYSTEMD_OFFLINE=1 TMPDIR=/var/tmp /usr/local/bin/inayos \
                      verify --installer-mode";

fn answers(disk: &str, extra: &str) -> Answers {
    Answers::parse(&format!(
        r#"
keyboard = "us"
language = "en_US.UTF-8"
timezone = "Europe/Paris"
hostname = "inayos"
{extra}
[disk]
{disk}
[user]
full_name = "Tester"
username = "tester"
password = "{PASSWORD}"
authorized_keys = ["ssh-ed25519 AAAA one", "ssh-ed25519 BBBB two"]
[questions]
"Turn it on now?" = true
"#
    ))
    .unwrap()
}

fn wipe() -> Answers {
    answers(
        "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
        "",
    )
}

fn settings() -> Settings {
    Settings {
        system_binary: "/usr/local/lib/inayos/inayos".into(),
        log_file: Some("/var/log/inayos-installer.log".into()),
    }
}

const LSBLK: &str = r#"{"blockdevices":[{"path":"/dev/vda","type":"disk","mountpoints":[null],
    "children":[{"path":"/dev/vda1","type":"part","mountpoints":[null]},
                {"path":"/dev/vda2","type":"part","mountpoints":[null]}]}]}"#;

/// A live USB system booted with UEFI, with a 64 GiB disk at /dev/vda and a new system whose
/// files `pacstrap` and `useradd` would create already in place.
fn live_system() -> FakeHost {
    let host = FakeHost::default();
    host.dir("/sys/firmware/efi/efivars", (0, 0));
    host.file("/usr/local/lib/inayos/inayos", "binary");
    host.file("/usr/share/zoneinfo/Europe/Paris", "");
    host.file("/etc/locale.gen", "#en_US.UTF-8 UTF-8\n");
    host.file("/proc/cpuinfo", "vendor_id\t: GenuineIntel\n");
    host.file("/var/log/inayos-installer.log", "the log");
    host.file(
        "/etc/NetworkManager/system-connections/Home.nmconnection",
        "[connection]",
    );
    host.file(
        "/mnt/etc/locale.gen",
        "#en_GB.UTF-8 UTF-8\n#en_US.UTF-8 UTF-8\n",
    );
    host.file(
        "/mnt/etc/passwd",
        "root:x:0:0::/root:/bin/bash\ntester:x:1000:1000:Tester:/home/tester:/bin/bash\n",
    );
    host.file(
        "/mnt/etc/group",
        "root:x:0:root\nusers:x:100:\nwheel:x:998:\n",
    );
    host.dir("/mnt/var/log", (0, 0));
    host.dir("/mnt/var/tmp", (0, 0));
    host.respond("mountpoint -q /mnt", [exit(1), ok("")]);
    host.respond(
        "lsblk --json --output PATH,TYPE,MOUNTPOINTS /dev/vda",
        [ok(LSBLK)],
    );
    host.respond("blockdev --getss /dev/vda", [ok("512\n")]);
    host.respond(
        "blockdev --getsize64 /dev/vda",
        [ok(&format!("{}\n", 64 * GIB))],
    );
    host.respond("genfstab -U /mnt", [ok("UUID=1 / btrfs subvol=/@ 0 0\n")]);
    host
}

fn btrfs(partition: &str) -> Vec<String> {
    let mut commands = vec![
        format!("mkfs.btrfs -f -L inayos-system {partition}"),
        format!("mount {partition} /mnt"),
    ];
    for subvolume in [
        "@",
        "@snapshots",
        "@var_cache",
        "@var_log",
        "@var_lib_docker",
    ] {
        commands.push(format!("btrfs subvolume create /mnt/{subvolume}"));
    }
    commands.push("umount /mnt".into());
    commands
}

fn mounts(system: &str) -> Vec<String> {
    [
        ("@", "/mnt"),
        ("@snapshots", "/mnt/.snapshots"),
        ("@var_cache", "/mnt/var/cache"),
        ("@var_log", "/mnt/var/log"),
        ("@var_lib_docker", "/mnt/var/lib/docker"),
    ]
    .iter()
    .map(|(subvolume, point)| {
        format!("mount -o compress=zstd:1,subvol={subvolume} {system} {point}")
    })
    .collect()
}

fn base_system(ucode: &str) -> Vec<String> {
    vec![
        format!(
            "pacstrap -K /mnt base linux linux-firmware {ucode}btrfs-progs e2fsprogs dosfstools sudo networkmanager"
        ),
        "systemd-firstboot --root=/mnt --force --locale=en_US.UTF-8 --keymap=us \
         --timezone=Europe/Paris --hostname=inayos"
            .into(),
        "arch-chroot /mnt locale-gen".into(),
        "systemctl --root=/mnt enable NetworkManager.service".into(),
    ]
}

#[test]
fn a_wiped_disk_gets_the_default_layout_a_base_system_a_user_and_the_modules() {
    let host = live_system();
    let sink = FakeSink::default();
    let answers = answers(
        "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
        &format!("[network]\nwifi_ssid = \"Home\"\nwifi_password = \"{WIFI_PASSWORD}\""),
    );
    install(&answers, &settings(), &host, &sink).unwrap();

    // 2 GiB of EFI from 1 MiB, then 40 GiB of system, then home to the end of the disk.
    let system_last = 4_196_352 + 40 * GIB / 512 - 1;
    let mut expected: Vec<String> = vec![
        "nmcli device wifi connect Home password <hidden>".into(),
        "pacman -Sy".into(),
        "sgdisk --zap-all /dev/vda".into(),
        "sgdisk --new=1:2048:4196351 --typecode=1:EF00 --change-name=1:EFI /dev/vda".into(),
        format!(
            "sgdisk --new=2:4196352:{system_last} --typecode=2:8300 --change-name=2:inayos-system /dev/vda"
        ),
        format!(
            "sgdisk --new=3:{}:0 --typecode=3:8300 --change-name=3:inayos-home /dev/vda",
            system_last + 1
        ),
        "udevadm settle".into(),
        "mkfs.fat -F 32 -n EFI /dev/vda1".into(),
    ];
    expected.extend(btrfs("/dev/vda2"));
    expected.push("mkfs.ext4 -F -L inayos-home /dev/vda3".into());
    expected.extend(mounts("/dev/vda2"));
    expected.push("mount /dev/vda1 /mnt/efi".into());
    expected.push("mount /dev/vda3 /mnt/home".into());
    expected.extend(base_system("intel-ucode "));
    expected.extend([
        "arch-chroot /mnt useradd -m -G wheel -c Tester tester".into(),
        "arch-chroot /mnt chpasswd".into(),
        INSTALL.into(),
        VERIFY.into(),
        "umount -R /mnt".into(),
    ]);
    assert_eq!(host.actions(), expected);

    assert_eq!(
        host.inputs.borrow().as_slice(),
        [(
            "arch-chroot /mnt chpasswd".to_owned(),
            format!("tester:{PASSWORD}\n")
        )]
    );
    assert_eq!(
        host.text("/mnt/etc/fstab").as_deref(),
        Some("UUID=1 / btrfs subvol=/@ 0 0\n")
    );
    assert_eq!(
        host.text("/mnt/etc/locale.gen").as_deref(),
        Some("#en_GB.UTF-8 UTF-8\nen_US.UTF-8 UTF-8\n")
    );
    assert_eq!(
        host.node("/mnt/etc/sudoers.d/10-inayos-wheel"),
        Some(Node::File {
            text: "%wheel ALL=(ALL:ALL) ALL\n".into(),
            mode: 0o440,
            owner: (0, 0)
        })
    );
    assert_eq!(
        host.node("/mnt/home/tester/.ssh/authorized_keys"),
        Some(Node::File {
            text: "ssh-ed25519 AAAA one\nssh-ed25519 BBBB two\n".into(),
            mode: 0o600,
            owner: (1000, 1000)
        })
    );
    assert_eq!(
        host.node("/mnt/home/tester/.ssh"),
        Some(Node::Dir {
            mode: 0o700,
            owner: (1000, 1000)
        })
    );
    assert_eq!(
        host.node("/mnt/usr/local/bin/inayos"),
        Some(Node::File {
            text: "binary".into(),
            mode: 0o755,
            owner: (0, 0)
        })
    );
    assert_eq!(
        host.node("/mnt/etc/NetworkManager/system-connections/Home.nmconnection"),
        Some(Node::File {
            text: "[connection]".into(),
            mode: 0o600,
            owner: (0, 0)
        })
    );
    assert_eq!(
        host.text("/mnt/var/log/inayos-installer.log").as_deref(),
        Some("the log")
    );
    // The questions reached the new system for the install, owned by the user, then left it
    // with the installer's sudo rule.
    let log = host.log.borrow().clone();
    let questions = log
        .iter()
        .position(|line| line == "write /mnt/var/tmp/inayos-answers.json 600")
        .unwrap();
    assert_eq!(
        log[questions + 1],
        "chown /mnt/var/tmp/inayos-answers.json 1000:1000"
    );
    assert!(log.contains(&"write /mnt/etc/sudoers.d/99-inayos-installer 440".to_owned()));
    assert_eq!(host.node("/mnt/etc/sudoers.d/99-inayos-installer"), None);
    assert_eq!(host.node("/mnt/var/tmp/inayos-answers.json"), None);

    let reported = sink.all().join("\n");
    assert!(!reported.contains(PASSWORD) && !reported.contains(WIFI_PASSWORD));
    assert!(reported.contains("step InayOS is installed"));
}

#[test]
fn the_questions_reach_the_new_system_as_a_json_object() {
    let host = live_system();
    install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap();
    let written = host.written.borrow();
    let (_, json) = written
        .iter()
        .find(|(path, _)| path.as_os_str() == "/mnt/var/tmp/inayos-answers.json")
        .unwrap();
    let json: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(json, serde_json::json!({"Turn it on now?": true}));
}

/// A disk that holds an earlier InayOS: EFI, system and home partitions, with the home folder
/// of `tester` owned by `owner`.
fn earlier_inayos(owner: (u32, u32)) -> FakeHost {
    let host = live_system();
    host.respond(
        "sgdisk --print /dev/vda",
        [ok(
            "Number  Start (sector)    End (sector)  Size       Code  Name\n\
               1            2048         4196351   2.0 GiB     EF00  EFI\n\
               2         4196352        88082431   40.0 GiB    8300  inayos-system\n\
               3        88082432       134215679   22.0 GiB    8300  inayos-home\n",
        )],
    );
    host.respond(
        "blkid --output value --match-tag TYPE /dev/vda1",
        [ok("vfat\n")],
    );
    host.respond(
        "blkid --output value --match-tag TYPE /dev/vda3",
        [ok("ext4\n")],
    );
    host.dir("/mnt/home/tester", owner);
    host.file("/mnt/home/tester/notes.txt", "kept");
    host.file("/mnt/home/tester/.local/state/inayos/state.json", "{}");
    host.file("/mnt/home/tester/.local/state/myconfig/state.json", "{}");
    host.file(
        "/mnt/etc/passwd",
        &format!(
            "tester:x:{}:{}:Tester:/home/tester:/bin/bash\n",
            owner.0, owner.1
        ),
    );
    host
}

fn reinstall() -> Answers {
    answers(
        "choice = \"reinstall-keep-home\"\ndevice = \"/dev/vda\"\nefi_partition = \"/dev/vda1\"\n\
         system_partition = \"/dev/vda2\"\nhome_partition = \"/dev/vda3\"",
        "",
    )
}

#[test]
fn a_reinstall_formats_only_the_system_and_keeps_the_owner_of_the_home_folder() {
    let host = earlier_inayos((1001, 1002));
    install(&reinstall(), &settings(), &host, &FakeSink::default()).unwrap();

    let mut expected: Vec<String> = vec![
        "mount -o ro,noload /dev/vda3 /mnt/home".into(),
        "umount /mnt/home".into(),
        "pacman -Sy".into(),
    ];
    expected.extend(btrfs("/dev/vda2"));
    expected.extend(mounts("/dev/vda2"));
    expected.push("mount /dev/vda1 /mnt/efi".into());
    expected.push("mount /dev/vda3 /mnt/home".into());
    expected.extend(base_system("intel-ucode "));
    expected.extend([
        "arch-chroot /mnt groupadd -g 1002 tester".into(),
        "arch-chroot /mnt useradd -m -G wheel -c Tester -u 1001 -g 1002 tester".into(),
        "arch-chroot /mnt chpasswd".into(),
        INSTALL.into(),
        VERIFY.into(),
        "umount -R /mnt".into(),
    ]);
    assert_eq!(host.actions(), expected);
    assert_eq!(
        host.text("/mnt/home/tester/notes.txt").as_deref(),
        Some("kept")
    );
    assert_eq!(
        host.node("/mnt/home/tester/.local/state/inayos/state.json"),
        None
    );
    assert_eq!(
        host.node("/mnt/home/tester/.local/state/myconfig/state.json"),
        None
    );
}

#[test]
fn a_kept_home_keeps_its_group_when_the_new_system_has_it_and_keeps_its_own_ssh_keys() {
    // The home's group is `users`, which every Arch system has.
    let host = earlier_inayos((1001, 100));
    host.file(
        "/mnt/home/tester/.ssh/authorized_keys",
        "ssh-ed25519 OLD laptop\nssh-ed25519 AAAA one",
    );
    install(&reinstall(), &settings(), &host, &FakeSink::default()).unwrap();

    let actions = host.actions();
    assert!(!actions.iter().any(|action| action.contains("groupadd")));
    assert!(actions.contains(
        &"arch-chroot /mnt useradd -m -G wheel -c Tester -u 1001 -g 100 tester".to_owned()
    ));
    assert_eq!(
        host.node("/mnt/home/tester/.ssh/authorized_keys"),
        Some(Node::File {
            text: "ssh-ed25519 OLD laptop\nssh-ed25519 AAAA one\nssh-ed25519 BBBB two\n".into(),
            mode: 0o600,
            owner: (1001, 100)
        })
    );
}

#[test]
fn a_kept_home_folder_owned_by_root_stops_before_the_disk_changes() {
    for owner in [(0, 0), (1001, 0)] {
        let host = earlier_inayos(owner);
        let failure = install(&reinstall(), &settings(), &host, &FakeSink::default()).unwrap_err();
        assert_eq!(failure.stage, Stage::Checks);
        assert!(matches!(failure.error, Error::RootOwnedHome { .. }));
        assert_eq!(
            host.actions(),
            ["mount -o ro,noload /dev/vda3 /mnt/home", "umount /mnt/home"]
        );
    }

    let host = laptop();
    host.dir("/mnt/home/home/tester", (0, 0));
    let failure = install(&shrink(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::RootOwnedHome { .. }));
    assert_eq!(
        host.actions(),
        ["mount -o ro,noload /dev/vda2 /mnt/home", "umount /mnt/home"]
    );
}

/// A laptop disk: a 512 MiB EFI partition, then one 100 GiB ext4 partition holding an older
/// system and its home folders.
fn laptop() -> FakeHost {
    let host = live_system();
    let home_last = 1_050_624 + 100 * GIB / 512 - 1;
    host.respond(
        "sgdisk --print /dev/vda",
        [ok(&format!(
            "Number  Start (sector)    End (sector)  Size       Code  Name\n\
                1            2048         1050623   512.0 MiB   EF00  EFI\n\
                2         1050624      {home_last}   100.0 GiB   8300  root\n"
        ))],
    );
    host.respond(
        "sgdisk --info=2 /dev/vda",
        [ok(&format!(
            "Partition GUID code: 0FC63DAF-8483-4772-8E79-3D69D8477DE4 (Linux filesystem)\n\
             Partition unique GUID: 11111111-2222-3333-4444-555555555555\n\
             First sector: 1050624 (at 513.0 MiB)\nLast sector: {home_last} (at 100.5 GiB)\n\
             Partition name: 'root'\n"
        ))],
    );
    // 30 GiB used of 100 GiB.
    host.respond(
        "dumpe2fs -h /dev/vda2",
        [ok(&format!(
            "Block count: {}\nFree blocks: {}\nBlock size: 4096\n",
            100 * GIB / 4096,
            70 * GIB / 4096
        ))],
    );
    host.respond(
        "blkid --output value --match-tag TYPE /dev/vda1",
        [ok("vfat\n")],
    );
    host.respond(
        "blkid --output value --match-tag TYPE /dev/vda2",
        [ok("ext4\n")],
    );
    // The old system's partition, seen at /mnt/home once mounted there.
    for folder in ["bin", "etc", "usr/lib", "lost+found"] {
        host.dir(&format!("/mnt/home/{folder}"), (0, 0));
    }
    host.file("/mnt/home/swapfile", "");
    host.dir("/mnt/home/home/tester", (1000, 1000));
    host.file("/mnt/home/home/tester/thesis.odt", "years of work");
    host.file(
        "/mnt/home/home/tester/.local/state/myconfig/state.json",
        "{}",
    );
    host.dir("/mnt/home/home/guest", (1001, 1001));
    host.file("/mnt/efi/vmlinuz-linux", "");
    host.file("/mnt/efi/initramfs-linux.img", "");
    host.file("/mnt/efi/amd-ucode.img", "");
    host.file("/mnt/efi/EFI/refind/refind_x64.efi", "");
    host
}

fn shrink() -> Answers {
    answers(
        "choice = \"shrink-ext4-keep-home\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40\n\
         efi_partition = \"/dev/vda1\"\nhome_partition = \"/dev/vda2\"",
        "",
    )
}

#[test]
fn a_shrink_keeps_the_home_folders_and_puts_the_system_after_them() {
    let host = laptop();
    install(&shrink(), &settings(), &host, &FakeSink::default()).unwrap();

    // 100 GiB minus 40 GiB leaves 60 GiB from sector 1050624.
    let home_last = 1_050_624 + 60 * GIB / 512 - 1;
    let mut expected: Vec<String> = vec![
        "mount -o ro,noload /dev/vda2 /mnt/home".into(),
        "umount /mnt/home".into(),
        "pacman -Sy".into(),
        "e2fsck -f -y /dev/vda2".into(),
        format!("resize2fs /dev/vda2 {}K", 60 * GIB / 1024),
        format!(
            "sgdisk --delete=2 --new=2:1050624:{home_last} \
             --typecode=2:0FC63DAF-8483-4772-8E79-3D69D8477DE4 \
             --partition-guid=2:11111111-2222-3333-4444-555555555555 --change-name=2:root /dev/vda"
        ),
        format!(
            "sgdisk --new=3:{}:0 --typecode=3:8300 --change-name=3:inayos-system /dev/vda",
            home_last + 1
        ),
        "udevadm settle".into(),
    ];
    expected.extend(btrfs("/dev/vda3"));
    expected.extend(mounts("/dev/vda3"));
    expected.push("mount /dev/vda2 /mnt/home".into());
    expected.push("mount /dev/vda1 /mnt/efi".into());
    expected.extend(base_system("intel-ucode "));
    expected.extend([
        "arch-chroot /mnt groupadd -g 1000 tester".into(),
        "arch-chroot /mnt useradd -m -G wheel -c Tester -u 1000 -g 1000 tester".into(),
        "arch-chroot /mnt chpasswd".into(),
        INSTALL.into(),
        VERIFY.into(),
        "umount -R /mnt".into(),
    ]);
    assert_eq!(host.actions(), expected);

    let mut left: Vec<String> = host
        .files
        .borrow()
        .keys()
        .filter_map(|path| {
            let path = path.to_str()?;
            (path.starts_with("/mnt/home/") || path.starts_with("/mnt/efi/"))
                .then(|| path.to_owned())
        })
        .collect();
    left.sort();
    assert_eq!(
        left,
        [
            "/mnt/efi/EFI/refind/refind_x64.efi",
            "/mnt/home/guest",
            "/mnt/home/lost+found",
            "/mnt/home/tester",
            "/mnt/home/tester/.ssh",
            "/mnt/home/tester/.ssh/authorized_keys",
            "/mnt/home/tester/thesis.odt",
        ]
    );
    assert_eq!(
        host.text("/mnt/home/tester/thesis.odt").as_deref(),
        Some("years of work")
    );
}

#[test]
fn files_that_do_not_fit_the_shrunk_home_stop_before_the_disk_changes() {
    let host = laptop();
    host.respond(
        "dumpe2fs -h /dev/vda2",
        [ok(&format!(
            "Block count: {}\nFree blocks: {}\nBlock size: 4096\n",
            100 * GIB / 4096,
            42 * GIB / 4096
        ))],
    );
    let failure = install(&shrink(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Checks);
    assert!(matches!(failure.error, Error::HomeTooFull(_)));
    assert!(host.actions().is_empty());
}

#[test]
fn a_partition_after_the_home_partition_stops_the_shrink() {
    let host = laptop();
    host.respond(
        "sgdisk --print /dev/vda",
        [ok(
            "Number  Start (sector)    End (sector)  Size       Code  Name\n\
               1            2048         1050623   512.0 MiB   EF00  EFI\n\
               2         1050624        99999999   47.0 GiB    8300  root\n\
               3       100000000       134215679   16.0 GiB    8200  swap\n",
        )],
    );
    let failure = install(&shrink(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::NotLastPartition { .. }));
    assert!(host.actions().is_empty());
}

#[test]
fn a_partition_without_a_home_folder_is_never_emptied() {
    let host = laptop();
    host.files
        .borrow_mut()
        .retain(|path, _| !path.starts_with("/mnt/home/home"));
    let failure = install(&shrink(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::NoHomeFolder { .. }));
    assert_eq!(
        host.actions(),
        ["mount -o ro,noload /dev/vda2 /mnt/home", "umount /mnt/home"]
    );
}

#[test]
fn checks_refuse_before_the_disk_changes() {
    let mounted = r#"{"blockdevices":[{"path":"/dev/vda","type":"disk","mountpoints":[null],
        "children":[{"path":"/dev/vda1","type":"part","mountpoints":["/boot"]}]}]}"#;
    let live = r#"{"blockdevices":[{"path":"/dev/vda","type":"disk","mountpoints":[null],
        "children":[{"path":"/dev/vda1","type":"part","mountpoints":["/run/archiso/bootmnt"]}]}]}"#;
    let partition = r#"{"blockdevices":[{"path":"/dev/vda","type":"part","mountpoints":[null]}]}"#;
    type Expected = fn(&Error) -> bool;
    let cases: [(&str, Expected); 3] = [
        (
            mounted,
            |e| matches!(e, Error::Mounted { mountpoint, .. } if mountpoint == "/boot"),
        ),
        (live, |e| matches!(e, Error::LiveMedium { .. })),
        (partition, |e| matches!(e, Error::NotWholeDisk { .. })),
    ];
    for (lsblk, expected) in cases {
        let host = live_system();
        host.respond(
            "lsblk --json --output PATH,TYPE,MOUNTPOINTS /dev/vda",
            [ok(lsblk)],
        );
        let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
        assert_eq!(failure.stage, Stage::Checks);
        assert!(expected(&failure.error), "{:?}", failure.error);
        assert!(host.actions().is_empty());
    }

    let mut host = live_system();
    host.uid = 1000;
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::NotRoot));

    let host = live_system();
    host.files
        .borrow_mut()
        .remove(std::path::Path::new("/sys/firmware/efi/efivars"));
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::NotUefi));

    let host = live_system();
    host.respond("mountpoint -q /mnt", [ok("")]);
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::TargetBusy));
    assert!(host.actions().is_empty());

    let host = live_system();
    host.respond(
        "blockdev --getsize64 /dev/vda",
        [ok(&format!("{}", 40 * GIB))],
    );
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::DiskTooSmall(_)));

    let host = live_system();
    host.respond("pacman -Sy", [exit(1)]);
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(matches!(failure.error, Error::MirrorUnreachable { .. }));
    assert_eq!(host.actions(), ["pacman -Sy"]);
}

#[test]
fn options_that_are_not_built_refuse_before_any_command() {
    for (disk, extra) in [
        (
            "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
            "[security]\nsecure_boot = true",
        ),
        (
            "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
            "[security]\nencryption = true",
        ),
        ("choice = \"beside-windows\"\ndevice = \"/dev/vda\"", ""),
    ] {
        let host = live_system();
        let failure = install(
            &answers(disk, extra),
            &settings(),
            &host,
            &FakeSink::default(),
        )
        .unwrap_err();
        assert!(matches!(failure.error, Error::NotBuilt { .. }));
        assert!(failure.to_string().contains("not built yet"));
        assert!(host.log.borrow().is_empty());
    }
}

#[test]
fn skipped_modules_leave_the_rest_of_the_profile_in_order() {
    let host = live_system();
    host.respond(
        "/usr/local/lib/inayos/inayos list",
        [ok("base\nssh\nlogin\nkde-plasma\n")],
    );
    install(
        &answers(
            "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
            "[modules]\nskip = [\"login\", \"ssh\"]",
        ),
        &settings(),
        &host,
        &FakeSink::default(),
    )
    .unwrap();
    let actions = host.actions();
    assert!(actions.contains(&format!("{INSTALL} base kde-plasma")));
    assert!(actions.contains(&format!("{VERIFY} base kde-plasma")));

    let host = live_system();
    host.respond("/usr/local/lib/inayos/inayos list", [ok("base\n")]);
    let failure = install(
        &answers(
            "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
            "[modules]\nskip = [\"lgoin\"]",
        ),
        &settings(),
        &host,
        &FakeSink::default(),
    )
    .unwrap_err();
    assert!(matches!(failure.error, Error::UnknownModule { ref module } if module == "lgoin"));
    assert!(host.actions().is_empty());
}

#[test]
fn a_failed_module_install_still_removes_the_sudo_rule_and_unmounts() {
    let host = live_system();
    host.respond(INSTALL, [exit(1)]);
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Configuration);
    assert!(matches!(failure.error, Error::Command { .. }));
    assert!(failure.cleanup.is_empty());
    let actions = host.actions();
    assert_eq!(actions[actions.len() - 2..], [INSTALL, "umount -R /mnt"]);
    assert_eq!(host.node("/mnt/etc/sudoers.d/99-inayos-installer"), None);
    assert_eq!(host.node("/mnt/var/tmp/inayos-answers.json"), None);
    assert_eq!(
        host.text("/mnt/var/log/inayos-installer.log").as_deref(),
        Some("the log")
    );
}

#[test]
fn e2fsck_corrections_continue_the_shrink_and_its_failures_stop_it() {
    let host = laptop();
    host.respond("e2fsck -f -y /dev/vda2", [exit(1)]);
    install(&shrink(), &settings(), &host, &FakeSink::default()).unwrap();

    let host = laptop();
    host.respond("e2fsck -f -y /dev/vda2", [exit(4)]);
    let failure = install(&shrink(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Disk);
    assert!(!host.actions().iter().any(|a| a.starts_with("resize2fs")));
}
