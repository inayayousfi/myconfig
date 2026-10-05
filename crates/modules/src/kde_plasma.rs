//! The Black & Pink KDE Plasma look, panels, input settings and Glass effect.
use std::{fs, path::Path};

use embedded_dotfiles::DOTFILES;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope, Setting,
    deploy::verify_package,
    plasma_version::require_supported_plasma,
    support::{
        require_executable, require_file, require_programs, user_runtime_directory, verify_packages,
    },
};

pub struct KdePlasma;

const PACKAGES: &[Package] = &[
    Package::IosevkaFont,
    Package::DesktopFileUtils,
    Package::Libinput,
];
const LOOK_AND_FEEL: &str = "org.myconfig.blacknpink.desktop";
const CURSOR_THEME: &str = "blacknpink-crosshair";
const CURSOR_SIZE: &str = "40";
const GLASS_PACKAGE: &str = "myconfig-kde-glass";
const GLASS_DATA: &str = "/usr/share/myconfig/kde-glass";
const POINTER_PLUGIN: &str = "/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua";
const GLASS_SERVICE: &str = "myconfig-kde-plasma-glass.service";

const REQUIRED_FILES: [&str; 12] = [
    ".local/bin/myconfig-kde-plasma-glass-repair",
    ".local/share/color-schemes/BlackPink.colors",
    ".local/share/plasma/desktoptheme/blacknpink/metadata.json",
    ".local/share/plasma/desktoptheme/blacknpink/widgets/panel-background.svg",
    ".local/share/plasma/desktoptheme/blacknpink/dialogs/background.svg",
    ".local/share/plasma/desktoptheme/blacknpink/solid/dialogs/background.svg",
    ".local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop/metadata.json",
    ".local/share/plasma/look-and-feel/org.myconfig.blacknpink.desktop/contents/defaults",
    ".local/share/kwin/scripts/myconfig-plasma-panels/metadata.json",
    ".local/share/kwin/scripts/myconfig-plasma-panels/contents/code/main.js",
    ".config/systemd/user/myconfig-kde-plasma-layout.service",
    ".config/systemd/user/myconfig-kde-plasma-glass.service",
];

/// Every KDE key this module sets, with its value.
pub(crate) fn kde_settings() -> Vec<(Setting, &'static str)> {
    let font = "Iosevka Nerd Font,12,-1,5,50,0,0,0,0,0";
    let small = "Iosevka Nerd Font,10,-1,5,50,0,0,0,0,0";
    let mut settings = Vec::new();
    for (group, key, value) in [
        ("General", "font", font),
        ("General", "fixed", font),
        ("General", "menuFont", font),
        ("General", "toolBarFont", font),
        ("General", "smallestReadableFont", small),
        ("WM", "activeFont", font),
    ] {
        settings.push((Setting::kde("kdeglobals", &[group], key), value));
    }
    for (group, key, value) in [
        ("Windows", "PerOutputVirtualDesktops", "true"),
        ("Windows", "ElectricBorderPushbackPixels", "0"),
        ("EdgeBarrier", "CornerBarrier", "false"),
        ("EdgeBarrier", "EdgeBarrier", "0"),
        ("Effect-overview", "BorderActivate", "9"),
    ] {
        settings.push((Setting::kde("kwinrc", &[group], key), value));
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
        settings.push((
            Setting::kde("kcminputrc", &["Libinput", "Defaults", device], key),
            value,
        ));
    }
    settings.push((
        Setting::kde("kcminputrc", &["Mouse"], "cursorSize"),
        CURSOR_SIZE,
    ));
    settings
}

fn glass_package_version() -> ModuleResult<String> {
    let content = std::str::from_utf8(DOTFILES.assets.kde_plasma.kde_glass.PKGBUILD.content)?;
    let value = |key: &str| -> ModuleResult<&str> {
        content
            .lines()
            .find_map(|line| line.strip_prefix(key))
            .filter(|value| {
                !value.is_empty()
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b".-_".contains(&byte))
            })
            .ok_or_else(|| format!("Glass PKGBUILD has no simple {key} value").into())
    };
    Ok(format!(
        "{} {}-{}",
        value("pkgname=")?,
        value("pkgver=")?,
        value("pkgrel=")?
    ))
}

