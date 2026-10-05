//! The Tailscale service. Logging in is left to the person.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope, support::verify_packages,
};

pub struct Tailscale;

impl Module for Tailscale {
    fn name(&self) -> &'static str {
        "tailscale"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Tailscale],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Tailscale])?;
        ctx.enable_service("tailscaled", ServiceScope::System)?;
        ctx.run(cmd!(ctx.shell, "sudo systemctl start tailscaled"))
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Tailscale])?;
        let status = ctx.read(cmd!(ctx.shell, "systemctl is-active tailscaled"))?;
        if status != "active" {
            return Err(format!("tailscaled is not active: {status}").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
