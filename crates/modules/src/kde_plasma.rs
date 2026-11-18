//! KDE Plasma with Breeze Dark, the pink accent, IBM Plex fonts, a top bar and a dock.
use std::{fs, path::Path};

use embedded_dotfiles::KDE_PLASMA_ASSETS;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use crate::{
    Applied, Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope, Setting,
    deploy::verify_package,
    plasma_version::require_supported_plasma,
    support::{
        require_executable, require_file, require_programs, user_runtime_directory, verify_packages,
    },
};

pub struct KdePlasma;

/// Ark opens archives from Dolphin and adds its Extract and Compress entries to Dolphin's
/// context menu. It reads and writes 7z archives through the 7-Zip program. Noto CJK
/// draws Chinese, Japanese and Korean text, which the IBM Plex fonts lack.
const PACKAGES: &[Package] = &[
    Package::PlexFonts,
    Package::BlexMonoFont,
    Package::NotoFontsCjk,
    Package::Libinput,
    Package::Ark,
    Package::SevenZip,
];
/// The Plasma desktop, package by package: what `plasma-meta` holds, without the
/// browser integration, and with the disk health monitor, the crash reporter and the
/// Flatpak permissions page.
const PLASMA: &[Package] = &[
    Package::Aurorae,
    Package::Bluedevil,
    Package::Breeze,
    Package::BreezeGtk,
    Package::Kactivitymanagerd,
    Package::KdeCliTools,
    Package::Kdecoration,
    Package::KdeGtkConfig,
    Package::KdeplasmaAddons,
    Package::Kglobalacceld,
    Package::Kinfocenter,
    Package::Kmenuedit,
    Package::Knighttime,
    Package::Kpipewire,
    Package::Kscreen,
    Package::Kscreenlocker,
    Package::Ksystemstats,
    Package::KwalletPam,
    Package::Kwayland,
    Package::Kwin,
    Package::LayerShellQt,
    Package::Libkscreen,
    Package::Libksysguard,
    Package::Libplasma,
    Package::Milou,
    Package::OceanSoundTheme,
    Package::PlasmaActivities,
    Package::PlasmaActivitiesStats,
    Package::PlasmaDesktop,
    Package::PlasmaFirewall,
    Package::PlasmaIntegration,
    Package::PlasmaLoginManager,
    Package::PlasmaNm,
    Package::PlasmaPa,
    Package::PlasmaSystemmonitor,
    Package::PlasmaThunderbolt,
    Package::PlasmaWorkspace,
    Package::PolkitKdeAgent,
    Package::Powerdevil,
    Package::Spectacle,
    Package::Systemsettings,
    Package::Union,
    Package::XdgDesktopPortalKde,
    Package::PlasmaDisks,
    Package::Drkonqi,
    Package::FlatpakKcm,
];
/// The global theme of the `plasma` package: Breeze Dark's settings, with the top bar and
/// dock as the layout that Plasma builds when it starts with no desktops.
const LOOK_AND_FEEL: &str = "inayos.desktop";
/// The pink that Black & Pink uses in the terminal and Zsh.
const ACCENT_COLOR: &str = "255,78,173";
const CURSOR_THEME: &str = "breeze_cursors";
const CURSOR_SIZE: &str = "24";
const LIBINPUT_PLUGINS: &str = "/etc/libinput/plugins";
const LAYOUT_SERVICE: &str = "inayos-plasma-layout.service";
/// Plasma Login Manager, the login screen.
const LOGIN_SERVICE: &str = "plasmalogin.service";
const WINDOW_RULE: &str = "inayos-picture-in-picture";

const LAYOUT_COMMAND: &str = ".local/bin/inayos-plasma-layout";
const PANEL_SCRIPT: &str = ".local/share/kwin/scripts/inayos-panels/metadata.json";

/// The fonts that the `plasma` package's fontconfig file makes the defaults for programs
/// that ask for a kind of font instead of naming one.
const FONT_KINDS: [(&str, &str); 3] = [
    ("sans-serif", "IBM Plex Sans"),
    ("serif", "IBM Plex Serif"),
    ("monospace", "BlexMono Nerd Font Mono"),
];

