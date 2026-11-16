//! The state lists of a guest at one moment, in the text form that the list scripts print
//! and that the cycle saves.
//!
//! The text has one `## <list>` line before the entries of each list. An entry is a key,
//! such as a path or a package name, and an optional value, such as a checksum, after a tab.

use std::collections::BTreeMap;
use std::fmt;

const HEADER: &str = "## ";

/// The entries of each named list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub lists: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("line {line} of the state lists comes before any `## <list>` line: {text}")]
pub struct ParseError {
    pub line: usize,
    pub text: String,
}

impl Snapshot {
    /// Reads the text that the list scripts print. Several texts can be read into one snapshot.
    pub fn read(&mut self, text: &str) -> Result<(), ParseError> {
        let mut current: Option<String> = None;
        for (index, line) in text.lines().enumerate() {
            let line = line.trim_end_matches('\r');
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix(HEADER) {
                let name = name.trim().to_owned();
                self.lists.entry(name.clone()).or_default();
                current = Some(name);
                continue;
            }
            let Some(name) = &current else {
                return Err(ParseError {
                    line: index + 1,
                    text: line.to_owned(),
                });
            };
            let (key, value) = line.split_once('\t').unwrap_or((line, ""));
            if let Some(list) = self.lists.get_mut(name) {
                list.insert(key.to_owned(), value.to_owned());
            }
        }
        Ok(())
    }
}

impl fmt::Display for Snapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (name, entries) in &self.lists {
            writeln!(formatter, "{HEADER}{name}")?;
            for (key, value) in entries {
                if value.is_empty() {
                    writeln!(formatter, "{key}")?;
                } else {
                    writeln!(formatter, "{key}\t{value}")?;
                }
            }
        }
        Ok(())
    }
}
