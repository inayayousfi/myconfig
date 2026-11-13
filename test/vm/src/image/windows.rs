//! The Windows image: Microsoft's current Windows 11 consumer ISO, installed unattended
//! with an answer file on the seed disc.
//!
//! Microsoft offers the ISO through its download page only, behind a session that a browser
//! script opens. The requests here follow the Mido script, which quickget also uses: read the
//! product edition from the page, register a session, ask for the language's SKU, then for the
//! download link. The SHA-256 comes from the table of hashes on the same page.

use std::fs;
use std::thread;
use std::time::Duration;

use regex::Regex;
use serde_json::Value;

use super::{ImageError, Origin, Source, SourceError, SourceKind};
use crate::download;
use crate::os::{GUEST_PASSWORD, GUEST_USER};
use crate::seed::SeedFile;
use crate::ssh::Guest;
use crate::vm::Vm;

const DOWNLOAD_PAGE: &str = "https://www.microsoft.com/en-us/software-download/windows11";
const CONNECTOR: &str = "https://www.microsoft.com/software-download-connector/api";
/// The fixed identifiers that the download page's own script sends.
const PROFILE: &str = "606624d44113";
const ORGANIZATION: &str = "y6jn8c31";
/// The ISO language. The SKU list and the hash table both call it this.
const LANGUAGE: &str = "English";

pub const SEED_LABEL: &str = "MYCONFIG";
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(3 * 60 * 60);

/// UEFI starts the Windows disc only after a key press at "Press any key to boot from CD or
/// DVD", which waits a few seconds. Enter is pressed once a second for this long.
const KEY_PRESSES: u32 = 30;

const ANSWER_FILE: &str = include_str!("../../image/windows/autounattend.xml");
const SETUP_SCRIPT: &str = include_str!("../../image/windows/myconfig-setup.ps1");

/// The Windows version the download page offers now, such as `25H2`.
pub fn latest_release() -> Result<String, SourceError> {
    version(&download::fetch_text(DOWNLOAD_PAGE, None)?)
}

pub fn source() -> Result<Source, SourceError> {
    let page = download::fetch_text(DOWNLOAD_PAGE, None)?;
    let release = version(&page)?;
    let edition = capture(
        &page,
        r#"<option value="(\d+)">Windows 11"#,
        "a Windows 11 product edition",
    )?;
    let sha256 = capture(
        &page,
        &format!(r"<td>{LANGUAGE} 64-bit</td><td>([0-9A-Fa-f]{{64}})</td>"),
        "the SHA-256 of the 64-bit English ISO",
    )?
    .to_ascii_lowercase();

    let session = new_session_id()?;
    download::fetch_text(
        &format!("https://vlscppe.microsoft.com/tags?org_id={ORGANIZATION}&session_id={session}"),
        None,
    )?;

    let skus_url = format!(
        "{CONNECTOR}/getskuinformationbyproductedition?profile={PROFILE}&ProductEditionId={edition}&SKU=undefined&friendlyFileName=undefined&Locale=en-US&sessionID={session}"
    );
    let skus = json(&skus_url, None)?;
    let sku = skus
        .get("Skus")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|sku| sku.get("Language").and_then(Value::as_str) == Some(LANGUAGE))
        .and_then(|sku| sku.get("Id"))
        .map(|id| id.as_str().map_or_else(|| id.to_string(), str::to_owned))
        .ok_or(SourceError::Format {
            url: skus_url,
            expected: "an English SKU",
        })?;

    let links_url = format!(
        "{CONNECTOR}/GetProductDownloadLinksBySku?profile={PROFILE}&productEditionId=undefined&SKU={sku}&friendlyFileName=undefined&Locale=en-US&sessionID={session}"
    );
    let links_text = download::fetch_text(&links_url, Some(DOWNLOAD_PAGE))?;
    // Microsoft refuses links to some network addresses. The hash table above still names
    // the ISO, so a copy that the user saves by hand can be checked.
    if links_text.contains("Sentinel marked this request as rejected") {
        return Ok(Source {
            release,
            sha256,
            origin: Origin::ByHand {
                page: DOWNLOAD_PAGE.to_owned(),
            },
            kind: SourceKind::BootDisc,
        });
    }
    let links: Value = serde_json::from_str(&links_text).map_err(|_| SourceError::Format {
        url: links_url.clone(),
        expected: "a JSON list of download links",
    })?;
    let url = links
        .get("ProductDownloadOptions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|option| option.get("Uri").and_then(Value::as_str))
        .find(|uri| uri.contains("x64"))
        .ok_or(SourceError::Format {
            url: links_url,
            expected: "an x64 download link",
        })?
        .to_owned();
    let file_name = download::strip_query(&url)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned();
    Ok(Source {
        release,
        sha256,
        origin: Origin::Link { url, file_name },
        kind: SourceKind::BootDisc,
    })
}

