//! A folder in the Windows machine PATH.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct MachinePathEntry<'a> {
    pub entry: &'a str,
}

impl Kind for MachinePathEntry<'_> {
    fn describe(&self) -> String {
        format!("machine PATH entry {}", self.entry)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let pwsh = crate::support::powershell_7(ctx)?;
        let read_path = "[Environment]::GetEnvironmentVariable('Path', 'Machine')";
        let path = ctx.read(cmd!(ctx.shell, "{pwsh} -NoProfile -Command {read_path}"))?;
        Ok(path
            .split(';')
            .any(|item| same_path(item, self.entry))
            .then(|| "present".to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let quoted = self.entry.replace('\'', "''");
        set_machine_path(
            ctx,
            &format!(
                "$entry = '{quoted}'; $current = [Environment]::GetEnvironmentVariable('Path', 'Machine'); \
                 $entries = @($current -split ';' | Where-Object {{ $_ }}); \
                 if (-not ($entries | Where-Object {{ $_.TrimEnd('\\') -ieq $entry.TrimEnd('\\') }})) {{ \
                 [Environment]::SetEnvironmentVariable('Path', (($entries + $entry) -join ';'), 'Machine') }}"
            ),
        )
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let quoted = self.entry.replace('\'', "''");
        set_machine_path(
            ctx,
            &format!(
                "$entry = '{quoted}'; $current = [Environment]::GetEnvironmentVariable('Path', 'Machine'); \
                 $entries = @($current -split ';' | Where-Object {{ $_ -and $_.TrimEnd('\\') -ine $entry.TrimEnd('\\') }}); \
                 [Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'Machine')"
            ),
        )
    }
}

fn same_path(left: &str, right: &str) -> bool {
    left.trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(right.trim_end_matches(['\\', '/']))
}

/// Runs a script that changes the machine PATH in an elevated PowerShell.
fn set_machine_path(ctx: &Context, script: &str) -> ModuleResult {
    let pwsh = crate::support::powershell_7(ctx)?;
    let elevate = "$encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($env:MYCONFIG_PATH_SCRIPT)); $process = Start-Process -FilePath $env:MYCONFIG_PWSH -Verb RunAs -ArgumentList '-NoProfile', '-EncodedCommand', $encoded -Wait -PassThru -ErrorAction Stop; if ($process.ExitCode -ne 0) { throw 'Elevated PATH update failed' }";
    ctx.run(
        cmd!(ctx.shell, "{pwsh} -NoProfile -Command {elevate}")
            .env("MYCONFIG_PATH_SCRIPT", script)
            .env("MYCONFIG_PWSH", &pwsh),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_entries_compare_without_case_or_trailing_separator() {
        assert!(same_path(
            "C:\\Program Files\\LLVM\\bin\\",
            "c:\\program files\\llvm\\bin"
        ));
        assert!(!same_path("C:\\LLVM", "C:\\LLVM\\bin"));
    }
}
