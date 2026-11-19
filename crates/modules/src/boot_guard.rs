//! The boot guard: its program, the pacman hook that runs its checks after every update of
//! the boot files, the system service that checks each boot, and the user units that
//! notify the desktop when a check failed. The guard writes the rEFInd entries of the
//! "Previous system" and the default entry into `inayos-guard.conf`, which the refind
//! module's managed block includes.
//!
//! The unit files and the hook are under `dotfiles/assets/boot-guard`, installed at the
//! same path under `/`.
use std::{
    fs,
    path::{Path, PathBuf},
};

use embedded_dotfiles::BOOT_GUARD_ASSETS;
use serde::Deserialize;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, SecureBoot, ServiceScope,
    refind::{GUARD_CONFIG, refind_config},
    support::{read_system_bytes, read_system_file, verify_packages},
};

pub struct BootGuard;

impl BootGuard {
    pub const NAME: &'static str = "boot-guard";
}

/// `notify-send`, which the notification service runs.
const PACKAGES: &[Package] = &[Package::Libnotify];
/// Where the pacman hook and the services run the guard. The InayOS installer puts it
/// there, next to `inayos`.
const GUARD: &str = "/usr/local/bin/inayos-guard";
/// The guard's own folder, with the result of its last check.
const STATE_FOLDER: &str = "/var/lib/inayos-guard";
const STATUS: &str = "/var/lib/inayos-guard/status.json";
const BOOT_SERVICE: &str = "inayos-guard-boot.service";
const NOTIFY_PATH: &str = "inayos-guard-notify.path";
const NO_GUARD: &str =
    "the guard program, at /usr/local/bin/inayos-guard or next to the running inayos binary,";

/// The fields of the guard's status file that the module reads.
#[derive(Deserialize)]
struct Status {
    passed: bool,
    #[serde(default)]
    failures: Vec<String>,
}

/// Each embedded file with the absolute path it is installed at.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile)> {
    BOOT_GUARD_ASSETS
        .files()
        .into_iter()
        .map(|file| (Path::new("/").join(file.path_from_root), file))
        .collect()
}

fn is_current(ctx: &Context, path: &Path, file: &EmbeddedFile) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("a boot guard path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(file.content))
}

fn guard_installed(ctx: &Context) -> ModuleResult<bool> {
    let sh = ctx.shell;
    Ok(ctx.succeeds(cmd!(sh, "sudo test -f {GUARD}"))?
        && ctx.succeeds(cmd!(sh, "sudo test -x {GUARD}"))?)
}

/// Puts the guard program in place and says whether it is there. A running system copies
/// the one next to the running `inayos` binary when the guard is not there yet.
fn place_guard(ctx: &Context) -> ModuleResult<bool> {
    if guard_installed(ctx)? {
        return Ok(true);
    }
    let beside = std::env::current_exe()?.with_file_name("inayos-guard");
    if !beside.is_file() {
        return Ok(false);
    }
    ctx.write_system_file(Path::new(GUARD), &fs::read(&beside)?, "0755")?;
    Ok(true)
}

/// The result of the guard's last check, as a failure that names what failed.
fn last_check(ctx: &Context) -> ModuleResult {
    let text = read_system_file(ctx, STATUS)?
        .ok_or_else(|| format!("{STATUS} is missing, so the guard never checked the boot"))?;
    let status: Status =
        serde_json::from_str(&text).map_err(|error| format!("could not read {STATUS}: {error}"))?;
    if !status.passed {
        return Err(format!(
            "the boot guard's last check failed: {}",
            status.failures.join("; ")
        )
        .into());
    }
    Ok(())
}

impl Module for BootGuard {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[
            Need::Hard("refind"),
            Need::Hard("snapshots"),
            Need::Soft(SecureBoot::NAME),
        ]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let Some(config) = refind_config(ctx)? else {
            return ctx.missing_part("a rEFInd installation", "the boot guard");
        };
        // Checked before anything is written: a hook without its program would fail after
        // every update of the boot files.
        if !place_guard(ctx)? {
            return ctx.missing_part(NO_GUARD, "the boot guard");
        }
        ctx.install_packages(PACKAGES)?;
        for (path, file) in files() {
            ctx.write_system_file(&path, file.content, "0644")?;
        }
        ctx.enable_service(BOOT_SERVICE, ServiceScope::System)?;
        ctx.enable_service(NOTIFY_PATH, ServiceScope::User)?;
        // Without a desktop, notify-send has nowhere to show the notification. The unit
        // starts with the next graphical session.
        if ctx.graphical_session()? {
            ctx.reload_user_manager()?;
            let _ = ctx.start_service(NOTIFY_PATH, ServiceScope::User)?;
        }
        // The first check writes the rEFInd file and the status, so the entries exist
        // before the first reboot.
        let guard_config = config
            .parent()
            .ok_or("rEFInd configuration has no parent directory")?
            .join(GUARD_CONFIG);
        ctx.record_system_path(&guard_config)?;
        if !ctx.succeeds(cmd!(sh, "sudo test -e {STATE_FOLDER}"))? {
            ctx.created(Path::new(STATE_FOLDER), true)?;
        }
        ctx.run(cmd!(sh, "sudo {GUARD} check-update"))
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let Some(config) = refind_config(ctx)? else {
            return ctx.missing_part("a rEFInd installation", "the boot guard check");
        };
        if !guard_installed(ctx)? {
            return ctx.missing_part(NO_GUARD, "the boot guard check");
        }
        verify_packages(ctx, PACKAGES)?;
        for (path, file) in files() {
            if !is_current(ctx, &path, file)? {
                return Err(format!(
                    "{} does not hold the InayOS boot guard file",
                    path.display()
                )
                .into());
            }
        }
        if !ctx
            .service_state(BOOT_SERVICE, ServiceScope::System)?
            .enabled
        {
            return Err(format!("{BOOT_SERVICE} is not enabled").into());
        }
        if !ctx.service_state(NOTIFY_PATH, ServiceScope::User)?.enabled {
            return Err(format!("{NOTIFY_PATH} is not enabled").into());
        }
        let guard_config = config
            .parent()
            .ok_or("rEFInd configuration has no parent directory")?
            .join(GUARD_CONFIG);
        if !ctx.succeeds(cmd!(sh, "sudo test -f {guard_config}"))? {
            return Err(format!("{} is missing", guard_config.display()).into());
        }
        last_check(ctx)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
