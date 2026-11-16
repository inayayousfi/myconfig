//! A Windows environment variable of the current user.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct UserEnvironment<'a> {
    pub name: &'a str,
}

impl Kind for UserEnvironment<'_> {
    fn describe(&self) -> String {
        format!("user environment variable {}", self.name)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let script = "[Environment]::GetEnvironmentVariable($env:MYCONFIG_NAME, 'User')";
        let pwsh = crate::support::powershell_7(ctx)?;
        let value = ctx.read(
            cmd!(ctx.shell, "{pwsh} -NoProfile -Command {script}").env("MYCONFIG_NAME", self.name),
        )?;
        Ok((!value.is_empty()).then_some(value))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let script = "[Environment]::SetEnvironmentVariable($env:MYCONFIG_NAME, $env:MYCONFIG_VALUE, 'User')";
        let pwsh = crate::support::powershell_7(ctx)?;
        ctx.run(
            cmd!(ctx.shell, "{pwsh} -NoProfile -Command {script}")
                .env("MYCONFIG_NAME", self.name)
                .env("MYCONFIG_VALUE", value),
        )
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let script = "[Environment]::SetEnvironmentVariable($env:MYCONFIG_NAME, $null, 'User')";
        let pwsh = crate::support::powershell_7(ctx)?;
        ctx.run(
            cmd!(ctx.shell, "{pwsh} -NoProfile -Command {script}").env("MYCONFIG_NAME", self.name),
        )
    }
}
