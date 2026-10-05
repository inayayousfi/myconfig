//! Kanata keyboard remapping as a user service.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope,
    deploy::verify_package,
    support::{
        active_group, configure_input_access, input_access_settings, require_file,
        require_programs, verify_input_access, verify_packages,
    },
};

pub struct Kanata {
    /// Starts Kanata at login. CachyOS turns this off because the Kanata KDE tray starts it.
    pub start_at_login: bool,
}

const SERVICE: &str = "myconfig-kanata.service";

impl Module for Kanata {
    fn name(&self) -> &'static str {
        "kanata"
    }

    fn footprint(&self, ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Kanata],
            settings: input_access_settings(ctx),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.install_packages(&[Package::Kanata])?;
        require_programs(ctx, &["kanata", "systemctl"])?;
        ctx.deploy_config("kanata")?;
        let config = ctx.home.join(".config/kanata/config.kbd");
        ctx.run(cmd!(sh, "kanata --check --cfg {config}"))?;
        configure_input_access(ctx, "kanata")?;
        ctx.run(cmd!(sh, "systemctl --user daemon-reload"))?;
        if !self.start_at_login {
            return Ok(());
        }
        ctx.enable_service(SERVICE, ServiceScope::User)?;
        if active_group(ctx, "input")? && active_group(ctx, "uinput")? {
            ctx.run(cmd!(sh, "systemctl --user restart {SERVICE}"))?;
            ctx.run(cmd!(sh, "systemctl --user --quiet is-active {SERVICE}"))?;
        } else {
            ctx.note("Log out and back in before Kanata can access input devices");
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, &[Package::Kanata])?;
        verify_package(ctx, "kanata")?;
        let config = ctx.home.join(".config/kanata/config.kbd");
        require_file(&config)?;
        ctx.run(cmd!(sh, "kanata --check --cfg {config}"))?;
        verify_input_access(ctx, "kanata")?;
        if self.start_at_login {
            ctx.run(cmd!(sh, "systemctl --user --quiet is-enabled {SERVICE}"))?;
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
