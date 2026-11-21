//! Hibernation: a swap file on the ext4 home partition, as large as the memory, which a
//! swap unit turns on at boot with a priority below zram's. systemd hibernates into it and
//! records where in the `HibernateLocation` firmware variable, which the initramfs reads to
//! resume, so no `resume=` kernel option is needed. On an encrypted system, the initramfs
//! opens the home volume before it resumes, because `/etc/crypttab.initramfs` lists it.
//!
//! The unit is under `dotfiles/assets/hibernation`, installed at the same path under `/`.
use std::path::{Path, PathBuf};

use embedded_dotfiles::HIBERNATION_ASSETS;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, ServiceScope,
    boot_config::images_without,
    support::{read_system_bytes, read_system_file},
};

pub struct Hibernation;

impl Hibernation {
    pub const NAME: &'static str = "hibernation";
}

const HOME: &str = "/home";
const SWAP_FILE: &str = "/home/swapfile";
/// The unit named after the swap file's path, as systemd requires.
const SWAP_UNIT: &str = "home-swapfile.swap";
/// The program that the systemd hook adds to the initramfs to resume.
const RESUME: &str = "usr/lib/systemd/systemd-hibernate-resume";
const GIB: u64 = 1024 * 1024 * 1024;
const NO_HOME: &str = "an ext4 filesystem mounted at /home";

/// Each embedded file with the absolute path it is installed at.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile)> {
    HIBERNATION_ASSETS
        .files()
        .into_iter()
        .map(|file| (Path::new("/").join(file.path_from_root), file))
        .collect()
}

fn is_current(ctx: &Context, path: &Path, file: &EmbeddedFile) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("a hibernation path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(file.content))
}

fn home_is_ext4(ctx: &Context) -> ModuleResult<bool> {
    let (mounted, filesystem) = ctx.read_unchecked(cmd!(
        ctx.shell,
        "findmnt --noheadings --output FSTYPE --mountpoint {HOME}"
    ))?;
    Ok(mounted && filesystem.lines().any(|line| line.trim() == "ext4"))
}

/// The memory size in whole GiB, rounded up, from the text of `/proc/meminfo`.
fn memory_gib(meminfo: &str) -> Option<u64> {
    let kib: u64 = meminfo.lines().find_map(|line| {
        let value = line.strip_prefix("MemTotal:")?;
        value.trim().strip_suffix("kB")?.trim().parse().ok()
    })?;
    Some((kib * 1024).div_ceil(GIB))
}

/// The size the swap file must have: the memory size, rounded up to a whole GiB.
fn wanted_bytes(ctx: &Context) -> ModuleResult<u64> {
    let meminfo = read_system_file(ctx, "/proc/meminfo")?.unwrap_or_default();
    let gib = memory_gib(&meminfo).ok_or("/proc/meminfo gives no MemTotal")?;
    Ok(gib * GIB)
}

/// What the swap file looks like now.
#[derive(Debug, PartialEq, Eq)]
enum SwapFile {
    Missing,
    /// A swap area of the wanted size, owned by root and readable by root only.
    Ready,
    /// Anything else, with what is wrong.
    Wrong(String),
}

fn swap_file(ctx: &Context, wanted: u64) -> ModuleResult<SwapFile> {
    let sh = ctx.shell;
    if !ctx.succeeds(cmd!(sh, "sudo test -e {SWAP_FILE}"))? {
        return Ok(SwapFile::Missing);
    }
    let stat = ctx.read(cmd!(sh, "sudo stat --format %s:%a:%U {SWAP_FILE}"))?;
    let expected = format!("{wanted}:600:root");
    if stat.trim() != expected {
        return Ok(SwapFile::Wrong(format!(
            "{SWAP_FILE} has size, mode and owner {}, not {expected}",
            stat.trim()
        )));
    }
    let (_, kind) = ctx.read_unchecked(cmd!(
        sh,
        "sudo blkid --probe --output value --match-tag TYPE {SWAP_FILE}"
    ))?;
    if kind.trim() != "swap" {
        return Ok(SwapFile::Wrong(format!(
            "{SWAP_FILE} holds no swap signature"
        )));
    }
    Ok(SwapFile::Ready)
}

