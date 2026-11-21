//! The rollback from the repair workspace: puts an InayOS system back to one of its snapper
//! snapshots, kernel included. The system subvolume `@` becomes `@rollback-<time>`, and a
//! writable snapshot of `@snapshots/<n>/snapshot` becomes the new `@`. `/boot` lives inside
//! `@`, so a plain system then starts the snapshot's kernel. An encrypted system starts a
//! unified kernel image on the EFI partition, so the rollback builds it again from the new
//! `@` with `mkinitcpio -P` in a chroot, and signs it with `sbctl sign-all` when the
//! system has Secure Boot keys.
//!
//! A snapshot from before the system had its Secure Boot keys, or, on an encrypted system,
//! from before its unified kernel image preset, would not start, so the rollback refuses it
//! before changing anything. The new `@` gets an `/etc/fstab` without `subvolid=`, because
//! its subvolume id is new and `subvol=/@` names it.
//!
//! Nothing here mounts anything read-write before the hibernation checks have passed.
use std::path::{Path, PathBuf};

use crate::{
    Error, Sink, hibernation,
    host::{Command, Host},
    install::machine::Machine,
    inventory, parse, windows,
};

/// Where the rollback mounts the top level of the Btrfs system filesystem.
const TOP: &str = "/run/inayos-rollback/top";
/// Where the rollback mounts the new `@` for the chroot of an encrypted system.
const ROOT: &str = "/run/inayos-rollback/root";
/// The mapper names of the volumes the rollback opens. They differ from the installer's,
/// so the two never meet.
const SYSTEM_NAME: &str = "inayos-rollback-system";
const HOME_NAME: &str = "inayos-rollback-home";
const SYSTEM_LABEL: &str = "inayos-system";
const SNAPSHOTS: &str = "@snapshots";
const SYSTEM_SUBVOLUME: &str = "@";
/// sbctl's keys in the system: a system that has them is signed for Secure Boot.
const SBCTL_KEYS: &str = "var/lib/sbctl/keys";
/// The mkinitcpio preset of the kernel, in the system.
const PRESET: &str = "etc/mkinitcpio.d/linux.preset";
/// The unified kernel image that the InayOS preset builds, in the chroot.
const UKI: &str = "efi/EFI/Linux/inayos-linux.efi";
/// The copy of the image from before the rollback, in the chroot, until the rollback ends.
/// It does not end in `.efi`, so neither rEFInd nor sbctl takes it for an image.
const UKI_BEFORE: &str = "efi/EFI/Linux/inayos-linux.efi.before-rollback";

/// An InayOS system on a disk of the machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledSystem {
    /// The whole disk, such as `/dev/vda`.
    pub disk: String,
    /// The system partition: Btrfs, or LUKS2 with Btrfs inside.
    pub system: String,
    /// The home partition: ext4, or LUKS2 with ext4 inside. None when the disk has none.
    pub home: Option<String>,
    /// The EFI system partition. None when the disk has none.
    pub efi: Option<String>,
    pub encrypted: bool,
}

/// One snapper snapshot of the system.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub number: u32,
    /// `pre`, `post` or `single`.
    pub kind: String,
    /// When snapper took it, as snapper writes it: `YYYY-MM-DD HH:MM:SS` in UTC.
    pub date: String,
    /// For a pacman snapshot, the pacman command or the packages it changed.
    pub description: String,
}

/// How the encrypted volumes open.
#[derive(Clone, Copy)]
pub enum Unlock<'a> {
    /// With the TPM, through the systemd-tpm2 token of each volume.
    Tpm,
    /// With the recovery key, which opens both volumes.
    RecoveryKey(&'a str),
}

/// What a finished rollback left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RolledBack {
    /// The subvolume that holds the system from before the rollback, such as
    /// `@rollback-20251119-101500`.
    pub kept: String,
}

