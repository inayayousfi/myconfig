//! A program on the allowed-apps list of Defender's Controlled Folder Access, whose value
//! is `allowed`.
use std::path::Path;

use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult, support::powershell_7};

pub(super) struct ControlledFolderAccessApp<'a> {
    pub path: &'a str,
}

/// A PowerShell 7 command that changes the allowed-apps list with `cmdlet` in an elevated
/// pwsh.exe, because Defender preferences need administrator privileges.
fn elevated(pwsh: &Path, cmdlet: &str, app: &str) -> String {
    let quoted = |text: &str| format!("'{}'", text.replace('\'', "''"));
    let command = quoted(&format!(
        "{cmdlet} -ControlledFolderAccessAllowedApplications {} -ErrorAction Stop",
        quoted(app)
    ));
    let pwsh = quoted(&pwsh.display().to_string());
    format!(
        "$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes({command})); $process = Start-Process -FilePath {pwsh} -Verb RunAs -ArgumentList '-NoLogo', '-NoProfile', '-EncodedCommand', $encoded -Wait -PassThru -ErrorAction Stop; exit $process.ExitCode"
    )
}

fn change(ctx: &Context, cmdlet: &str, app: &str) -> ModuleResult {
    let pwsh = powershell_7(ctx)?;
    let command = elevated(&pwsh, cmdlet, app);
    ctx.run(cmd!(ctx.shell, "{pwsh} -NoProfile -Command {command}"))
}

impl Kind for ControlledFolderAccessApp<'_> {
    fn describe(&self) -> String {
        format!("Controlled Folder Access allowed app {}", self.path)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let pwsh = powershell_7(ctx)?;
        let check = "$p = Get-MpPreference -ErrorAction SilentlyContinue; if ($p -and $p.ControlledFolderAccessAllowedApplications -contains $env:MYCONFIG_APP) { 'allowed' } else { 'absent' }";
        let state = ctx.read(
            cmd!(ctx.shell, "{pwsh} -NoProfile -Command {check}").env("MYCONFIG_APP", self.path),
        )?;
        Ok((state == "allowed").then_some(state))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        change(ctx, "Add-MpPreference", self.path)
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        change(ctx, "Remove-MpPreference", self.path)
    }
}
