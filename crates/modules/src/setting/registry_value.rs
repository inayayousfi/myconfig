//! A Windows registry value, recorded as `TYPE:data`.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct RegistryValue<'a> {
    pub key: &'a str,
    pub name: &'a str,
}

impl Kind for RegistryValue<'_> {
    fn describe(&self) -> String {
        format!("registry {} {}", self.key, self.name)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let Self { key, name } = *self;
        let (found, output) =
            ctx.read_unchecked(cmd!(ctx.shell, "reg.exe query {key} /v {name}"))?;
        if !found {
            return Ok(None);
        }
        Ok(Some(
            value(&output, name).ok_or("reg.exe returned no value line")?,
        ))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let Self { key, name } = *self;
        let (kind, data) = value
            .split_once(':')
            .ok_or("recorded registry value has no type")?;
        registry(
            ctx,
            key,
            &["add", key, "/v", name, "/t", kind, "/d", data, "/f"],
        )
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let Self { key, name } = *self;
        registry(ctx, key, &["delete", key, "/v", name, "/f"])
    }
}

/// The value names in `reg.exe query` output.
pub(crate) fn value_names(output: &str) -> Vec<String> {
    output
        .lines()
        .filter_map(|line| Some(line[..line.find("    REG_")?].trim().to_owned()))
        .filter(|name| !name.is_empty())
        .collect()
}

/// Finds `name` in `reg.exe query` output and returns it as `TYPE:data`.
/// Value names can contain spaces, so the line is split at its `REG_` type.
fn value(output: &str, name: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let start = line.find("    REG_")?;
        if line[..start].trim() != name {
            return None;
        }
        let rest = line[start..].trim();
        let (kind, data) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        Some(format!("{kind}:{}", data.trim()))
    })
}

/// Runs `reg.exe`, through an administrator prompt for machine-wide keys.
fn registry(ctx: &Context, key: &str, arguments: &[&str]) -> ModuleResult {
    if !key.starts_with("HKEY_LOCAL_MACHINE") && !key.starts_with("HKLM") {
        return ctx.run(ctx.shell.cmd("reg.exe").args(arguments));
    }
    let arguments: Vec<String> = arguments
        .iter()
        .map(|argument| crate::support::powershell_quote(argument))
        .collect();
    crate::support::run_elevated(
        ctx,
        &format!("& reg.exe {}; exit $LASTEXITCODE", arguments.join(" ")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_values_keep_names_with_spaces() {
        let output = "\r\nHKEY_CURRENT_USER\\Fonts\r\n    Iosevka Nerd Font (TrueType)    REG_SZ    C:\\Fonts\\Iosevka.ttf\r\n";
        assert_eq!(
            value(output, "Iosevka Nerd Font (TrueType)").as_deref(),
            Some("REG_SZ:C:\\Fonts\\Iosevka.ttf")
        );
        assert_eq!(
            value("    Hidden    REG_DWORD    0x1", "Hidden").as_deref(),
            Some("REG_DWORD:0x1")
        );
        assert_eq!(value("    Hidden    REG_DWORD    0x1", "Other"), None);
    }
}
