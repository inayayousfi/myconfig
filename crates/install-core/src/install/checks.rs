//! Checks the machine and the answers before the disk changes, and works out the exact disk
//! changes. The only changes here are to the live system: the Wi-Fi connection, the package
//! database, and a read-only look into a partition whose home folders stay. Nothing here
//! mounts a partition read-write, so a hibernated InayOS is refused before anything could
//! write to its disk.
use std::path::Path;

use super::{Machine, Settings, TARGET};
use crate::{
    Answers, Disk, Error,
    disk_names::partition_number,
    hibernation,
    host::Command,
    inventory,
    modules::{ENCRYPTION_MODULE, SECURE_BOOT_MODULE},
    parse::{self, BlockDevice, GptEntry, TableRow},
    sizes::{self, BesideLayout, ShrinkPlan, WipeLayout},
    windows,
};

/// Where archiso mounts the medium it booted from.
pub(crate) const LIVE_MEDIUM: &str = "/run/archiso/bootmnt";

/// The disk changes, with every number they need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiskWork {
    Wipe {
        device: String,
        layout: WipeLayout,
        /// Whether the system and home partitions are LUKS2 volumes.
        encrypted: bool,
    },
    Reinstall {
        efi: String,
        system: String,
        home: String,
    },
    Shrink {
        device: String,
        efi: String,
        home: String,
        home_number: u32,
        /// The GPT entry of the home partition before the shrink.
        entry: GptEntry,
        plan: ShrinkPlan,
        system_number: u32,
    },
    BesideWindows {
        device: String,
        efi: String,
        windows: String,
        windows_number: u32,
        /// The GPT entry of the Windows partition before the shrink.
        entry: GptEntry,
        layout: BesideLayout,
        sector_size: u64,
        system_number: u32,
        home_number: u32,
    },
}

/// What the checks found.
pub(crate) struct Checked {
    pub disk: DiskWork,
    /// The modules to install, in profile order, when the answers skip some. None installs
    /// the whole profile.
    pub modules: Option<Vec<String>>,
    /// The user and group ids that own the user's kept home folder, which the new user
    /// takes over. None when no home folder of that name is kept.
    pub kept_owner: Option<(u32, u32)>,
}

pub(crate) fn check(m: &Machine, answers: &Answers, settings: &Settings) -> Result<Checked, Error> {
    answers.check()?;
    if answers.security.encryption && !matches!(answers.disk, Disk::WipeDisk { .. }) {
        return Err(Error::EncryptionNeedsWholeDisk);
    }
    let uid = m.host.effective_uid().map_err(|error| Error::Io {
        action: "read the user id of",
        path: "/proc/self".into(),
        error,
    })?;
    if uid != 0 {
        return Err(Error::NotRoot);
    }
    if !m.exists(Path::new("/sys/firmware/efi/efivars")) {
        return Err(Error::NotUefi);
    }
    if hibernation::firmware_hibernated(m) {
        return Err(Error::Hibernated);
    }
    if answers.security.encryption && !inventory::has_tpm2(m.host) {
        return Err(Error::EncryptionWithoutTpm);
    }
    if answers.security.secure_boot && !inventory::secure_boot_setup_mode(m.host)? {
        return Err(Error::SetupModeOff);
    }
    if !m.exists(&settings.system_binary) || m.is_dir(&settings.system_binary) {
        return Err(Error::NoSystemBinary {
            path: settings.system_binary.clone(),
        });
    }
    if !m.exists(&settings.guard_binary) || m.is_dir(&settings.guard_binary) {
        return Err(Error::NoGuardBinary {
            path: settings.guard_binary.clone(),
        });
    }
    let modules = kept_modules(m, answers, settings)?;

    if !m.exists(&Path::new("/usr/share/zoneinfo").join(&answers.timezone)) {
        return Err(Error::UnknownTimezone {
            timezone: answers.timezone.clone(),
        });
    }
    let locale_gen = m.read(Path::new("/etc/locale.gen"))?;
    if parse::enable_locale(&locale_gen, &answers.language).is_none() {
        return Err(Error::UnknownLocale {
            locale: answers.language.clone(),
        });
    }

    if m.run_any(&Command::query("mountpoint").args(["-q", TARGET]))?
        .success()
    {
        return Err(Error::TargetBusy);
    }
    let device = answers.disk.device();
    let tree = check_unused_disk(m, device)?;
    check_not_hibernated(m, &tree, &answers.disk)?;
    let swap_gib = hibernation::swap_gib(m)?;
    let (disk, kept_owner) = plan_disk(
        m,
        &answers.disk,
        &answers.user.username,
        answers.security.encryption,
        swap_gib,
    )?;

    if let Some(wifi) = &answers.wifi {
        m.sink
            .step(&format!("Connecting to the Wi-Fi network {}", wifi.ssid));
        let mut connect = Command::action("nmcli").args(["device", "wifi", "connect", &wifi.ssid]);
        if let Some(password) = &wifi.password {
            connect = connect.arg("password").secret(password.0.as_str());
        }
        m.run(connect)?;
    }
    m.sink.step("Checking the Arch mirrors");
    let sync = m.run_any(&Command::action("pacman").arg("-Sy"))?;
    if !sync.success() {
        return Err(Error::MirrorUnreachable {
            status: sync.status(),
        });
    }
    Ok(Checked {
        disk,
        modules,
        kept_owner,
    })
}

