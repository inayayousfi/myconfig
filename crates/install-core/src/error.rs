//! Why an install failed, and at which stage, which says what the failure left on the disk.
use std::{fmt, io, path::PathBuf};

use crate::sizes::{HomeTooFull, NoRoomBesideWindows, TooSmall};

/// The stages of an install, in order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Checks of the machine and the answers. The disk is unchanged.
    Checks,
    /// Partitioning, formatting and mounting.
    Disk,
    /// The base system: packages, fstab, locale, keymap, time zone, host name, network.
    System,
    /// The user account and its sudo rules.
    User,
    /// `inayos install` and `inayos verify` inside the new system.
    Configuration,
    /// The recovery key and the TPM unlock of the encrypted volumes, which replace the
    /// install's temporary key.
    Keys,
    /// Cleanup: the installer's sudo rule, the log copy and the unmount.
    Finish,
}

impl Stage {
    /// What a failure at this stage leaves on the disk.
    pub fn left(self) -> &'static str {
        match self {
            Stage::Checks => "the disk is unchanged",
            Stage::Disk => "the disk is partly partitioned or formatted",
            Stage::System | Stage::User | Stage::Configuration => {
                "the disk holds an unfinished system that must not be booted"
            }
            Stage::Keys => {
                "the disk holds an installed system whose encrypted volumes may not open \
                 without the installer's temporary key, which is gone once the installer \
                 stops; install again"
            }
            Stage::Finish => "the system is installed, but its cleanup is unfinished",
        }
    }
}

/// One failure. Each variant names a distinct cause; the fields carry its details.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("the answers are invalid: {0}")]
    InvalidAnswers(#[from] crate::AnswersError),
    #[error("{}", crate::hibernation::HIBERNATED)]
    Hibernated,
    #[error("encryption needs a TPM 2.0, and this machine has none")]
    EncryptionWithoutTpm,
    #[error("encryption needs a whole disk for now")]
    EncryptionNeedsWholeDisk,
    #[error("`systemd-cryptenroll --recovery-key` printed no recovery key")]
    NoRecoveryKey,
    #[error("the installer must run as root")]
    NotRoot,
    #[error("the machine did not start in UEFI mode: /sys/firmware/efi/efivars is missing")]
    NotUefi,
    #[error("/mnt is already a mount point; unmount it first")]
    TargetBusy,
    #[error("{device} is not a whole disk")]
    NotWholeDisk { device: String },
    #[error("{device} holds the USB stick the installer runs from")]
    LiveMedium { device: String },
    #[error("{device} is mounted at {mountpoint}; unmount it first")]
    Mounted { device: String, mountpoint: String },
    #[error("{partition} is not in the partition table of the disk")]
    MissingPartition { partition: String },
    #[error("{partition} holds {found}, but {expected} is needed")]
    WrongFilesystem {
        partition: String,
        expected: &'static str,
        found: String,
    },
    #[error("{partition} must be the last partition on the disk, so the system fits after it")]
    NotLastPartition { partition: String },
    #[error("{partition} has no home folder at its top level")]
    NoHomeFolder { partition: String },
    #[error(
        "the folder {} on {partition} belongs to user id {uid} and group id {gid}; the new \
         user takes over that owner, and id 0 is root's",
        folder.display()
    )]
    RootOwnedHome {
        partition: String,
        folder: PathBuf,
        uid: u32,
        gid: u32,
    },
    #[error("the disk is too small: {0}")]
    DiskTooSmall(TooSmall),
    #[error("the home partition cannot shrink enough: {0}")]
    HomeTooFull(HomeTooFull),
    /// InayOS cannot go beside the Windows on the disk; `reason` says why in plain words,
    /// as the disk screen shows it.
    #[error("{reason}")]
    NotBesideWindows { reason: String },
    #[error("{0}")]
    NoRoomBesideWindows(NoRoomBesideWindows),
    /// The recreated GPT entry of the Windows partition is not the planned one. The Windows
    /// filesystem has already shrunk, and nothing else on the disk changed.
    #[error(
        "the new GPT entry of the Windows partition {partition} is wrong: {problem}. Windows \
         cannot start until the entry starts at sector {first} again with its old type and \
         unique GUID; the install made no other change to the disk"
    )]
    WindowsEntryChanged {
        partition: String,
        problem: String,
        first: u64,
    },
    #[error(
        "{partition} has {free_gib:.1} GiB free, and the hibernation swap file needs {needed_gib} \
         GiB, the size of the memory"
    )]
    NoRoomForSwapFile {
        partition: String,
        free_gib: f64,
        needed_gib: u64,
    },
    #[error("the system binary {path} is missing")]
    NoSystemBinary { path: PathBuf },
    #[error("the InayOS package repository {path} is missing or has no inayos.db")]
    NoRepository { path: PathBuf },
    #[error(
        "Secure Boot needs the firmware in Setup Mode, and it is not: turn on Setup Mode in the \
         firmware settings, or install without Secure Boot"
    )]
    SetupModeOff,
    #[error(
        "the firmware is still in Setup Mode after `sbctl enroll-keys`, so it holds no Secure \
         Boot keys of this machine"
    )]
    KeysNotEnrolled,
    #[error("the locale {locale} is not in /etc/locale.gen")]
    UnknownLocale { locale: String },
    #[error("the time zone {timezone} is not in /usr/share/zoneinfo")]
    UnknownTimezone { timezone: String },
    #[error("{module} is not a module of the InayOS profile")]
    UnknownModule { module: String },
    #[error("the Arch mirrors cannot be reached: `pacman -Sy` ended with {status}")]
    MirrorUnreachable { status: String },
    #[error("the new system has no passwd entry for {username}")]
    NoUserEntry { username: String },
    #[error("`{command}` ended with {status}{}", stderr_suffix(stderr))]
    Command {
        command: String,
        status: String,
        stderr: String,
    },
    #[error("could not start `{command}`: {error}")]
    Spawn {
        command: String,
        #[source]
        error: io::Error,
    },
    #[error("`{command}` printed something unexpected: {problem}")]
    UnexpectedOutput { command: String, problem: String },
    #[error("could not {action} {}: {error}", path.display())]
    Io {
        action: &'static str,
        path: PathBuf,
        #[source]
        error: io::Error,
    },
}

fn stderr_suffix(stderr: &str) -> String {
    let stderr = stderr.trim();
    if stderr.is_empty() {
        String::new()
    } else {
        format!(": {stderr}")
    }
}

/// A failed install: the stage that failed, its error, any cleanup that failed after it,
/// and what the install made before it failed.
#[derive(Debug)]
pub struct Failure {
    pub stage: Stage,
    pub error: Error,
    pub cleanup: Vec<Error>,
    /// The recovery key once the install created it, which the interface shows the user
    /// even after a failure: the encrypted volumes may open with nothing else.
    pub installed: crate::Installed,
}

impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}; {}", self.error, self.stage.left())?;
        for error in &self.cleanup {
            write!(formatter, "\ncleanup also failed: {error}")?;
        }
        Ok(())
    }
}

impl std::error::Error for Failure {}