/// Why a rollback stopped. Each variant says what it left on the disk.
#[derive(Debug, thiserror::Error)]
pub enum RollbackError {
    #[error("{}", hibernation::HIBERNATED)]
    Hibernated,
    #[error("the TPM did not open {partition}; the recovery key opens it")]
    TpmRefused { partition: String },
    #[error("the system has no snapshot {number}")]
    NoSnapshot { number: u32 },
    #[error("snapshot {number} cannot start this machine: {reason}; the disk is unchanged")]
    Unsuitable { number: u32, reason: &'static str },
    #[error("{name} already exists on the system partition; the disk is unchanged")]
    NameTaken { name: String },
    /// A failure before the system subvolume changed.
    #[error("{0}; the disk is unchanged")]
    Unchanged(#[source] Error),
    /// A failure after the system subvolume changed, which the rollback undid.
    #[error("{0}; the rollback was undone, and the system is as before")]
    Undone(#[source] Error),
    /// A failure whose undo failed too. The system from before the rollback is in `kept`.
    #[error(
        "{error}; undoing the rollback failed too: {undo}. The system from before is the \
         subvolume {kept} at the top of the system partition: rename it back to @ before \
         starting the system"
    )]
    Unfinished {
        error: Box<Error>,
        undo: Box<Error>,
        kept: String,
    },
    /// The new boot image is built but not signed, so Secure Boot refuses it.
    #[error(
        "{error}; @ holds the snapshot and its unified kernel image is built, but not signed, \
         so Secure Boot refuses it: run `sbctl sign-all` in a chroot of it. The system from \
         before is the subvolume {kept}, and its boot image is \
         /efi/EFI/Linux/inayos-linux.efi.before-rollback"
    )]
    Unsigned { error: Box<Error>, kept: String },
}

impl From<Error> for RollbackError {
    fn from(error: Error) -> Self {
        RollbackError::Unchanged(error)
    }
}

/// The InayOS systems on the disks of the machine: each disk with a partition labeled
/// `inayos-system`. Only reads.
pub fn find(host: &dyn Host, sink: &dyn Sink) -> Result<Vec<InstalledSystem>, RollbackError> {
    let m = Machine { host, sink };
    let lsblk = Command::query("lsblk").args(["--json", "--bytes", "--output", inventory::COLUMNS]);
    let shown = lsblk.shown();
    let devices =
        parse::lsblk_inventory(&m.query(lsblk)?).map_err(|problem| Error::UnexpectedOutput {
            command: shown,
            problem,
        })?;
    Ok(devices
        .iter()
        .filter(|device| device.kind == "disk")
        .filter_map(|disk| {
            let labeled = |label: &str| {
                disk.children
                    .iter()
                    .find(|part| part.label == label && part.kind == "part")
            };
            let system = labeled(SYSTEM_LABEL)
                .filter(|part| matches!(part.fstype.as_str(), "btrfs" | "crypto_LUKS"))?;
            Some(InstalledSystem {
                disk: disk.path.clone(),
                system: system.path.clone(),
                home: labeled(inventory::HOME_LABEL).map(|part| part.path.clone()),
                efi: disk
                    .children
                    .iter()
                    .find(|part| part.parttype == windows::EFI_TYPE && part.fstype == "vfat")
                    .map(|part| part.path.clone()),
                encrypted: system.fstype == "crypto_LUKS",
            })
        })
        .collect())
}

/// An InayOS system whose system filesystem is open and its top level mounted. `close`
/// unmounts and closes what `open` opened.
pub struct Opened<'a> {
    m: Machine<'a>,
    system: InstalledSystem,
    /// The Btrfs filesystem: the system partition, or its opened volume.
    filesystem: String,
}

fn mapper(name: &str) -> String {
    format!("/dev/mapper/{name}")
}

/// Opens the LUKS2 volume `partition` as `name`, read-only when `read_only`.
fn open_volume(
    m: &Machine,
    partition: &str,
    name: &str,
    unlock: Unlock,
    read_only: bool,
) -> Result<(), RollbackError> {
    let mut command = Command::action("cryptsetup").arg("open");
    if read_only {
        command = command.arg("--readonly");
    }
    match unlock {
        Unlock::Tpm => {
            let command = command.args([
                "--token-only",
                "--token-type",
                "systemd-tpm2",
                partition,
                name,
            ]);
            if !m.run_any(&command)?.success() {
                return Err(RollbackError::TpmRefused {
                    partition: partition.to_owned(),
                });
            }
        }
        // `--key-file=-` reads the whole input as the key, as the installer stored it.
        Unlock::RecoveryKey(key) => {
            m.run(
                command
                    .args(["--key-file=-", partition, name])
                    .stdin(key.trim()),
            )?;
        }
    }
    Ok(())
}