/// The profile modules minus the skipped ones, when the answers skip any. Without Secure
/// Boot in the answers, the secure-boot module is skipped too, and without encryption, the
/// encryption module.
fn kept_modules(
    m: &Machine,
    answers: &Answers,
    settings: &Settings,
) -> Result<Option<Vec<String>>, Error> {
    let answered_off: Vec<&str> = [
        (answers.security.secure_boot, SECURE_BOOT_MODULE),
        (answers.security.encryption, ENCRYPTION_MODULE),
    ]
    .into_iter()
    .filter(|(on, _)| !on)
    .map(|(_, module)| module)
    .collect();
    if answers.skip.is_empty() && answered_off.is_empty() {
        return Ok(None);
    }
    let binary = settings.system_binary.to_string_lossy();
    let listed = m.query(Command::query(&binary).arg("list"))?;
    let profile: Vec<&str> = listed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    for module in &answers.skip {
        if !profile.contains(&module.as_str()) {
            return Err(Error::UnknownModule {
                module: module.clone(),
            });
        }
    }
    let mut skip: Vec<&str> = answers.skip.iter().map(String::as_str).collect();
    skip.extend(
        answered_off
            .into_iter()
            .filter(|module| profile.contains(module)),
    );
    if skip.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        profile
            .into_iter()
            .filter(|name| !skip.contains(name))
            .map(str::to_owned)
            .collect(),
    ))
}

/// Fails unless `device` is a whole disk with nothing of it mounted, and returns its
/// partitions.
fn check_unused_disk(m: &Machine, device: &str) -> Result<BlockDevice, Error> {
    let lsblk = Command::query("lsblk").args([
        "--json",
        "--output",
        "PATH,TYPE,FSTYPE,LABEL,MOUNTPOINTS",
        device,
    ]);
    let shown = lsblk.shown();
    let tree = parse::lsblk(&m.query(lsblk)?).map_err(|problem| Error::UnexpectedOutput {
        command: shown,
        problem,
    })?;
    if tree.kind != "disk" {
        return Err(Error::NotWholeDisk {
            device: device.to_owned(),
        });
    }
    let all = tree.all();
    if all
        .iter()
        .any(|part| part.mountpoints.iter().any(|point| point == LIVE_MEDIUM))
    {
        return Err(Error::LiveMedium {
            device: device.to_owned(),
        });
    }
    for part in all {
        if let Some(mountpoint) = part.mountpoints.first() {
            return Err(Error::Mounted {
                device: part.path.clone(),
                mountpoint: mountpoint.clone(),
            });
        }
    }
    Ok(tree)
}

