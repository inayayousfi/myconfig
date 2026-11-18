//! Compares two snapshots of a guest and keeps only the differences that the allowed-changes
//! file of its system does not allow. Any difference left fails the cycle.

use std::collections::BTreeMap;
use std::fmt;

use regex::Regex;

use crate::os::Os;
use crate::snapshot::Snapshot;

/// The allowed-changes file of `os`, from `test/vm/allowed-changes/`.
pub fn allowed_changes(os: Os) -> &'static str {
    match os {
        Os::Ubuntu => include_str!("../allowed-changes/ubuntu.txt"),
        Os::Windows => include_str!("../allowed-changes/windows.txt"),
    }
}

/// The allowed-changes file of the Arch WSL distribution in the Windows guest.
pub const ARCH_WSL_ALLOWED_CHANGES: &str = include_str!("../allowed-changes/arch-wsl.txt");

/// One entry that differs between two snapshots.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Difference {
    Added {
        list: String,
        key: String,
        value: String,
    },
    Removed {
        list: String,
        key: String,
        value: String,
    },
    Changed {
        list: String,
        key: String,
        before: String,
        after: String,
    },
}

impl Difference {
    fn place(&self) -> (&str, &str) {
        match self {
            Difference::Added { list, key, .. }
            | Difference::Removed { list, key, .. }
            | Difference::Changed { list, key, .. } => (list, key),
        }
    }
}

impl fmt::Display for Difference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Difference::Added { list, key, value } => {
                write!(formatter, "+ {list}: {key}")?;
                if !value.is_empty() {
                    write!(formatter, " ({value})")?;
                }
                Ok(())
            }
            Difference::Removed { list, key, value } => {
                write!(formatter, "- {list}: {key}")?;
                if !value.is_empty() {
                    write!(formatter, " ({value})")?;
                }
                Ok(())
            }
            Difference::Changed {
                list,
                key,
                before,
                after,
            } => write!(formatter, "~ {list}: {key} ({before} -> {after})"),
        }
    }
}

/// What a comparison checks, which decides the rules that apply to it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// A second install, which must change nothing.
    Install,
    /// A removal, which must leave nothing.
    Removal,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line} of the allowed-changes file is not `[removal] <list> <pattern>`: {text}")]
pub struct AllowedError {
    pub line: usize,
    pub text: String,
}

/// The entries allowed to differ. Each rule names a list, or `*` for every list, and a
/// pattern for the key in which `*` stands for any text, `/` and `\` included. A rule marked
/// `removal` applies only to a removal.
pub struct Allowed {
    rules: Vec<Rule>,
}

struct Rule {
    removal_only: bool,
    list: String,
    pattern: Regex,
}

impl Allowed {
    /// Reads an allowed-changes file: one `[removal] <list> <pattern>` rule per line. Blank
    /// lines and lines that start with `#` are skipped.
    pub fn parse(text: &str) -> Result<Allowed, AllowedError> {
        let mut rules = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let error = || AllowedError {
                line: index + 1,
                text: line.to_owned(),
            };
            let (removal_only, rule) = match line.split_once(char::is_whitespace) {
                Some(("removal", rest)) => (true, rest.trim_start()),
                _ => (false, line),
            };
            let (list, pattern) = rule.split_once(char::is_whitespace).ok_or_else(error)?;
            rules.push(Rule {
                removal_only,
                list: list.to_owned(),
                pattern: glob(pattern.trim()),
            });
        }
        Ok(Allowed { rules })
    }

    fn allows(&self, difference: &Difference, comparison: Comparison) -> bool {
        let (list, key) = difference.place();
        self.rules.iter().any(|rule| {
            (!rule.removal_only || comparison == Comparison::Removal)
                && (rule.list == "*" || rule.list == list)
                && rule.pattern.is_match(key)
        })
    }
}

/// The differences between `before` and `after` that `allowed` does not allow in
/// `comparison`, list by list and key by key in sorted order.
pub fn compare(
    before: &Snapshot,
    after: &Snapshot,
    allowed: &Allowed,
    comparison: Comparison,
) -> Vec<Difference> {
    let empty = BTreeMap::new();
    let mut names: Vec<&String> = before.lists.keys().chain(after.lists.keys()).collect();
    names.sort();
    names.dedup();

    let mut differences = Vec::new();
    for name in names {
        let old = before.lists.get(name).unwrap_or(&empty);
        let new = after.lists.get(name).unwrap_or(&empty);
        for (key, value) in old {
            match new.get(key) {
                None => differences.push(Difference::Removed {
                    list: name.clone(),
                    key: key.clone(),
                    value: value.clone(),
                }),
                Some(after) if after != value => differences.push(Difference::Changed {
                    list: name.clone(),
                    key: key.clone(),
                    before: value.clone(),
                    after: after.clone(),
                }),
                Some(_) => {}
            }
        }
        for (key, value) in new {
            if !old.contains_key(key) {
                differences.push(Difference::Added {
                    list: name.clone(),
                    key: key.clone(),
                    value: value.clone(),
                });
            }
        }
    }
    differences.retain(|difference| !allowed.allows(difference, comparison));
    differences
}

