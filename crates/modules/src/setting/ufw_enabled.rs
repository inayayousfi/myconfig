//! Whether the ufw firewall is active.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct UfwEnabled;

impl Kind for UfwEnabled {
    fn describe(&self) -> String {
        "firewall enabled state".to_owned()
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let status = ctx.read(cmd!(ctx.shell, "sudo ufw status"))?;
        Ok(Some(
            if status.lines().next() == Some("Status: active") {
                "active"
            } else {
                "inactive"
            }
            .to_owned(),
        ))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        match value {
            "active" => ctx.run(cmd!(ctx.shell, "sudo ufw --force enable")),
            _ => self.delete(ctx),
        }
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        ctx.run(cmd!(ctx.shell, "sudo ufw disable"))
    }
}
