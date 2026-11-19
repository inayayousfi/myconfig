//! snapper snapshots of the Btrfs system: snap-pac takes a pre and a post snapshot of
//! every pacman transaction, a number cleanup keeps the newest ones, and the initramfs
//! gets the `sd-volatile` hook that boots a read-only snapshot with an overlay on top.
//!
//! Each file is under `dotfiles/assets/snapshots`, installed at the same path under `/`.
use std::path::{Path, PathBuf};

use embedded_dotfiles::SNAPSHOTS_ASSETS;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    support::{edit_system_file, read_system_bytes, read_system_file, verify_packages},
};

pub struct Snapshots;

impl Snapshots {
    pub const NAME: &'static str = "snapshots";
}

const PACKAGES: &[Package] = &[Package::Snapper, Package::SnapPac];
/// Where the installer mounts the `@snapshots` subvolume, which snapper uses as the
/// snapshot folder of the root configuration.
const SNAPSHOTS: &str = "/.snapshots";
/// snapper and snap-pac run the configurations that this file lists.
const SNAPPER_CONFIGS: &str = "/etc/conf.d/snapper";
const CONFIG_NAME: &str = "root";
const CLEANUP_TIMER: &str = "snapper-cleanup.timer";
/// The initramfs drop-in that adds `sd-volatile` to the hooks.
const DROP_IN: &str = "/etc/mkinitcpio.conf.d/inayos-volatile.conf";
const NO_SNAPSHOT_FOLDER: &str = "a Btrfs subvolume mounted at /.snapshots";
/// The program that the `sd-volatile` hook adds to the initramfs.
const VOLATILE_ROOT: &str = "usr/lib/systemd/systemd-volatile-root";

/// Each embedded file with the absolute path it is installed at.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile)> {
    SNAPSHOTS_ASSETS
        .files()
        .into_iter()
        .map(|file| (Path::new("/").join(file.path_from_root), file))
        .collect()
}

fn is_current(ctx: &Context, path: &Path, file: &EmbeddedFile) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("a snapshots path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(file.content))
}

/// Whether a Btrfs filesystem is mounted at `/.snapshots`.
fn snapshot_folder_mounted(ctx: &Context) -> ModuleResult<bool> {
    let (mounted, filesystem) = ctx.read_unchecked(cmd!(
        ctx.shell,
        "findmnt --noheadings --output FSTYPE --mountpoint {SNAPSHOTS}"
    ))?;
    Ok(mounted && filesystem.lines().any(|line| line.trim() == "btrfs"))
}

/// The initramfs images in /boot that lack the `sd-volatile` hook, so a snapshot that
/// one of them starts would not get its overlay.
fn images_without_overlay(ctx: &Context) -> ModuleResult<Vec<String>> {
    let sh = ctx.shell;
    let listed = ctx.read(cmd!(
        sh,
        "sudo find /boot/ -maxdepth 1 -type f -name initramfs-*.img -printf %f\\n"
    ))?;
    let mut missing = Vec::new();
    for name in listed.lines() {
        let image = format!("/boot/{name}");
        let contents = ctx.read(cmd!(sh, "sudo lsinitcpio {image}"))?;
        if !contents
            .lines()
            .any(|line| line.trim().trim_start_matches("./") == VOLATILE_ROOT)
        {
            missing.push(image);
        }
    }
    Ok(missing)
}

/// The configuration names of the `SNAPPER_CONFIGS="..."` line of `conf`.
fn listed_configs(conf: &str) -> Option<Vec<&str>> {
    conf.lines().find_map(|line| {
        let value = line.trim().strip_prefix("SNAPPER_CONFIGS=")?;
        Some(value.trim_matches('"').split_whitespace().collect())
    })
}