fn glass_effect_id() -> ModuleResult<String> {
    let effect = fs::read_to_string(Path::new(GLASS_DATA).join("effect-id"))?;
    let effect = effect.trim();
    if !effect.starts_with("myconfig_glass_")
        || !effect
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(format!("invalid Glass effect identifier: {effect}").into());
    }
    Ok(effect.to_owned())
}

fn plugin_enabled(effect: &str) -> Setting {
    Setting::kde("kwinrc", &["Plugins"], &format!("{effect}Enabled"))
}

/// Builds the Glass KWin effect for the installed KWin and turns it on in place of Blur.
fn install_glass(ctx: &Context) -> ModuleResult {
    let sh = ctx.shell;
    let data = Path::new(GLASS_DATA);
    let previous = fs::read_to_string(data.join("effect-id")).unwrap_or_default();
    let built_for = fs::read_to_string(data.join("kwin-version")).unwrap_or_default();
    let kwin = ctx.read(cmd!(sh, "pacman -Q kwin"))?;
    let expected = glass_package_version()?;
    let (was_installed, installed) = ctx.read_unchecked(cmd!(sh, "pacman -Q {GLASS_PACKAGE}"))?;
    if !was_installed || installed != expected || built_for.trim() != kwin {
        ctx.find_program("makepkg")?;
        let build = crate::context::temporary_path("glass");
        fs::create_dir(&build)?;
        let result = (|| -> ModuleResult {
            for file in DOTFILES.assets.kde_plasma.kde_glass.files() {
                let name = Path::new(file.path_from_root)
                    .file_name()
                    .ok_or("Glass resource has no name")?;
                myconfig_utils::install_embedded_file(file, &build.join(name))?;
            }
            let _cwd = sh.push_dir(&build);
            ctx.run(cmd!(sh, "makepkg --syncdeps --noconfirm"))?;
            let mut packages = fs::read_dir(&build)?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name().is_some_and(|name| {
                        let name = name.to_string_lossy();
                        name.starts_with("myconfig-kde-glass-") && name.ends_with(".pkg.tar.zst")
                    })
                });
            let package = packages.next().ok_or("Glass build produced no package")?;
            if packages.next().is_some() {
                return Err("Glass build produced multiple matching packages".into());
            }
            if !was_installed {
                ctx.local_package_installed(GLASS_PACKAGE)?;
            }
            let elevate = sh
                .var_os("MYCONFIG_GLASS_ELEVATE")
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| "sudo".into());
            ctx.run(cmd!(sh, "{elevate} pacman -U --noconfirm {package}"))
        })();
        if let Err(error) = result {
            return Err(format!(
                "Glass build failed; build files retained in {}: {error}",
                build.display()
            )
            .into());
        }
        fs::remove_dir_all(build)?;
    }
    let effect = glass_effect_id()?;
    let config = std::str::from_utf8(
        DOTFILES
            .kde_plasma
            ._local
            .share
            .myconfig
            .kde_plasma
            .glass_conf
            .content,
    )?;
    for line in config.lines() {
        if line.is_empty() || line.starts_with('[') {
            continue;
        }
        let (key, value) = line.split_once('=').ok_or("invalid Glass setting")?;
        ctx.set(Setting::kde("kwinrc", &["Effect-blurplus"], key), value)?;
    }
    for old in ["blur", "glass", "myconfig_glass", previous.trim()] {
        if !old.is_empty() && old != effect {
            ctx.set(plugin_enabled(old), "false")?;
        }
    }
    ctx.set(plugin_enabled(&effect), "true")
}

