//! The boot guard: the `inayos` package with its program and the pacman hook that runs its
//! checks after every update of the boot files, the system service that checks each boot,
//! and the user units that notify the desktop when a check failed. The guard writes the
//! rEFInd entries of the "Previous system" and the default entry into `inayos-guard.conf`,
//! which the refind module's managed block includes.
//!
//! The unit files are under `dotfiles/assets/boot-guard`, installed at the same path under
//! `/`. The hook is part of the package, from `packaging/inayos`, and checks the boot only
//! once this module has created the guard's folder.
use std::path::{Path, PathBuf};

use embedded_dotfiles::BOOT_GUARD_ASSETS;
use serde::Deserialize;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, InayosRepository, Module, ModuleResult, Need, Package, SecureBoot,
    ServiceScope,
    refind::{GUARD_CONFIG, refind_config},
    support::{read_system_bytes, read_system_file, verify_packages},
};

pub struct BootGuard;

impl BootGuard {
    pub const NAME: &'static str = "boot-guard";
}

/// The guard and its hook, and `notify-send`, which the notification service runs.
const PACKAGES: &[Package] = &[Package::Inayos, Package::Libnotify];
/// Where the `inayos` package puts the guard, which the hook and the services run.
pub(crate) const GUARD: &str = "/usr/bin/inayos-guard";
/// The pacman hook of the `inayos` package.
const HOOK: &str = "/usr/share/libalpm/hooks/zzz-inayos-guard.hook";
/// The guard's own folder, with the result of its last check.
const STATE_FOLDER: &str = "/var/lib/inayos-guard";
const STATUS: &str = "/var/lib/inayos-guard/status.json";
const BOOT_SERVICE: &str = "inayos-guard-boot.service";
const NOTIFY_PATH: &str = "inayos-guard-notify.path";

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

/// Fails unless the `inayos` package put the guard and its hook in place.
fn verify_guard(ctx: &Context) -> ModuleResult {
    let sh = ctx.shell;
    if !ctx.succeeds(cmd!(sh, "sudo test -x {GUARD}"))? {
        return Err(format!("{GUARD} is missing, so the inayos package is incomplete").into());
    }
    if !ctx.succeeds(cmd!(sh, "sudo test -f {HOOK}"))? {
        return Err(format!(
            "{HOOK} is missing, so the guard does not check the boot files after an update"
        )
        .into());
    }
    Ok(())
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
            Need::Hard(InayosRepository::NAME),
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
        ctx.install_packages(PACKAGES)?;
        verify_guard(ctx)?;
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
        // before the first reboot. The folder turns the package's hook on.
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
        verify_packages(ctx, PACKAGES)?;
        verify_guard(ctx)?;
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
