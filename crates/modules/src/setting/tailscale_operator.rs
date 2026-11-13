//! The user allowed to change Tailscale without sudo.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct TailscaleOperator;

impl Kind for TailscaleOperator {
    fn describe(&self) -> String {
        "Tailscale operator".to_owned()
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let prefs: serde_json::Value =
            serde_json::from_str(&ctx.read(cmd!(ctx.shell, "tailscale debug prefs"))?)?;
        Ok(prefs
            .get("OperatorUser")
            .and_then(serde_json::Value::as_str)
            .filter(|user| !user.is_empty())
            .map(str::to_owned))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let operator = format!("--operator={value}");
        ctx.run(cmd!(ctx.shell, "sudo tailscale set {operator}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        ctx.run(cmd!(ctx.shell, "sudo tailscale set --operator="))
    }
}
