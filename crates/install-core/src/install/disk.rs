//! Partitions and formats the disk as the checks planned it, and mounts the new system's
//! filesystems under `/mnt`.
use std::path::{Path, PathBuf};

use super::{Machine, TARGET, checks::DiskWork};
use crate::{Error, disk_names::partition_path, host::Command, parse};

/// The Btrfs subvolumes and where each is mounted, relative to the root.
pub(crate) const SUBVOLUMES: [(&str, &str); 5] = [
    ("@", ""),
    ("@snapshots", ".snapshots"),
    ("@var_cache", "var/cache"),
    ("@var_log", "var/log"),
    ("@var_lib_docker", "var/lib/docker"),
];

const BTRFS_OPTIONS: &str = "compress=zstd:1";

/// The folder the shrink parks the old home folders in while it deletes the old system.
const PARKED_HOMES: &str = ".inayos-homes";

pub(crate) fn prepare(m: &Machine, work: &DiskWork, username: &str) -> Result<(), Error> {
    match work {
        DiskWork::Wipe { device, layout } => {
            m.sink.step(&format!("Partitioning {device}"));
            m.run(Command::action("sgdisk").args(["--zap-all", device]))?;
            let parts = [
                (
                    1,
                    layout.efi.first,
                    layout.efi.last.to_string(),
                    "EF00",
                    "EFI",
                ),
                (
                    2,
                    layout.system.first,
                    layout.system.last.to_string(),
                    "8300",
                    "inayos-system",
                ),
                (3, layout.home_first, "0".to_owned(), "8300", "inayos-home"),
            ];
            for (number, first, last, code, name) in parts {
                m.run(Command::action("sgdisk").args([
                    format!("--new={number}:{first}:{last}"),
                    format!("--typecode={number}:{code}"),
                    format!("--change-name={number}:{name}"),
                    device.clone(),
                ]))?;
            }
            m.run(Command::action("udevadm").arg("settle"))?;
            let efi = partition_path(device, 1);
            let system = partition_path(device, 2);
            let home = partition_path(device, 3);
            m.sink
                .step("Formatting the EFI, system and home partitions");
            m.run(Command::action("mkfs.fat").args(["-F", "32", "-n", "EFI", &efi]))?;
            format_system(m, &system)?;
            m.run(Command::action("mkfs.ext4").args(["-F", "-L", "inayos-home", &home]))?;
            mount_system(m, &system)?;
            mount(m, &efi, "efi")?;
            mount(m, &home, "home")
        }
        DiskWork::Reinstall { efi, system, home } => {
            m.sink
                .step(&format!("Formatting the system partition {system}"));
            format_system(m, system)?;
            mount_system(m, system)?;
            mount(m, efi, "efi")?;
            mount(m, home, "home")?;
            forget_old_state(m, username)
        }
        DiskWork::Shrink {
            device,
            efi,
            home,
            home_number,
            entry,
            plan,
            system_number,
        } => {
            m.sink.step(&format!("Shrinking {home}"));
            let check = m.run_any(&Command::action("e2fsck").args(["-f", "-y", home]))?;
            // e2fsck exits 1 when it corrected errors, which leaves the filesystem usable.
            if !matches!(check.code, Some(0 | 1)) {
                return Err(Error::Command {
                    command: format!("e2fsck -f -y {home}"),
                    status: check.status(),
                    stderr: check.stderr,
                });
            }
            m.run(
                Command::action("resize2fs")
                    .args([home.clone(), format!("{}K", plan.filesystem_bytes / 1024)]),
            )?;
            let n = home_number;
            m.run(Command::action("sgdisk").args([
                format!("--delete={n}"),
                format!("--new={n}:{}:{}", plan.home.first, plan.home.last),
                format!("--typecode={n}:{}", entry.type_guid),
                format!("--partition-guid={n}:{}", entry.unique_guid),
                format!("--change-name={n}:{}", entry.name),
                device.clone(),
            ]))?;
            let s = system_number;
            m.run(Command::action("sgdisk").args([
                format!("--new={s}:{}:0", plan.system_first),
                format!("--typecode={s}:8300"),
                format!("--change-name={s}:inayos-system"),
                device.clone(),
            ]))?;
            m.run(Command::action("udevadm").arg("settle"))?;
            let system = partition_path(device, *system_number);
            m.sink
                .step(&format!("Formatting the system partition {system}"));
            format_system(m, &system)?;
            mount_system(m, &system)?;
            mount(m, home, "home")?;
            m.sink
                .step("Turning the old system's partition into the home partition");
            keep_only_homes(m, &target("home"))?;
            mount(m, efi, "efi")?;
            remove_old_boot_files(m, &target("efi"))?;
            forget_old_state(m, username)
        }
    }
}

