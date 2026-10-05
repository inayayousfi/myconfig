//! The KDE tray that selects Kanata profiles, and the Overview shortcut it frees.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope, Setting,
    deploy::verify_package,
    plasma_version::require_supported_plasma,
    support::{
        active_group, graphical_session, require_executable, require_programs, verify_packages,
    },
};

pub struct KanataKde;

const PACKAGES: &[Package] = &[Package::Python, Package::Pyside6];
const SERVICE: &str = "myconfig-kanata-tray.service";
const OVERVIEW: &str = "Meta+W,Meta+W,Toggle Overview";

fn overview_shortcut() -> Setting {
    Setting::kde("kglobalshortcutsrc", &["kwin"], "Overview")
}

impl Module for KanataKde {
    fn name(&self) -> &'static str {
        "kanata-kde"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            // The tray drives the Kanata module's program.
            packages: [PACKAGES, &[Package::Kanata]].concat(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        require_supported_plasma(ctx)?;
        ctx.install_packages(PACKAGES)?;
        require_programs(ctx, &["kwriteconfig6", "python", "systemctl"])?;
        ctx.deploy_config("kanata-kde")?;
        let tray = ctx.home.join(".local/bin/myconfig-kanata-tray");
        require_executable(&tray)?;
        ctx.run(cmd!(sh, "python -m py_compile {tray}"))?;
        ctx.set(overview_shortcut(), OVERVIEW)?;
        // The tray starts Kanata itself. Earlier installs enabled Kanata on its own; undo that.
        ctx.delete(
            &ctx.home
                .join(".config/systemd/user/default.target.wants/myconfig-kanata.service"),
        )?;
        ctx.enable_service(SERVICE, ServiceScope::User)?;
        ctx.run(cmd!(sh, "systemctl --user daemon-reload"))?;
        if !active_group(ctx, "input")? || !active_group(ctx, "uinput")? {
            ctx.run(cmd!(sh, "systemctl --user stop myconfig-kanata.service"))?;
            ctx.note("Log out and back in before Kanata and its KDE tray start together");
        } else if graphical_session(ctx)? {
            ctx.run(cmd!(sh, "systemctl --user restart {SERVICE}"))?;
            ctx.run(cmd!(sh, "systemctl --user --quiet is-active {SERVICE}"))?;
        } else {
            ctx.run(cmd!(sh, "systemctl --user stop myconfig-kanata.service"))?;
            ctx.note("Kanata and its KDE tray are enabled for the next Plasma login");
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "kanata-kde")?;
        let tray = ctx.home.join(".local/bin/myconfig-kanata-tray");
        require_executable(&tray)?;
        ctx.run(cmd!(sh, "python -m py_compile {tray}"))?;
        if overview_shortcut().read(ctx)?.as_deref() != Some(OVERVIEW) {
            return Err("the KDE Overview shortcut is not Meta+W".into());
        }
        ctx.run(cmd!(sh, "systemctl --user --quiet is-enabled {SERVICE}"))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
