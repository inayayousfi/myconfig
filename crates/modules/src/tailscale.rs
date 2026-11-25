//! The Tailscale service, with the current user as its operator. Logging in is left to
//! the person.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope, Setting,
    support::{current_user, verify_packages},
};

pub struct Tailscale;

impl Module for Tailscale {
    fn name(&self) -> &'static str {
        "tailscale"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Tailscale],
            settings: vec![Setting::TailscaleOperator],
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Tailscale])?;
        ctx.enable_service("tailscaled", ServiceScope::System)?;
        ctx.run(cmd!(ctx.shell, "sudo systemctl start tailscaled"))?;
        // The user runs tailscale commands, such as Serve, without sudo.
        ctx.set(Setting::TailscaleOperator, &current_user(ctx)?)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Tailscale])?;
        let status = ctx.read(cmd!(ctx.shell, "systemctl is-active tailscaled"))?;
        if status != "active" {
            return Err(format!("tailscaled is not active: {status}").into());
        }
        let user = current_user(ctx)?;
        if Setting::TailscaleOperator.read(ctx)?.as_deref() != Some(user.as_str()) {
            return Err(format!("{user} is not the Tailscale operator").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
