//! Whether a hibernated InayOS waits on the machine. Writing to its disk before it resumes
//! loses the work it saved, so the install refuses such a disk before it mounts anything
//! read-write. Both checks only read. The swap file's size, which a home partition must
//! have room for, comes from here too.
use std::path::Path;

use crate::{Error, host::Command, install::machine::Machine, parse};

/// Why a disk is refused while a hibernated InayOS waits on it.
pub const HIBERNATED: &str =
    "This disk holds a hibernated InayOS: start it and shut it down properly first";
/// The firmware variable that systemd writes before it hibernates and clears when it
/// resumes. Every system that starts on the machine sees it.
pub(crate) const HIBERNATE_LOCATION: &str =
    "/sys/firmware/efi/efivars/HibernateLocation-8cf2644b-4b0b-428f-9387-6d876050dc67";
/// The hibernation swap file at the top of the home partition, `/home/swapfile` once
/// mounted.
const SWAP_FILE: &str = "/swapfile";
const MEMINFO: &str = "/proc/meminfo";
/// The bytes of the swap header page, which ends with the swap signature.
const PAGE: u64 = 4096;

/// Whether the firmware says that a system on this machine hibernated.
pub(crate) fn firmware_hibernated(m: &Machine) -> bool {
    m.exists(Path::new(HIBERNATE_LOCATION))
}

/// Whether the ext4 filesystem on `partition` holds a swap file whose header says that a
/// system hibernated into it. It reads the file's first block through debugfs, without
/// mounting the filesystem. A partition without the file, or that debugfs cannot read, is
/// not hibernated.
pub(crate) fn swap_file_hibernated(m: &Machine, partition: &str) -> Result<bool, Error> {
    let bmap = m.run_any(&Command::query("debugfs").args([
        "-R".to_owned(),
        format!("bmap {SWAP_FILE} 0"),
        partition.to_owned(),
    ]))?;
    let Some(block) = parse::first_block(&bmap.stdout).filter(|_| bmap.success()) else {
        return Ok(false);
    };
    let dump = Command::query("dumpe2fs").args(["-h", partition]);
    let shown = dump.shown();
    let usage = parse::ext4_usage(&m.query(dump)?).map_err(|problem| Error::UnexpectedOutput {
        command: shown,
        problem,
    })?;
    // The page holds binary data, which stays out of the log.
    let page = m.query(
        Command::query("dd")
            .args([
                format!("if={partition}"),
                "iflag=skip_bytes,count_bytes".to_owned(),
                format!("skip={}", block * usage.block_size),
                format!("count={PAGE}"),
                "status=none".to_owned(),
            ])
            .hidden_output(),
    )?;
    Ok(parse::holds_hibernation_signature(&page))
}

/// The size in GiB of the swap file that the hibernation module creates in home: the
/// memory size, rounded up to a whole GiB.
pub(crate) fn swap_gib(m: &Machine) -> Result<u64, Error> {
    let meminfo = m.read(Path::new(MEMINFO))?;
    parse::memory_gib(&meminfo).ok_or_else(|| Error::UnexpectedOutput {
        command: format!("cat {MEMINFO}"),
        problem: "it gives no MemTotal".to_owned(),
    })
}

/// The size of the swap file on the ext4 filesystem on `partition`, read through debugfs
/// without mounting it. Zero when there is none.
pub(crate) fn swap_file_bytes(m: &Machine, partition: &str) -> Result<u64, Error> {
    let stat = m.run_any(&Command::query("debugfs").args([
        "-R".to_owned(),
        format!("stat {SWAP_FILE}"),
        partition.to_owned(),
    ]))?;
    Ok(parse::debugfs_size(&stat.stdout)
        .filter(|_| stat.success())
        .unwrap_or(0))
}