/// `conf` with the root configuration added to `SNAPPER_CONFIGS`, keeping the others.
fn with_root_config(conf: &str) -> String {
    match listed_configs(conf) {
        Some(names) if names.contains(&CONFIG_NAME) => conf.to_owned(),
        Some(names) => {
            let mut names: Vec<&str> = names;
            names.push(CONFIG_NAME);
            let line = format!("SNAPPER_CONFIGS=\"{}\"", names.join(" "));
            conf.split_inclusive('\n')
                .map(|current| {
                    if current.trim().starts_with("SNAPPER_CONFIGS=") {
                        format!("{line}{}", &current[current.trim_end_matches('\n').len()..])
                    } else {
                        current.to_owned()
                    }
                })
                .collect()
        }
        None => {
            let mut conf = conf.to_owned();
            if !conf.is_empty() && !conf.ends_with('\n') {
                conf.push('\n');
            }
            conf.push_str("SNAPPER_CONFIGS=\"root\"\n");
            conf
        }
    }
}

impl Module for Snapshots {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        if !snapshot_folder_mounted(ctx)? {
            return ctx.missing_part(NO_SNAPSHOT_FOLDER, "the snapshots");
        }
        ctx.install_packages(PACKAGES)?;
        // The configuration exists before SNAPPER_CONFIGS names it, so snap-pac never runs
        // a configuration that is missing.
        for (path, file) in files() {
            if path == Path::new(DROP_IN) && !is_current(ctx, &path, file)? {
                // Recorded before the drop-in, so remove rebuilds once the hook is gone.
                ctx.record_undo_command(Path::new("sudo"), &["mkinitcpio", "-P"])?;
            }
            ctx.write_system_file(&path, file.content, "0644")?;
        }
        if edit_system_file(ctx, SNAPPER_CONFIGS, with_root_config)?.is_none() {
            ctx.write_system_file(
                Path::new(SNAPPER_CONFIGS),
                with_root_config("").as_bytes(),
                "0644",
            )?;
        }
        // Read from the images rather than from the drop-in, so a run that stopped before
        // the rebuild rebuilds on the next one.
        if !images_without_overlay(ctx)?.is_empty() {
            ctx.run(cmd!(sh, "sudo mkinitcpio -P"))?;
        }
        ctx.enable_service(CLEANUP_TIMER, ServiceScope::System)?;
        let _ = ctx.start_service(CLEANUP_TIMER, ServiceScope::System)?;
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if !snapshot_folder_mounted(ctx)? {
            return ctx.missing_part(NO_SNAPSHOT_FOLDER, "the snapshots check");
        }
        verify_packages(ctx, PACKAGES)?;
        for (path, file) in files() {
            if !is_current(ctx, &path, file)? {
                return Err(format!(
                    "{} does not hold the InayOS snapshot settings",
                    path.display()
                )
                .into());
            }
        }
        let conf = read_system_file(ctx, SNAPPER_CONFIGS)?.unwrap_or_default();
        if !listed_configs(&conf).is_some_and(|names| names.contains(&CONFIG_NAME)) {
            return Err(
                format!("{SNAPPER_CONFIGS} does not list the {CONFIG_NAME} configuration").into(),
            );
        }
        let missing = images_without_overlay(ctx)?;
        if !missing.is_empty() {
            return Err(format!(
                "these initramfs images lack the sd-volatile hook, so a snapshot they start gets no overlay: {}",
                missing.join(", ")
            )
            .into());
        }
        // snap-pac stops every pacman transaction when snapper cannot use the configuration.
        if !ctx.succeeds(cmd!(
            ctx.shell,
            "sudo snapper --no-dbus -c {CONFIG_NAME} list"
        ))? {
            return Err(format!(
                "`snapper --no-dbus -c {CONFIG_NAME} list` fails, so snap-pac would stop every pacman transaction"
            )
            .into());
        }
        let _ = ctx.require_active(CLEANUP_TIMER, ServiceScope::System)?;
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_root_configuration_joins_the_listed_ones_once() {
        let shipped =
            "## Path: System/Snapper\n\n# List of snapper configurations.\nSNAPPER_CONFIGS=\"\"\n";
        let edited = with_root_config(shipped);
        assert_eq!(
            edited,
            "## Path: System/Snapper\n\n# List of snapper configurations.\nSNAPPER_CONFIGS=\"root\"\n"
        );
        assert_eq!(with_root_config(&edited), edited);
        assert_eq!(
            with_root_config("SNAPPER_CONFIGS=\"home\"\n"),
            "SNAPPER_CONFIGS=\"home root\"\n"
        );
        assert_eq!(with_root_config(""), "SNAPPER_CONFIGS=\"root\"\n");
    }
}