/// Opens `system` for a rollback. Refuses a system that hibernated, before anything opens
/// read-write: the firmware variable comes first, then the swap file header of the home
/// partition, read through an encrypted home opened read-only.
pub fn open<'a>(
    host: &'a dyn Host,
    sink: &'a dyn Sink,
    system: &InstalledSystem,
    unlock: Unlock,
) -> Result<Opened<'a>, RollbackError> {
    let m = Machine { host, sink };
    if hibernation::firmware_hibernated(&m) {
        return Err(RollbackError::Hibernated);
    }
    if let Some(home) = &system.home {
        let hibernated = if system.encrypted {
            sink.step(&format!(
                "Opening {home} read-only to check for hibernation"
            ));
            open_volume(&m, home, HOME_NAME, unlock, true)?;
            let found = hibernation::swap_file_hibernated(&m, &mapper(HOME_NAME));
            m.run(Command::action("cryptsetup").args(["close", HOME_NAME]))?;
            found?
        } else {
            hibernation::swap_file_hibernated(&m, home)?
        };
        if hibernated {
            return Err(RollbackError::Hibernated);
        }
    }
    let filesystem = if system.encrypted {
        sink.step(&format!("Opening {}", system.system));
        open_volume(&m, &system.system, SYSTEM_NAME, unlock, false)?;
        mapper(SYSTEM_NAME)
    } else {
        system.system.clone()
    };
    let opened = Opened {
        m,
        system: system.clone(),
        filesystem,
    };
    if let Err(error) = opened.mount_top() {
        for leftover in opened.close() {
            sink.detail(&format!("cleanup failed: {leftover}"));
        }
        return Err(error.into());
    }
    Ok(opened)
}