/// Whether the swap file is turned on, which `/proc/swaps` lists.
fn swap_file_on(ctx: &Context) -> ModuleResult<bool> {
    let swaps = read_system_file(ctx, "/proc/swaps")?.unwrap_or_default();
    Ok(swaps
        .lines()
        .any(|line| line.split_whitespace().next() == Some(SWAP_FILE)))
}

/// Creates the swap file. `mkswap --file` allocates it on the filesystem, which the
/// kernel needs to write the memory image into it.
fn create_swap_file(ctx: &Context, wanted: u64) -> ModuleResult {
    let sh = ctx.shell;
    let size = wanted.to_string();
    ctx.created(Path::new(SWAP_FILE), true)?;
    ctx.run(cmd!(sh, "sudo mkswap --file --size {size} {SWAP_FILE}"))?;
    ctx.run(cmd!(sh, "sudo chmod 600 {SWAP_FILE}"))?;
    ctx.run(cmd!(sh, "sudo chown root:root {SWAP_FILE}"))
}

impl Module for Hibernation {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: Vec::new(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        if !home_is_ext4(ctx)? {
            return ctx.missing_part(NO_HOME, "the hibernation swap file");
        }
        let wanted = wanted_bytes(ctx)?;
        match swap_file(ctx, wanted)? {
            SwapFile::Ready => {}
            SwapFile::Missing => create_swap_file(ctx, wanted)?,
            // A swap file from before a memory change, or one that InayOS did not make.
            // Install never deletes it: `remove` undoes this module's own swap file.
            SwapFile::Wrong(problem) => {
                let fix = if ctx.was_created(Path::new(SWAP_FILE)) {
                    format!("run `inayos remove {}`, then install again", Self::NAME)
                } else {
                    format!(
                        "turn it off with `sudo swapoff {SWAP_FILE}` when it is on, delete it, \
                         then install again"
                    )
                };
                return Err(format!("{problem}; {fix}").into());
            }
        }
        for (path, file) in files() {
            ctx.write_system_file(&path, file.content, "0644")?;
        }
        ctx.enable_service(SWAP_UNIT, ServiceScope::System)?;
        let _ = ctx.start_service(SWAP_UNIT, ServiceScope::System)?;
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if !home_is_ext4(ctx)? {
            return ctx.missing_part(NO_HOME, "the hibernation check");
        }
        for (path, file) in files() {
            if !is_current(ctx, &path, file)? {
                return Err(format!(
                    "{} does not hold the InayOS hibernation swap unit",
                    path.display()
                )
                .into());
            }
        }
        match swap_file(ctx, wanted_bytes(ctx)?)? {
            SwapFile::Ready => {}
            SwapFile::Missing => return Err(format!("{SWAP_FILE} is missing").into()),
            SwapFile::Wrong(problem) => return Err(problem.into()),
        }
        let missing = images_without(ctx, RESUME)?;
        if !missing.is_empty() {
            return Err(format!(
                "these initramfs images cannot resume from hibernation, because they lack systemd-hibernate-resume: {}",
                missing.join(", ")
            )
            .into());
        }
        let _ = ctx.require_active(SWAP_UNIT, ServiceScope::System)?;
        Ok(())
    }

    /// Turns the swap file off first, so the undo can delete it.
    fn remove(&self, ctx: &Context) -> ModuleResult {
        if swap_file_on(ctx)? {
            ctx.stop_service(SWAP_UNIT, ServiceScope::System)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::memory_gib;

    #[test]
    fn the_swap_file_is_the_memory_rounded_up_to_a_whole_gib() {
        assert_eq!(
            memory_gib("MemTotal:        8040956 kB\nMemFree: 1 kB\n"),
            Some(8)
        );
        assert_eq!(memory_gib("MemTotal:        8388608 kB\n"), Some(8));
        assert_eq!(memory_gib("MemTotal:        8388609 kB\n"), Some(9));
        assert_eq!(memory_gib("MemFree: 1 kB\n"), None);
    }
}