/// Every KDE key this module sets, with its value. The window rule keys are separate,
/// because the rule list depends on the rules already present.
pub(crate) fn kde_settings() -> Vec<(Setting, &'static str)> {
    let font = "IBM Plex Sans,12,-1,5,50,0,0,0,0,0";
    let fixed = "BlexMono Nerd Font Mono,12,-1,5,50,0,0,0,0,0";
    let small = "IBM Plex Sans,10,-1,5,50,0,0,0,0,0";
    let mut settings = Vec::new();
    let mut add = |file: &str, groups: &[&str], key: &str, value: &'static str| {
        settings.push((Setting::kde(file, groups, key), value));
    };
    for (group, key, value) in [
        ("General", "font", font),
        ("General", "fixed", fixed),
        ("General", "menuFont", font),
        ("General", "toolBarFont", font),
        ("General", "smallestReadableFont", small),
        ("WM", "activeFont", font),
        ("KDE", "AnimationDurationFactor", "0.25"),
        ("KDE", "SingleClick", "true"),
    ] {
        add("kdeglobals", &[group], key, value);
    }
    for (group, key, value) in [
        ("Windows", "PerOutputVirtualDesktops", "true"),
        ("Windows", "ElectricBorderPushbackPixels", "0"),
        ("EdgeBarrier", "CornerBarrier", "false"),
        ("EdgeBarrier", "EdgeBarrier", "0"),
        ("Effect-overview", "BorderActivate", "9"),
        ("Plugins", "inayos-panelsEnabled", "true"),
    ] {
        add("kwinrc", &[group], key, value);
    }
    // Plasma turns on Do Not Disturb while a window is fullscreen, which hides every
    // notification until the window leaves fullscreen.
    add(
        "plasmanotifyrc",
        &["DoNotDisturb"],
        "WhenFullscreen",
        "false",
    );
    add(
        "kglobalshortcutsrc",
        &["services", "org.kde.krunner.desktop"],
        "_launch",
        "Search\tAlt+F2\tMeta+Space",
    );
    add(
        "baloofilerc",
        &["Basic Settings"],
        "Indexing-Enabled",
        "false",
    );
    add("plasmaparc", &["General"], "RaiseMaximumVolume", "true");
    // Closing the lid sleeps even while an external display is connected.
    for state in ["AC", "Battery", "LowBattery"] {
        add(
            "powerdevilrc",
            &[state, "SuspendAndShutdown"],
            "LidAction",
            "1",
        );
        add(
            "powerdevilrc",
            &[state, "SuspendAndShutdown"],
            "InhibitLidActionWhenExternalMonitorPresent",
            "false",
        );
    }
    for (device, key, value) in [
        ("Pointer", "PointerAcceleration", "1.000"),
        ("Pointer", "PointerAccelerationProfile", "1"),
        ("Touchpad", "PointerAcceleration", "1.000"),
        ("Touchpad", "PointerAccelerationProfile", "1"),
        ("Touchpad", "NaturalScroll", "true"),
        ("Touchpad", "TapDragLock", "true"),
        ("Touchpad", "ClickMethod", "2"),
    ] {
        add("kcminputrc", &["Libinput", "Defaults", device], key, value);
    }
    // libinput enables button scrolling on the Wooting Two HE mouse device by default. It
    // holds every middle-button press until release, so no program ever sees the middle
    // button held down.
    add(
        "kcminputrc",
        &[
            "Libinput",
            "12771",
            "4657",
            "Wooting Wooting Two HE (ARM) Mouse",
        ],
        "ScrollMethod",
        "0",
    );
    add("kcminputrc", &["Mouse"], "cursorSize", CURSOR_SIZE);
    // A US layout whose right Alt is the Compose key: press and release it, then type a
    // sequence such as ' e for é. The login and lock screens keep the system layout.
    for (key, value) in [
        ("Use", "true"),
        ("LayoutList", "us"),
        ("ResetOldOptions", "true"),
        ("Options", "compose:ralt"),
    ] {
        add("kxkbrc", &["Layout"], key, value);
    }
    settings
}

/// The picture-in-picture window rule. KWin rule values: `titlematch` 3 is a regular
/// expression, and a rule type of 2 forces the property. Firefox titles its French player
/// "Incrustation vidéo".
fn window_rule_settings() -> Vec<(Setting, &'static str)> {
    [
        (
            "Description",
            "Hide picture-in-picture windows from screen capture",
        ),
        ("title", "(?i)picture[ -]?in[ -]?picture|incrustation vidéo"),
        ("titlematch", "3"),
        ("excludefromcapture", "true"),
        ("excludefromcapturerule", "2"),
    ]
    .into_iter()
    .map(|(key, value)| (Setting::kde("kwinrulesrc", &[WINDOW_RULE], key), value))
    .collect()
}

