//! The chores that Arch leaves to the user: unread Arch news that announces a manual step
//! shows before each upgrade and at login, a notification names the `.pacnew` files and
//! opens them for merging, and reflector ranks the mirrors every week.
//!
//! The hooks, units, scripts and the news configuration are under
//! `dotfiles/assets/maintenance`, installed at the same path under `/`. The reflector
//! configuration names the country of the system's time zone, so the module writes it.
use std::path::{Path, PathBuf};

use embedded_dotfiles::MAINTENANCE_ASSETS;
use typed_fs_rs::{EmbeddedDirectory, EmbeddedFile};
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    support::{read_system_bytes, read_system_file, verify_packages},
};

pub struct Maintenance;

impl Maintenance {
    pub const NAME: &'static str = "maintenance";
}

/// `inayos`, whose package holds `inayos-news`; curl, which `inayos-news` fetches the feed
/// with; pacdiff from pacman-contrib; reflector; Kompare, the diff program for `.pacnew`
/// files; `notify-send`; and Ghostty, the terminal that the notifications open.
const PACKAGES: &[Package] = &[
    Package::Inayos,
    Package::Curl,
    Package::PacmanContrib,
    Package::Reflector,
    Package::Kompare,
    Package::Libnotify,
    Package::Ghostty,
];
/// The program that the news hook and the login notification run.
const NEWS: &str = "/usr/bin/inayos-news";
/// The folder of the news read mark. Root and the wheel group own it, and new files in it
/// get the wheel group, so the user marks the news read without sudo.
const NEWS_FOLDER: &str = "/var/lib/inayos-news";
const NEWS_FOLDER_MODE: &str = "2775:wheel";
/// The publication time of the newest item read, in seconds since 1970 UTC.
const NEWS_READ: &str = "/var/lib/inayos-news/read";
/// The folder where `inayos-pacnew.service` keeps the list of `.pacnew` files.
const PACNEW_FOLDER: &str = "/var/lib/inayos-pacnew";
const REFLECTOR_CONF: &str = "/etc/xdg/reflector/reflector.conf";
const LOCALTIME: &str = "/etc/localtime";
/// tzdata's table of time zones, one country per line.
const ZONE_TABLE: &str = "/usr/share/zoneinfo/zone.tab";
const REFLECTOR_TIMER: &str = "reflector.timer";
const PACNEW_TIMER: &str = "inayos-pacnew.timer";
const NEWS_NOTIFY: &str = "inayos-news-notify.service";
const PACNEW_NOTIFY: &str = "inayos-pacnew-notify.path";
/// The embedded files that are programs.
const PROGRAM_FOLDER: &str = "usr/local/libexec/";

/// Each embedded file with the absolute path it is installed at and its mode.
fn files() -> Vec<(PathBuf, &'static EmbeddedFile, &'static str)> {
    MAINTENANCE_ASSETS
        .files()
        .into_iter()
        .map(|file| {
            let mode = if file.path_from_root.starts_with(PROGRAM_FOLDER) {
                "0755"
            } else {
                "0644"
            };
            (Path::new("/").join(file.path_from_root), file, mode)
        })
        .collect()
}

fn is_current(ctx: &Context, path: &Path, content: &[u8]) -> ModuleResult<bool> {
    let path = path.to_str().ok_or("a maintenance path is not UTF-8")?;
    Ok(read_system_bytes(ctx, path)?.as_deref() == Some(content))
}

/// The time zone that `/etc/localtime` links to, such as `Europe/Paris`.
fn zone_of_link(target: &str) -> Option<&str> {
    let (_, zone) = target.trim().split_once("zoneinfo/")?;
    (!zone.is_empty()).then_some(zone)
}

/// The country code of `zone` in tzdata's `zone.tab`, when the table gives it exactly one.
fn country_of(zone: &str, table: &str) -> Option<String> {
    let mut countries: Vec<&str> = table
        .lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let code = fields.next()?;
            let _coordinates = fields.next()?;
            (fields.next()? == zone).then_some(code)
        })
        .collect();
    countries.dedup();
    match countries.as_slice() {
        [code] => Some((*code).to_owned()),
        _ => None,
    }
}

/// The reflector options: the 20 most recently synced HTTPS mirrors, of `country` when
/// there is one, sorted by download rate, saved as pacman's mirror list.
fn reflector_conf(country: Option<&str>) -> String {
    let mut conf = String::from(
        "# InayOS maintenance: reflector.timer runs reflector with these options every week.\n\
         --save /etc/pacman.d/mirrorlist\n\
         --protocol https\n",
    );
    if let Some(country) = country {
        conf.push_str(&format!(
            "# The country of the system's time zone.\n--country {country}\n"
        ));
    }
    conf.push_str("--latest 20\n--sort rate\n");
    conf
}

/// Whether `text` holds the reflector options that install writes for some time zone: the
/// time zone can change after install without breaking anything.
fn is_reflector_conf(text: &str) -> bool {
    let country = text
        .lines()
        .find_map(|line| line.strip_prefix("--country "))
        .map(str::trim);
    text == reflector_conf(country)
}

/// The reflector options for this system's time zone.
fn wanted_reflector_conf(ctx: &Context) -> ModuleResult<String> {
    let (linked, target) = ctx.read_unchecked(cmd!(ctx.shell, "readlink {LOCALTIME}"))?;
    let table = std::fs::read_to_string(ZONE_TABLE).unwrap_or_default();
    let country = linked
        .then(|| zone_of_link(&target))
        .flatten()
        .and_then(|zone| country_of(zone, &table));
    Ok(reflector_conf(country.as_deref()))
}

fn exists(ctx: &Context, path: &str) -> ModuleResult<bool> {
    ctx.succeeds(cmd!(ctx.shell, "sudo test -e {path}"))
}

