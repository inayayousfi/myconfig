//! A program on the allowed-apps list of Defender's Controlled Folder Access.
use xshell::cmd;

use super::{Kind, PRESENT};
use crate::{
    Context, ModuleResult,
    support::{powershell_7, powershell_quote, run_elevated},
};

pub(super) struct ControlledFolderAccessApp<'a> {
    pub path: &'a str,
}

/// Changes the allowed-apps list with `cmdlet`, elevated, because Defender preferences
/// need administrator rights.
fn change(ctx: &Context, cmdlet: &str, app: &str) -> ModuleResult {
    run_elevated(
        ctx,
        &format!(
            "{cmdlet} -ControlledFolderAccessAllowedApplications {}",
            powershell_quote(app)
        ),
    )
}

impl Kind for ControlledFolderAccessApp<'_> {
    fn describe(&self) -> String {
        format!("Controlled Folder Access allowed app {}", self.path)
    }

    fn presence(&self) -> bool {
        true
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let pwsh = powershell_7(ctx)?;
        let check = "$p = Get-MpPreference -ErrorAction SilentlyContinue; if ($p -and $p.ControlledFolderAccessAllowedApplications -contains $env:MYCONFIG_APP) { 'allowed' } else { 'absent' }";
        let state = ctx.read(
            cmd!(ctx.shell, "{pwsh} -NoProfile -Command {check}").env("MYCONFIG_APP", self.path),
        )?;
        Ok((state == "allowed").then(|| PRESENT.to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        change(ctx, "Add-MpPreference", self.path)
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        change(ctx, "Remove-MpPreference", self.path)
    }
}