/// Fails when an InayOS home partition of the disk, or the partition the answers keep as
/// home, holds a swap file that a system hibernated into.
fn check_not_hibernated(m: &Machine, tree: &BlockDevice, disk: &Disk) -> Result<(), Error> {
    let mut homes: Vec<&str> = tree
        .all()
        .into_iter()
        .filter(|part| part.label == inventory::HOME_LABEL && part.fstype == "ext4")
        .map(|part| part.path.as_str())
        .collect();
    if let Disk::ReinstallKeepHome { home_partition, .. }
    | Disk::ShrinkExt4KeepHome { home_partition, .. } = disk
        && !homes.contains(&home_partition.as_str())
    {
        homes.push(home_partition);
    }
    for home in homes {
        if hibernation::swap_file_hibernated(m, home)? {
            return Err(Error::Hibernated);
        }
    }
    Ok(())
}

fn number(m: &Machine, command: Command) -> Result<u64, Error> {
    let shown = command.shown();
    let text = m.query(command)?;
    text.trim().parse().map_err(|_| Error::UnexpectedOutput {
        command: shown,
        problem: format!("{:?} is not a number", text.trim()),
    })
}

fn sector_size(m: &Machine, device: &str) -> Result<u64, Error> {
    let command = Command::query("blockdev").args(["--getss", device]);
    let shown = command.shown();
    let size = number(m, command)?;
    if size == 512 || size == 4096 {
        Ok(size)
    } else {
        Err(Error::UnexpectedOutput {
            command: shown,
            problem: format!("a sector size of {size} bytes is not supported"),
        })
    }
}

fn table(m: &Machine, device: &str) -> Result<Vec<TableRow>, Error> {
    let command = Command::query("sgdisk").args(["--print", device]);
    let shown = command.shown();
    parse::partition_table(&m.query(command)?).map_err(|problem| Error::UnexpectedOutput {
        command: shown,
        problem,
    })
}

/// The table row of `partition`, which must be a partition of `device` in `rows`.
fn row(rows: &[TableRow], device: &str, partition: &str) -> Result<TableRow, Error> {
    partition_number(device, partition)
        .and_then(|number| rows.iter().find(|row| row.number == number).copied())
        .ok_or_else(|| Error::MissingPartition {
            partition: partition.to_owned(),
        })
}

fn require_filesystem(m: &Machine, partition: &str, expected: &'static str) -> Result<(), Error> {
    let probe = m.run_any(&Command::query("blkid").args([
        "--output",
        "value",
        "--match-tag",
        "TYPE",
        partition,
    ]))?;
    let found = probe.stdout.trim();
    if probe.success() && found == expected {
        return Ok(());
    }
    Err(Error::WrongFilesystem {
        partition: partition.to_owned(),
        expected,
        found: if found.is_empty() {
            "no filesystem".to_owned()
        } else {
            found.to_owned()
        },
    })
}

