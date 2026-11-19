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
        guard_binary: "/usr/local/lib/inayos/inayos-guard".into(),
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
    host.file("/usr/local/lib/inayos/inayos-guard", "guard");
    host.file("/usr/share/zoneinfo/Europe/Paris", "");
    host.file("/etc/locale.gen", "#en_US.UTF-8 UTF-8\n");
    host.file("/proc/cpuinfo", "vendor_id\t: GenuineIntel\n");
    // 1 GiB of memory, so the swap file needs 1 GiB of home.
    host.file("/proc/meminfo", "MemTotal:        1000000 kB\n");
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
        "lsblk --json --output PATH,TYPE,FSTYPE,LABEL,MOUNTPOINTS /dev/vda",
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

#[test]
fn the_desktop_keyboard_layout_goes_where_localed_reads_it() {
    let host = live_system();
    let answers = answers(
        "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
        "xkb_layout = \"fr\"",
    );
    install(&answers, &settings(), &host, &FakeSink::default()).unwrap();
    assert_eq!(
        host.node("/mnt/etc/X11/xorg.conf.d/00-keyboard.conf"),
        Some(Node::File {
            text: "# Written by the InayOS installer. `localectl set-x11-keymap` replaces it.\n\
                   Section \"InputClass\"\n        Identifier \"system-keyboard\"\n        \
                   MatchIsKeyboard \"on\"\n        Option \"XkbLayout\" \"fr\"\nEndSection\n"
                .into(),
            mode: 0o644,
            owner: (0, 0)
        })
    );

    let host = live_system();
    install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap();
    assert_eq!(host.node("/mnt/etc/X11/xorg.conf.d/00-keyboard.conf"), None);
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
    // 2 GiB used of 22 GiB.
    host.respond(
        "dumpe2fs -h /dev/vda3",
        [ok(&format!(
            "Block count: {}\nFree blocks: {}\nBlock size: 4096\n",
            22 * GIB / 4096,
            20 * GIB / 4096
        ))],
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
fn a_reinstall_deletes_the_earlier_swap_file_and_needs_room_for_the_new_one() {
    let host = earlier_inayos((1000, 1000));
    host.file("/mnt/home/swapfile", "old swap");
    install(&reinstall(), &settings(), &host, &FakeSink::default()).unwrap();
    assert_eq!(host.node("/mnt/home/swapfile"), None);

    // 8 GiB of memory, and 7 GiB free besides a 1 GiB earlier swap file, fit exactly.
    let fits = |free_gib: u64| {
        let host = earlier_inayos((1000, 1000));
        host.file("/proc/meminfo", "MemTotal:        8000000 kB\n");
        host.respond(
            "dumpe2fs -h /dev/vda3",
            [ok(&format!(
                "Block count: {}\nFree blocks: {}\nBlock size: 4096\n",
                22 * GIB / 4096,
                free_gib * GIB / 4096
            ))],
        );
        host.respond(
            "debugfs -R 'stat /swapfile' /dev/vda3",
            [ok(&format!("Inode: 12   Type: regular\nSize: {GIB}\n"))],
        );
        let result = install(&reinstall(), &settings(), &host, &FakeSink::default());
        (result, host)
    };
    assert!(fits(7).0.is_ok());
    let (result, host) = fits(6);
    let failure = result.unwrap_err();
    assert_eq!(failure.stage, Stage::Checks);
    assert!(
        matches!(
            failure.error,
            Error::NoRoomForSwapFile { needed_gib: 8, .. }
        ),
        "{:?}",
        failure.error
    );
    assert!(host.actions().is_empty());
}

#[test]
fn a_wiped_home_keeps_room_for_a_swap_file_as_large_as_the_memory() {
    // 64 GiB minus 2 GiB of EFI and 40 GiB of system leaves 21 GiB of home after 1 GiB of
    // swap file, and only 19 after 3 GiB.
    let host = live_system();
    host.file("/proc/meminfo", "MemTotal:        3000000 kB\n");
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Checks);
    assert!(
        matches!(failure.error, Error::DiskTooSmall(_)),
        "{:?}",
        failure.error
    );
    assert!(host.actions().is_empty());
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
        "e2label /dev/vda2 inayos-home".into(),
        format!(
            "sgdisk --delete=2 --new=2:1050624:{home_last} \
             --typecode=2:0FC63DAF-8483-4772-8E79-3D69D8477DE4 \
             --partition-guid=2:11111111-2222-3333-4444-555555555555 --change-name=2:inayos-home /dev/vda"
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
            "lsblk --json --output PATH,TYPE,FSTYPE,LABEL,MOUNTPOINTS /dev/vda",
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
fn answers_an_interface_built_with_a_bad_name_refuse_before_any_command() {
    let mut username = wipe();
    username.user.username = "Inaya Y".into();
    let mut hostname = wipe();
    hostname.hostname = "My PC".into();
    for answers in [username, hostname] {
        let host = live_system();
        let failure = install(&answers, &settings(), &host, &FakeSink::default()).unwrap_err();
        assert_eq!(failure.stage, Stage::Checks);
        assert!(
            matches!(failure.error, Error::InvalidAnswers(_)),
            "{:?}",
            failure.error
        );
        assert!(host.actions().is_empty());
    }
}

#[test]
fn installing_beside_windows_refuses_before_any_command() {
    let host = live_system();
    let failure = install(
        &answers("choice = \"beside-windows\"\ndevice = \"/dev/vda\"", ""),
        &settings(),
        &host,
        &FakeSink::default(),
    )
    .unwrap_err();
    assert!(matches!(failure.error, Error::NotBuilt { .. }));
    assert!(failure.to_string().contains("not built yet"));
    assert!(host.log.borrow().is_empty());
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

const SETUP_MODE: &str = "/sys/firmware/efi/efivars/SetupMode-8be4df61-93ca-11d2-aa0d-00e098032b8c";

/// The firmware's SetupMode variable as efivarfs gives it: 4 attribute bytes, then the value.
fn setup_mode(host: &FakeHost, on: bool) {
    host.file(
        SETUP_MODE,
        if on {
            "\u{6}\0\0\0\u{1}"
        } else {
            "\u{6}\0\0\0\0"
        },
    );
}

#[test]
fn the_secure_boot_answer_chooses_whether_the_secure_boot_module_runs() {
    let profile = "/usr/local/lib/inayos/inayos list";
    let host = live_system();
    host.respond(profile, [ok("base\nrefind\nsecure-boot\nboot-guard\n")]);
    install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap();
    let actions = host.actions();
    assert!(actions.contains(&format!("{INSTALL} base refind boot-guard")));
    assert!(actions.contains(&format!("{VERIFY} base refind boot-guard")));

    let host = live_system();
    host.respond(profile, [ok("base\nrefind\nsecure-boot\nboot-guard\n")]);
    setup_mode(&host, true);
    host.file(FIRMWARE_DEFAULT_DB, "\u{6}\0\0\0db");
    host.writes_when_run(ENROLL, SETUP_MODE, "\u{6}\0\0\0\0");
    install(&secure_boot(), &settings(), &host, &FakeSink::default()).unwrap();
    let actions = host.actions();
    let installed = actions.iter().position(|a| a == INSTALL).unwrap();
    let enrolled = actions.iter().position(|a| a == ENROLL).unwrap();
    let verified = actions.iter().position(|a| a == VERIFY).unwrap();
    assert!(installed < enrolled && enrolled < verified, "{actions:?}");
    // The guard signs its images only once the keys are enrolled.
    let guarded = actions
        .iter()
        .position(|a| a == "arch-chroot /mnt /usr/local/bin/inayos-guard check-update")
        .unwrap();
    assert!(enrolled < guarded && guarded < verified, "{actions:?}");
}

const ENROLL: &str = "arch-chroot /mnt sbctl enroll-keys --microsoft --firmware-builtin";
/// Firmware without a built-in signature database, such as OVMF, gets only Microsoft's keys.
const ENROLL_WITHOUT_BUILTIN: &str = "arch-chroot /mnt sbctl enroll-keys --microsoft";
const FIRMWARE_DEFAULT_DB: &str =
    "/sys/firmware/efi/efivars/dbDefault-8be4df61-93ca-11d2-aa0d-00e098032b8c";

fn secure_boot() -> Answers {
    answers(
        "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
        "[security]\nsecure_boot = true",
    )
}

#[test]
fn the_keys_are_enrolled_only_after_the_install_passed_and_must_reach_the_firmware() {
    // A failed install leaves the firmware in Setup Mode, so the stick still starts.
    let host = live_system();
    setup_mode(&host, true);
    host.respond(INSTALL, [exit(1)]);
    let failure = install(&secure_boot(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Configuration);
    assert!(!host.actions().iter().any(|a| a == ENROLL));

    // A firmware still in Setup Mode after enrolling fails the install before the check.
    let host = live_system();
    setup_mode(&host, true);
    let failure = install(&secure_boot(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(
        matches!(failure.error, Error::KeysNotEnrolled),
        "{:?}",
        failure.error
    );
    assert!(!host.actions().iter().any(|a| a == VERIFY));
}

#[test]
fn firmware_without_built_in_keys_gets_only_microsofts_keys() {
    let host = live_system();
    setup_mode(&host, true);
    host.writes_when_run(ENROLL_WITHOUT_BUILTIN, SETUP_MODE, "\u{6}\0\0\0\0");
    install(&secure_boot(), &settings(), &host, &FakeSink::default()).unwrap();
    let actions = host.actions();
    assert!(
        actions.iter().any(|a| a == ENROLL_WITHOUT_BUILTIN),
        "{actions:?}"
    );
    assert!(!actions.iter().any(|a| a == ENROLL));
}

#[test]
fn secure_boot_outside_setup_mode_stops_before_the_disk_changes() {
    for variable in [Some(false), None] {
        let host = live_system();
        if let Some(on) = variable {
            setup_mode(&host, on);
        }
        let failure = install(
            &answers(
                "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
                "[security]\nsecure_boot = true",
            ),
            &settings(),
            &host,
            &FakeSink::default(),
        )
        .unwrap_err();
        assert_eq!(failure.stage, Stage::Checks);
        match variable {
            Some(_) => assert!(
                matches!(failure.error, Error::SetupModeOff),
                "{:?}",
                failure.error
            ),
            None => assert!(
                matches!(failure.error, Error::Io { .. }),
                "{:?}",
                failure.error
            ),
        }
        assert!(host.actions().is_empty());
    }
}

#[test]
fn the_guard_program_reaches_the_new_system_before_the_modules_run() {
    let host = live_system();
    install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap();
    let log = host.log.borrow().clone();
    let copied = log
        .iter()
        .position(|line| {
            line == "copy /usr/local/lib/inayos/inayos-guard /mnt/usr/local/bin/inayos-guard 755"
        })
        .expect("the guard was not copied");
    let installed = log
        .iter()
        .position(|line| line == &format!("! {INSTALL}"))
        .unwrap();
    assert!(copied < installed);

    let host = live_system();
    host.files
        .borrow_mut()
        .remove(std::path::Path::new("/usr/local/lib/inayos/inayos-guard"));
    let failure = install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(
        matches!(failure.error, Error::NoGuardBinary { .. }),
        "{:?}",
        failure.error
    );
    assert!(host.actions().is_empty());
}

const RECOVERY_KEY: &str =
    "fkhjdulc-hbinrcig-ntvcnejb-uifjlvbn-cerbtnkg-ivnbvjil-tggkjhel-nedrlkrj";
const TEMPORARY_KEY: &str = "/run/inayos-installer-keys/temporary.key";
const RECOVERY_ENROLL: &str = "systemd-cryptenroll --unlock-key-file=/run/inayos-installer-keys/temporary.key \
                               --recovery-key /dev/vda2";

fn encrypted() -> Answers {
    answers(
        "choice = \"wipe-disk\"\ndevice = \"/dev/vda\"\nsystem_size_gib = 40",
        "[security]\nencryption = true",
    )
}

/// The live system of `live_system` with a TPM 2.0, and LUKS tools that answer like the
/// real ones: opening a volume creates its mapper device, and enrolling a recovery key
/// prints it.
fn tpm_system() -> FakeHost {
    let host = live_system();
    host.file("/sys/class/tpm/tpm0/tpm_version_major", "2\n");
    for (partition, name, uuid) in [
        ("/dev/vda2", "system", "1111"),
        ("/dev/vda3", "home", "3333"),
    ] {
        host.writes_when_run(
            &format!("cryptsetup open --key-file {TEMPORARY_KEY} {partition} {name}"),
            &format!("/dev/mapper/{name}"),
            "",
        );
        host.respond(
            &format!("cryptsetup luksUUID {partition}"),
            [ok(&format!("{uuid}\n"))],
        );
    }
    host.writes_when_run(
        &format!("dd if=/dev/urandom of={TEMPORARY_KEY} bs=64 count=1 iflag=fullblock status=none"),
        TEMPORARY_KEY,
        "random",
    );
    host.respond(RECOVERY_ENROLL, [ok(&format!("{RECOVERY_KEY}\n"))]);
    host
}

#[test]
fn an_encrypted_install_opens_with_a_temporary_key_then_leaves_only_the_recovery_key_and_the_tpm() {
    let host = tpm_system();
    let sink = FakeSink::default();
    let installed = install(&encrypted(), &settings(), &host, &sink).unwrap();

    let actions = host.actions();
    let unlock = format!("--unlock-key-file={TEMPORARY_KEY}");
    let mut expected: Vec<String> = vec![
        format!("dd if=/dev/urandom of={TEMPORARY_KEY} bs=64 count=1 iflag=fullblock status=none"),
        format!(
            "cryptsetup luksFormat --batch-mode --type luks2 --key-file {TEMPORARY_KEY} --key-slot 0 --label inayos-system /dev/vda2"
        ),
        format!("cryptsetup open --key-file {TEMPORARY_KEY} /dev/vda2 system"),
        format!(
            "cryptsetup luksFormat --batch-mode --type luks2 --key-file {TEMPORARY_KEY} --key-slot 0 --label inayos-home /dev/vda3"
        ),
        format!("cryptsetup open --key-file {TEMPORARY_KEY} /dev/vda3 home"),
    ];
    expected.extend(btrfs("/dev/mapper/system"));
    expected.push("mkfs.ext4 -F -L inayos-home /dev/mapper/home".into());
    expected.extend(mounts("/dev/mapper/system"));
    expected.push("mount /dev/vda1 /mnt/efi".into());
    expected.push("mount /dev/mapper/home /mnt/home".into());
    let formatted = actions
        .iter()
        .position(|a| a == "mkfs.fat -F 32 -n EFI /dev/vda1")
        .unwrap();
    assert_eq!(
        actions[formatted + 1..formatted + 1 + expected.len()],
        expected
    );

    // Once the modules passed: the recovery key on both volumes, the TPM slot without
    // measurements on both, then the temporary key out of both, and the volumes closed after
    // the unmount.
    let verified = actions.iter().position(|a| a == VERIFY).unwrap();
    assert_eq!(
        actions[verified + 1..],
        [
            format!("cryptsetup luksAddKey --key-file {TEMPORARY_KEY} /dev/vda3 /dev/stdin"),
            format!("systemd-cryptenroll {unlock} --tpm2-device=auto --tpm2-pcrs= /dev/vda2"),
            format!("systemd-cryptenroll {unlock} --tpm2-device=auto --tpm2-pcrs= /dev/vda3"),
            "systemd-cryptenroll --wipe-slot=0 /dev/vda2".to_owned(),
            "systemd-cryptenroll --wipe-slot=0 /dev/vda3".to_owned(),
            "umount -R /mnt".to_owned(),
            "cryptsetup close home".to_owned(),
            "cryptsetup close system".to_owned(),
        ]
    );
    assert!(host.log.borrow().contains(&format!("? {RECOVERY_ENROLL}")));
    // Before the temporary key leaves, the recovery key and the TPM each open both volumes,
    // and the copy of the key is saved.
    let log = host.log.borrow().clone();
    let at = |line: &str| {
        log.iter()
            .position(|logged| logged == line)
            .unwrap_or_else(|| panic!("{line} is missing"))
    };
    let first_wipe = at("! systemd-cryptenroll --wipe-slot=0 /dev/vda2");
    for partition in ["/dev/vda2", "/dev/vda3"] {
        let passphrase = format!("cryptsetup open --test-passphrase --key-file=- {partition}");
        assert!(at(&format!("? {passphrase}")) < first_wipe);
        assert!(
            host.inputs
                .borrow()
                .contains(&(passphrase, RECOVERY_KEY.to_owned()))
        );
        assert!(
            at(&format!(
                "? cryptsetup open --test-passphrase --token-only --token-type systemd-tpm2 {partition}"
            )) < first_wipe
        );
    }
    assert!(at("write /mnt/home/tester/inayos-recovery-key.txt 600") < first_wipe);
    // The home volume gets the same passphrase, without a line break.
    assert!(host.inputs.borrow().contains(&(
        format!("cryptsetup luksAddKey --key-file {TEMPORARY_KEY} /dev/vda3 /dev/stdin"),
        RECOVERY_KEY.to_owned()
    )));
    assert_eq!(host.node("/run/inayos-installer-keys"), None);
    assert_eq!(host.node(TEMPORARY_KEY), None);

    assert_eq!(
        host.node("/mnt/etc/crypttab.initramfs")
            .map(|node| match node {
                Node::File { text, mode, .. } => (
                    text.lines()
                        .filter(|line| !line.starts_with('#'))
                        .map(str::to_owned)
                        .collect::<Vec<_>>(),
                    mode
                ),
                Node::Dir { .. } => panic!("a folder"),
            }),
        Some((
            vec![
                "system UUID=1111 none tpm2-device=auto,discard".to_owned(),
                "home UUID=3333 none tpm2-device=auto,discard".to_owned()
            ],
            0o600
        ))
    );
    assert_eq!(
        host.text("/mnt/etc/kernel/cmdline").as_deref(),
        Some("root=/dev/mapper/system rootflags=subvol=@ rw\n")
    );
    // The files that the modules build the boot image from exist before the modules run.
    let log = host.log.borrow().clone();
    let cmdline = log
        .iter()
        .position(|line| line == "write /mnt/etc/kernel/cmdline 644")
        .unwrap();
    let modules = log
        .iter()
        .position(|line| line == &format!("! {INSTALL}"))
        .unwrap();
    assert!(cmdline < modules);

    let Some(Node::File { text, mode, owner }) =
        host.node("/mnt/home/tester/inayos-recovery-key.txt")
    else {
        panic!("the recovery key file is missing");
    };
    assert!(text.lines().any(|line| line == RECOVERY_KEY), "{text}");
    assert_eq!((mode, owner), (0o600, (1000, 1000)));
    assert_eq!(
        installed.recovery_key.as_ref().map(|key| key.text()),
        Some(RECOVERY_KEY)
    );
    assert_eq!(
        installed.recovery_key_file.as_deref(),
        Some(std::path::Path::new("/home/tester/inayos-recovery-key.txt"))
    );
    assert!(!format!("{installed:?}").contains(RECOVERY_KEY));

    let reported = sink.all().join("\n");
    assert!(!reported.contains(RECOVERY_KEY), "{reported}");
    assert!(reported.contains("/home/tester/inayos-recovery-key.txt"));
}

#[test]
fn an_encrypted_install_without_the_encryption_answer_skips_the_encryption_module() {
    let profile = "/usr/local/lib/inayos/inayos list";
    let host = live_system();
    host.respond(profile, [ok("base\nencryption\nrefind\n")]);
    install(&wipe(), &settings(), &host, &FakeSink::default()).unwrap();
    assert!(host.actions().contains(&format!("{INSTALL} base refind")));
    assert!(!host.actions().iter().any(|a| a.starts_with("cryptsetup")));

    let host = tpm_system();
    host.respond(profile, [ok("base\nencryption\nrefind\n")]);
    install(&encrypted(), &settings(), &host, &FakeSink::default()).unwrap();
    // Without skipped modules the whole profile runs.
    assert!(host.actions().contains(&INSTALL.to_owned()));
}

#[test]
fn encryption_needs_a_tpm_and_a_whole_disk_and_refuses_before_any_change() {
    let host = live_system();
    let failure = install(&encrypted(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Checks);
    assert!(
        matches!(failure.error, Error::EncryptionWithoutTpm),
        "{:?}",
        failure.error
    );
    assert!(host.actions().is_empty());

    let host = earlier_inayos((1000, 1000));
    host.file("/sys/class/tpm/tpm0/tpm_version_major", "2\n");
    let mut answers = reinstall();
    answers.security.encryption = true;
    let failure = install(&answers, &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(
        matches!(failure.error, Error::EncryptionNeedsWholeDisk),
        "{:?}",
        failure.error
    );
    assert_eq!(
        failure.to_string(),
        "encryption needs a whole disk for now; the disk is unchanged"
    );
    assert!(host.log.borrow().is_empty());
}

#[test]
fn a_failed_enrollment_still_closes_the_volumes_and_deletes_the_temporary_key() {
    let host = tpm_system();
    host.respond("systemd-cryptenroll --wipe-slot=0 /dev/vda2", [exit(1)]);
    let failure = install(&encrypted(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Keys);
    assert!(failure.cleanup.is_empty());
    let actions = host.actions();
    assert_eq!(
        actions[actions.len() - 3..],
        [
            "umount -R /mnt",
            "cryptsetup close home",
            "cryptsetup close system"
        ]
    );
    assert_eq!(host.node(TEMPORARY_KEY), None);
    // The key already controls the disk, so the failure still hands it over.
    assert_eq!(
        failure
            .installed
            .recovery_key
            .as_ref()
            .map(|key| key.text()),
        Some(RECOVERY_KEY)
    );
    assert!(failure.installed.recovery_key_file.is_some());
    assert!(!failure.to_string().contains(RECOVERY_KEY));

    // A recovery key or a TPM slot that does not open a volume keeps the temporary key in
    // both volumes.
    for check in [
        "cryptsetup open --test-passphrase --key-file=- /dev/vda3",
        "cryptsetup open --test-passphrase --token-only --token-type systemd-tpm2 /dev/vda2",
    ] {
        let host = tpm_system();
        host.respond(check, [exit(2)]);
        let failure = install(&encrypted(), &settings(), &host, &FakeSink::default()).unwrap_err();
        assert_eq!(failure.stage, Stage::Keys);
        assert!(!host.actions().iter().any(|a| a.contains("--wipe-slot")));
        assert!(failure.installed.recovery_key.is_some());
    }

    // A cleanup failure after a finished install still hands the key over.
    let host = tpm_system();
    host.respond("cryptsetup close home", [exit(1)]);
    let failure = install(&encrypted(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert_eq!(failure.stage, Stage::Finish);
    assert_eq!(
        failure
            .installed
            .recovery_key
            .as_ref()
            .map(|key| key.text()),
        Some(RECOVERY_KEY)
    );

    // An enrollment that printed no key stops before the temporary key leaves the volumes.
    let host = tpm_system();
    host.respond(RECOVERY_ENROLL, [ok("")]);
    let failure = install(&encrypted(), &settings(), &host, &FakeSink::default()).unwrap_err();
    assert!(
        matches!(failure.error, Error::NoRecoveryKey),
        "{:?}",
        failure.error
    );
    assert!(!host.actions().iter().any(|a| a.contains("--wipe-slot")));
}

const HIBERNATE_LOCATION: &str =
    "/sys/firmware/efi/efivars/HibernateLocation-8cf2644b-4b0b-428f-9387-6d876050dc67";
const HIBERNATED: &str =
    "This disk holds a hibernated InayOS: start it and shut it down properly first";

/// The swap file at the top of /dev/vda3 has its first page in block 1000 of 4096 bytes,
/// and that page holds `page`.
fn swap_file(host: &FakeHost, page: &str) {
    host.respond("debugfs -R 'bmap /swapfile 0' /dev/vda3", [ok("1000\n")]);
    host.respond(
        "dumpe2fs -h /dev/vda3",
        [ok(
            "Block count: 5000000\nFree blocks: 4000000\nBlock size: 4096\n",
        )],
    );
    host.respond(
        "dd if=/dev/vda3 iflag=skip_bytes,count_bytes skip=4096000 count=4096 status=none",
        [ok(page)],
    );
}

#[test]
fn a_hibernated_system_stops_every_install_before_anything_is_mounted() {
    // The firmware variable that systemd writes before it hibernates.
    for answers in [wipe(), reinstall()] {
        let host = earlier_inayos((1000, 1000));
        host.file(HIBERNATE_LOCATION, "{\"uuid\":\"3333\"}");
        let failure = install(&answers, &settings(), &host, &FakeSink::default()).unwrap_err();
        assert_eq!(failure.stage, Stage::Checks);
        assert!(
            matches!(failure.error, Error::Hibernated),
            "{:?}",
            failure.error
        );
        assert_eq!(
            failure.to_string(),
            format!("{HIBERNATED}; the disk is unchanged")
        );
        assert!(host.actions().is_empty());
    }

    // The swap file of the kept home, and the InayOS home of a disk to wipe, read without
    // mounting them.
    let lsblk = r#"{"blockdevices":[{"path":"/dev/vda","type":"disk","mountpoints":[null],
        "children":[{"path":"/dev/vda1","type":"part","fstype":"vfat","label":"EFI","mountpoints":[null]},
                    {"path":"/dev/vda2","type":"part","fstype":"btrfs","label":"inayos-system","mountpoints":[null]},
                    {"path":"/dev/vda3","type":"part","fstype":"ext4","label":"inayos-home","mountpoints":[null]}]}]}"#;
    for answers in [wipe(), reinstall()] {
        let host = earlier_inayos((1000, 1000));
        host.respond(
            "lsblk --json --output PATH,TYPE,FSTYPE,LABEL,MOUNTPOINTS /dev/vda",
            [ok(lsblk)],
        );
        swap_file(&host, &format!("{}S1SUSPEND\0", "\0".repeat(4086)));
        let failure = install(&answers, &settings(), &host, &FakeSink::default()).unwrap_err();
        assert!(
            matches!(failure.error, Error::Hibernated),
            "{:?}",
            failure.error
        );
        assert!(host.actions().is_empty(), "{:?}", host.actions());
    }

    // A swap file that holds only the swap signature changes nothing.
    let host = earlier_inayos((1000, 1000));
    host.respond(
        "lsblk --json --output PATH,TYPE,FSTYPE,LABEL,MOUNTPOINTS /dev/vda",
        [ok(lsblk)],
    );
    swap_file(&host, &format!("{}SWAPSPACE2", "\0".repeat(4086)));
    install(&reinstall(), &settings(), &host, &FakeSink::default()).unwrap();
}