/// A pattern that matches the whole key, with `*` matching any text.
fn glob(pattern: &str) -> Regex {
    let parts: Vec<String> = pattern.split('*').map(regex::escape).collect();
    Regex::new(&format!("^{}$", parts.join(".*"))).expect("an escaped pattern is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(text: &str) -> Snapshot {
        let mut snapshot = Snapshot::default();
        snapshot.read(text).expect("the test lists are valid");
        snapshot
    }

    const BEFORE: &str = "## packages\nbash\n## files\n/home/tester/.bashrc\taaa\n";

    #[test]
    fn a_leftover_after_removal_fails() {
        let after = snapshot(
            "## packages\nbash\nzsh\n## files\n/home/tester/.bashrc\taaa\n/home/tester/.zshrc\tlink /home/tester/dotfiles/zsh/.zshrc\n",
        );
        let allowed = Allowed::parse("files /home/tester/.cache/*\n").unwrap();
        assert_eq!(
            compare(&snapshot(BEFORE), &after, &allowed, Comparison::Removal),
            vec![
                Difference::Added {
                    list: "files".to_owned(),
                    key: "/home/tester/.zshrc".to_owned(),
                    value: "link /home/tester/dotfiles/zsh/.zshrc".to_owned(),
                },
                Difference::Added {
                    list: "packages".to_owned(),
                    key: "zsh".to_owned(),
                    value: String::new(),
                },
            ]
        );
    }

    #[test]
    fn a_file_left_changed_after_removal_fails() {
        let after = snapshot("## packages\nbash\n## files\n/home/tester/.bashrc\tbbb\n");
        let allowed = Allowed::parse("").unwrap();
        assert_eq!(
            compare(&snapshot(BEFORE), &after, &allowed, Comparison::Removal),
            vec![Difference::Changed {
                list: "files".to_owned(),
                key: "/home/tester/.bashrc".to_owned(),
                before: "aaa".to_owned(),
                after: "bbb".to_owned(),
            }]
        );
    }

    #[test]
    fn an_allowed_change_passes() {
        let after = snapshot(
            "## packages\nbash\n## files\n/home/tester/.bashrc\taaa\n/home/tester/.cache/fontconfig/x.cache\tccc\n",
        );
        let allowed = Allowed::parse("# Caches\nfiles /home/tester/.cache/*\n").unwrap();
        assert_eq!(
            compare(&snapshot(BEFORE), &after, &allowed, Comparison::Removal),
            Vec::new()
        );
    }

    #[test]
    fn a_rule_allows_its_own_list_only() {
        // A package named like an allowed path is still a leftover.
        let after = snapshot(&format!("{BEFORE}## packages\nbash\ncache\n"));
        let allowed = Allowed::parse("files cache\n").unwrap();
        assert_eq!(
            compare(&snapshot(BEFORE), &after, &allowed, Comparison::Removal).len(),
            1
        );
    }

    #[test]
    fn a_removal_rule_allows_a_leftover_but_not_a_second_install_change() {
        let after = snapshot(&format!("{BEFORE}/home/tester/.gitconfig\tddd\n"));
        let allowed = Allowed::parse("removal files /home/tester/.gitconfig\n").unwrap();
        let before = snapshot(BEFORE);
        assert_eq!(
            compare(&before, &after, &allowed, Comparison::Removal),
            Vec::new()
        );
        assert_eq!(
            compare(&before, &after, &allowed, Comparison::Install).len(),
            1
        );
    }

    #[test]
    fn every_allowed_changes_file_parses() {
        for os in [Os::Ubuntu, Os::Windows] {
            assert!(Allowed::parse(allowed_changes(os)).is_ok(), "{os}");
        }
        assert!(Allowed::parse(ARCH_WSL_ALLOWED_CHANGES).is_ok(), "arch-wsl");
    }
}
