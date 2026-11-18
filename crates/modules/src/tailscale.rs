//! The Tailscale service, with the current user as its operator. Logging in is left to
//! the person.
use xshell::cmd;

use crate::{
    Applied, Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope, Setting,
    support::{current_user, verify_packages},
};

pub struct Tailscale;

const SERVICE: &str = "tailscaled";

/// Notes that `user` becomes the operator only once tailscaled runs and someone logged in
/// to Tailscale.
fn note_operator_pending(ctx: &Context, user: &str) {
    ctx.note(&format!(
        "The Tailscale operator waits for the Tailscale login. After it, make {user} the operator: sudo tailscale set --operator={user}"
    ));
}

/// Whether tailscaled has a logged-in account, as the login module checks it.
fn logged_in(ctx: &Context) -> ModuleResult<bool> {
    ctx.succeeds(cmd!(ctx.shell, "tailscale status"))
}

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
        ctx.enable_service(SERVICE, ServiceScope::System)?;
        let user = current_user(ctx)?;
        // Only the running tailscaled stores its operator.
        if ctx.start_service(SERVICE, ServiceScope::System)? == Applied::Deferred {
            note_operator_pending(ctx, &user);
            return Ok(());
        }
        // t3ctl publishes T3 Code through Tailscale Serve without sudo.
        ctx.set(Setting::TailscaleOperator, &user)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Tailscale])?;
        let user = current_user(ctx)?;
        if ctx.require_active(SERVICE, ServiceScope::System)? == Applied::Deferred {
            note_operator_pending(ctx, &user);
            return Ok(());
        }
        match Setting::TailscaleOperator.read(ctx)? {
            Some(operator) if operator == user => Ok(()),
            Some(operator) => {
                Err(format!("{operator} is the Tailscale operator instead of {user}").into())
            }
            None if !logged_in(ctx)? => {
                note_operator_pending(ctx, &user);
                Ok(())
            }
            None => Err(format!("{user} is not the Tailscale operator").into()),
        }
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