/// The disk changes, and the owner of the user's home folder when the disk keeps one. Every
/// home keeps room for the hibernation swap file of `swap_gib`.
fn plan_disk(
    m: &Machine,
    disk: &Disk,
    username: &str,
    encrypted: bool,
    swap_gib: u64,
) -> Result<(DiskWork, Option<(u32, u32)>), Error> {
    match disk {
        Disk::WipeDisk {
            device,
            system_size_gib,
        } => {
            let sector = sector_size(m, device)?;
            let bytes = number(m, Command::query("blockdev").args(["--getsize64", device]))?;
            let layout = sizes::wipe_layout(bytes / sector, sector, *system_size_gib, swap_gib)
                .map_err(Error::DiskTooSmall)?;
            Ok((
                DiskWork::Wipe {
                    device: device.clone(),
                    layout,
                    encrypted,
                },
                None,
            ))
        }
        Disk::ReinstallKeepHome {
            device,
            efi_partition,
            system_partition,
            home_partition,
        } => {
            let rows = table(m, device)?;
            for partition in [efi_partition, system_partition, home_partition] {
                row(&rows, device, partition)?;
            }
            require_filesystem(m, efi_partition, "vfat")?;
            require_filesystem(m, home_partition, "ext4")?;
            check_swap_room(m, home_partition, swap_gib)?;
            let owner = look_into_home(m, home_partition, Path::new(username), None)?;
            Ok((
                DiskWork::Reinstall {
                    efi: efi_partition.clone(),
                    system: system_partition.clone(),
                    home: home_partition.clone(),
                },
                owner,
            ))
        }
        Disk::ShrinkExt4KeepHome {
            device,
            system_size_gib,
            efi_partition,
            home_partition,
        } => {
            let sector = sector_size(m, device)?;
            let rows = table(m, device)?;
            row(&rows, device, efi_partition)?;
            let home = row(&rows, device, home_partition)?;
            require_filesystem(m, efi_partition, "vfat")?;
            require_filesystem(m, home_partition, "ext4")?;
            if rows.iter().any(|other| other.first > home.last) {
                return Err(Error::NotLastPartition {
                    partition: home_partition.clone(),
                });
            }
            let info =
                Command::query("sgdisk").args([format!("--info={}", home.number), device.clone()]);
            let shown = info.shown();
            let entry =
                parse::gpt_entry(&m.query(info)?).map_err(|problem| Error::UnexpectedOutput {
                    command: shown,
                    problem,
                })?;
            let dump = Command::query("dumpe2fs").args(["-h", home_partition.as_str()]);
            let shown = dump.shown();
            let usage =
                parse::ext4_usage(&m.query(dump)?).map_err(|problem| Error::UnexpectedOutput {
                    command: shown,
                    problem,
                })?;
            let plan = sizes::shrink_plan(
                sizes::Span {
                    first: entry.first,
                    last: entry.last,
                },
                sector,
                usage.block_size,
                usage.used_bytes + swap_gib * sizes::GIB,
                *system_size_gib,
            )
            .map_err(Error::HomeTooFull)?;
            let owner = look_into_home(
                m,
                home_partition,
                &Path::new("home").join(username),
                Some(Path::new("home")),
            )?;
            let system_number = rows.iter().map(|row| row.number).max().unwrap_or(0) + 1;
            Ok((
                DiskWork::Shrink {
                    device: device.clone(),
                    efi: efi_partition.clone(),
                    home: home_partition.clone(),
                    home_number: home.number,
                    entry,
                    plan,
                    system_number,
                },
                owner,
            ))
        }
        Disk::BesideWindows {
            device,
            system_size_gib,
        } => Ok((
            plan_beside_windows(m, device, *system_size_gib, swap_gib)?,
            None,
        )),
    }
}

