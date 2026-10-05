//! The PipeWire audio tray as a user service.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope,
    deploy::verify_package,
    support::{graphical_session, require_executable, require_programs, verify_packages},
};

pub struct Pipewire;

const PACKAGES: &[Package] = &[Package::Python, Package::Pyside6];
const SERVICE: &str = "myconfig-pipewire-tray.service";

impl Module for Pipewire {
    fn name(&self) -> &'static str {
        "pipewire"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.install_packages(PACKAGES)?;
        require_programs(ctx, &["python", "systemctl", "pactl"])?;
        ctx.deploy_config("pipewire")?;
        let tray = ctx.home.join(".local/bin/myconfig-pipewire-tray");
        require_executable(&tray)?;
        ctx.run(cmd!(sh, "python -m py_compile {tray}"))?;
        ctx.enable_service(SERVICE, ServiceScope::User)?;
        ctx.run(cmd!(sh, "systemctl --user daemon-reload"))?;
        if graphical_session(ctx)? {
            ctx.run(cmd!(sh, "systemctl --user restart {SERVICE}"))?;
            ctx.run(cmd!(sh, "systemctl --user --quiet is-active {SERVICE}"))
        } else {
            ctx.run(cmd!(sh, "systemctl --user stop {SERVICE}"))
        }
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "pipewire")?;
        let tray = ctx.home.join(".local/bin/myconfig-pipewire-tray");
        require_executable(&tray)?;
        ctx.run(cmd!(sh, "python -m py_compile {tray}"))?;
        ctx.run(cmd!(sh, "systemctl --user --quiet is-enabled {SERVICE}"))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
