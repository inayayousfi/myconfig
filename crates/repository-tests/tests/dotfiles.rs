//! The config packages under `dotfiles/` and their assets.
#![cfg(unix)]
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use repository_tests::{Scratch, dotfiles, program, repository, run};

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

/// The config packages the Linux installers deploy, read from the `ctx.deploy_config`
/// calls in the module sources.
fn linux_packages() -> Vec<String> {
    let modules = repository().join("crates/modules/src");
    let mut packages: Vec<String> = files_under(&modules)
        .into_iter()
        .filter(|file| {
            !file.starts_with(modules.join("tests"))
                && file.extension().is_some_and(|extension| extension == "rs")
        })
        .flat_map(|file| {
            text(file)
                .split("deploy_config(\"")
                .skip(1)
                .map(|rest| rest.split('"').next().unwrap().to_owned())
                .collect::<Vec<_>>()
        })
        .collect();
    packages.sort();
    packages.dedup();
    assert!(!packages.is_empty(), "no module calls deploy_config");
    packages
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
    for package in linux_packages() {
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
    // Command availability is simulated, including retired editors, so no program runs
    // and the host shell configuration is not read.
    let script = r#"
command() {
    if [[ "$1" = -v ]]; then
        case "$2" in
            codium) [[ "$INAYOS_EDITOR_CASE" = codium ]] ;;
            vim) [[ "$INAYOS_EDITOR_CASE" = vim ]] ;;
            emacs|nvim) return 0 ;;
            *) return 1 ;;
        esac
    else
        builtin command "$@"
    fi
}
uname() { print Linux; }
go() { print "$HOME/go"; }
bun() { print "$HOME/.bun/bin"; }
if [[ "$INAYOS_EDITOR_CASE" = codium ]]; then
    unset WSL_DISTRO_NAME
else
    export WSL_DISTRO_NAME=Arch
fi
source "$1" || exit 1
case "$INAYOS_EDITOR_CASE" in
    codium) expected="codium --wait" ;;
    windows-codium) expected="$HOME/.local/bin/windows-codium" ;;
    *) expected="$INAYOS_EDITOR_CASE" ;;
esac
[[ "$EDITOR" = "$expected" && "$VISUAL" = "$expected" ]] || exit 1
(( ! $+aliases[vim] && ! $+aliases[vi] && ! $+aliases[v] )) || exit 1
"#;
    let plugin = dotfiles().join("zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh");
    for editor in ["codium", "windows-codium", "vim", "vi"] {
        let scratch = Scratch::new(&format!("editor-{editor}"));
        if editor == "windows-codium" {
            let bin = scratch.0.join(".local/bin");
            fs::create_dir_all(&bin).unwrap();
            program(&bin.join("windows-codium"), "[ \"$1\" = --check ]");
        }
        run(Command::new("zsh")
            .args(["-f", "-c", script, "editor-selection"])
            .arg(&plugin)
            .env("HOME", &scratch.0)
            .env("INAYOS_EDITOR_CASE", editor));
    }
}
#[test]
fn the_pointer_plugin_multiplies_motion_by_four() {
    let plugin = dotfiles().join("assets/kde-plasma/libinput/90-inayos-pointer-sensitivity.lua");
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
dofile(os.getenv("INAYOS_POINTER_PLUGIN"))
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
        .env("INAYOS_POINTER_PLUGIN", plugin));
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

#[test]
fn the_user_services_are_valid() {
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
        .arg(copy(unit("plasma", "inayos-plasma-layout.service"))));
    run(Command::new("systemd-analyze")
        .arg("verify")
        .arg(copy(unit("kanata", "inayos-kanata.service")))
        .arg(copy(unit("kanata-kde", "inayos-kanata-tray.service")))
        .arg(copy(unit("handy", "inayos-handy.service"))));
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
    let engine = dotfiles().join("kanata/.config/systemd/user/inayos-kanata.service");
    let tray = dotfiles().join("kanata-kde/.config/systemd/user/inayos-kanata-tray.service");
    assert!(
        has_line(&tray, "Wants=inayos-kanata.service"),
        "the tray does not start the engine"
    );
    assert!(
        has_line(&engine, "After=inayos-handy.service"),
        "the engine does not wait for Handy"
    );
    assert!(has_line(&engine, "RestartMode=direct"));
    assert!(has_line(
        &tray,
        "ExecStopPost=-/usr/bin/systemctl --user stop inayos-kanata.service"
    ));
    let handy = text(dotfiles().join("handy/.config/systemd/user/inayos-handy.service"));
    assert!(
        !handy.contains("inayos-kanata"),
        "Handy kept the reversed Kanata ordering"
    );

    let scratch = Scratch::new("kanata-tray");
    let python = |arguments: &[&std::ffi::OsStr]| {
        run(Command::new("python3")
            .args(arguments)
            .env("PYTHONPYCACHEPREFIX", &scratch.0));
    };
    python(&[repository().join("test/test-kanata-tray.py").as_os_str()]);
    let script = dotfiles().join("kanata-kde/.local/bin/inayos-kanata-tray");
    python(&["-m".as_ref(), "py_compile".as_ref(), script.as_os_str()]);
}

