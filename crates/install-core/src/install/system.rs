//! Installs the base system and the InayOS packages into `/mnt` and sets its locale, keymaps,
//! time zone, host name and network. On an encrypted layout it also writes the files that
//! open the volumes at boot.
use std::path::Path;

use super::{
    Machine, Settings, TARGET,
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

/// The packages of the InayOS repository, which come from the stick's local copy of it.
const INAYOS_PACKAGES: [&str; 2] = ["inayos-keyring", "inayos"];
/// The database file that `repo-add` writes for the repository named `inayos`.
pub(crate) const REPOSITORY_DATABASE: &str = "inayos.db";
/// The unit of the live system that fills pacman's keyring at boot.
const PACMAN_INIT_UNIT: &str = "/etc/systemd/system/pacman-init.service";
/// The pacman configuration of the install's `pacstrap`, on the live system.
const PACSTRAP_CONFIG: &str = "/tmp/inayos-pacstrap.conf";

const CONNECTIONS: &str = "/etc/NetworkManager/system-connections";

/// The live system's pacman configuration with the stick's local InayOS repository added.
/// The stick is the trust root of everything it installs, the installer included, so its
/// local repository is not signed. `Optional TrustAll` applies to this file only, which only
/// this `pacstrap` reads: the new system's own configuration names the signed repository on
/// GitHub.
pub(crate) fn pacstrap_config(live: &str, repository: &Path) -> String {
    let mut config = live.to_owned();
    if !config.is_empty() && !config.ends_with('\n') {
        config.push('\n');
    }
    config.push_str(&format!(
        "\n[inayos]\nSigLevel = Optional TrustAll\nServer = file://{}\n",
        repository.display()
    ));
    config
}

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
    settings: &Settings,
    volumes: Option<&Volumes>,
) -> Result<(), Error> {
    m.sink.step("Installing the base system and InayOS");
    // The live system fills pacman's keyring at boot, and pacstrap checks every package
    // against it. Starting the unit waits for it, and does nothing once it has run.
    if m.host.exists(Path::new(PACMAN_INIT_UNIT)) {
        m.run(Command::action("systemctl").args(["start", "pacman-init.service"]))?;
    }
    let cpuinfo = m.read(Path::new("/proc/cpuinfo"))?;
    let mut packages = vec![BASE_PACKAGES[0], BASE_PACKAGES[1], BASE_PACKAGES[2]];
    packages.extend(parse::microcode(&cpuinfo));
    packages.extend(&BASE_PACKAGES[3..]);
    packages.extend(INAYOS_PACKAGES);
    let config = Path::new(PACSTRAP_CONFIG);
    let live = m.read(Path::new("/etc/pacman.conf"))?;
    m.write(config, &pacstrap_config(&live, &settings.repository), 0o644)?;
    let installed = m.run(
        Command::action("pacstrap")
            .args(["-K", "-C", PACSTRAP_CONFIG, TARGET])
            .args(packages),
    );
    let removed = m.remove_file(config);
    installed?;
    removed?;

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
