//! The Arch Linux image: Arch's official cloud image, set up at its first boot by cloud-init
//! from a NoCloud seed disc. It hosts the build of the InayOS USB stick.

use std::time::Duration;

use regex::Regex;

use super::{Origin, Source, SourceError, SourceKind};
use crate::download;
use crate::os::{GUEST_PASSWORD, GUEST_USER};
use crate::release::compare_releases;
use crate::seed::SeedFile;

/// The folder of the current Arch images. Each image is listed under a fixed name and under a
/// name with its build version; the xtask downloads the versioned file, so a new release that
/// appears during the download cannot replace the file the checksum names.
const LATEST: &str = "https://geo.mirror.pkgbuild.com/images/latest/";
const IMAGE_PREFIX: &str = "Arch-Linux-x86_64-cloudimg-";

/// cloud-init looks for a NoCloud seed on a disc with this label.
pub const SEED_LABEL: &str = "cidata";

/// cloud-init sets up the account and powers the guest off within a few minutes.
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

const USER_DATA: &str = include_str!("../../image/arch/user-data");

/// The build version of the newest cloud image, such as `20261115.625457`.
pub fn latest_release() -> Result<String, SourceError> {
    let listing = download::fetch_text(LATEST, None)?;
    release_in_listing(&listing).ok_or(SourceError::Format {
        url: LATEST.to_owned(),
        expected: "an Arch-Linux-x86_64-cloudimg-<version>.qcow2 file",
    })
}

pub fn source() -> Result<Source, SourceError> {
    let release = latest_release()?;
    let file_name = format!("{IMAGE_PREFIX}{release}.qcow2");
    let sums_url = format!("{LATEST}{file_name}.SHA256");
    let sums = download::fetch_text(&sums_url, None)?;
    let sha256 = sha256_in(&sums, &file_name).ok_or(SourceError::Format {
        url: sums_url,
        expected: "the SHA-256 of the cloud image",
    })?;
    Ok(Source {
        release,
        sha256,
        origin: Origin::Link {
            url: format!("{LATEST}{file_name}"),
            file_name,
        },
        kind: SourceKind::Disk,
    })
}

/// The newest build version among the versioned cloud images in the folder listing.
fn release_in_listing(listing: &str) -> Option<String> {
    let pattern = format!(r#"href="{IMAGE_PREFIX}(\d{{8}}\.\d+)\.qcow2""#);
    Regex::new(&pattern)
        .expect("the pattern is valid")
        .captures_iter(listing)
        .map(|captures| captures[1].to_owned())
        .max_by(|left, right| compare_releases(left, right))
}

/// The SHA-256 of `file_name` in a checksum file of `sha256sum` lines.
fn sha256_in(sums: &str, file_name: &str) -> Option<String> {
    sums.lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, name)| name.trim().trim_start_matches('*') == file_name)
        .map(|(hash, _)| hash.to_ascii_lowercase())
        .filter(|hash| hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
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
            content: "instance-id: inayos-arch\nlocal-hostname: inayos-arch\n".to_owned(),
            executable: false,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The folder listing of the mirror, cut to the cloud image lines and one other image.
    const LISTING: &str = r#"<a href="../">../</a>
<a href="Arch-Linux-x86_64-basic-20261115.625457.qcow2">Arch-Linux-x86_64-basic-20261115.625457.qcow2</a>      15-Nov-2026 12:11           525926400
<a href="Arch-Linux-x86_64-cloudimg-20261115.625457.qcow2">Arch-Linux-x86_64-cloudimg-20261115.625457.qcow2</a>   15-Nov-2026 12:11           569933312
<a href="Arch-Linux-x86_64-cloudimg-20261115.625457.qcow2.SHA256">Arch-Linux-x86_64-cloudimg-20261115.625457.qcow..&gt;</a> 15-Nov-2026 12:11                 115
<a href="Arch-Linux-x86_64-cloudimg-20261101.611111.qcow2">Arch-Linux-x86_64-cloudimg-20261101.611111.qcow2</a>   01-Nov-2026 12:11           569933312
<a href="Arch-Linux-x86_64-cloudimg.qcow2">Arch-Linux-x86_64-cloudimg.qcow2</a>                   15-Nov-2026 12:11           569933312
"#;

    #[test]
    fn the_release_is_the_newest_versioned_cloud_image_in_the_listing() {
        assert_eq!(
            release_in_listing(LISTING).as_deref(),
            Some("20261115.625457")
        );
    }

    #[test]
    fn a_listing_without_a_versioned_cloud_image_has_no_release() {
        let listing = r#"<a href="Arch-Linux-x86_64-cloudimg.qcow2">x</a>"#;
        assert_eq!(release_in_listing(listing), None);
    }

    #[test]
    fn the_checksum_is_read_from_the_line_that_names_the_file() {
        let hash = "dbd02830f6eb0fe07ef6e5edaddbae8b577ce4296d9e79118904704cce0a1f60";
        let sums = format!("{hash}  Arch-Linux-x86_64-cloudimg-20261115.625457.qcow2\n");
        assert_eq!(
            sha256_in(&sums, "Arch-Linux-x86_64-cloudimg-20261115.625457.qcow2").as_deref(),
            Some(hash)
        );
        assert_eq!(sha256_in(&sums, "Arch-Linux-x86_64-basic.qcow2"), None);
    }
}
