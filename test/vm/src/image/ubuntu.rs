//! The Ubuntu image: the official Ubuntu 26.04 LTS cloud image, set up at its first boot by
//! cloud-init from a NoCloud seed disc.

use std::time::Duration;

use regex::Regex;

use super::{Origin, Source, SourceError, SourceKind};
use crate::download;
use crate::os::{GUEST_PASSWORD, GUEST_USER};
use crate::release::compare_releases;
use crate::seed::SeedFile;

/// The cloud images of Ubuntu 26.04 LTS, by its code name. Each build has its own folder,
/// named after its build serial.
const RELEASES: &str = "https://cloud-images.ubuntu.com/releases/resolute/";
const IMAGE_FILE: &str = "ubuntu-26.04-server-cloudimg-amd64.img";

/// cloud-init looks for a NoCloud seed on a disc with this label.
pub const SEED_LABEL: &str = "cidata";

/// cloud-init sets up the account and powers the guest off within a few minutes.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

const USER_DATA: &str = include_str!("../../image/ubuntu/user-data");

/// The build serial of the newest cloud image, such as `20260823`.
pub fn latest_release() -> Result<String, SourceError> {
    let listing = download::fetch_text(RELEASES, None)?;
    Regex::new(r#"href="release-(\d{8}(?:\.\d+)?)/""#)
        .expect("the pattern is valid")
        .captures_iter(&listing)
        .map(|captures| captures[1].to_owned())
        .max_by(|left, right| compare_releases(left, right))
        .ok_or(SourceError::Format {
            url: RELEASES.to_owned(),
            expected: "a release-<serial> folder",
        })
}

pub fn source() -> Result<Source, SourceError> {
    let release = latest_release()?;
    let folder = format!("{RELEASES}release-{release}/");
    let sums_url = format!("{folder}SHA256SUMS");
    let sums = download::fetch_text(&sums_url, None)?;
    // Each line is a SHA-256, a space, then the file name after `*` for a binary file.
    let sha256 = sums
        .lines()
        .filter_map(|line| line.split_once(' '))
        .find(|(_, name)| name.trim_start_matches('*') == IMAGE_FILE)
        .map(|(hash, _)| hash.to_ascii_lowercase())
        .filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or(SourceError::Format {
            url: sums_url,
            expected: "the SHA-256 of the amd64 cloud image",
        })?;
    Ok(Source {
        release,
        sha256,
        origin: Origin::Link {
            url: format!("{folder}{IMAGE_FILE}"),
            file_name: IMAGE_FILE.to_owned(),
        },
        kind: SourceKind::Disk,
    })
}

/// The NoCloud seed: the user data with the account and key, and the instance metadata.
pub fn seed_files(public_key: &str) -> Vec<SeedFile> {
    // A JSON string is also a valid YAML string, which keeps the key line intact.
    let key = serde_json::Value::from(public_key).to_string();
    vec![
        SeedFile {
            name: "user-data",
            content: USER_DATA
                .replace("@USER@", GUEST_USER)
                .replace("@PASSWORD@", GUEST_PASSWORD)
                .replace("@KEY@", &key),
            executable: false,
        },
        SeedFile {
            name: "meta-data",
            content: "instance-id: myconfig-ubuntu\nlocal-hostname: myconfig-ubuntu\n".to_owned(),
            executable: false,
        },
    ]
}