fn rule_list() -> Setting {
    Setting::kde("kwinrulesrc", &["General"], "rules")
}

fn configure_window_rules(ctx: &Context) -> ModuleResult {
    for (setting, value) in window_rule_settings() {
        ctx.set(setting, value)?;
    }
    let current = rule_list().read(ctx)?.unwrap_or_default();
    let mut rules: Vec<&str> = current.split(',').filter(|rule| !rule.is_empty()).collect();
    if !rules.contains(&WINDOW_RULE) {
        rules.push(WINDOW_RULE);
    }
    ctx.set(rule_list(), &rules.join(","))?;
    ctx.set(
        Setting::kde("kwinrulesrc", &["General"], "count"),
        &rules.len().to_string(),
    )
}

/// Sets the GTK cursor size inside files that already have a size line. Plasma's theme
/// tool updates the KDE and X11 settings, but leaves these generated GTK settings at the
/// old size in an existing session.
pub(crate) fn set_gtk_cursor_size(ctx: &Context) -> ModuleResult {
    for (relative, prefix) in [
        (".gtkrc-2.0", "gtk-cursor-theme-size="),
        (".config/gtk-3.0/settings.ini", "gtk-cursor-theme-size="),
        (".config/gtk-4.0/settings.ini", "gtk-cursor-theme-size="),
        (".config/xsettingsd/xsettingsd.conf", "Gtk/CursorThemeSize "),
    ] {
        let file = ctx.home.join(relative);
        if !file.is_file() {
            continue;
        }
        let original = fs::read_to_string(&file)?;
        let updated: String = original
            .split_inclusive('\n')
            .map(|line| {
                let content = line.trim_end_matches('\n');
                match content.strip_prefix(prefix) {
                    Some(size)
                        if !size.is_empty() && size.bytes().all(|byte| byte.is_ascii_digit()) =>
                    {
                        format!("{prefix}{CURSOR_SIZE}{}", &line[content.len()..])
                    }
                    _ => line.to_owned(),
                }
            })
            .collect();
        ctx.write_file(&file, updated.as_bytes(), false)?;
    }
    Ok(())
}

fn configure_cursor(ctx: &Context) -> ModuleResult {
    ctx.set(Setting::CursorTheme, CURSOR_THEME)?;
    set_gtk_cursor_size(ctx)?;
    // Without dconf, GSettings keeps no value between sessions.
    if ctx.find_program("gsettings").is_ok()
        && ctx.find_program("dconf").is_ok()
        && ctx
            .read(cmd!(
                ctx.shell,
                "gsettings list-keys org.gnome.desktop.interface"
            ))
            .is_ok_and(|keys| keys.lines().any(|key| key == "cursor-size"))
    {
        ctx.set(
            Setting::Gsettings {
                schema: "org.gnome.desktop.interface".to_owned(),
                key: "cursor-size".to_owned(),
            },
            CURSOR_SIZE,
        )?;
    }
    Ok(())
}

fn require_default_fonts(ctx: &Context) -> ModuleResult {
    let format = "%{family[0]}";
    for (kind, family) in FONT_KINDS {
        let resolved = ctx.read(cmd!(ctx.shell, "fc-match -f {format} {kind}"))?;
        if resolved != family {
            return Err(format!("{kind} resolves to {resolved} instead of {family}").into());
        }
    }
    Ok(())
}

/// The libinput plugins, as their installed path and contents.
fn libinput_plugins() -> Vec<(std::path::PathBuf, &'static [u8])> {
    KDE_PLASMA_ASSETS
        .libinput
        .files()
        .into_iter()
        .filter_map(|file| {
            let name = Path::new(file.path_from_root).file_name()?;
            Some((Path::new(LIBINPUT_PLUGINS).join(name), file.content))
        })
        .collect()
}