/// The disk changes that put InayOS beside the Windows on `device`. Refuses with the same
/// words as the disk screen, read again from the real disk.
fn plan_beside_windows(
    m: &Machine,
    device: &str,
    system_size_gib: Option<u64>,
    swap_gib: u64,
) -> Result<DiskWork, Error> {
    let lsblk =
        Command::query("lsblk").args(["--json", "--bytes", "--output", inventory::COLUMNS, device]);
    let shown = lsblk.shown();
    let tree = parse::lsblk_inventory(&m.query(lsblk)?)
        .map_err(|problem| Error::UnexpectedOutput {
            command: shown.clone(),
            problem,
        })?
        .into_iter()
        .next()
        .ok_or_else(|| Error::UnexpectedOutput {
            command: shown,
            problem: "lsblk printed no device".to_owned(),
        })?;
    let found = windows::examine(m, &tree, &Path::new(TARGET).join("efi"))?
        .map_err(|reason| Error::NotBesideWindows { reason })?;
    let info = Command::query("sgdisk").args([
        format!("--info={}", found.windows_number),
        device.to_owned(),
    ]);
    let shown = info.shown();
    let entry = parse::gpt_entry(&m.query(info)?).map_err(|problem| Error::UnexpectedOutput {
        command: shown,
        problem,
    })?;
    let print = Command::query("sgdisk").args(["--print", device]);
    let shown = print.shown();
    let last_usable =
        parse::last_usable_sector(&m.query(print)?).map_err(|problem| Error::UnexpectedOutput {
            command: shown,
            problem,
        })?;
    // The partition table gives the exact sectors of the Windows partition and the end of
    // the room, which a table copied from a smaller disk puts before the end of this one.
    let mut room = found.room;
    room.windows = sizes::Span {
        first: entry.first,
        last: entry.last,
    };
    room.room_last = room.room_last.min(last_usable);
    let system_gib = match system_size_gib {
        Some(gib) => gib,
        None => inventory::beside_windows_limits(room, swap_gib)
            .map_err(|reason| Error::NotBesideWindows { reason })?
            .clamp(sizes::DEFAULT_SYSTEM_GIB),
    };
    let layout = sizes::beside_windows_layout(room, system_gib, swap_gib)
        .map_err(Error::NoRoomBesideWindows)?;
    Ok(DiskWork::BesideWindows {
        device: device.to_owned(),
        efi: found.esp,
        windows: found.windows,
        windows_number: found.windows_number,
        entry,
        layout,
        sector_size: room.sector_size,
        system_number: found.system_number,
        home_number: found.home_number,
    })
}

/// Fails unless the kept home on `partition` has room for the swap file of `swap_gib`. The
/// disk stage deletes the earlier system's swap file, so its room counts as free.
fn check_swap_room(m: &Machine, partition: &str, swap_gib: u64) -> Result<(), Error> {
    let dump = Command::query("dumpe2fs").args(["-h", partition]);
    let shown = dump.shown();
    let usage = parse::ext4_usage(&m.query(dump)?).map_err(|problem| Error::UnexpectedOutput {
        command: shown,
        problem,
    })?;
    let free = usage.free_bytes + hibernation::swap_file_bytes(m, partition)?;
    if free < swap_gib * sizes::GIB {
        return Err(Error::NoRoomForSwapFile {
            partition: partition.to_owned(),
            free_gib: free as f64 / sizes::GIB as f64,
            needed_gib: swap_gib,
        });
    }
    Ok(())
}

/// Mounts the kept partition read-only at `/mnt/home`, where the disk stage later mounts it,
/// and returns the owner of the user's home folder at `user_folder` inside it, when there is
/// one. With `required`, fails unless that folder exists at the partition's top level: the
/// shrink keeps it while deleting the rest of the old system. Fails when the user's home
/// folder belongs to root, because the new user takes over its owner.
fn look_into_home(
    m: &Machine,
    partition: &str,
    user_folder: &Path,
    required: Option<&Path>,
) -> Result<Option<(u32, u32)>, Error> {
    let point = Path::new(TARGET).join("home");
    let shown = point.display().to_string();
    m.create_dir(&point, 0o755)?;
    m.run(Command::action("mount").args(["-o", "ro,noload", partition, &shown]))?;
    let found = required.is_none_or(|folder| m.is_dir(&point.join(folder)));
    let folder = point.join(user_folder);
    let owner = if found && m.is_dir(&folder) {
        m.owner(&folder).map(Some)
    } else {
        Ok(None)
    };
    m.run(Command::action("umount").arg(&shown))?;
    if !found {
        return Err(Error::NoHomeFolder {
            partition: partition.to_owned(),
        });
    }
    match owner? {
        Some((uid, gid)) if uid == 0 || gid == 0 => Err(Error::RootOwnedHome {
            partition: partition.to_owned(),
            folder: user_folder.to_owned(),
            uid,
            gid,
        }),
        owner => Ok(owner),
    }
}
