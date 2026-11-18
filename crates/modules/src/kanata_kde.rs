//! The KDE tray that selects Kanata profiles, and the Overview shortcut it frees.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope, Setting,
    deploy::verify_package,
    plasma_version::require_supported_plasma,
    support::{require_executable, require_programs, verify_packages},
};

pub struct KanataKde;

const PACKAGES: &[Package] = &[Package::Python, Package::Pyside6];
const SERVICE: &str = "inayos-kanata-tray.service";
/// The tray starts Kanata's own service.
const KANATA_SERVICE: &str = "inayos-kanata.service";
const OVERVIEW: &str = "Meta+W,Meta+W,Toggle Overview";

fn overview_shortcut() -> Setting {
    Setting::kde("kglobalshortcutsrc", &["kwin"], "Overview")
}

impl Module for KanataKde {
    fn name(&self) -> &'static str {
        "kanata-kde"
    }

    fn needs(&self) -> &'static [Need] {
        &[
            Need::Hard("base"),
            Need::Hard("plasma-version"),
            Need::Hard("kanata"),
        ]
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
        let tray = ctx.home.join(".local/bin/inayos-kanata-tray");
        require_executable(&tray)?;
        ctx.run(cmd!(sh, "python -m py_compile {tray}"))?;
        ctx.set(overview_shortcut(), OVERVIEW)?;
        ctx.enable_service(SERVICE, ServiceScope::User)?;
        ctx.reload_user_manager()?;
        if !ctx.user_manager_has_group("input")? || !ctx.user_manager_has_group("uinput")? {
            ctx.stop_service(KANATA_SERVICE, ServiceScope::User)?;
            ctx.note("Log out and back in before Kanata and its KDE tray start together");
        } else if ctx.graphical_session()? {
            let _ = ctx.restart_service(SERVICE, ServiceScope::User)?;
        } else {
            ctx.stop_service(KANATA_SERVICE, ServiceScope::User)?;
            ctx.note("Kanata and its KDE tray are enabled for the next Plasma login");
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "kanata-kde")?;
        let tray = ctx.home.join(".local/bin/inayos-kanata-tray");
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
