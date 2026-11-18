//! The profile modules an interface lets the user leave out, with the ones the installer
//! always installs. It reads them from the system binary's `list --needs`.
use std::path::Path;

use crate::{Error, host::Command, host::Host, install::machine::Machine, sink::Silent};

/// The modules every InayOS system has: the desktop, its boot, its input and accessibility
/// tools, its sound and its remote access. The modules they hard-need are locked too.
pub const LOCKED_MODULES: &[&str] = &[
    "plasma-version",
    "base",
    "system-tuning",
    "kde-plasma",
    "refind",
    "bootvine",
    "kanata",
    "kanata-kde",
    "axidev-osk",
    "handy",
    "pipewire",
    "ssh",
    "ssh-server",
    "git-config",
];

/// One profile module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleEntry {
    pub name: String,
    /// The modules it cannot work without.
    pub hard_needs: Vec<String>,
    /// Whether the install always includes it, because it is in `LOCKED_MODULES` or a locked
    /// module needs it.
    pub locked: bool,
}

/// The modules of the profile in profile order, from `<system_binary> list --needs`. Fails
/// when the command fails or prints a line without `:`.
pub fn list(host: &dyn Host, system_binary: &Path) -> Result<Vec<ModuleEntry>, Error> {
    let m = Machine {
        host,
        sink: &Silent,
    };
    let command = Command::query(&system_binary.to_string_lossy()).args(["list", "--needs"]);
    let shown = command.shown();
    parse(&m.query(command)?).map_err(|problem| Error::UnexpectedOutput {
        command: shown,
        problem,
    })
}

/// Reads the lines `name: need1 need2` and marks the locked modules.
fn parse(text: &str) -> Result<Vec<ModuleEntry>, String> {
    let mut entries = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let (name, needs) = line
                .split_once(':')
                .ok_or_else(|| format!("the line {line:?} has no `:`"))?;
            Ok(ModuleEntry {
                name: name.trim().to_owned(),
                hard_needs: needs.split_whitespace().map(str::to_owned).collect(),
                locked: false,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut locked: Vec<String> = Vec::new();
    for name in LOCKED_MODULES {
        if entries.iter().any(|entry| entry.name == *name) {
            locked.push((*name).to_owned());
            locked.extend(requirements(&entries, name));
        }
    }
    for entry in &mut entries {
        entry.locked = locked.contains(&entry.name);
    }
    Ok(entries)
}

/// The modules that hard-need `name`, directly or through others, in profile order. Leaving
/// `name` out leaves them out too.
pub fn dependents(entries: &[ModuleEntry], name: &str) -> Vec<String> {
    closure(entries, name, |entry, found| {
        entry.hard_needs.iter().any(|need| found.contains(need))
    })
}

/// The modules that `name` hard-needs, directly or through others, in profile order.
/// Including `name` includes them too.
pub fn requirements(entries: &[ModuleEntry], name: &str) -> Vec<String> {
    closure(entries, name, |entry, found| {
        entries
            .iter()
            .filter(|other| found.contains(&other.name))
            .any(|other| other.hard_needs.contains(&entry.name))
    })
}

/// Every entry other than `name` that `linked` ties to `name` or to an entry already found,
/// in profile order.
fn closure(
    entries: &[ModuleEntry],
    name: &str,
    linked: impl Fn(&ModuleEntry, &[String]) -> bool,
) -> Vec<String> {
    let mut found = vec![name.to_owned()];
    loop {
        let before = found.len();
        for entry in entries {
            if !found.contains(&entry.name) && linked(entry, &found) {
                found.push(entry.name.clone());
            }
        }
        if found.len() == before {
            break;
        }
    }
    entries
        .iter()
        .filter(|entry| entry.name != name && found.contains(&entry.name))
        .map(|entry| entry.name.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ModuleEntry, dependents, list, requirements};
    use crate::{
        Error,
        fake::{FakeHost, exit, ok},
    };

    const BINARY: &str = "/usr/local/lib/inayos/inayos";
    const LISTED: &str = "plasma-version:
base: plasma-version
media: base
kde-plasma: base
pipewire: base media
runtimes: base
agents-packages: base runtimes
agent-config: base agents-packages
environment-inventory:
";

    fn listed() -> Vec<ModuleEntry> {
        let host = FakeHost::default();
        host.respond(&format!("{BINARY} list --needs"), [ok(LISTED)]);
        list(&host, Path::new(BINARY)).unwrap()
    }

    #[test]
    fn locked_modules_and_everything_they_need_are_locked() {
        let entries = listed();
        let locked: Vec<_> = entries
            .iter()
            .filter(|entry| entry.locked)
            .map(|entry| entry.name.as_str())
            .collect();
        // media is not in the locked list, but pipewire needs it.
        assert_eq!(
            locked,
            ["plasma-version", "base", "media", "kde-plasma", "pipewire"]
        );
        assert_eq!(entries[6].hard_needs, ["base", "runtimes"]);
        assert!(entries[8].hard_needs.is_empty());
    }

    #[test]
    fn leaving_a_module_out_leaves_out_everything_that_needs_it() {
        let entries = listed();
        assert_eq!(
            dependents(&entries, "runtimes"),
            ["agents-packages", "agent-config"]
        );
        assert!(dependents(&entries, "agent-config").is_empty());
        assert_eq!(
            requirements(&entries, "agent-config"),
            ["plasma-version", "base", "runtimes", "agents-packages"]
        );
    }

    #[test]
    fn a_failed_or_unreadable_listing_is_an_error() {
        let host = FakeHost::default();
        host.respond(&format!("{BINARY} list --needs"), [exit(1)]);
        assert!(matches!(
            list(&host, Path::new(BINARY)),
            Err(Error::Command { .. })
        ));
        host.respond(&format!("{BINARY} list --needs"), [ok("base\n")]);
        assert!(matches!(
            list(&host, Path::new(BINARY)),
            Err(Error::UnexpectedOutput { .. })
        ));
    }
}
