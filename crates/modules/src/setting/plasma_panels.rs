//! The KDE Plasma panel file, whose value is the whole file.
use std::{fs, io::ErrorKind, path::PathBuf};

use super::Kind;
use crate::{Context, ModuleResult, ServiceScope, support::config_home};

const SHELL_SERVICE: &str = "plasma-plasmashell.service";

pub(super) struct PlasmaPanels<'a> {
    pub(super) runtime: &'a str,
}

impl PlasmaPanels<'_> {
    fn path(ctx: &Context) -> PathBuf {
        config_home(ctx).join("plasma-org.kde.plasma.desktop-appletsrc")
    }

    /// Plasma keeps its panels in memory and saves them when it stops, so a running Plasma
    /// shell is stopped while the file changes. It is started again even when the change
    /// fails, so a failed change never leaves the desktop without its shell.
    fn with_shell_stopped(
        &self,
        ctx: &Context,
        change: impl FnOnce() -> ModuleResult,
    ) -> ModuleResult {
        let sh = ctx.shell;
        let runtime = self.runtime;
        let bus = sh
            .var_os("DBUS_SESSION_BUS_ADDRESS")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("unix:path={runtime}/bus").into());
        let _runtime = sh.push_env("XDG_RUNTIME_DIR", runtime);
        let _bus = sh.push_env("DBUS_SESSION_BUS_ADDRESS", bus);
        if !ctx.service_active(SHELL_SERVICE, ServiceScope::User)? {
            return change();
        }
        ctx.stop_service(SHELL_SERVICE, ServiceScope::User)?;
        let changed = change();
        let started = ctx
            .start_service(SHELL_SERVICE, ServiceScope::User)
            .map(drop);
        match (changed, started) {
            (Err(change), Err(start)) => Err(format!(
                "{change}; the Plasma shell did not start again either: {start}"
            )
            .into()),
            (changed, started) => changed.and(started),
        }
    }
}

impl Kind for PlasmaPanels<'_> {
    fn describe(&self) -> String {
        "KDE Plasma panels".to_owned()
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        match fs::read_to_string(Self::path(ctx)) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        self.with_shell_stopped(ctx, || Ok(fs::write(Self::path(ctx), value)?))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        self.with_shell_stopped(ctx, || match fs::remove_file(Self::path(ctx)) {
            Err(error) if error.kind() != ErrorKind::NotFound => Err(error.into()),
            _ => Ok(()),
        })
    }
}