/// Loads Glass into the running KWin, falling back to Blur when KWin rejects it.
fn activate_glass(ctx: &Context) -> ModuleResult {
    let sh = ctx.shell;
    let effect = glass_effect_id()?;
    let loaded = ctx.read(cmd!(
        sh,
        "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadedEffects"
    ))?;
    for old in loaded.lines().filter(|name| {
        matches!(*name, "blur" | "glass" | "myconfig_glass") || name.starts_with("myconfig_glass_")
    }) {
        ctx.run(cmd!(
            sh,
            "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.unloadEffect {old}"
        ))?;
        if old != effect {
            ctx.set(plugin_enabled(old), "false")?;
        }
    }
    let loaded = ctx.read(cmd!(
        sh,
        "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect {effect}"
    ));
    let ready = matches!(loaded.as_deref(), Ok("true"))
        && ctx
            .run(cmd!(
                sh,
                "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.reconfigureEffect {effect}"
            ))
            .is_ok()
        && ctx
            .read(cmd!(
                sh,
                "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.debug {effect} ''"
            ))
            .is_ok_and(|result| result.starts_with("valid=1 shaders=1 "));
    if ready {
        return Ok(());
    }
    let _ = ctx.succeeds(cmd!(
        sh,
        "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.unloadEffect {effect}"
    ));
    ctx.set(plugin_enabled(&effect), "false")?;
    if matches!(
        ctx.read(cmd!(
            sh,
            "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect blur"
        ))
        .as_deref(),
        Ok("true")
    ) {
        ctx.set(plugin_enabled("blur"), "true")?;
    }
    Err(format!("KWin could not initialize Glass: {effect}").into())
}

/// Sets the GTK cursor size inside files that already have a size line.
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

fn configure_appearance(ctx: &Context) -> ModuleResult {
    for (setting, value) in kde_settings() {
        ctx.set(setting, value)?;
    }
    require_file(
        &ctx.home
            .join(".local/share/icons/blacknpink-crosshair/cursors/default"),
    )?;
    ctx.set(Setting::CursorTheme, CURSOR_THEME)?;
    set_gtk_cursor_size(ctx)?;
    if ctx.find_program("gsettings").is_ok()
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
    ctx.set(plugin_enabled("myconfig-plasma-panels"), "true")
}

fn kwin_running(ctx: &Context) -> ModuleResult<bool> {
    ctx.succeeds(cmd!(ctx.shell, "qdbus6 org.kde.KWin /KWin"))
}

/// Plasma files whose shape must agree with the Glass shader.
const GLASS_CLIENTS: [&str; 3] = [
    ".local/share/plasma/plasmoids/myconfig.island/contents/ui/main.qml",
    ".local/share/plasma/desktoptheme/blacknpink/widgets/panel-background.svg",
    ".local/share/plasma/desktoptheme/blacknpink/dialogs/background.svg",
];

/// Rebuilds Glass for the running KWin and reloads it. The Glass repair user service runs
/// this at each Plasma login through `myconfig internal kde-plasma repair-glass`, so a
/// KWin update never leaves a Glass build made for the previous version.
pub fn repair_glass(ctx: &Context) -> ModuleResult {
    ctx.state()
        .borrow_mut()
        .begin(crate::state::Action::Install)?;
    ctx.set_module("kde-plasma");
    let sh = ctx.shell;
    let repaired = install_glass(ctx).and_then(|()| {
        if !kwin_running(ctx)? {
            return Err("KWin is not running".into());
        }
        activate_glass(ctx)
    });
    if let Err(error) = repaired {
        ctx.set(plugin_enabled("blur"), "true")?;
        let _ = ctx.succeeds(cmd!(
            sh,
            "qdbus6 org.kde.KWin /Effects org.kde.kwin.Effects.loadEffect blur"
        ));
        return Err(format!(
            "Glass repair did not complete; KDE standard blur was requested: {error}"
        )
        .into());
    }
    // The deployed package may come from an older run, so refresh the files that must
    // match the shader. They live in the generated ~/dotfiles tree, which is not recorded.
    let package = Path::new("kde-plasma");
    for relative in GLASS_CLIENTS {
        let file = DOTFILES
            .files()
            .into_iter()
            .find(|file| Path::new(file.path_from_root) == package.join(relative))
            .ok_or_else(|| format!("embedded kde-plasma package is missing {relative}"))?;
        let live = ctx.home.join(relative);
        if fs::read(&live).ok().as_deref() != Some(file.content) {
            fs::write(&live, file.content)?;
        }
    }
    ctx.run(cmd!(
        sh,
        "qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure"
    ))?;
    // Reloading the effect drops the blur regions of open windows, and they only register
    // them again when Plasma restarts.
    ctx.run(cmd!(
        sh,
        "systemctl --user try-restart plasma-plasmashell.service"
    ))?;
    ctx.note("Glass matches the running KWin version");
    Ok(())
}

