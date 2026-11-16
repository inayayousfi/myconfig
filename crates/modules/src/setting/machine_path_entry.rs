//! A folder in the Windows machine PATH.
use super::{Kind, PRESENT};
use crate::{
    Context, ModuleResult,
    support::{powershell, powershell_quote, run_elevated},
};

pub(super) struct MachinePathEntry<'a> {
    pub entry: &'a str,
}

/// Opens the stored machine PATH as `$key`, writable when `$writable` is true,
/// `$current` (its raw text, with `%NAME%` references unexpanded) and `$kind` (its
/// registry type), with `$entry` the folder. `[Environment]` would expand the references
/// and store the result as a plain string, which changes every other entry. `Test-Entry`
/// is the one rule that decides whether an item names the entry.
const OPEN: &str = "$key = [Microsoft.Win32.Registry]::LocalMachine.OpenSubKey('SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment', $writable); \
    $current = [string]$key.GetValue('Path', '', 'DoNotExpandEnvironmentNames'); \
    $kind = if ($key.GetValueNames() -contains 'Path') { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }; \
    function Test-Entry($item) { [Environment]::ExpandEnvironmentVariables($item).TrimEnd('\\') -ieq [Environment]::ExpandEnvironmentVariables($entry).TrimEnd('\\') }";

/// Stores `$new` with the type the value had, and tells running programs that the
/// environment changed, which a direct registry write does not do by itself.
const STORE: &str = "$key.SetValue('Path', $new, $kind); \
    Add-Type -Namespace MyConfig -Name Environment -MemberDefinition '[DllImport(\"user32.dll\", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);'; \
    $result = [UIntPtr]::Zero; \
    [void][MyConfig.Environment]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)";

impl Kind for MachinePathEntry<'_> {
    fn describe(&self) -> String {
        format!("machine PATH entry {}", self.entry)
    }

    fn presence(&self) -> bool {
        true
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let found = powershell(
            ctx,
            &format!(
                "$writable = $false; $entry = {}; {OPEN}; \
                 if (@($current -split ';') | Where-Object {{ Test-Entry $_ }}) {{ '{PRESENT}' }}",
                powershell_quote(self.entry)
            ),
        )?;
        Ok((found == PRESENT).then(|| PRESENT.to_owned()))
    }

    /// Appends the entry and leaves the rest of the text as it was.
    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        run_elevated(
            ctx,
            &format!(
                "$writable = $true; $entry = {}; {OPEN}; \
                 if (-not (@($current -split ';') | Where-Object {{ Test-Entry $_ }})) {{ \
                 $separator = if ($current -and -not $current.EndsWith(';')) {{ ';' }} else {{ '' }}; \
                 $new = $current + $separator + $entry; {STORE} }}",
                powershell_quote(self.entry)
            ),
        )
    }

    /// Drops every item that names the entry and keeps the other items as they were.
    fn delete(&self, ctx: &Context) -> ModuleResult {
        run_elevated(
            ctx,
            &format!(
                "$writable = $true; $entry = {}; {OPEN}; \
                 $new = (@($current -split ';') | Where-Object {{ -not (Test-Entry $_) }}) -join ';'; \
                 if ($new -cne $current) {{ {STORE} }}",
                powershell_quote(self.entry)
            ),
        )
    }
}
