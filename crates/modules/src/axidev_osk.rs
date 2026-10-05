//! Axidev OSK, an on-screen keyboard for the desktop and the login screen.
//!
//! Axidev ships its own lifecycle installer, so `remove` calls its `uninstall`.
use std::path::Path;

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package,
    support::{
        current_user, input_access_settings, require_executable, require_programs, verify_packages,
        with_temporary_directory,
    },
};

pub struct AxidevOsk;

const PACKAGES: &[Package] = &[
    Package::Python,
    Package::Pyside6,
    Package::Qt6Wayland,
    Package::LayerShellQt,
    Package::Libinput,
    Package::Systemd,
    Package::Libxkbcommon,
];
const LIFECYCLE: &str = "/usr/local/sbin/axidev-osk-install";
const APP: &str = "/usr/local/bin/axidev-osk";
const INSTALLER_URL: &str =
    "https://github.com/axide-dev/axidev-osk/releases/latest/download/axidev-osk-install";

/// Installs or upgrades Axidev through its lifecycle installer at `lifecycle`, then sets up
/// the application at `app` for the desktop and the login screen.
pub(crate) fn configure(ctx: &Context, lifecycle: &Path, app: &Path) -> ModuleResult {
    let sh = ctx.shell;
    ctx.install_packages(PACKAGES)?;
    require_programs(ctx, &["curl", "sudo"])?;
    let user = current_user(ctx)?;
    if require_executable(lifecycle).is_ok() {
        ctx.run(cmd!(sh, "sudo {lifecycle} upgrade --user {user}"))?;
    } else {
        with_temporary_directory("axidev", |directory| {
            let installer = directory.join("axidev-osk-install");
            ctx.run(cmd!(
                sh,
                "curl --fail --location --show-error --silent --output {installer} {INSTALLER_URL}"
            ))?;
            crate::state::set_executable(&installer, true)?;
            ctx.run(cmd!(sh, "sudo {installer} install --user {user}"))
        })?;
    }
    require_executable(app)?;
    ctx.run(cmd!(sh, "sudo {app} linux setup-permissions --user {user}"))?;
    ctx.run(cmd!(sh, "{app} linux setup-autostart --user {user}"))?;
    // The greeter setup asks its own questions on the terminal.
    ctx.with_terminal(cmd!(sh, "sudo {app} linux setup-greeter").into())
}

impl Module for AxidevOsk {
    fn name(&self) -> &'static str {
        "axidev-osk"
    }

    fn footprint(&self, ctx: &Context) -> Footprint {
        // Axidev's own permission setup puts the user in the uinput group too.
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: input_access_settings(ctx),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        // The login-screen setup asks its own questions, so fail before changing anything.
        if !ctx.has_terminal() {
            return Err("Axidev OSK greeter setup requires a terminal".into());
        }
        configure(ctx, Path::new(LIFECYCLE), Path::new(APP))
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        require_executable(Path::new(APP))?;
        let user = current_user(ctx)?;
        ctx.run(cmd!(
            sh,
            "sudo {APP} linux status-permissions --user {user}"
        ))?;
        ctx.run(cmd!(sh, "{APP} linux status-autostart --user {user}"))?;
        ctx.run(cmd!(sh, "sudo {APP} linux status-greeter"))
    }

    fn remove(&self, ctx: &Context) -> ModuleResult {
        if require_executable(Path::new(LIFECYCLE)).is_err() {
            return Ok(());
        }
        let user = current_user(ctx)?;
        ctx.run(cmd!(ctx.shell, "sudo {LIFECYCLE} uninstall --user {user}"))
    }
}