impl Module for KdePlasma {
    fn name(&self) -> &'static str {
        "kde-plasma"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        require_supported_plasma(ctx)?;
        ctx.install_packages(PACKAGES)?;
        require_programs(
            ctx,
            &[
                "plasma-apply-cursortheme",
                "plasma-apply-lookandfeel",
                "kwriteconfig6",
                "kreadconfig6",
                "qdbus6",
                "fc-match",
                "desktop-file-validate",
                "systemctl",
                "sudo",
            ],
        )?;
        ctx.deploy_config("kde-plasma")?;
        install_glass(ctx)?;
        let pointer = DOTFILES
            .assets
            .kde_plasma
            .libinput
            ._90_myconfig_pointer_sensitivity_lua
            .content;
        ctx.write_system_file(Path::new(POINTER_PLUGIN), pointer, "0644")?;

        let home = ctx.home;
        require_executable(&home.join(".local/bin/myconfig-kde-plasma-layout"))?;
        for relative in REQUIRED_FILES {
            require_file(&home.join(relative))?;
        }
        let desktop = home.join(".config/autostart/myconfig-kde-plasma-layout.desktop");
        ctx.run(cmd!(sh, "desktop-file-validate {desktop}"))?;
        let font_name = "Iosevka Nerd Font";
        if !ctx
            .read(cmd!(sh, "fc-match {font_name}"))?
            .contains("Iosevka")
        {
            return Err("Iosevka Nerd Font is not available after installation".into());
        }
        ctx.set(Setting::LookAndFeel, LOOK_AND_FEEL)?;
        configure_appearance(ctx)?;

        let runtime = user_runtime_directory(ctx)?;
        let bus = sh
            .var_os("DBUS_SESSION_BUS_ADDRESS")
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| format!("unix:path={}/bus", runtime.display()).into());
        let _runtime = sh.push_env("XDG_RUNTIME_DIR", &runtime);
        let _bus = sh.push_env("DBUS_SESSION_BUS_ADDRESS", bus);
        ctx.run(cmd!(sh, "systemctl --user daemon-reload"))?;
        ctx.enable_service(GLASS_SERVICE, ServiceScope::User)?;
        if kwin_running(ctx)? {
            activate_glass(ctx)?;
            ctx.run(cmd!(
                sh,
                "qdbus6 org.kde.KWin /KWin org.kde.KWin.reconfigure"
            ))?;
        } else {
            ctx.note("Glass will load at the next KDE Plasma login");
        }

        // The layout script rewrites the panel layout and records its own version.
        ctx.record_path(&home.join(".config/plasma-org.kde.plasma.desktop-appletsrc"))?;
        ctx.record_path(&crate::state::state_directory(home).join("kde-plasma-layout-version"))?;
        let layout = home.join(".local/bin/myconfig-kde-plasma-layout");
        match ctx.run_status(cmd!(sh, "{layout}"))? {
            Some(0) => ctx.run(cmd!(
                sh,
                "systemctl --user try-restart plasma-plasmashell.service"
            )),
            // The layout script exits with 75 when Plasma is not running.
            Some(75) => {
                ctx.note(
                    "KDE Plasma is not active; the layout will apply at the next KDE Plasma login",
                );
                Ok(())
            }
            code => Err(format!("KDE Plasma layout failed with exit code {code:?}").into()),
        }
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        require_supported_plasma(ctx)?;
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "kde-plasma")?;
        if !ctx.succeeds(cmd!(ctx.shell, "pacman -Q {GLASS_PACKAGE}"))? {
            return Err(format!("{GLASS_PACKAGE} is not installed").into());
        }
        require_file(Path::new(POINTER_PLUGIN))?;
        if Setting::LookAndFeel.read(ctx)?.as_deref() != Some(LOOK_AND_FEEL) {
            return Err("the Black & Pink global theme is not applied".into());
        }
        if Setting::CursorTheme.read(ctx)?.as_deref() != Some(CURSOR_THEME) {
            return Err("the Black & Pink cursor theme is not applied".into());
        }
        for (setting, value) in kde_settings() {
            if setting.read(ctx)?.as_deref() != Some(value) {
                return Err(format!("{} is not {value}", setting.describe()).into());
            }
        }
        ctx.run(cmd!(
            ctx.shell,
            "systemctl --user --quiet is-enabled {GLASS_SERVICE}"
        ))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
