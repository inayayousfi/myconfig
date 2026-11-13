//! T3 Code: the server with its background service, or the desktop app that runs its own
//! server.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    deploy::verify_package,
    support::{require_executable, require_programs, verify_packages},
};

/// The server from npm, run at boot by the user service `t3code.service`. The deployed
/// `t3ctl` owns the install and the service, so a later `t3ctl repair` does exactly what
/// this module did.
pub struct T3Code;

/// The desktop app, which replaces the background service instead of running beside it.
pub struct T3CodeDesktop;

/// t3ctl draws pairing links as QR codes.
const SERVER_PACKAGES: &[Package] = &[Package::Qrencode];
const DESKTOP_PACKAGES: &[Package] = &[Package::T3codeDesktop];

fn service_active(ctx: &Context) -> ModuleResult {
    if !ctx.succeeds(cmd!(
        ctx.shell,
        "systemctl --user --quiet is-active t3code.service"
    ))? {
        return Err("t3code.service is not active".into());
    }
    Ok(())
}

impl Module for T3Code {
    fn name(&self) -> &'static str {
        "t3code"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("runtimes")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: SERVER_PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        require_programs(ctx, &["npm", "systemctl"])?;
        ctx.install_packages(SERVER_PACKAGES)?;
        ctx.deploy_config("t3code")?;
        let t3ctl = ctx.home.join(".local/bin/t3ctl");
        require_executable(&t3ctl)?;
        for path in [".local/bin/t3", ".local/lib/node_modules/t3"] {
            let path = ctx.home.join(path);
            if !path.exists() {
                ctx.created(&path, false)?;
            }
        }
        let (_, service) = ctx.read_unchecked(cmd!(
            ctx.shell,
            "systemctl --user is-enabled t3code.service"
        ))?;
        if service != "enabled" {
            // t3ctl installs the service with the t3 it installed from npm into ~/.local.
            ctx.record_undo_command(&ctx.home.join(".local/bin/t3"), &["service", "uninstall"])?;
        }
        ctx.run(cmd!(ctx.shell, "{t3ctl} repair"))
            .map_err(|error| format!("t3ctl could not install the T3 Code server: {error}"))?;
        service_active(ctx)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, SERVER_PACKAGES)?;
        verify_package(ctx, "t3code")?;
        service_active(ctx)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

impl Module for T3CodeDesktop {
    fn name(&self) -> &'static str {
        "t3code-desktop"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: DESKTOP_PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(DESKTOP_PACKAGES)?;
        ctx.deploy_config("t3code-desktop")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, DESKTOP_PACKAGES)?;
        verify_package(ctx, "t3code-desktop")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
