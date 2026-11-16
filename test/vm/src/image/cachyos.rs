//! The CachyOS image: the current Desktop ISO, installed unattended by the CachyOS
//! command-line installer from a settings file on the seed disc.

use std::collections::BTreeSet;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use regex::Regex;
use serde_json::json;

use super::{ImageError, Origin, Source, SourceError, SourceKind};
use crate::download;
use crate::os::{GUEST_PASSWORD, GUEST_USER};
use crate::seed::SeedFile;
use crate::vm::Vm;

const DOWNLOAD_PAGE: &str = "https://cachyos.org/download/";
pub const SEED_LABEL: &str = "MYCONFIG";
pub const INSTALL_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

/// The live session needs time to reach its desktop before a terminal can open.
const FIRST_ATTEMPT_DELAY: Duration = Duration::from_secs(90);
const ATTEMPT_WAIT: Duration = Duration::from_secs(60);
const ATTEMPTS: u32 = 10;

/// Typed into a terminal of the live session. It mounts the seed disc by its label and runs
/// its start script as root.
const START_COMMAND: &str = "sudo bash -c 'mkdir -p /run/myconfig-seed; \
    mount -o ro /dev/disk/by-label/MYCONFIG /run/myconfig-seed; \
    exec bash /run/myconfig-seed/start.sh'\n";

const STARTED: &str = "myconfig-image: started";

const START_SCRIPT: &str = include_str!("../../image/cachyos/start.sh");
const POST_INSTALL_SCRIPT: &str = include_str!("../../image/cachyos/post-install.sh");

/// The ISO number of the Desktop ISO that the download page offers now.
pub fn latest_release() -> Result<String, SourceError> {
    desktop_iso(&download::fetch_text(DOWNLOAD_PAGE, None)?).map(|(_, release)| release)
}

pub fn source() -> Result<Source, SourceError> {
    let (url, release) = desktop_iso(&download::fetch_text(DOWNLOAD_PAGE, None)?)?;
    let file_name = url.rsplit('/').next().unwrap_or_default().to_owned();
    // The checksum file next to the ISO holds one line: the SHA-256, then the file name.
    let checksum_url = format!("{url}.sha256");
    let checksum = download::fetch_text(&checksum_url, None)?;
    let mut fields = checksum.split_whitespace();
    let sha256 = match (fields.next(), fields.next(), fields.next()) {
        (Some(hash), Some(name), None)
            if is_sha256(hash) && name.trim_start_matches('*') == file_name =>
        {
            hash.to_ascii_lowercase()
        }
        _ => {
            return Err(SourceError::Format {
                url: checksum_url,
                expected: "one SHA-256 for the discovered ISO",
            });
        }
    };
    Ok(Source {
        release,
        sha256,
        origin: Origin::Link { url, file_name },
        kind: SourceKind::BootDisc,
    })
}

/// The URL and ISO number of the one Desktop ISO linked from the download page. Mirror links
/// are percent-encoded on the page and do not match, and the SourceForge link continues with
/// `/download` after the file name.
fn desktop_iso(page: &str) -> Result<(String, String), SourceError> {
    let pattern = Regex::new(
        r"https://[A-Za-z0-9.-]+/[A-Za-z0-9/._-]*?desktop/(\d+)/cachyos-desktop-linux-(\d+)\.iso(?:[^/A-Za-z0-9]|$)",
    )
    .expect("the pattern is valid");
    let mut found = Vec::new();
    for captures in pattern.captures_iter(page) {
        if captures[1] == captures[2] {
            let whole = &captures[0];
            let end = whole.find(".iso").expect("the pattern ends with .iso") + ".iso".len();
            found.push((whole[..end].to_owned(), captures[1].to_owned()));
        }
    }
    let releases: BTreeSet<&str> = found.iter().map(|(_, release)| release.as_str()).collect();
    if releases.len() != 1 {
        return Err(SourceError::Format {
            url: DOWNLOAD_PAGE.to_owned(),
            expected: "links to exactly one Desktop ISO release",
        });
    }
    Ok(found.swap_remove(0))
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.chars().all(|character| character.is_ascii_hexdigit())
}

/// The files of the seed disc. `settings.json` follows the configuration reference of
/// cachyos-cli-installer-new; the disk is the guest's first virtio disk.
pub fn seed_files(public_key: &str) -> Vec<SeedFile> {
    let settings = json!({
        "menus": 1,
        "headless_mode": true,
        "device": "/dev/vda",
        "fs_name": "btrfs",
        "partitions": [
            { "name": "/dev/vda1", "mountpoint": "/boot", "size": "2G", "fs_name": "vfat", "type": "boot" },
            { "name": "/dev/vda2", "mountpoint": "/", "size": "100%", "type": "root" },
        ],
        "hostname": "myconfig-cachyos",
        "locale": "en_US",
        "xkbmap": "us",
        "timezone": "UTC",
        "user_name": GUEST_USER,
        "user_pass": GUEST_PASSWORD,
        "user_shell": "/bin/bash",
        "root_pass": GUEST_PASSWORD,
        "kernel": "linux-cachyos",
        "desktop": "kde",
        "bootloader": "refind",
    });
    vec![
        SeedFile {
            name: "settings.json",
            content: format!("{settings:#}\n"),
            executable: false,
        },
        SeedFile {
            name: "start.sh",
            content: START_SCRIPT.to_owned(),
            executable: true,
        },
        SeedFile {
            name: "post-install.sh",
            content: POST_INSTALL_SCRIPT.replace("@USER@", GUEST_USER),
            executable: true,
        },
        SeedFile {
            name: "authorized_keys",
            content: format!("{public_key}\n"),
            executable: false,
        },
    ]
}

/// Starts the unattended installation in the live session: opens a terminal with the KDE
/// shortcut Ctrl+Alt+T and types the start command, until the start script reports on the
/// serial port that it runs.
///
/// Unverified: no CachyOS live ISO has been installed this way yet. This assumes that the live
/// session logs in to KDE by itself, that Ctrl+Alt+T opens Konsole with the keyboard focus over
/// the welcome window, and that the live user runs `sudo` without a password. If one of these
/// is false, this function is the one place to change.
pub fn start_install(vm: &mut Vm, serial: &Path) -> Result<(), ImageError> {
    wait_for_marker(vm, serial, FIRST_ATTEMPT_DELAY)?;
    for _ in 0..ATTEMPTS {
        if super::serial_contains(serial, STARTED)? {
            return Ok(());
        }
        let mut qmp = vm.qmp()?;
        qmp.send_keys(&["ctrl", "alt", "t"])?;
        thread::sleep(Duration::from_secs(5));
        qmp.type_text(START_COMMAND)?;
        drop(qmp);
        if wait_for_marker(vm, serial, ATTEMPT_WAIT)? {
            return Ok(());
        }
    }
    Err(ImageError::NotStarted {
        serial: serial.to_owned(),
    })
}

/// Waits up to `timeout` for the start marker, and returns whether it appeared.
fn wait_for_marker(vm: &mut Vm, serial: &Path, timeout: Duration) -> Result<bool, ImageError> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        vm.check_running()?;
        if super::serial_contains(serial, STARTED)? {
            return Ok(true);
        }
        thread::sleep(Duration::from_secs(2));
    }
    Ok(false)
}