impl Opened<'_> {
    fn top(&self, relative: &str) -> PathBuf {
        Path::new(TOP).join(relative)
    }

    fn mount_top(&self) -> Result<(), Error> {
        self.m.create_dir(Path::new(TOP), 0o700)?;
        self.m.run(Command::action("mount").args([
            "-o",
            "subvolid=5",
            self.filesystem.as_str(),
            TOP,
        ]))?;
        Ok(())
    }

    /// The snapshots in `@snapshots`, by number. A folder without a readable `info.xml` or
    /// without its `snapshot` subvolume is left out.
    pub fn snapshots(&self) -> Result<Vec<Snapshot>, RollbackError> {
        let folder = self.top(SNAPSHOTS);
        let mut found: Vec<Snapshot> = self
            .m
            .list(&folder)?
            .iter()
            .filter(|name| name.parse::<u32>().is_ok())
            .filter(|name| self.m.is_dir(&folder.join(name).join("snapshot")))
            .filter_map(|name| {
                let info = self.m.read(&folder.join(name).join("info.xml")).ok()?;
                snapshot_info(&info)
            })
            .collect();
        found.sort_by_key(|snapshot| snapshot.number);
        Ok(found)
    }

    /// Puts the system back to snapshot `number`, as the module text describes. A failure
    /// after `@` changed is undone when it can be, and says what it left otherwise.
    pub fn roll_back(&self, number: u32) -> Result<RolledBack, RollbackError> {
        let snapshot = self.top(&format!("{SNAPSHOTS}/{number}/snapshot"));
        if !self.m.is_dir(&snapshot) {
            return Err(RollbackError::NoSnapshot { number });
        }
        // The current system decides: sbctl's keys are in `@`, and the firmware that
        // enrolled them refuses an unsigned kernel.
        let signed = self.m.is_dir(&self.top(SYSTEM_SUBVOLUME).join(SBCTL_KEYS));
        if let Some(reason) = self.unsuitable(&snapshot, signed) {
            return Err(RollbackError::Unsuitable { number, reason });
        }
        let stamp = self
            .m
            .query(Command::query("date").args(["-u", "+%Y%m%d-%H%M%S"]))?;
        let kept = format!("@rollback-{}", stamp.trim());
        if self.m.exists(&self.top(&kept)) {
            return Err(RollbackError::NameTaken { name: kept });
        }
        let current = self.top(SYSTEM_SUBVOLUME);
        self.m
            .sink
            .step(&format!("Keeping the current system as {kept}"));
        self.m.rename(&current, &self.top(&kept))?;
        self.m
            .sink
            .step(&format!("Making snapshot {number} the system"));
        let made = self.m.run(Command::action("btrfs").args([
            "subvolume".to_owned(),
            "snapshot".to_owned(),
            snapshot.display().to_string(),
            current.display().to_string(),
        ]));
        if let Err(error) = made {
            return Err(self.undo(error, &kept, false));
        }
        if let Err(error) = self.drop_subvolume_ids(&snapshot) {
            return Err(self.undo(error, &kept, true));
        }
        if self.system.encrypted {
            self.rebuild_boot_image(&kept, signed)?;
        }
        Ok(RolledBack { kept })
    }

    /// Why `snapshot` would not start this machine, when it would not. `signed` says
    /// whether the current system has Secure Boot keys.
    fn unsuitable(&self, snapshot: &Path, signed: bool) -> Option<&'static str> {
        if signed && !self.m.is_dir(&snapshot.join(SBCTL_KEYS)) {
            return Some(
                "it is from before Secure Boot was set up, so its kernel is not signed and the \
                 firmware refuses it",
            );
        }
        if self.system.encrypted
            && !self
                .m
                .read(&snapshot.join(PRESET))
                .is_ok_and(|preset| preset.contains(&format!("/{UKI}")))
        {
            return Some(
                "it is from before the encryption was set up, so it builds no unified kernel \
                 image for the encrypted system",
            );
        }
        None
    }

    /// Writes the new `@`'s `/etc/fstab` without `subvolid=` options. genfstab writes both
    /// `subvolid=` and `subvol=`, and the id of `@` is now the one of the kept system.
    fn drop_subvolume_ids(&self, snapshot: &Path) -> Result<(), Error> {
        let source = snapshot.join("etc/fstab");
        if !self.m.exists(&source) {
            return Ok(());
        }
        let fstab = self.m.read(&source)?;
        let fixed = without_subvolume_ids(&fstab);
        if fixed != fstab {
            self.m
                .write(&self.top(SYSTEM_SUBVOLUME).join("etc/fstab"), &fixed, 0o644)?;
        }
        Ok(())
    }

    /// Builds the unified kernel image again from the new `@`, and signs it when `signed`.
    /// The image from before stays aside until the new one is built, and comes back when
    /// the rollback is undone.
    fn rebuild_boot_image(&self, kept: &str, signed: bool) -> Result<(), RollbackError> {
        let Some(efi) = &self.system.efi else {
            return Err(self.undo(
                Error::MissingPartition {
                    partition: "the EFI system partition".to_owned(),
                },
                kept,
                true,
            ));
        };
        self.m
            .sink
            .step("Building the boot image of the snapshot's kernel");
        let root = Path::new(ROOT);
        let (image, before) = (root.join(UKI), root.join(UKI_BEFORE));
        let mut set_aside = false;
        let built = (|| -> Result<(), Error> {
            self.m.create_dir(root, 0o700)?;
            self.m.run(Command::action("mount").args([
                "-o",
                "subvol=@",
                self.filesystem.as_str(),
                ROOT,
            ]))?;
            self.m
                .run(Command::action("mount").args([efi.as_str(), &format!("{ROOT}/efi")]))?;
            if self.m.exists(&image) {
                self.m.copy(&image, &before, 0o644)?;
                set_aside = true;
            }
            self.m
                .run(Command::action("arch-chroot").args([ROOT, "mkinitcpio", "-P"]))?;
            Ok(())
        })();
        if let Err(error) = built {
            let restored = (|| -> Result<(), Error> {
                if set_aside {
                    self.m.copy(&before, &image, 0o644)?;
                    self.m.remove_file(&before)?;
                }
                self.unmount_root()
            })();
            if let Err(undo) = restored {
                return Err(RollbackError::Unfinished {
                    error: Box::new(error),
                    undo: Box::new(undo),
                    kept: kept.to_owned(),
                });
            }
            return Err(self.undo(error, kept, true));
        }
        if signed {
            self.m.sink.step("Signing the boot files for Secure Boot");
            if let Err(error) = self
                .m
                .run(Command::action("arch-chroot").args([ROOT, "sbctl", "sign-all"]))
            {
                return Err(RollbackError::Unsigned {
                    error: Box::new(error),
                    kept: kept.to_owned(),
                });
            }
        }
        if set_aside && let Err(error) = self.m.remove_file(&before) {
            self.m
                .sink
                .detail(&format!("could not delete {}: {error}", before.display()));
        }
        // `close` unmounts the chroot.
        Ok(())
    }

    fn unmount_root(&self) -> Result<(), Error> {
        if self
            .m
            .run_any(&Command::query("mountpoint").args(["-q", ROOT]))?
            .success()
        {
            self.m.run(Command::action("umount").args(["-R", ROOT]))?;
        }
        Ok(())
    }

    /// Puts `kept` back as `@` after `error`. `made` says whether the new `@` exists and
    /// must go first.
    fn undo(&self, error: Error, kept: &str, made: bool) -> RollbackError {
        let current = self.top(SYSTEM_SUBVOLUME);
        let undone = (|| -> Result<(), Error> {
            if made || self.m.exists(&current) {
                self.m.run(Command::action("btrfs").args([
                    "subvolume".to_owned(),
                    "delete".to_owned(),
                    current.display().to_string(),
                ]))?;
            }
            self.m.rename(&self.top(kept), &current)
        })();
        match undone {
            Ok(()) => RollbackError::Undone(error),
            Err(undo) => RollbackError::Unfinished {
                error: Box::new(error),
                undo: Box::new(undo),
                kept: kept.to_owned(),
            },
        }
    }

    /// Unmounts what the rollback mounted and closes the volume it opened. Returns every
    /// step that failed.
    pub fn close(&self) -> Vec<Error> {
        let mut errors = Vec::new();
        if let Err(error) = self.unmount_root() {
            errors.push(error);
        }
        match self
            .m
            .run_any(&Command::query("mountpoint").args(["-q", TOP]))
        {
            Ok(output) if output.success() => {
                if let Err(error) = self.m.run(Command::action("umount").arg(TOP)) {
                    errors.push(error);
                }
            }
            Ok(_) => {}
            Err(error) => errors.push(error),
        }
        if self.system.encrypted
            && self.m.exists(Path::new(&mapper(SYSTEM_NAME)))
            && let Err(error) = self
                .m
                .run(Command::action("cryptsetup").args(["close", SYSTEM_NAME]))
        {
            errors.push(error);
        }
        errors
    }
}