fn seconds_since_1970() -> ModuleResult<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

impl Module for Maintenance {
    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        // The repository module installs the inayos package, which holds inayos-news.
        &[Need::Hard("base"), Need::Hard("inayos-repository")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.install_packages(PACKAGES)?;
        if !exists(ctx, NEWS_FOLDER)? {
            ctx.created(Path::new(NEWS_FOLDER), true)?;
            ctx.run(cmd!(
                sh,
                "sudo install -d -m 2775 -o root -g wheel {NEWS_FOLDER}"
            ))?;
        }
        // The system starts with current packages, so the news published before it
        // announces nothing left to do.
        if !exists(ctx, NEWS_READ)? {
            let now = format!("{}\n", seconds_since_1970()?);
            ctx.write_system_file(Path::new(NEWS_READ), now.as_bytes(), "0664")?;
        }
        if !exists(ctx, PACNEW_FOLDER)? {
            // inayos-pacnew.service creates it, and remove deletes it with its list.
            ctx.created(Path::new(PACNEW_FOLDER), true)?;
        }
        for (path, file, mode) in files() {
            ctx.write_system_file(&path, file.content, mode)?;
        }
        let conf = wanted_reflector_conf(ctx)?;
        ctx.write_system_file(Path::new(REFLECTOR_CONF), conf.as_bytes(), "0644")?;
        for timer in [REFLECTOR_TIMER, PACNEW_TIMER] {
            ctx.enable_service(timer, ServiceScope::System)?;
            let _ = ctx.start_service(timer, ServiceScope::System)?;
        }
        ctx.enable_service(NEWS_NOTIFY, ServiceScope::User)?;
        ctx.enable_service(PACNEW_NOTIFY, ServiceScope::User)?;
        // Without a desktop, notify-send has nowhere to show the notification. The units
        // start with the next graphical session.
        if ctx.graphical_session()? {
            ctx.reload_user_manager()?;
            let _ = ctx.start_service(PACNEW_NOTIFY, ServiceScope::User)?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        if !ctx.succeeds(cmd!(sh, "test -x {NEWS}"))? {
            return Err(
                format!("{NEWS} is missing, so the news hook fails before every upgrade").into(),
            );
        }
        for (path, file, _) in files() {
            if !is_current(ctx, &path, file.content)? {
                return Err(format!(
                    "{} does not hold the InayOS maintenance file",
                    path.display()
                )
                .into());
            }
        }
        if !read_system_file(ctx, REFLECTOR_CONF)?
            .as_deref()
            .is_some_and(is_reflector_conf)
        {
            return Err(format!("{REFLECTOR_CONF} does not hold the InayOS mirror options").into());
        }
        let (found, mode) = ctx.read_unchecked(cmd!(sh, "stat -c %a:%G {NEWS_FOLDER}"))?;
        if !found || mode.trim() != NEWS_FOLDER_MODE {
            return Err(format!(
                "{NEWS_FOLDER} is not a folder of mode 2775 and group wheel, so the user cannot mark the news read"
            )
            .into());
        }
        for timer in [REFLECTOR_TIMER, PACNEW_TIMER] {
            let _ = ctx.require_active(timer, ServiceScope::System)?;
        }
        for unit in [NEWS_NOTIFY, PACNEW_NOTIFY] {
            if !ctx.service_state(unit, ServiceScope::User)?.enabled {
                return Err(format!("{unit} is not enabled").into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lines of tzdata's zone.tab.
    const ZONE_TAB: &str = "# tzdb timezone descriptions\n\
        FR\t+4852+00220\tEurope/Paris\n\
        DE\t+5230+01322\tEurope/Berlin\tmost of Germany\n\
        CH\t+4723+00832\tEurope/Zurich\n\
        US\t+404251-0740023\tAmerica/New_York\tEastern (most areas)\n";

    #[test]
    fn the_time_zone_names_the_country_of_the_mirrors() {
        let zone = zone_of_link("/usr/share/zoneinfo/Europe/Paris\n").unwrap();
        assert_eq!(zone, "Europe/Paris");
        assert_eq!(
            zone_of_link("../usr/share/zoneinfo/America/New_York"),
            Some("America/New_York")
        );
        assert_eq!(country_of(zone, ZONE_TAB).as_deref(), Some("FR"));
        assert_eq!(
            country_of("America/New_York", ZONE_TAB).as_deref(),
            Some("US")
        );
    }

    #[test]
    fn a_zone_without_one_country_names_none() {
        assert_eq!(country_of("UTC", ZONE_TAB), None);
        assert_eq!(
            country_of(
                "Europe/Paris",
                "FR\t+4852+00220\tEurope/Paris\nMC\t+4342+00723\tEurope/Paris\n"
            ),
            None
        );
        assert_eq!(zone_of_link("/etc/other"), None);
    }

    #[test]
    fn the_mirror_options_name_the_country_only_when_there_is_one() {
        assert_eq!(
            reflector_conf(Some("FR")),
            "# InayOS maintenance: reflector.timer runs reflector with these options every week.\n\
             --save /etc/pacman.d/mirrorlist\n--protocol https\n\
             # The country of the system's time zone.\n--country FR\n\
             --latest 20\n--sort rate\n"
        );
        assert!(!reflector_conf(None).contains("--country"));
    }

    #[test]
    fn verify_takes_the_mirror_options_of_any_time_zone() {
        assert!(is_reflector_conf(&reflector_conf(Some("DE"))));
        assert!(is_reflector_conf(&reflector_conf(None)));
        assert!(!is_reflector_conf("--save /etc/pacman.d/mirrorlist\n"));
        assert!(!is_reflector_conf(
            &reflector_conf(Some("FR")).replace("--latest 20", "--latest 5")
        ));
    }
}
