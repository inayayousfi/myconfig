//! Values outside a module's own files that `remove` must put back.
use serde::{Deserialize, Serialize};
use xshell::cmd;

use crate::{Context, ModuleResult};

const KDE_MISSING: &str = "__myconfig_missing__";

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Setting {
    KdeKey {
        file: String,
        groups: Vec<String>,
        key: String,
    },
    /// The applied KDE Plasma global theme.
    LookAndFeel,
    /// The applied KDE cursor theme.
    CursorTheme,
    Gsettings {
        schema: String,
        key: String,
    },
    GitConfig {
        key: String,
    },
    LoginShell {
        user: String,
    },
    RustupDefault,
    /// A `ufw` rule, given as the arguments that add it.
    UfwRule {
        rule: Vec<String>,
    },
    UfwEnabled,
    GroupExists {
        group: String,
    },
    GroupMember {
        group: String,
        user: String,
    },
    ClaudeMcpServer {
        name: String,
    },
    RegistryValue {
        key: String,
        name: String,
    },
    MachinePathEntry {
        entry: String,
    },
    TaskbarAutoHide,
}

impl Setting {
    pub(crate) fn registry_value_names(output: &str) -> Vec<String> {
        output
            .lines()
            .filter_map(|line| Some(line[..line.find("    REG_")?].trim().to_owned()))
            .filter(|name| !name.is_empty())
            .collect()
    }

    pub fn kde(file: &str, groups: &[&str], key: &str) -> Self {
        Self::KdeKey {
            file: file.to_owned(),
            groups: groups.iter().map(|group| (*group).to_owned()).collect(),
            key: key.to_owned(),
        }
    }

    /// Settings applied by a program that also updates the running session, so they
    /// run again even when the stored value already matches.
    pub(crate) fn reapplies(&self) -> bool {
        matches!(self, Self::LookAndFeel | Self::CursorTheme)
    }

    pub fn describe(&self) -> String {
        match self {
            Self::KdeKey { file, groups, key } => format!("{file} [{}] {key}", groups.join("][")),
            Self::LookAndFeel => "KDE global theme".to_owned(),
            Self::CursorTheme => "KDE cursor theme".to_owned(),
            Self::Gsettings { schema, key } => format!("gsettings {schema} {key}"),
            Self::GitConfig { key } => format!("git config --global {key}"),
            Self::LoginShell { user } => format!("login shell of {user}"),
            Self::RustupDefault => "default Rust toolchain".to_owned(),
            Self::UfwRule { rule } => format!("firewall rule: ufw {}", rule.join(" ")),
            Self::UfwEnabled => "firewall enabled state".to_owned(),
            Self::GroupExists { group } => format!("group {group}"),
            Self::GroupMember { group, user } => format!("{user} in group {group}"),
            Self::ClaudeMcpServer { name } => format!("Claude MCP server {name}"),
            Self::RegistryValue { key, name } => format!("registry {key} {name}"),
            Self::MachinePathEntry { entry } => format!("machine PATH entry {entry}"),
            Self::TaskbarAutoHide => "taskbar auto-hide".to_owned(),
        }
    }

