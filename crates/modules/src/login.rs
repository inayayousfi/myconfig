//! Offers the GitHub CLI and Tailscale logins that are missing. A login belongs to the
//! person, so nothing is recorded and `remove` leaves every login in place.
use xshell::Cmd;

use crate::{Context, Footprint, Module, ModuleResult, Need, Package};

pub struct Login;

/// One service to log in to, with each command as a program and its arguments.
struct Service {
    /// Says that the service is logged out, before the question or the command.
    state: &'static str,
    /// Succeeds when the person is logged in.
    status: &'static [&'static str],
    log_in: &'static [&'static str],
}

const SERVICES: [Service; 2] = [
    Service {
        state: "GitHub CLI is not authenticated.",
        status: &["gh", "auth", "status"],
        log_in: &["gh", "auth", "login"],
    },
    Service {
        state: "Tailscale is not authenticated.",
        status: &["tailscale", "status"],
        log_in: &["sudo", "tailscale", "up"],
    },
];

fn command<'a>(ctx: &Context<'a>, words: &[&str]) -> Cmd<'a> {
    ctx.shell.cmd(words[0]).args(&words[1..])
}

impl Module for Login {
    fn name(&self) -> &'static str {
        "login"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("cli"), Need::Hard("tailscale")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::GithubCli, Package::Tailscale],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        for service in &SERVICES {
            if ctx.succeeds(command(ctx, service.status))? {
                continue;
            }
            if ctx.confirm_optional(&format!("{} Log in now?", service.state)) {
                ctx.with_terminal(command(ctx, service.log_in).into())?;
            } else {
                ctx.note(&format!(
                    "{} Run: {}",
                    service.state,
                    service.log_in.join(" ")
                ));
            }
        }
        Ok(())
    }

    fn verify(&self, _ctx: &Context) -> ModuleResult {
        // Staying logged out is the person's choice.
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
