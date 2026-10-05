//! The config packages under `dotfiles/` and their assets.
#![cfg(unix)]
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use repository_tests::{Scratch, dotfiles, repository, run};
use sha2::{Digest, Sha256};

/// The config packages the Linux installers deploy.
const LINUX_PACKAGES: [&str; 11] = [
    "zsh",
    "yazi",
    "ai",
    "ghostty",
    "kanata",
    "kanata-kde",
    "handy",
    "kde-plasma",
    "emacs",
    "phone",
    "pipewire",
];

fn text(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path.as_ref())
        .unwrap_or_else(|error| panic!("{}: {error}", path.as_ref().display()))
}

fn lines(path: impl AsRef<Path>) -> Vec<String> {
    text(path).lines().map(str::to_owned).collect()
}

fn has_line(path: impl AsRef<Path>, line: &str) -> bool {
    lines(path).iter().any(|candidate| candidate == line)
}

fn json(path: impl AsRef<Path>) -> serde_json::Value {
    serde_json::from_str(&text(path)).unwrap()
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(path) = pending.pop() {
        let metadata = fs::symlink_metadata(&path).unwrap();
        if metadata.is_dir() {
            pending.extend(
                fs::read_dir(&path)
                    .unwrap()
                    .map(|entry| entry.unwrap().path()),
            );
        } else if metadata.is_file() {
            found.push(path);
        }
    }
    found
}

fn lua() -> &'static str {
    if Command::new("lua5.4").arg("-v").output().is_ok() {
        "lua5.4"
    } else {
        "lua"
    }
}

#[test]
fn linux_config_packages_have_no_windows_line_endings() {
    for package in LINUX_PACKAGES {
        for file in files_under(&dotfiles().join(package)) {
            if file.extension().is_some_and(|extension| extension == "ps1") {
                continue;
            }
            if let Ok(contents) = fs::read_to_string(&file) {
                assert!(
                    !contents.contains('\r'),
                    "{} has Windows line endings",
                    file.display()
                );
            }
        }
    }
}

#[test]
fn a_crlf_checkout_still_gets_lf_dotfiles() {
    let scratch = Scratch::new("gitattributes");
    let source = scratch.0.join("source");
    fs::create_dir_all(source.join("dotfiles/zsh")).unwrap();
    fs::copy(
        repository().join(".gitattributes"),
        source.join(".gitattributes"),
    )
    .unwrap();
    fs::write(
        source.join("dotfiles/zsh/config.toml"),
        "line one\nline two\n",
    )
    .unwrap();
    fs::write(source.join("dotfiles/zsh/image.data"), b"\0binary\r\n").unwrap();
    let git = |arguments: &[&str], directory: &Path| {
        run(Command::new("git").args(arguments).current_dir(directory));
    };
    git(&["init", "-q"], &source);
    git(
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "add",
            ".",
        ],
        &source,
    );
    git(
        &[
            "-c",
            "user.email=test@example.com",
            "-c",
            "user.name=Test",
            "commit",
            "-qm",
            "fixture",
        ],
        &source,
    );
    git(
        &[
            "-c",
            "core.autocrlf=true",
            "clone",
            "-q",
            "source",
            "checkout",
        ],
        &scratch.0,
    );
    assert!(!text(scratch.0.join("checkout/dotfiles/zsh/config.toml")).contains('\r'));
    assert_eq!(
        fs::read(scratch.0.join("checkout/dotfiles/zsh/image.data")).unwrap(),
        b"\0binary\r\n",
        "the LF rule changed a binary file"
    );
}

#[test]
fn the_tracked_claude_config_holds_no_personal_data() {
    let ai = dotfiles().join("ai");
    run(Command::new("python3")
        .arg(ai.join(".local/bin/claude-config-helper"))
        .arg("check")
        .arg(ai.join(".claude/settings.json"))
        .arg(ai.join(".config/claude-config-helper/mcp-servers.json")));
}

#[test]
fn the_ai_package_owns_the_opencode_config() {
    assert!(dotfiles().join("ai/.config/opencode").is_dir());
    assert!(
        !dotfiles().join("opencode").exists(),
        "OpenCode is still a separate package"
    );
}