fn target(relative: &str) -> PathBuf {
    if relative.is_empty() {
        PathBuf::from(TARGET)
    } else {
        Path::new(TARGET).join(relative)
    }
}

/// Formats `partition` as Btrfs and creates the subvolumes.
fn format_system(m: &Machine, partition: &str) -> Result<(), Error> {
    m.run(Command::action("mkfs.btrfs").args(["-f", "-L", "inayos-system", partition]))?;
    m.run(Command::action("mount").args([partition, TARGET]))?;
    for (subvolume, _) in SUBVOLUMES {
        m.run(Command::action("btrfs").args([
            "subvolume".to_owned(),
            "create".to_owned(),
            format!("{TARGET}/{subvolume}"),
        ]))?;
    }
    m.run(Command::action("umount").arg(TARGET))?;
    Ok(())
}

/// Mounts the root subvolume at `/mnt` and each other subvolume at its place inside it.
fn mount_system(m: &Machine, partition: &str) -> Result<(), Error> {
    for (subvolume, place) in SUBVOLUMES {
        let point = target(place);
        if !place.is_empty() {
            m.create_dir(&point, 0o755)?;
        }
        m.run(Command::action("mount").args([
            "-o".to_owned(),
            format!("{BTRFS_OPTIONS},subvol={subvolume}"),
            partition.to_owned(),
            point.display().to_string(),
        ]))?;
    }
    Ok(())
}

fn mount(m: &Machine, partition: &str, place: &str) -> Result<(), Error> {
    let point = target(place);
    m.create_dir(&point, 0o755)?;
    m.run(Command::action("mount").args([partition.to_owned(), point.display().to_string()]))?;
    Ok(())
}

/// Inside the old system's partition mounted at `root`: moves every folder of `home` to the
/// top level and deletes the rest of the old system, keeping `lost+found`. The home folders
/// stay in a parked folder until the old system is gone, so no name can collide.
fn keep_only_homes(m: &Machine, root: &Path) -> Result<(), Error> {
    let parked = root.join(PARKED_HOMES);
    m.rename(&root.join("home"), &parked)?;
    for name in m.list(root)? {
        if name != "lost+found" && name != PARKED_HOMES {
            m.remove_all(&root.join(&name))?;
        }
    }
    for name in m.list(&parked)? {
        m.rename(&parked.join(&name), &root.join(&name))?;
    }
    m.remove_all(&parked)
}

/// Deletes the kernels, initramfs images and microcode of the old system from the top of the
/// EFI partition mounted at `root`.
fn remove_old_boot_files(m: &Machine, root: &Path) -> Result<(), Error> {
    for name in m.list(root)? {
        let path = root.join(&name);
        if parse::is_old_boot_file(&name) && !m.is_dir(&path) {
            m.remove_file(&path)?;
        }
    }
    Ok(())
}

/// Deletes the InayOS state record in the kept home, which describes the erased system.
/// `myconfig` is the earlier name of InayOS, whose record InayOS would otherwise migrate.
fn forget_old_state(m: &Machine, username: &str) -> Result<(), Error> {
    for name in ["inayos", "myconfig"] {
        let record = target("home")
            .join(username)
            .join(".local/state")
            .join(name)
            .join("state.json");
        if m.exists(&record) {
            m.remove_file(&record)?;
        }
    }
    Ok(())
}
