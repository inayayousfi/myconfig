//! Installs the base system into `/mnt` and sets its locale, keymaps, time zone, host name and
//! network. On an encrypted layout it also writes the files that open the volumes at boot.
use std::path::Path;

use super::{
    Machine, TARGET,
    encryption::{self, Volumes},
};
use crate::{Answers, Error, host::Command, parse};

const BASE_PACKAGES: [&str; 8] = [
    "base",
    "linux",
    "linux-firmware",
    "btrfs-progs",
    "e2fsprogs",
    "dosfstools",
    "sudo",
    "networkmanager",
];

const CONNECTIONS: &str = "/etc/NetworkManager/system-connections";

/// The system keyboard layout as `localectl set-x11-keymap` writes it, which systemd-localed
/// reads at boot. The Plasma session does not follow it: the kde-plasma module sets each
/// user's layout to US in `kxkbrc`.
pub(crate) fn keyboard_conf(layout: &str) -> String {
    format!(
        "# Written by the InayOS installer. `localectl set-x11-keymap` replaces it.\n\
         Section \"InputClass\"\n\
         \x20       Identifier \"system-keyboard\"\n\
         \x20       MatchIsKeyboard \"on\"\n\
         \x20       Option \"XkbLayout\" \"{layout}\"\n\
         EndSection\n"
    )
}

pub(crate) fn install(
    m: &Machine,
    answers: &Answers,
    volumes: Option<&Volumes>,
) -> Result<(), Error> {
    m.sink.step("Installing the base system");
    let cpuinfo = m.read(Path::new("/proc/cpuinfo"))?;
    let mut packages = vec![BASE_PACKAGES[0], BASE_PACKAGES[1], BASE_PACKAGES[2]];
    packages.extend(parse::microcode(&cpuinfo));
    packages.extend(&BASE_PACKAGES[3..]);
    m.run(
        Command::action("pacstrap")
            .args(["-K", TARGET])
            .args(packages),
    )?;

    let fstab = m.query(Command::query("genfstab").args(["-U", TARGET]))?;
    m.write(&Path::new(TARGET).join("etc/fstab"), &fstab, 0o644)?;
    if let Some(volumes) = volumes {
        encryption::write_boot_files(m, volumes)?;
    }

    m.sink
        .step("Setting the language, keyboard, time zone and computer name");
    m.run(Command::action("systemd-firstboot").args([
        format!("--root={TARGET}"),
        "--force".to_owned(),
        format!("--locale={}", answers.language),
        format!("--keymap={}", answers.keyboard),
        format!("--timezone={}", answers.timezone),
        format!("--hostname={}", answers.hostname),
    ]))?;
    let locale_gen = Path::new(TARGET).join("etc/locale.gen");
    let enabled =
        parse::enable_locale(&m.read(&locale_gen)?, &answers.language).ok_or_else(|| {
            Error::UnknownLocale {
                locale: answers.language.clone(),
            }
        })?;
    m.write(&locale_gen, &enabled, 0o644)?;
    m.run(Command::action("arch-chroot").args([TARGET, "locale-gen"]))?;
    if let Some(layout) = &answers.xkb_layout {
        let folder = Path::new(TARGET).join("etc/X11/xorg.conf.d");
        m.create_dir(&folder, 0o755)?;
        m.write(
            &folder.join("00-keyboard.conf"),
            &keyboard_conf(layout),
            0o644,
        )?;
    }

    m.sink.step("Turning on NetworkManager");
    m.run(Command::action("systemctl").args([
        &format!("--root={TARGET}"),
        "enable",
        "NetworkManager.service",
    ]))?;
    let live = Path::new(CONNECTIONS);
    if m.is_dir(live) {
        let copied = Path::new(TARGET).join(CONNECTIONS.trim_start_matches('/'));
        m.create_dir(&copied, 0o700)?;
        for name in m.list(live)? {
            let from = live.join(&name);
            if !m.is_dir(&from) {
                m.copy(&from, &copied.join(&name), 0o600)?;
            }
        }
    }
    Ok(())
}