#[test]
fn the_zsh_files_parse() {
    for file in [
        ".zshrc",
        ".oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh",
        ".oh-my-zsh/custom/themes/blacknpink.zsh-theme",
    ] {
        run(Command::new("zsh")
            .arg("-n")
            .arg(dotfiles().join("zsh").join(file)));
    }
}

#[test]
fn zsh_picks_the_installed_editor_without_retired_aliases() {
    // Command availability is simulated, including a retired editor, so no program runs
    // and the host shell configuration is not read.
    let script = r#"
command() {
    if [[ "$1" = -v ]]; then
        case "$2" in
            emacs) [[ "$MYCONFIG_EDITOR_CASE" = emacs ]] ;;
            vim) [[ "$MYCONFIG_EDITOR_CASE" = vim ]] ;;
            nvim) return 0 ;;
            *) return 1 ;;
        esac
    else
        builtin command "$@"
    fi
}
uname() { print Linux; }
go() { print "$HOME/go"; }
bun() { print "$HOME/.bun/bin"; }
if [[ "$MYCONFIG_EDITOR_CASE" = emacs ]]; then
    unset WSL_DISTRO_NAME
else
    export WSL_DISTRO_NAME=Arch
fi
source "$1" || exit 1
[[ "$EDITOR" = "$MYCONFIG_EDITOR_CASE" && "$VISUAL" = "$MYCONFIG_EDITOR_CASE" ]] || exit 1
if [[ "$MYCONFIG_EDITOR_CASE" = emacs ]]; then
    (( ! $+aliases[vim] )) && [[ "${aliases[vi]}" = emacs && "${aliases[v]}" = emacs ]] || exit 1
else
    (( ! $+aliases[vim] && ! $+aliases[vi] && ! $+aliases[v] )) || exit 1
fi
"#;
    let plugin = dotfiles().join("zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh");
    for editor in ["emacs", "vim", "vi"] {
        let scratch = Scratch::new(&format!("editor-{editor}"));
        run(Command::new("zsh")
            .args(["-f", "-c", script, "editor-selection"])
            .arg(&plugin)
            .env("HOME", &scratch.0)
            .env("MYCONFIG_EDITOR_CASE", editor));
    }
}

#[test]
fn the_pinned_ghostty_web_assets_in_remot_are_unchanged() {
    let source = text(dotfiles().join("emacs/.config/emacs/lisp/remot.el"));
    for (name, digest) in [
        (
            "ghostty-web-js",
            "078b3fe37e4ef469d3f3d7772ee263070f8613e5a6f5ff305c85778907f45e72",
        ),
        (
            "ghostty-vt-wasm",
            "d6f0326f1874ad2ce9f289e3a4a0c5f3507d4cb38d8747e4b287def470a0c60a",
        ),
        (
            "ghostty-web-license",
            "5eccd0eeca906db6d661b64dd05e1d4a4b2e49d37d43bbcfd2c8cce4d7832920",
        ),
    ] {
        let start = format!("(defconst remot--{name}-base64\n  (concat\n");
        let begin = source
            .find(&start)
            .unwrap_or_else(|| panic!("missing embedded asset: {name}"))
            + start.len();
        let end = begin + source[begin..].find("\n   ))").unwrap();
        let encoded: String = source[begin..end].split('"').skip(1).step_by(2).collect();
        let decoded = STANDARD.decode(encoded).unwrap();
        let actual: String = Sha256::digest(&decoded)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            actual, digest,
            "checksum mismatch for embedded asset: {name}"
        );
    }
}