/// `fstab` without its `subvolid=` mount options. `subvol=` names each subvolume by its
/// path, which a rollback keeps.
pub fn without_subvolume_ids(fstab: &str) -> String {
    let mut fixed: String = fstab
        .lines()
        .map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if line.trim_start().starts_with('#') || fields.len() < 4 {
                return line.to_owned();
            }
            let options: Vec<&str> = fields[3]
                .split(',')
                .filter(|option| !option.starts_with("subvolid="))
                .collect();
            if options.len() == fields[3].split(',').count() {
                return line.to_owned();
            }
            let mut fields = fields;
            let joined = options.join(",");
            fields[3] = &joined;
            fields.join("\t")
        })
        .collect::<Vec<_>>()
        .join("\n");
    if fstab.ends_with('\n') {
        fixed.push('\n');
    }
    fixed
}

/// The snapshot that snapper's `info.xml` describes. None without a number.
pub fn snapshot_info(xml: &str) -> Option<Snapshot> {
    Some(Snapshot {
        number: element(xml, "num")?.trim().parse().ok()?,
        kind: element(xml, "type").unwrap_or_default(),
        date: element(xml, "date").unwrap_or_default(),
        description: element(xml, "description").unwrap_or_default(),
    })
}

/// The text of the first `<name>` element of `xml`, with the XML entities decoded.
fn element(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = xml.find(&open)? + open.len();
    let end = start + xml[start..].find(&close)?;
    Some(
        xml[start..end]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&amp;", "&")
            .trim()
            .to_owned(),
    )
}

#[cfg(test)]
mod tests;