    /// The current value, or `None` when the setting does not exist.
    pub fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let sh = ctx.shell;
        Ok(match self {
            Self::KdeKey { file, groups, key } => {
                let groups = group_arguments(groups);
                let value = ctx.read(cmd!(
                    sh,
                    "kreadconfig6 --file {file} {groups...} --key {key} --default {KDE_MISSING}"
                ))?;
                (value != KDE_MISSING).then_some(value)
            }
            Self::LookAndFeel => {
                Self::kde("kdeglobals", &["KDE"], "LookAndFeelPackage").read(ctx)?
            }
            Self::CursorTheme => Self::kde("kcminputrc", &["Mouse"], "cursorTheme").read(ctx)?,
            Self::Gsettings { schema, key } => {
                Some(ctx.read(cmd!(sh, "gsettings get {schema} {key}"))?)
            }
            Self::GitConfig { key } => {
                let (found, value) =
                    ctx.read_unchecked(cmd!(sh, "git config --global --get {key}"))?;
                found.then_some(value)
            }
            Self::LoginShell { user } => {
                let account = ctx.read(cmd!(sh, "getent passwd {user}"))?;
                Some(
                    account
                        .rsplit(':')
                        .next()
                        .ok_or("getent returned no shell")?
                        .to_owned(),
                )
            }
            Self::RustupDefault => {
                let (configured, output) = ctx.read_unchecked(cmd!(sh, "rustup default"))?;
                output
                    .split_whitespace()
                    .next()
                    .filter(|_| configured && !output.contains("no default"))
                    .map(str::to_owned)
            }
            Self::UfwRule { rule } => {
                let added = ctx.read(cmd!(sh, "sudo ufw show added"))?;
                added
                    .lines()
                    .any(|line| ufw_rule_matches(rule, line))
                    .then(|| "present".to_owned())
            }
            Self::UfwEnabled => {
                let status = ctx.read(cmd!(sh, "sudo ufw status"))?;
                Some(
                    if status.lines().next() == Some("Status: active") {
                        "active"
                    } else {
                        "inactive"
                    }
                    .to_owned(),
                )
            }
            Self::GroupExists { group } => ctx
                .succeeds(cmd!(sh, "getent group {group}"))?
                .then(|| "present".to_owned()),
            Self::GroupMember { group, user } => {
                let groups = ctx.read(cmd!(sh, "id -nG {user}"))?;
                groups
                    .split_whitespace()
                    .any(|name| name == group)
                    .then(|| "member".to_owned())
            }
            Self::ClaudeMcpServer { name } => {
                let path = ctx.home.join(".claude.json");
                match std::fs::read(&path) {
                    Ok(contents) => {
                        let json: serde_json::Value = serde_json::from_slice(&contents)?;
                        json.get("mcpServers")
                            .and_then(|servers| servers.get(name))
                            .map(|server| server.to_string())
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error.into()),
                }
            }
            Self::RegistryValue { key, name } => {
                let (found, output) =
                    ctx.read_unchecked(cmd!(sh, "reg.exe query {key} /v {name}"))?;
                if !found {
                    None
                } else {
                    Some(registry_value(&output, name).ok_or("reg.exe returned no value line")?)
                }
            }
            Self::MachinePathEntry { entry } => {
                let path = machine_path(ctx)?;
                path.split(';')
                    .any(|item| same_path(item, entry))
                    .then(|| "present".to_owned())
            }
            Self::TaskbarAutoHide => Some(
                if crate::taskbar_auto_hide::enabled()? {
                    "on"
                } else {
                    "off"
                }
                .to_owned(),
            ),
        })
    }

    pub fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let sh = ctx.shell;
        match self {
            Self::KdeKey { file, groups, key } => {
                let groups = group_arguments(groups);
                ctx.run(cmd!(
                    sh,
                    "kwriteconfig6 --file {file} {groups...} --key {key} {value}"
                ))
            }
            Self::LookAndFeel => ctx.run(
                cmd!(sh, "plasma-apply-lookandfeel --apply {value}")
                    .env("QT_QPA_PLATFORM", "offscreen"),
            ),
            Self::CursorTheme => {
                let size = Self::kde("kcminputrc", &["Mouse"], "cursorSize")
                    .read(ctx)?
                    .unwrap_or_else(|| "24".to_owned());
                ctx.run(
                    cmd!(sh, "plasma-apply-cursortheme --size {size} {value}")
                        .env("QT_QPA_PLATFORM", "offscreen"),
                )
            }
            Self::Gsettings { schema, key } => {
                ctx.run(cmd!(sh, "gsettings set {schema} {key} {value}"))
            }
            Self::GitConfig { key } => ctx.run(cmd!(sh, "git config --global {key} {value}")),
            Self::LoginShell { user } => {
                ctx.run(cmd!(sh, "sudo chsh -s {value} {user}"))?;
                if self.read(ctx)?.as_deref() != Some(value) {
                    return Err(format!("could not verify the login shell for {user}").into());
                }
                Ok(())
            }
            Self::RustupDefault => ctx.run(cmd!(sh, "rustup default {value}")),
            Self::UfwRule { rule } => ctx.run(cmd!(sh, "sudo ufw {rule...}")),
            Self::UfwEnabled => match value {
                "active" => ctx.run(cmd!(sh, "sudo ufw --force enable")),
                _ => ctx.run(cmd!(sh, "sudo ufw disable")),
            },
            Self::GroupExists { group } => {
                ctx.run(cmd!(sh, "sudo groupadd --system --force {group}"))
            }
            Self::GroupMember { group, user } => {
                ctx.run(cmd!(sh, "sudo usermod -aG {group} {user}"))
            }
            Self::ClaudeMcpServer { name } => {
                if self.read(ctx)?.is_some() {
                    ctx.run(cmd!(sh, "claude mcp remove --scope user {name}"))?;
                }
                ctx.run(cmd!(sh, "claude mcp add-json --scope user {name} {value}"))
            }
            Self::RegistryValue { key, name } => {
                let (kind, data) = value
                    .split_once(':')
                    .ok_or("recorded registry value has no type")?;
                registry(
                    ctx,
                    key,
                    &["add", key, "/v", name, "/t", kind, "/d", data, "/f"],
                )
            }
            Self::MachinePathEntry { entry } => {
                let quoted = entry.replace('\'', "''");
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
            Self::TaskbarAutoHide => crate::taskbar_auto_hide::set(value == "on"),
        }
    }

    pub fn delete(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        match self {
            Self::KdeKey { file, groups, key } => {
                let groups = group_arguments(groups);
                ctx.run(cmd!(
                    sh,
                    "kwriteconfig6 --file {file} {groups...} --key {key} --delete"
                ))
            }
            Self::LookAndFeel => self.write(ctx, "org.kde.breeze.desktop"),
            Self::CursorTheme => self.write(ctx, "breeze_cursors"),
            Self::Gsettings { schema, key } => ctx.run(cmd!(sh, "gsettings reset {schema} {key}")),
            Self::GitConfig { key } => ctx.run(cmd!(sh, "git config --global --unset {key}")),
            Self::LoginShell { user } => {
                Err(format!("the login shell of {user} cannot be deleted").into())
            }
            Self::RustupDefault => ctx.run(cmd!(sh, "rustup default none")),
            Self::UfwRule { rule } => ctx.run(cmd!(sh, "sudo ufw delete {rule...}")),
            Self::UfwEnabled => ctx.run(cmd!(sh, "sudo ufw disable")),
            Self::GroupExists { group } => ctx.run(cmd!(sh, "sudo groupdel {group}")),
            Self::GroupMember { group, user } => {
                ctx.run(cmd!(sh, "sudo gpasswd -d {user} {group}"))
            }
            Self::ClaudeMcpServer { name } => {
                ctx.run(cmd!(sh, "claude mcp remove --scope user {name}"))
            }
            Self::RegistryValue { key, name } => {
                registry(ctx, key, &["delete", key, "/v", name, "/f"])
            }
            Self::MachinePathEntry { entry } => {
                let quoted = entry.replace('\'', "''");
                set_machine_path(
                    ctx,
                    &format!(
                        "$entry = '{quoted}'; $current = [Environment]::GetEnvironmentVariable('Path', 'Machine'); \
                         $entries = @($current -split ';' | Where-Object {{ $_ -and $_.TrimEnd('\\') -ine $entry.TrimEnd('\\') }}); \
                         [Environment]::SetEnvironmentVariable('Path', ($entries -join ';'), 'Machine')"
                    ),
                )
            }
            Self::TaskbarAutoHide => crate::taskbar_auto_hide::set(false),
        }
    }
}

