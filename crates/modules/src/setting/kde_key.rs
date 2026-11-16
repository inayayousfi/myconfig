//! A key in a KDE configuration file, through `kreadconfig6` and `kwriteconfig6`.
use std::collections::BTreeMap;

use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult, support::config_home};

const MISSING: &str = "__myconfig_missing__";

pub(super) struct KdeKey<'a> {
    pub file: &'a str,
    pub groups: &'a [String],
    pub key: &'a str,
}

impl KdeKey<'_> {
    fn group_arguments(&self) -> Vec<&str> {
        self.groups
            .iter()
            .flat_map(|group| ["--group", group.as_str()])
            .collect()
    }
}

impl Kind for KdeKey<'_> {
    fn describe(&self) -> String {
        format!("{} [{}] {}", self.file, self.groups.join("]["), self.key)
    }

    /// Reads the user's own file only. Given a bare file name, `kreadconfig6` also reads
    /// the defaults in `/etc/xdg`, which `remove` would then write into the user's file.
    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let Self { key, .. } = self;
        let file = config_home(ctx).join(self.file);
        let groups = self.group_arguments();
        let value = ctx.read(cmd!(
            ctx.shell,
            "kreadconfig6 --file {file} {groups...} --key {key} --default {MISSING}"
        ))?;
        Ok((value != MISSING).then_some(value))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let Self { file, key, .. } = self;
        let groups = self.group_arguments();
        ctx.run(cmd!(
            ctx.shell,
            "kwriteconfig6 --notify --file {file} {groups...} --key {key} {value}"
        ))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let Self { file, key, .. } = self;
        let groups = self.group_arguments();
        ctx.run(cmd!(
            ctx.shell,
            "kwriteconfig6 --file {file} {groups...} --key {key} --delete"
        ))
    }
}

/// The keys of one KDE configuration file, by their groups and name, with their values as
/// `kreadconfig6` returns them.
pub(crate) type Entries = BTreeMap<(Vec<String>, String), String>;

/// Reads the keys of a KDE configuration file the way KDE's own parser does. A key that
/// appears twice in a group keeps its last value.
pub(super) fn parse(text: &str) -> Entries {
    let mut entries = Entries::new();
    let mut groups = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(header) = line.strip_prefix('[') {
            groups = header
                .trim_end_matches(']')
                .split("][")
                .filter(|group| !group.starts_with('$'))
                .map(str::to_owned)
                .collect();
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        // A trailing `[$...]` holds flags, such as `$e` for expansion, not part of the name.
        let key = match key.rfind("[$") {
            Some(start) if key.ends_with(']') => key[..start].trim_end(),
            _ => key,
        };
        entries.insert((groups.clone(), key.to_owned()), unescape(value.trim()));
    }
    entries
}

/// Turns the escapes that KDE writes into the characters they stand for. KDE keeps `\;`
/// and `\,` as they are, because they separate list items.
fn unescape(value: &str) -> String {
    let mut text = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(char) = chars.next() {
        if char != '\\' {
            text.push(char);
            continue;
        }
        match chars.next() {
            Some('s') => text.push(' '),
            Some('t') => text.push('\t'),
            Some('n') => text.push('\n'),
            Some('r') => text.push('\r'),
            Some('\\') => text.push('\\'),
            Some('x') => {
                let digits: String = chars.clone().take(2).collect();
                match u8::from_str_radix(&digits, 16) {
                    Ok(byte) if digits.len() == 2 => {
                        text.push(char::from(byte));
                        chars.nth(1);
                    }
                    _ => text.push_str("\\x"),
                }
            }
            Some(other) => {
                text.push('\\');
                text.push(other);
            }
            None => text.push('\\'),
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(groups: &[&str], key: &str) -> (Vec<String>, String) {
        (
            groups.iter().map(|group| (*group).to_owned()).collect(),
            key.to_owned(),
        )
    }

    #[test]
    fn parse_reads_keys_by_nested_group_with_escapes_and_flags() {
        let entries = parse(
            "Top=1\n\n# comment\n[General]\nfont = IBM Plex Sans,10\nTerminalApplication=ghostty\n\
             [Libinput][Defaults][Pointer]\nPointerAcceleration=\\s0.2\\\\x\\x41\n\
             [KDE]\nwidgetStyle[$e]=Breeze\nwidgetStyle=Fusion\n",
        );
        assert_eq!(
            entries.get(&entry(&[], "Top")).map(String::as_str),
            Some("1")
        );
        assert_eq!(
            entries
                .get(&entry(&["General"], "font"))
                .map(String::as_str),
            Some("IBM Plex Sans,10")
        );
        assert_eq!(
            entries
                .get(&entry(
                    &["Libinput", "Defaults", "Pointer"],
                    "PointerAcceleration"
                ))
                .map(String::as_str),
            Some(" 0.2\\xA")
        );
        assert_eq!(
            entries
                .get(&entry(&["KDE"], "widgetStyle"))
                .map(String::as_str),
            Some("Fusion")
        );
        assert_eq!(entries.len(), 5);
    }
}
