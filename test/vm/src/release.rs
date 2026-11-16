//! The release a clean image was built from, and whether its source now lists a newer one.

use std::cmp::Ordering;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::image::{self, SourceError};
use crate::os::Os;
use crate::paths::Paths;

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("could not access {0}: {1}")]
    Io(PathBuf, io::Error),
    #[error("{0} is not a release record")]
    Format(PathBuf),
}

/// What an image was built from. `release` is the identifier its source publishes: the
/// CachyOS ISO number, the Ubuntu cloud image build serial, or the Windows version. `build`
/// holds what only the installed guest can tell, which is the Windows build number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub release: String,
    pub build: Option<String>,
    /// The file the image was installed from.
    pub source: String,
}

impl Release {
    pub fn read(path: &Path) -> Result<Release, ReleaseError> {
        let text =
            fs::read_to_string(path).map_err(|error| ReleaseError::Io(path.into(), error))?;
        let format = || ReleaseError::Format(path.to_owned());
        let value: Value = serde_json::from_str(&text).map_err(|_| format())?;
        let text_field = |name: &str| value.get(name).and_then(Value::as_str).map(str::to_owned);
        Ok(Release {
            release: text_field("release").ok_or_else(format)?,
            build: text_field("build"),
            source: text_field("source").ok_or_else(format)?,
        })
    }

    pub fn write(&self, path: &Path) -> Result<(), ReleaseError> {
        let value = json!({
            "release": self.release,
            "build": self.build,
            "source": self.source,
        });
        let text = format!("{value:#}\n");
        fs::write(path, text).map_err(|error| ReleaseError::Io(path.to_owned(), error))
    }
}

impl fmt::Display for Release {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.release)?;
        if let Some(build) = &self.build {
            write!(formatter, " (build {build})")?;
        }
        Ok(())
    }
}

/// The outcome of comparing an image with its source. A failed comparison is an outcome too,
/// because it only informs: the cycle reports it and continues.
#[derive(Debug)]
pub enum Freshness {
    Current(Release),
    Outdated {
        os: Os,
        image: Release,
        latest: String,
    },
    Unknown(FreshnessError),
}

#[derive(Debug, thiserror::Error)]
pub enum FreshnessError {
    #[error(transparent)]
    Release(#[from] ReleaseError),
    #[error(transparent)]
    Source(#[from] SourceError),
}

/// Compares the clean image of `os` with the release its source lists now.
pub fn check(os: Os, paths: &Paths) -> Freshness {
    let compare = || -> Result<Freshness, FreshnessError> {
        let image = Release::read(&paths.image(os).release())?;
        let latest = image::latest_release(os)?;
        Ok(if is_newer(&latest, &image.release) {
            Freshness::Outdated { os, image, latest }
        } else {
            Freshness::Current(image)
        })
    };
    compare().unwrap_or_else(Freshness::Unknown)
}

impl fmt::Display for Freshness {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Freshness::Current(image) => {
                write!(formatter, "the image is from {image}, the latest release")
            }
            Freshness::Outdated { os, image, latest } => write!(
                formatter,
                "warning: the {os} image is from {image}, and its source now lists {latest}; rebuild it with: cargo xtask vm image build {os}"
            ),
            Freshness::Unknown(error) => {
                write!(
                    formatter,
                    "warning: could not check for a newer release: {error}"
                )
            }
        }
    }
}

/// Whether `latest` is a later release than `saved`. Release identifiers are compared piece by
/// piece, with runs of digits compared as numbers, so `20260823.1` follows `20260823` and
/// `26H2` follows `25H2`.
fn is_newer(latest: &str, saved: &str) -> bool {
    compare_releases(latest, saved) == Ordering::Greater
}

pub(crate) fn compare_releases(left: &str, right: &str) -> Ordering {
    let (left, right) = (pieces(left), pieces(right));
    for (left, right) in left.iter().zip(&right) {
        let order = match (left.parse::<u64>(), right.parse::<u64>()) {
            (Ok(left), Ok(right)) => left.cmp(&right),
            _ => left.cmp(right),
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    left.len().cmp(&right.len())
}

/// Splits a release identifier into runs of digits and runs of other characters.
fn pieces(text: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let mut start = 0;
    for (index, character) in text.char_indices().skip(1) {
        let previous = text[..index].chars().next_back().unwrap_or(character);
        if previous.is_ascii_digit() != character.is_ascii_digit() {
            pieces.push(&text[start..index]);
            start = index;
        }
    }
    if start < text.len() {
        pieces.push(&text[start..]);
    }
    pieces
}
