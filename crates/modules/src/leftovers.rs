//! What myconfig, the earlier name of InayOS, installed under its own names.
//!
//! Units, scripts, system files, KDE keys and shortcuts carried the name `myconfig`, and
//! InayOS installs the same things under its own name. The migration to state format 3
//! moves every pending record that names one of them into the `myconfig-leftovers` group,
//! which no module owns. `remove myconfig-leftovers` undoes only that group.
use std::path::Path;

use crate::{Setting, state::Change};

/// The recorded group that holds what myconfig installed under its own names.
pub const GROUP: &str = "myconfig-leftovers";

const EARLIER: &str = "myconfig";

/// The lines around the block that myconfig added to `refind.conf`.
const REFIND_BEGIN: &str = "# BEGIN MYCONFIG REFIND";
const REFIND_END: &str = "# END MYCONFIG REFIND";

/// Whether `name`, one file name or one identifier, is a name that myconfig gave to
/// something it installed: `myconfig` joined to other text by `-` or `.`, at the start or
/// after `-` or `.`. A Git repository address ends its last path part with `myconfig` or
/// `myconfig.git`, so neither counts.
pub(crate) fn earlier_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.match_indices(EARLIER).any(|(start, _)| {
        let before = lower[..start].chars().next_back();
        let rest = &lower[start + EARLIER.len()..];
        let repository = rest
            .strip_prefix(".git")
            .is_some_and(|after| !after.starts_with(|c: char| c.is_ascii_alphanumeric()));
        matches!(before, None | Some('-' | '.')) && rest.starts_with(['-', '.']) && !repository
    })
}

/// Whether one part of `text`, split at `/` and `\`, is an earlier name. Paths on Linux
/// and Windows, unit names and setting identifiers all split this way.
fn names_earlier(text: &str) -> bool {
    text.split(['/', '\\']).any(earlier_name)
}

fn path_names_earlier(path: &Path) -> bool {
    names_earlier(&path.to_string_lossy())
}

/// Whether the identity of `setting`, every text in its fields, holds an earlier name.
fn setting_names_earlier(setting: &Setting) -> bool {
    fn any_text(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::String(text) => names_earlier(text),
            serde_json::Value::Array(values) => values.iter().any(any_text),
            serde_json::Value::Object(fields) => fields.values().any(any_text),
            _ => false,
        }
    }
    serde_json::to_value(setting).is_ok_and(|value| any_text(&value))
}

/// Whether `change` installed something under an earlier name. Only what the change
/// installed counts: its path, its unit or its setting, never the value it replaced. A
/// disabled service, a recorded undo command, a move and a package never count, because
/// undoing them brings something back rather than taking an earlier name away.
pub(crate) fn names_earlier_thing(change: &Change) -> bool {
    match change {
        Change::File { path, .. }
        | Change::Created { path, .. }
        | Change::CreatedDirectory { path, .. } => path_names_earlier(path),
        Change::Service { unit, .. } => names_earlier(unit),
        Change::Setting { setting, .. } => setting_names_earlier(setting),
        _ => false,
    }
}

