//! Handy offline push-to-talk dictation as a user service.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope,
    deploy::verify_package,
    support::{
        active_group, configure_input_access, graphical_session, input_access_settings,
        require_executable, require_programs, verify_input_access, verify_packages,
    },
};

pub struct Handy;

const SERVICE: &str = "myconfig-handy.service";

impl Module for Handy {
    fn name(&self) -> &'static str {
        "handy"
    }

    fn footprint(&self, ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Handy, Package::Jq],
            settings: input_access_settings(ctx),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.install_packages(&[Package::Handy])?;
        require_programs(ctx, &["handy", "jq", "systemctl"])?;
        ctx.deploy_config("handy")?;
        let configure = ctx.home.join(".local/bin/myconfig-handy-configure");
        require_executable(&configure)?;
        configure_input_access(ctx, "handy")?;
        ctx.run(cmd!(sh, "systemctl --user daemon-reload"))?;
        ctx.run(cmd!(sh, "systemctl --user stop {SERVICE}"))?;
        let settings = ctx
            .shell
            .var_os("XDG_CONFIG_HOME")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.home.join(".config"))
            .join("com.pais.handy/settings_store.json");
        ctx.record_path(&settings)?;
        ctx.run(cmd!(sh, "{configure}"))?;
        ctx.enable_service(SERVICE, ServiceScope::User)?;
        if !active_group(ctx, "input")? || !active_group(ctx, "uinput")? {
            ctx.note("Log out and back in before Handy can read keyboard input");
        } else if graphical_session(ctx)? {
            ctx.run(cmd!(sh, "systemctl --user restart {SERVICE}"))?;
            ctx.run(cmd!(sh, "systemctl --user --quiet is-active {SERVICE}"))?;
        } else {
            ctx.note("Handy is enabled for the next KDE Plasma login");
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Handy])?;
        verify_package(ctx, "handy")?;
        require_executable(&ctx.home.join(".local/bin/myconfig-handy-configure"))?;
        verify_input_access(ctx, "handy")?;
        ctx.run(cmd!(
            ctx.shell,
            "systemctl --user --quiet is-enabled {SERVICE}"
        ))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