fn version(page: &str) -> Result<String, SourceError> {
    capture(page, r"Version (\d{2}H\d)", "the Windows version")
}

fn capture(page: &str, pattern: &str, expected: &'static str) -> Result<String, SourceError> {
    Regex::new(pattern)
        .expect("the pattern is valid")
        .captures(page)
        .map(|captures| captures[1].to_owned())
        .ok_or(SourceError::Format {
            url: DOWNLOAD_PAGE.to_owned(),
            expected,
        })
}

fn json(url: &str, referer: Option<&str>) -> Result<Value, SourceError> {
    serde_json::from_str(&download::fetch_text(url, referer)?).map_err(|_| SourceError::Format {
        url: url.to_owned(),
        expected: "JSON",
    })
}

/// A random session identifier in the UUID form the download service expects, from the kernel.
fn new_session_id() -> Result<String, SourceError> {
    let path = "/proc/sys/kernel/random/uuid";
    fs::read_to_string(path)
        .map(|uuid| uuid.trim().to_owned())
        .map_err(|error| SourceError::Io(path.into(), error))
}

/// The files of the seed disc. Windows Setup finds `autounattend.xml` at the root of any disc.
pub fn seed_files(public_key: &str) -> Vec<SeedFile> {
    let fill = |text: &str| {
        text.replace("@USER@", GUEST_USER)
            .replace("@PASSWORD@", GUEST_PASSWORD)
    };
    vec![
        SeedFile {
            name: "autounattend.xml",
            content: fill(ANSWER_FILE),
            executable: false,
        },
        SeedFile {
            name: "myconfig-setup.ps1",
            content: fill(SETUP_SCRIPT),
            executable: false,
        },
        SeedFile {
            name: "authorized_keys",
            content: format!("{public_key}\n"),
            executable: false,
        },
    ]
}

/// Answers the "Press any key to boot from CD or DVD" prompt of the Windows disc.
///
/// Unverified: the prompt timing has not been measured with this firmware. After Setup
/// restarts, the prompt appears again and its timeout lets the firmware start the disk, which
/// is why the presses stop after the first boot.
pub fn start_install(vm: &mut Vm) -> Result<(), ImageError> {
    for _ in 0..KEY_PRESSES {
        vm.check_running()?;
        vm.qmp()?.send_keys(&["ret"])?;
        thread::sleep(Duration::from_secs(1));
    }
    Ok(())
}

/// The full Windows build number, such as `10.0.26200.6584`, read from `ver` in the guest.
pub fn build_number(guest: &Guest) -> Result<String, ImageError> {
    let output = guest.run("ver")?;
    Regex::new(r"Version (\d+\.\d+\.\d+\.\d+)")
        .expect("the pattern is valid")
        .captures(&output)
        .map(|captures| captures[1].to_owned())
        .ok_or(ImageError::NoBuildNumber(output.trim().to_owned()))
}