fn group_arguments(groups: &[String]) -> Vec<&str> {
    groups
        .iter()
        .flat_map(|group| ["--group", group.as_str()])
        .collect()
}

/// Finds `name` in `reg.exe query` output and returns it as `TYPE:data`.
/// Value names can contain spaces, so the line is split at its `REG_` type.
fn registry_value(output: &str, name: &str) -> Option<String> {
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

/// The parts of a ufw rule that identify it: its action, then `from`, `to`, `port`
/// and `proto` with their values. The direction `in` is ufw's default and is not listed.
fn ufw_rule_parts(words: &[&str]) -> Vec<String> {
    let end = words
        .iter()
        .position(|word| *word == "comment")
        .unwrap_or(words.len());
    let words = &words[..end];
    let mut parts: Vec<String> = words
        .first()
        .map(|action| (*action).to_owned())
        .into_iter()
        .collect();
    for pair in words.windows(2) {
        if matches!(pair[0], "from" | "to" | "port" | "proto") {
            parts.push(format!("{} {}", pair[0], pair[1]));
        }
    }
    parts.sort();
    parts
}

/// Whether a line of `ufw show added` is the rule given by these `ufw` arguments.
/// ufw prints rules in its own word order, so the parts are compared, not the text.
fn ufw_rule_matches(rule: &[String], line: &str) -> bool {
    let Some(listed) = line.trim().strip_prefix("ufw ") else {
        return false;
    };
    let wanted: Vec<&str> = rule
        .iter()
        .map(String::as_str)
        .filter(|word| *word != "in")
        .collect();
    let listed: Vec<&str> = listed
        .split_whitespace()
        .filter(|word| *word != "in")
        .collect();
    ufw_rule_parts(&wanted) == ufw_rule_parts(&listed)
}

fn same_path(left: &str, right: &str) -> bool {
    left.trim_end_matches(['\\', '/'])
        .eq_ignore_ascii_case(right.trim_end_matches(['\\', '/']))
}

/// Runs `reg.exe`, through an administrator prompt for machine-wide keys.
fn registry(ctx: &Context, key: &str, arguments: &[&str]) -> ModuleResult {
    if !key.starts_with("HKEY_LOCAL_MACHINE") && !key.starts_with("HKLM") {
        return ctx.run(ctx.shell.cmd("reg.exe").args(arguments));
    }
    let pwsh = crate::support::powershell_7(ctx)?;
    let quoted: Vec<String> = arguments
        .iter()
        .map(|argument| format!("'\"{}\"'", argument.replace('\'', "''")))
        .collect();
    let script = format!(
        "$process = Start-Process -FilePath reg.exe -Verb RunAs -ArgumentList {} -Wait -PassThru -ErrorAction Stop; if ($process.ExitCode -ne 0) {{ throw 'Elevated registry change failed' }}",
        quoted.join(", ")
    );
    ctx.run(cmd!(ctx.shell, "{pwsh} -NoProfile -Command {script}"))
}

fn machine_path(ctx: &Context) -> ModuleResult<String> {
    let pwsh = crate::support::powershell_7(ctx)?;
    let read_path = "[Environment]::GetEnvironmentVariable('Path', 'Machine')";
    ctx.read(cmd!(ctx.shell, "{pwsh} -NoProfile -Command {read_path}"))
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
    fn ufw_rules_match_the_form_ufw_lists_them_in() {
        let rule: Vec<String> = [
            "allow",
            "in",
            "proto",
            "tcp",
            "from",
            "10.0.0.0/8",
            "to",
            "any",
            "port",
            "18080,18081",
            "comment",
            "myconfig Emacs browser terminal",
        ]
        .iter()
        .map(|word| (*word).to_owned())
        .collect();
        // The form ufw's own parser prints for these arguments.
        let listed = "ufw allow from 10.0.0.0/8 to any port 18080,18081 proto tcp comment 'myconfig Emacs browser terminal'";
        assert!(ufw_rule_matches(&rule, listed));
        assert!(!ufw_rule_matches(
            &rule,
            "ufw allow from 192.168.0.0/16 to any port 18080,18081 proto tcp"
        ));
        assert!(!ufw_rule_matches(
            &rule,
            "ufw deny from 10.0.0.0/8 to any port 18080,18081 proto tcp"
        ));
    }

    #[test]
    fn registry_values_keep_names_with_spaces() {
        let output = "\r\nHKEY_CURRENT_USER\\Fonts\r\n    Iosevka Nerd Font (TrueType)    REG_SZ    C:\\Fonts\\Iosevka.ttf\r\n";
        assert_eq!(
            registry_value(output, "Iosevka Nerd Font (TrueType)").as_deref(),
            Some("REG_SZ:C:\\Fonts\\Iosevka.ttf")
        );
        assert_eq!(
            registry_value("    Hidden    REG_DWORD    0x1", "Hidden").as_deref(),
            Some("REG_DWORD:0x1")
        );
        assert_eq!(
            registry_value("    Hidden    REG_DWORD    0x1", "Other"),
            None
        );
    }

    #[test]
    fn path_entries_compare_without_case_or_trailing_separator() {
        assert!(same_path(
            "C:\\Program Files\\LLVM\\bin\\",
            "c:\\program files\\llvm\\bin"
        ));
        assert!(!same_path("C:\\LLVM", "C:\\LLVM\\bin"));
    }
}