impl Module for KdePlasma {
    fn name(&self) -> &'static str {
        "kde-plasma"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("plasma-version")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: [PLASMA, PACKAGES].concat(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        require_supported_plasma(ctx)?;
        ctx.install_packages(PLASMA)?;
        ctx.install_packages(PACKAGES)?;
        require_programs(
            ctx,
            &[
                "plasma-apply-colorscheme",
                "plasma-apply-cursortheme",
                "plasma-apply-lookandfeel",
                "kwriteconfig6",
                "kreadconfig6",
                "qdbus6",
                "fc-match",
                "systemctl",
                "sudo",
            ],
        )?;
        ctx.enable_service(LOGIN_SERVICE, ServiceScope::System)?;
        ctx.deploy_config("plasma")?;
        let home = ctx.home;
        require_executable(&home.join(LAYOUT_COMMAND))?;
        require_file(&home.join(PANEL_SCRIPT))?;

        for (path, contents) in libinput_plugins() {
            ctx.write_system_file(&path, contents, "0644")?;
        }
        require_default_fonts(ctx)?;

        ctx.set(Setting::LookAndFeel, LOOK_AND_FEEL)?;
        ctx.set(Setting::AccentColor, ACCENT_COLOR)?;
        for (setting, value) in kde_settings() {
            ctx.set(setting, value)?;
        }
        configure_window_rules(ctx)?;
        configure_cursor(ctx)?;

        let runtime = user_runtime_directory(ctx)?;
        // The running Plasma session is reached through its runtime folder and bus.
        let session = || {
            let bus = sh
                .var_os("DBUS_SESSION_BUS_ADDRESS")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| format!("unix:path={}/bus", runtime.display()).into());
            (
                sh.push_env("XDG_RUNTIME_DIR", &runtime),
                sh.push_env("DBUS_SESSION_BUS_ADDRESS", bus),
            )
        };
        {
            let _session = session();
            ctx.reload_user_manager()?;
            // The installed system's KWin reads the new settings when it starts.
            let _ = ctx.on_running_system("reconfigure the running KWin", || {
                if ctx.succeeds(cmd!(sh, "qdbus6 org.kde.KWin /KWin"))? {
                    ctx.run(cmd!(
                        sh,
                        "qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure"
                    ))?;
                }
                Ok(())
            })?;
        }

        // The layout script rewrites the panel layout and records its own version.
        ctx.record_setting(Setting::PlasmaPanels {
            runtime: runtime.to_string_lossy().into_owned(),
        })?;
        ctx.record_path(&crate::state::state_directory(home).join("plasma-layout-version"))?;
        // Undone before the panels, so the layout service cannot replace the restored
        // panels.
        ctx.enable_service(LAYOUT_SERVICE, ServiceScope::User)?;
        let layout = home.join(LAYOUT_COMMAND);
        let applied = ctx.on_running_system("apply the KDE Plasma top bar and dock", || {
            let _session = session();
            match ctx.run_status(cmd!(sh, "{layout}").env("INAYOS_PLASMA_ENSURE_OVERVIEW", "1"))? {
                Some(0) => {
                    ctx.note("Applied the KDE Plasma top bar and dock");
                    Ok(())
                }
                // The layout script exits with 75 when Plasma is not running.
                Some(75) => {
                    ctx.note(
                        "KDE Plasma is not active; the top bar and dock will apply at the next KDE Plasma login",
                    );
                    Ok(())
                }
                code => Err(format!("KDE Plasma layout failed with exit code {code:?}").into()),
            }
        })?;
        if applied == Applied::Deferred {
            ctx.note("The top bar and dock will apply at the first KDE Plasma login");
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        require_supported_plasma(ctx)?;
        verify_packages(ctx, PLASMA)?;
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "plasma")?;
        // The plugins are system files, so root checks them, as it wrote them.
        for (path, _) in libinput_plugins() {
            if !ctx.succeeds(cmd!(ctx.shell, "sudo test -f {path}"))? {
                return Err(format!("required file is missing: {}", path.display()).into());
            }
        }
        require_default_fonts(ctx)?;
        for (setting, value) in [
            (Setting::LookAndFeel, LOOK_AND_FEEL),
            (Setting::AccentColor, ACCENT_COLOR),
            (Setting::CursorTheme, CURSOR_THEME),
        ]
        .into_iter()
        .chain(kde_settings())
        .chain(window_rule_settings())
        {
            if setting.read(ctx)?.as_deref() != Some(value) {
                return Err(format!("{} is not {value}", setting.describe()).into());
            }
        }
        let rules = rule_list().read(ctx)?.unwrap_or_default();
        if !rules.split(',').any(|rule| rule == WINDOW_RULE) {
            return Err(format!("the KWin rule list lacks {WINDOW_RULE}").into());
        }
        if !ctx
            .service_state(LOGIN_SERVICE, ServiceScope::System)?
            .enabled
        {
            return Err(format!("{LOGIN_SERVICE} is not enabled").into());
        }
        ctx.run(cmd!(
            ctx.shell,
            "systemctl --user --quiet is-enabled {LAYOUT_SERVICE}"
        ))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