#[test]
fn the_plasma_layout_script_records_its_version_and_defers_without_plasma() {
    let scratch = Scratch::new("plasma-layout");
    let home = scratch.0.join("home");
    let bin = scratch.0.join("bin");
    fs::create_dir_all(home.join(".config")).unwrap();
    fs::create_dir_all(
        home.join(".local/share/plasma/look-and-feel/inayos.desktop/contents/layouts"),
    )
    .unwrap();
    fs::create_dir_all(&bin).unwrap();
    let script = home.join("inayos-plasma-layout");
    fs::copy(
        dotfiles().join("plasma/.local/bin/inayos-plasma-layout"),
        &script,
    )
    .unwrap();
    fs::copy(
        dotfiles().join("plasma/.local/share/plasma/look-and-feel/inayos.desktop/contents/layouts/org.kde.plasma.desktop-layout.js"),
        home.join(".local/share/plasma/look-and-feel/inayos.desktop/contents/layouts/org.kde.plasma.desktop-layout.js"),
    )
    .unwrap();
    let applets = home.join(".config/plasma-org.kde.plasma.desktop-appletsrc");
    fs::write(&applets, "default panel configuration\n").unwrap();
    program(
        &bin.join("qdbus6"),
        "if [ \"$#\" -eq 2 ]; then exit \"${INAYOS_PLASMA_DBUS_STATUS:-0}\"; fi\nprintf '%s\\n' \"${INAYOS_PLASMA_EVALUATE_OUTPUT:-INAYOS_PLASMA_LAYOUT=ok:version=7:screens=2}\"",
    );
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
        text(home.join(".local/state/inayos/plasma-layout-version")).trim(),
        "7"
    );
    // The installer records the panel file, and remove restores it from that record.
    assert!(
        backups().is_empty(),
        "the script left a backup that remove cannot see"
    );
    assert!(
        !layout(&[(
            "INAYOS_PLASMA_EVALUATE_OUTPUT",
            "INAYOS_PLASMA_LAYOUT=missing:org.kde.plasma.icontasks"
        )])
        .status
        .success(),
        "a missing required widget was accepted"
    );
    assert_eq!(
        layout(&[("INAYOS_PLASMA_DBUS_STATUS", "1")]).status.code(),
        Some(75)
    );
}

#[test]
fn t3ctl_repair_binds_the_server_locally_and_keeps_other_settings_and_the_lan_route() {
    let scratch = Scratch::new("t3ctl");
    let home = scratch.0.join("home");
    let bin = scratch.0.join("bin");
    let log = scratch.0.join("calls");
    fs::create_dir_all(home.join(".t3/userdata")).unwrap();
    fs::create_dir_all(&bin).unwrap();
    // The server writes its runtime file once it is ready.
    fs::write(home.join(".t3/userdata/server-runtime.json"), "{}").unwrap();
    let service_env = home.join(".t3/service.env");
    fs::write(&service_env, "OPENAI_API_KEY=kept\nT3CODE_PORT=4000\n").unwrap();
    program(
        &bin.join("npm"),
        r#"printf 'npm %s\n' "$*" >> "$T3_TEST_LOG"
mkdir -p "$HOME/.local/bin"
printf '#!/bin/sh\nprintf "t3 %%s\\n" "$*" >> "$T3_TEST_LOG"\n' > "$HOME/.local/bin/t3"
chmod 755 "$HOME/.local/bin/t3""#,
    );
    program(
        &bin.join("systemctl"),
        r#"printf 'systemctl %s\n' "$*" >> "$T3_TEST_LOG"
case "$*" in *is-active*) [ -z "${T3_TEST_INACTIVE:-}" ] ;; esac"#,
    );
    program(
        &bin.join("loginctl"),
        r#"case "$1" in show-user) echo no ;; *) printf 'loginctl %s\n' "$*" >> "$T3_TEST_LOG" ;; esac"#,
    );
    program(&bin.join("curl"), "exit 0");
    let repair = |inactive: bool| {
        let mut command = Command::new("bash");
        command
            .arg(dotfiles().join("t3code/.local/bin/t3ctl"))
            .arg("repair")
            .env("HOME", &home)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("T3_TEST_LOG", &log)
            .env_remove("T3CODE_HOME");
        if inactive {
            command.env("T3_TEST_INACTIVE", "1");
        }
        command.output().unwrap()
    };
    let sorted_env = || {
        let mut lines = lines(&service_env);
        lines.sort();
        lines
    };

    let output = repair(false);
    assert!(output.status.success(), "{output:?}");
    assert!(has_line(
        &log,
        &format!("npm install -g --prefix {}/.local t3", home.display())
    ));
    assert!(has_line(&log, "t3 service install"));
    assert!(has_line(&log, "systemctl --user restart t3code.service"));
    assert_eq!(
        sorted_env(),
        [
            "OPENAI_API_KEY=kept",
            "T3CODE_HOST=127.0.0.1",
            "T3CODE_PORT=3773"
        ],
        "service.env does not bind 127.0.0.1:3773 while keeping other settings"
    );

    fs::write(
        &service_env,
        text(&service_env).replace("T3CODE_HOST=127.0.0.1", "T3CODE_HOST=0.0.0.0"),
    )
    .unwrap();
    let output = repair(false);
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        sorted_env(),
        [
            "OPENAI_API_KEY=kept",
            "T3CODE_HOST=0.0.0.0",
            "T3CODE_PORT=3773"
        ],
        "repair turned the LAN route off"
    );

    assert!(
        !repair(true).status.success(),
        "repair succeeded without an active service"
    );
}