/// The records that take away an earlier name which `change` brought along without
/// naming it itself:
///
/// - a stowed config package brings the links that Stow made for its files under earlier
///   names;
/// - a write to `refind.conf` brings the block that myconfig marked with its name;
/// - a KWin rule under an earlier name brings its entry in KWin's rule list.
pub(crate) fn companions(change: &Change) -> Option<Change> {
    match change {
        Change::Stowed { package } => Some(Change::EarlierNameLinks {
            package: package.clone(),
        }),
        Change::File {
            path, system: true, ..
        } if path.file_name().is_some_and(|name| name == "refind.conf") => {
            Some(Change::MarkedBlock {
                path: path.clone(),
                begin: REFIND_BEGIN.to_owned(),
                end: REFIND_END.to_owned(),
            })
        }
        Change::Setting {
            setting: Setting::KdeKey { file, groups, .. },
            ..
        } if file == "kwinrulesrc" && groups.len() == 1 && earlier_name(&groups[0]) => {
            Some(Change::Setting {
                setting: Setting::KwinRule {
                    name: groups[0].clone(),
                },
                previous: None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::ServiceScope;

    #[test]
    fn every_name_that_myconfig_installed_counts() {
        for name in [
            "myconfig-kanata.service",
            "myconfig-kanata-tray",
            "myconfig-plasma",
            "myconfig-panels",
            "myconfig.session",
            "50-myconfig-fonts.conf",
            "myconfig.conf",
            "90-myconfig-pointer-sensitivity.lua",
            "myconfig-kanata.conf",
            "99-myconfig-kanata.rules",
            "10-myconfig.conf",
            "myconfig-picture-in-picture",
            "myconfig-panelsEnabled",
            "myconfig-docker.sources",
            "myconfig-docker.asc",
            "myconfig-autohotkey.lnk",
            "myconfig.exe",
            "myconfig.ahk",
            "myconfig-wsl-autostart.lnk",
            "myconfig.blacknpink-theme",
            "MYCONFIG-KANATA.SERVICE",
        ] {
            assert!(
                earlier_name(name),
                "{name} was not taken for an earlier name"
            );
        }
    }

    #[test]
    fn repository_addresses_and_other_names_do_not_count() {
        for name in [
            "myconfig",
            "myconfig.git",
            "inayos-kanata.service",
            "notmyconfig-x",
            "myconfigs",
            "MYCONFIG_APP",
            "",
        ] {
            assert!(!earlier_name(name), "{name} was taken for an earlier name");
        }
        for text in [
            "https://github.com/someone/myconfig",
            "git@github.com:someone/myconfig.git",
            "url.https://github.com/someone/myconfig.git.insteadOf",
            "/home/me/Projets/myconfig/target",
        ] {
            assert!(!names_earlier(text), "{text} was taken for an earlier name");
        }
    }

    #[test]
    fn a_change_counts_by_what_it_installed() {
        let file = |path: &str| Change::File {
            path: PathBuf::from(path),
            system: true,
            previous: crate::state::Previous::Missing,
        };
        assert!(names_earlier_thing(&file(
            "/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua"
        )));
        assert!(names_earlier_thing(&file(
            r"C:\Users\me\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\myconfig-autohotkey.lnk"
        )));
        assert!(!names_earlier_thing(&file(
            "/etc/apt/sources.list.d/inayos-docker.sources"
        )));
        assert!(names_earlier_thing(&Change::Service {
            unit: "myconfig-handy.service".into(),
            scope: ServiceScope::User,
            was_enabled: false,
        }));
        assert!(!names_earlier_thing(&Change::ServiceDisabled {
            unit: "myconfig-handy.service".into(),
            scope: ServiceScope::User,
            was_enabled: true,
            was_active: true,
        }));
        assert!(names_earlier_thing(&Change::Setting {
            setting: Setting::kde("kwinrc", &["Plugins"], "myconfig-panelsEnabled"),
            previous: None,
        }));
        assert!(names_earlier_thing(&Change::Setting {
            setting: Setting::VscodiumExtension {
                id: "myconfig.blacknpink-theme".into(),
                editor: "codium".into(),
            },
            previous: None,
        }));
        // The rule list names the earlier rule only in its value.
        assert!(!names_earlier_thing(&Change::Setting {
            setting: Setting::kde("kwinrulesrc", &["General"], "rules"),
            previous: Some("myconfig-picture-in-picture".into()),
        }));
        assert!(!names_earlier_thing(&Change::Setting {
            setting: Setting::GitConfig {
                key: "url.git@github.com:someone/myconfig.git.insteadOf".into(),
            },
            previous: None,
        }));
        assert!(!names_earlier_thing(&Change::UndoCommand {
            program: PathBuf::from("/home/me/.local/bin/myconfig-tool"),
            args: Vec::new(),
        }));
    }
}