#[test]
fn the_pointer_plugin_multiplies_motion_by_four() {
    let plugin = dotfiles().join("assets/kde-plasma/libinput/90-myconfig-pointer-sensitivity.lua");
    assert!(text(&plugin).contains("local multiplier = 4"));
    run(Command::new(lua())
        .arg("-e")
        .arg(
            r#"
libinput = {
    register = function() return 1 end,
    connect = function(self, name, callback) self.callback = callback end,
}
evdev = { REL_X = 1, REL_Y = 2 }
dofile(os.getenv("MYCONFIG_POINTER_PLUGIN"))
local handler
local device = {
    usages = function() return { [evdev.REL_X] = true, [evdev.REL_Y] = true } end,
    connect = function(self, name, callback) handler = callback end,
}
libinput.callback(device)
local frame = { { usage = evdev.REL_X, value = 1 }, { usage = evdev.REL_Y, value = -1 } }
assert(handler(device, frame) == frame)
assert(frame[1].value == 4 and frame[2].value == -4)
"#,
        )
        .env("MYCONFIG_POINTER_PLUGIN", plugin));
}

#[test]
fn the_plasma_dock_fits_its_content_and_the_panel_scripts_pass_their_tests() {
    let kde = dotfiles().join("kde-plasma/.local/share");
    let layout = kde.join("myconfig/kde-plasma/layout.js");
    assert!(
        text(&layout).contains(r#"panel.lengthMode = "fit";"#),
        "the dock does not fit its content"
    );
    assert!(
        text(&layout).contains(r#"tasks.writeConfig("fill", false);"#),
        "the task manager fills the dock"
    );
    let panels = kde.join("kwin/scripts/myconfig-plasma-panels");
    run(Command::new("node")
        .arg(repository().join("test/test-kde-plasma-panels.js"))
        .arg(panels.join("contents/code/main.js")));
    run(Command::new("node")
        .arg(repository().join("test/test-kde-plasma-layout.js"))
        .arg(&layout));
    json(panels.join("metadata.json"));
}

#[test]
fn the_plasma_widgets_offer_their_approved_controls() {
    let root = dotfiles().join("kde-plasma/.local/share/plasma/plasmoids");
    for widget in ["overview", "session", "power", "island"] {
        let id = format!("myconfig.{widget}");
        assert_eq!(
            json(root.join(&id).join("metadata.json"))["KPlugin"]["Id"],
            id.as_str()
        );
        assert!(
            root.join(&id).join("contents/ui/main.qml").is_file(),
            "{id} has no QML entry point"
        );
    }
    let qml = |widget: &str| text(root.join(format!("myconfig.{widget}/contents/ui/main.qml")));
    let (overview, session, power, island) =
        (qml("overview"), qml("session"), qml("power"), qml("island"));
    for needle in [
        r#"text: qsTr("Overview")"#,
        "font.pointSize: 14",
        "Layout.minimumWidth: implicitWidth",
        "Layout.fillHeight: true",
        "invokeShortcut Overview",
        "CanFillArea",
    ] {
        assert!(
            overview.contains(needle),
            "the Overview widget lacks {needle}"
        );
    }
    let ordered = |source: &str, values: &[&str]| {
        let positions: Vec<_> = values.iter().map(|value| source.find(value)).collect();
        positions.iter().all(Option::is_some) && positions.windows(2).all(|pair| pair[0] < pair[1])
    };
    assert!(ordered(
        &session,
        &[
            r#"qsTr("Lock")"#,
            r#"qsTr("Log Out")"#,
            r#"qsTr("Switch User")"#
        ]
    ));
    assert!(session.contains("enabled: session.canSwitchUser"));
    assert!(ordered(
        &power,
        &[
            r#"qsTr("Restart")"#,
            r#"qsTr("Shut Down")"#,
            r#"qsTr("Sleep")"#,
            r#"qsTr("Hibernate")"#
        ]
    ));
    assert!(!session.contains("ToolTip") && !power.contains("ToolTip"));
    assert!(
        session.contains("popupType: QQC2.Popup.Window")
            && power.contains("popupType: QQC2.Popup.Window")
    );
    for forbidden in [
        "/tmp/",
        "ISLAND_",
        "configuration.readyWidgets",
        "configuration.trayGroups",
    ] {
        assert!(
            !island.contains(forbidden),
            "the island still uses {forbidden}"
        );
    }
    assert!(island.contains("function closeAfterDeactivation()"));
    assert!(island.contains(
        "function onVisibleChanged() { if (!target.visible) island.closeAfterDeactivation(); }"
    ));
    for widget in [
        "systemmonitor.cpu",
        "systemmonitor.memory",
        "systemmonitor.net",
        "calendar",
        "notifications",
        "systemtray",
    ] {
        assert!(
            island.contains(&format!(r#""org.kde.plasma.{widget}""#)),
            "the island lacks {widget}"
        );
    }
    for capability in [
        "canLock",
        "canSwitchUser",
        "canLogout",
        "canSuspend",
        "canHibernate",
        "canReboot",
        "canShutdown",
    ] {
        assert!(
            island.contains(&format!("enabled: session.{capability}")),
            "the island ignores {capability}"
        );
    }
}

#[test]
fn the_refind_theme_keeps_its_historical_minimal_layout() {
    let root = dotfiles().join("assets/refind/refind");
    assert!(has_line(root.join("global.conf"), "enable_mouse true"));
    let theme = root.join("themes/black-pink/theme.conf");
    assert!(has_line(&theme, "banner themes/black-pink/banner.png"));
    assert!(has_line(
        &theme,
        "hideui hints,label,singleuser,arrows,badges"
    ));
    for line in lines(&theme) {
        assert!(
            !line.starts_with("icons_dir") && !line.starts_with("showtools"),
            "the theme overrides built-in icons or tools: {line}"
        );
    }
    assert!(
        !root.join("themes/black-pink/icons").exists(),
        "the theme ships custom icons"
    );
}

mod cursors {
    use super::*;

    const SIZE: usize = 40;

    fn root() -> PathBuf {
        dotfiles().join("assets/cursor-theme/cursors/blacknpink-crosshair")
    }

    fn u32_at(data: &[u8], offset: usize) -> u32 {
        u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap())
    }

    /// The 40 by 40 ARGB pixels of image `index` in an Xcursor file.
    fn pixels(data: &[u8], index: usize) -> Vec<u32> {
        let offset = u32_at(data, 24 + 12 * index) as usize;
        (0..SIZE * SIZE)
            .map(|pixel| u32_at(data, offset + 36 + 4 * pixel))
            .collect()
    }

    #[test]
    fn the_theme_inherits_breeze_and_has_every_cursor() {
        assert!(has_line(
            root().join("index.theme"),
            "Inherits=breeze_cursors"
        ));
        for cursor in [
            "default", "pointer", "progress", "text", "wait", "size_hor", "size_ver",
        ] {
            assert!(
                fs::metadata(root().join("cursors").join(cursor))
                    .unwrap()
                    .len()
                    > 0,
                "{cursor} is missing"
            );
        }
        for cursor in [
            "pointer", "grab", "grabbing", "move", "dnd-move", "dnd-copy", "text",
        ] {
            assert_eq!(
                fs::read_link(root().join("cursors").join(cursor)).unwrap(),
                Path::new("crosshair"),
                "{cursor} does not use Precision Select"
            );
        }
    }

    #[test]
    fn every_image_and_slot_is_40_pixels() {
        let mut checked = 0;
        for entry in fs::read_dir(root().join("cursors")).unwrap() {
            let path = entry.unwrap().path();
            if fs::symlink_metadata(&path)
                .unwrap()
                .file_type()
                .is_symlink()
                || !path.is_file()
            {
                continue;
            }
            let data = fs::read(&path).unwrap();
            assert_eq!(
                &data[..4],
                b"Xcur",
                "{} is not an Xcursor file",
                path.display()
            );
            let count = u32_at(&data, 12) as usize;
            assert!(count > 0);
            for index in 0..count {
                let slot = u32_at(&data, 16 + index * 12 + 4) as usize;
                let offset = u32_at(&data, 16 + index * 12 + 8) as usize;
                assert_eq!(slot, SIZE, "{}", path.display());
                assert!(offset + 36 + SIZE * SIZE * 4 <= data.len());
                for field in [8, 16, 20] {
                    assert_eq!(
                        u32_at(&data, offset + field) as usize,
                        SIZE,
                        "{}",
                        path.display()
                    );
                }
            }
            checked += 1;
        }
        assert!(checked > 0, "no cursor files were checked");
    }

    #[test]
    fn the_cursors_match_their_svg_sources() {
        let scratch = Scratch::new("cursor-build");
        run(Command::new("python3")
            .arg(root().join("build.py"))
            .arg("--output-dir")
            .arg(&scratch.0));
        for cursor in [
            "default",
            "crosshair",
            "help",
            "no-drop",
            "up-arrow",
            "person",
            "location",
            "size_hor",
            "size_ver",
            "size_fdiag",
            "size_bdiag",
            "progress",
            "wait",
        ] {
            assert_eq!(
                fs::read(root().join("cursors").join(cursor)).unwrap(),
                fs::read(scratch.0.join(cursor)).unwrap(),
                "{cursor} does not match its SVG source"
            );
        }
    }

    #[test]
    fn the_default_and_crosshair_differ_only_at_their_center() {
        let default = pixels(&fs::read(root().join("cursors/default")).unwrap(), 0);
        let crosshair = pixels(&fs::read(root().join("cursors/crosshair")).unwrap(), 0);
        for image in [&default, &crosshair] {
            for y in 0..SIZE {
                for x in 0..SIZE / 2 {
                    assert_eq!(
                        image[y * SIZE + x],
                        image[y * SIZE + SIZE - 1 - x],
                        "not mirrored at {x},{y}"
                    );
                }
            }
        }
        for y in 0..SIZE {
            for x in 0..SIZE {
                assert_eq!(
                    default[y * SIZE + x],
                    default[x * SIZE + SIZE - 1 - y],
                    "not symmetric at {x},{y}"
                );
            }
        }
        for (x, y) in [(19, 19), (20, 19), (19, 20), (20, 20)] {
            assert_eq!(default[y * SIZE + x], 0x6600_0000);
            let pixel = crosshair[y * SIZE + x];
            assert!(pixel >> 24 == 255 && (pixel >> 16) & 255 > (pixel >> 8) & 255);
        }
        for index in 0..SIZE * SIZE {
            let (x, y) = (index % SIZE, index / SIZE);
            if (17..23).contains(&x) && (17..23).contains(&y) {
                assert_eq!(default[index], 0x6600_0000);
                if (19..21).contains(&x) && (19..21).contains(&y) {
                    assert_eq!(crosshair[index], 0xffff_4ead);
                }
            } else {
                assert_eq!(
                    default[index], crosshair[index],
                    "they differ outside the center at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn the_resize_arrows_share_the_cross_and_point_the_right_way() {
        for name in ["size_hor", "size_ver", "size_fdiag", "size_bdiag"] {
            let data = fs::read(root().join("cursors").join(name)).unwrap();
            assert_eq!(u32_at(&data, 12), 2, "{name} has not two frames");
            for index in 0..2 {
                let offset = u32_at(&data, 24 + 12 * index) as usize;
                assert_eq!(
                    (
                        u32_at(&data, offset + 24),
                        u32_at(&data, offset + 28),
                        u32_at(&data, offset + 32)
                    ),
                    (20, 20, 167),
                    "{name} hotspot or delay"
                );
                let image = pixels(&data, index);
                for y in 17..23 {
                    for x in 17..23 {
                        assert_eq!(image[y * SIZE + x], 0x6600_0000, "{name} center at {x},{y}");
                    }
                }
                if index == 0 {
                    let at = |x: usize, y: usize| image[y * SIZE + x] != 0;
                    let (nw, ne, sw, se) = (at(11, 11), at(28, 11), at(11, 28), at(28, 28));
                    match name {
                        "size_fdiag" => assert!(nw && se && !ne && !sw),
                        "size_bdiag" => assert!(ne && sw && !nw && !se),
                        _ => assert!(!(nw || ne || sw || se), "{name} reaches a corner"),
                    }
                }
            }
        }
    }
}

#[test]
fn the_plasma_themes_select_the_black_and_pink_parts() {
    let share = dotfiles().join("kde-plasma/.local/share/plasma");
    let theme = share.join("desktoptheme/blacknpink");
    assert_eq!(
        json(theme.join("metadata.json"))["KPlugin"]["Id"],
        "blacknpink"
    );
    assert!(has_line(theme.join("plasmarc"), "FallbackTheme=default"));
    let global = share.join("look-and-feel/org.myconfig.blacknpink.desktop");
    let metadata = json(global.join("metadata.json"));
    assert_eq!(metadata["KPackageStructure"], "Plasma/LookAndFeel");
    assert_eq!(metadata["KPlugin"]["Id"], "org.myconfig.blacknpink.desktop");
    for line in [
        "ColorScheme=BlackPink",
        "name=blacknpink",
        "cursorTheme=blacknpink-crosshair",
    ] {
        assert!(
            has_line(global.join("contents/defaults"), line),
            "the global theme lacks {line}"
        );
    }
    let panel = theme.join("widgets/panel-background.svg");
    let svg = text(&panel);
    for (element, meaning) in [
        (
            r#"id="thick-hint-right-margin" x="95" y="-56" width="4" height="4""#,
            "4-pixel floating-panel trailing margin",
        ),
        (
            r#"id="thick-hint-left-margin" x="95" y="-20" width="4" height="8""#,
            "Breeze floating-panel leading margin",
        ),
        (
            r#"id="hint-top-margin" x="20" y="10" width="4" height="4""#,
            "top panel vertical content margin",
        ),
        (
            r#"id="hint-left-margin" x="0" y="30" width=".00000001" height="4""#,
            "Overview control reaching the screen edge",
        ),
    ] {
        assert!(
            svg.contains(element),
            "the panel background lost its {meaning}"
        );
    }
    let scratch = Scratch::new("panel-render");
    run(Command::new("resvg")
        .arg(&panel)
        .arg(scratch.0.join("panel.png")));
}

#[test]
fn the_user_services_and_autostart_entry_are_valid() {
    let scratch = Scratch::new("units");
    let unit = |package: &str, name: &str| {
        dotfiles()
            .join(package)
            .join(".config/systemd/user")
            .join(name)
    };
    // Programs that only exist after install are replaced, so systemd checks the rest.
    let copy = |source: PathBuf| {
        let contents: String = text(&source)
            .lines()
            .map(|line| {
                if line.starts_with("ExecStartPre=") {
                    "ExecStartPre=/bin/true".to_owned()
                } else if line.starts_with("ExecStart=") {
                    "ExecStart=/bin/true".to_owned()
                } else {
                    line.to_owned()
                }
            })
            .map(|line| line + "\n")
            .collect();
        let copy = scratch.0.join(source.file_name().unwrap());
        fs::write(&copy, contents).unwrap();
        copy
    };
    run(Command::new("systemd-analyze")
        .arg("verify")
        .arg(unit("kde-plasma", "myconfig-kde-plasma-layout.service"))
        .arg(unit("kde-plasma", "myconfig-kde-plasma-glass.service")));
    run(Command::new("systemd-analyze")
        .arg("verify")
        .arg(copy(unit("kanata", "myconfig-kanata.service")))
        .arg(copy(unit("kanata-kde", "myconfig-kanata-tray.service")))
        .arg(copy(unit("handy", "myconfig-handy.service"))));
    run(Command::new("desktop-file-validate")
        .arg(dotfiles().join("kde-plasma/.config/autostart/myconfig-kde-plasma-layout.desktop")));
}

#[test]
fn kanata_starts_on_its_off_layer_and_follows_its_tray() {
    let config = text(dotfiles().join("kanata/.config/kanata/config.kbd"));
    assert!(
        config.contains("(deflayer off"),
        "the Off layer is not the startup layer"
    );
    assert_eq!(
        config.matches("tap-hold 0 400").count(),
        18,
        "not every mapping uses the 400 ms timing"
    );
    assert!(
        config.contains("overview    M-w"),
        "F19 does not emit the KDE Overview shortcut"
    );
    let engine = dotfiles().join("kanata/.config/systemd/user/myconfig-kanata.service");
    let tray = dotfiles().join("kanata-kde/.config/systemd/user/myconfig-kanata-tray.service");
    assert!(
        has_line(&tray, "Wants=myconfig-kanata.service"),
        "the tray does not start the engine"
    );
    assert!(
        has_line(&engine, "After=myconfig-handy.service"),
        "the engine does not wait for Handy"
    );
    assert!(has_line(&engine, "RestartMode=direct"));
    assert!(has_line(
        &tray,
        "ExecStopPost=-/usr/bin/systemctl --user stop myconfig-kanata.service"
    ));
    let handy = text(dotfiles().join("handy/.config/systemd/user/myconfig-handy.service"));
    assert!(
        !handy.contains("myconfig-kanata"),
        "Handy kept the reversed Kanata ordering"
    );

    let scratch = Scratch::new("kanata-tray");
    let python = |arguments: &[&std::ffi::OsStr]| {
        run(Command::new("python3")
            .args(arguments)
            .env("PYTHONPYCACHEPREFIX", &scratch.0));
    };
    python(&[repository().join("test/test-kanata-tray.py").as_os_str()]);
    let script = dotfiles().join("kanata-kde/.local/bin/myconfig-kanata-tray");
    python(&["-m".as_ref(), "py_compile".as_ref(), script.as_os_str()]);
}

#[test]
fn the_plasma_layout_script_backs_up_once_and_defers_without_plasma() {
    let scratch = Scratch::new("plasma-layout");
    let home = scratch.0.join("home");
    let bin = scratch.0.join("bin");
    fs::create_dir_all(home.join(".config")).unwrap();
    fs::create_dir_all(home.join(".local/share/myconfig/kde-plasma")).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let script = home.join("myconfig-kde-plasma-layout");
    fs::copy(
        dotfiles().join("kde-plasma/.local/bin/myconfig-kde-plasma-layout"),
        &script,
    )
    .unwrap();
    fs::copy(
        dotfiles().join("kde-plasma/.local/share/myconfig/kde-plasma/layout.js"),
        home.join(".local/share/myconfig/kde-plasma/layout.js"),
    )
    .unwrap();
    let applets = home.join(".config/plasma-org.kde.plasma.desktop-appletsrc");
    fs::write(&applets, "default panel configuration\n").unwrap();
    let qdbus = bin.join("qdbus6");
    fs::write(
        &qdbus,
        "#!/usr/bin/env bash\nif [ \"$#\" -eq 2 ]; then exit \"${MYCONFIG_PLASMA_DBUS_STATUS:-0}\"; fi\nprintf '%s\\n' \"${MYCONFIG_PLASMA_EVALUATE_OUTPUT:-MYCONFIG_STATUS=ok:screens=2}\"\n",
    )
    .unwrap();
    fs::set_permissions(&qdbus, fs::Permissions::from_mode(0o755)).unwrap();
    let layout = |variables: &[(&str, &str)]| {
        let mut command = Command::new("bash");
        command
            .arg(&script)
            .env("HOME", &home)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env_remove("XDG_STATE_HOME");
        for (key, value) in variables {
            command.env(key, value);
        }
        command.output().unwrap()
    };
    let backups = || {
        fs::read_dir(home.join(".config"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .filter(|path| path.to_string_lossy().contains("appletsrc.backup."))
            .collect::<Vec<_>>()
    };

    assert!(layout(&[]).status.success());
    assert_eq!(
        text(home.join(".local/state/myconfig/kde-plasma-layout-version")).trim(),
        "5"
    );
    assert_eq!(
        backups().len(),
        1,
        "the first run did not back up the panels once"
    );
    assert_eq!(text(&backups()[0]), "default panel configuration\n");
    assert!(layout(&[]).status.success());
    assert_eq!(backups().len(), 1, "a rerun repeated the first-run backup");
    assert!(
        !layout(&[(
            "MYCONFIG_PLASMA_EVALUATE_OUTPUT",
            "MYCONFIG_STATUS=missing:org.kde.plasma.icontasks"
        )])
        .status
        .success(),
        "a missing required widget was accepted"
    );
    assert_eq!(
        layout(&[("MYCONFIG_PLASMA_DBUS_STATUS", "1")])
            .status
            .code(),
        Some(75)
    );
}
