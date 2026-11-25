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
    // Vim and the retired editors are simulated, so the host shell configuration is not
    // read. Neovim is looked up for real on a PATH that starts empty, because the plugin
    // must find it in ~/.local/bin, where release installs put it.
    let script = r#"
command() {
    if [[ "$1" = -v ]]; then
        case "$2" in
            nvim) builtin command -v nvim ;;
            vim) [[ "$MYCONFIG_EDITOR_CASE" = vim ]] ;;
            emacs|codium) return 0 ;;
            *) return 1 ;;
        esac
    else
        builtin command "$@"
    fi
}
uname() { print Linux; }
go() { print "$HOME/go"; }
bun() { print "$HOME/.bun/bin"; }
PATH="$MYCONFIG_EMPTY_PATH"
source "$1" || exit 1
[[ "$EDITOR" = "$MYCONFIG_EDITOR_CASE" && "$VISUAL" = "$MYCONFIG_EDITOR_CASE" ]] || exit 1
(( ! $+aliases[vim] && ! $+aliases[vi] && ! $+aliases[v] )) || exit 1
"#;
    let plugin = dotfiles().join("zsh/.oh-my-zsh/custom/plugins/inaya/inaya.plugin.zsh");
    for editor in ["nvim", "vim", "vi"] {
        let scratch = Scratch::new(&format!("editor-{editor}"));
        let empty = scratch.0.join("empty");
        fs::create_dir_all(&empty).unwrap();
        if editor == "nvim" {
            let bin = scratch.0.join(".local/bin");
            fs::create_dir_all(&bin).unwrap();
            program(&bin.join("nvim"), "exit 0");
        }
        run(Command::new("zsh")
            .args(["-f", "-c", script, "editor-selection"])
            .arg(&plugin)
            .env("HOME", &scratch.0)
            .env("MYCONFIG_EMPTY_PATH", &empty)
            .env("MYCONFIG_EDITOR_CASE", editor));
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
        .arg(copy(unit("plasma", "myconfig-plasma-layout.service"))));
    run(Command::new("systemd-analyze")
        .arg("verify")
        .arg(copy(unit("kanata", "myconfig-kanata.service")))
        .arg(copy(unit("kanata-kde", "myconfig-kanata-tray.service")))
        .arg(copy(unit("handy", "myconfig-handy.service"))));
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
fn the_plasma_layout_script_records_its_version_and_defers_without_plasma() {
    let scratch = Scratch::new("plasma-layout");
    let home = scratch.0.join("home");
    let bin = scratch.0.join("bin");
    fs::create_dir_all(home.join(".config")).unwrap();
    fs::create_dir_all(home.join(".local/share/myconfig-plasma")).unwrap();
    fs::create_dir_all(&bin).unwrap();
    let script = home.join("myconfig-plasma-layout");
    fs::copy(
        dotfiles().join("plasma/.local/bin/myconfig-plasma-layout"),
        &script,
    )
    .unwrap();
    fs::copy(
        dotfiles().join("plasma/.local/share/myconfig-plasma/layout.js"),
        home.join(".local/share/myconfig-plasma/layout.js"),
    )
    .unwrap();
    let applets = home.join(".config/plasma-org.kde.plasma.desktop-appletsrc");
    fs::write(&applets, "default panel configuration\n").unwrap();
    program(
        &bin.join("qdbus6"),
        "if [ \"$#\" -eq 2 ]; then exit \"${MYCONFIG_PLASMA_DBUS_STATUS:-0}\"; fi\nprintf '%s\\n' \"${MYCONFIG_PLASMA_EVALUATE_OUTPUT:-MYCONFIG_PLASMA_LAYOUT=ok:version=7:screens=2}\"",
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
        text(home.join(".local/state/myconfig/plasma-layout-version")).trim(),
        "7"
    );
    // The installer records the panel file, and remove restores it from that record.
    assert!(
        backups().is_empty(),
        "the script left a backup that remove cannot see"
    );
    assert!(
        !layout(&[(
            "MYCONFIG_PLASMA_EVALUATE_OUTPUT",
            "MYCONFIG_PLASMA_LAYOUT=missing:org.kde.plasma.icontasks"
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

/// Runs the boot rescue's `watch` against a fake systemd whose state lives in files under
/// the scratch folder, on a clock that `sleep` advances. The console is replaced by a
/// function that prints its reason, and the result is everything the run printed.
fn boot_rescue_watch(
    scratch: &Scratch,
    state: &[&str],
    jobs: &str,
    fixed_by_restart: bool,
) -> String {
    boot_rescue_watch_from(scratch, 5, state, jobs, fixed_by_restart)
}

/// `boot_rescue_watch` with the watcher starting `start` seconds after the kernel.
fn boot_rescue_watch_from(
    scratch: &Scratch,
    start: u32,
    state: &[&str],
    jobs: &str,
    fixed_by_restart: bool,
) -> String {
    let root = &scratch.0;
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for file in state {
        fs::write(root.join(file), "").unwrap();
    }
    fs::write(root.join("jobs"), jobs).unwrap();
    let fix = if fixed_by_restart {
        r#": > "$STATE/session""#
    } else {
        ":"
    };
    program(
        &bin.join("systemctl"),
        &format!(
            r#"[ "$1" = -q ] && shift
case "$1 $2" in
    "is-active emergency.target") [ -e "$STATE/emergency" ] ;;
    "is-active rescue.target") [ -e "$STATE/rescue" ] ;;
    "show plasmalogin.service") if [ -e "$STATE/dm-failed" ]; then echo failed
        elif [ -e "$STATE/dm-active" ]; then echo active; else echo inactive; fi ;;
    "list-jobs --no-legend") cat "$STATE/jobs" ;;
    "reset-failed plasmalogin.service") rm -f "$STATE/dm-failed" ;;
    "restart --no-block") echo "restarted plasmalogin"; {fix} ;;
    *) exit 1 ;;
esac"#
        ),
    );
    program(
        &bin.join("loginctl"),
        r#"case "$1" in
    show-seat) [ -e "$STATE/session" ] && echo 2 ;;
    show-session) echo wayland ;;
esac"#,
    );
    let harness = r#"
. "$1"
clock=$2
uptime_seconds() { echo "$clock"; }
sleep() { clock=$((clock + $1)); [ "$clock" -lt 1200 ] || { echo "still watching"; exit 1; }; }
open_console() { echo "console: $1"; exit 0; }
watch
"#;
    let output = Command::new("bash")
        .args(["-c", harness, "harness"])
        .arg(dotfiles().join("assets/boot-rescue/usr/local/libexec/inayos-boot-rescue"))
        .arg(start.to_string())
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("STATE", root)
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn the_boot_rescue_ends_once_the_login_screen_is_up() {
    let scratch = Scratch::new("rescue-up");
    let output = boot_rescue_watch(&scratch, &["dm-active", "session"], "", false);
    assert!(output.contains("a graphical session is active"), "{output}");
    assert!(!output.contains("restarted"), "{output}");
}

#[test]
fn the_boot_rescue_restarts_a_login_screen_that_runs_without_a_session() {
    let scratch = Scratch::new("rescue-restart");
    let output = boot_rescue_watch(&scratch, &["dm-active"], "", true);
    assert_eq!(
        output.matches("restarted plasmalogin").count(),
        1,
        "{output}"
    );
    assert!(output.contains("a graphical session is active"), "{output}");
}

#[test]
fn the_boot_rescue_opens_the_console_after_three_failed_restarts() {
    let scratch = Scratch::new("rescue-console");
    let output = boot_rescue_watch(&scratch, &["dm-active"], "", false);
    assert_eq!(
        output.matches("restarted plasmalogin").count(),
        3,
        "{output}"
    );
    assert!(
        output.contains(
            "console: plasmalogin.service runs, but no login screen came up after 3 restarts"
        ),
        "{output}"
    );
}

#[test]
fn the_boot_rescue_opens_the_console_at_once_in_emergency_mode() {
    let scratch = Scratch::new("rescue-emergency");
    let output = boot_rescue_watch(
        &scratch,
        &["emergency"],
        "1 local-fs.target start waiting\n",
        false,
    );
    assert_eq!(
        output.trim(),
        "console: the system is in emergency or rescue mode"
    );
}

#[test]
fn the_boot_rescue_opens_the_console_when_the_boot_ends_without_a_login_screen() {
    let scratch = Scratch::new("rescue-no-dm");
    let output = boot_rescue_watch(&scratch, &[], "", false);
    assert!(
        output.contains("console: the boot ended without starting plasmalogin.service"),
        "{output}"
    );
}

#[test]
fn the_boot_rescue_opens_the_console_when_jobs_still_wait_after_two_minutes() {
    let scratch = Scratch::new("rescue-stuck");
    let output = boot_rescue_watch(
        &scratch,
        &[],
        "7 plasmalogin.service start waiting\n",
        false,
    );
    assert!(output.contains("console: the boot is stuck"), "{output}");
    assert!(!output.contains("restarted"), "{output}");
}

#[test]
fn the_boot_rescue_service_is_valid() {
    let scratch = Scratch::new("rescue-unit");
    let source =
        dotfiles().join("assets/boot-rescue/etc/systemd/system/inayos-boot-rescue.service");
    // The program only exists after install, so systemd checks the rest of the unit.
    let contents: String = text(&source)
        .lines()
        .map(|line| {
            if line.starts_with("ExecStart=") {
                "ExecStart=/bin/true\n".to_owned()
            } else {
                format!("{line}\n")
            }
        })
        .collect();
    let copy = scratch.0.join("inayos-boot-rescue.service");
    fs::write(&copy, contents).unwrap();
    run(Command::new("systemd-analyze").arg("verify").arg(copy));
}

#[test]
fn time_spent_in_the_initramfs_does_not_count_as_a_stuck_boot() {
    let scratch = Scratch::new("rescue-slow-initramfs");
    let output = boot_rescue_watch_from(
        &scratch,
        300,
        &[],
        "7 plasmalogin.service start waiting\n",
        false,
    );
    assert!(
        output.contains("console: the boot is stuck: jobs still wait 120s later"),
        "{output}"
    );
}

/// A fake sysfs for the wake sources hook, under `root/sys`.
struct FakeSysfs {
    sys: PathBuf,
}

impl FakeSysfs {
    fn new(root: &Path) -> Self {
        let sys = root.join("sys");
        fs::create_dir_all(sys.join("devices")).unwrap();
        Self { sys }
    }

    /// A device at `devices/<path>` whose wake setting is `wakeup`, with an optional driver
    /// and subsystem, and extra attribute files.
    fn device(
        &self,
        path: &str,
        wakeup: Option<&str>,
        driver: Option<&str>,
        subsystem: Option<&str>,
        attributes: &[(&str, &str)],
    ) {
        let dir = self.sys.join("devices").join(path);
        fs::create_dir_all(&dir).unwrap();
        if let Some(wakeup) = wakeup {
            fs::create_dir_all(dir.join("power")).unwrap();
            fs::write(dir.join("power/wakeup"), format!("{wakeup}\n")).unwrap();
        }
        if let Some(driver) = driver {
            let target = self.sys.join("bus/any/drivers").join(driver);
            fs::create_dir_all(&target).unwrap();
            std::os::unix::fs::symlink(&target, dir.join("driver")).unwrap();
        }
        if let Some(subsystem) = subsystem {
            let target = self.sys.join("bus").join(subsystem);
            fs::create_dir_all(&target).unwrap();
            std::os::unix::fs::symlink(&target, dir.join("subsystem")).unwrap();
        }
        for (name, value) in attributes {
            fs::write(dir.join(name), format!("{value}\n")).unwrap();
        }
    }

    /// A USB device with one input device per entry of `inputs`, holding those udev
    /// properties.
    fn usb(&self, path: &str, wakeup: &str, class: &str, inputs: &[&str]) {
        self.device(
            path,
            Some(wakeup),
            Some("usb"),
            Some("usb"),
            &[("idVendor", "1234"), ("bDeviceClass", class)],
        );
        let name = path.rsplit('/').next().unwrap();
        for (number, properties) in inputs.iter().enumerate() {
            let input =
                format!("{path}/{name}:1.{number}/0003:1234:0001.000{number}/input/input{number}");
            self.device(&input, None, None, Some("input"), &[("props", properties)]);
        }
    }

    fn wakeup(&self, path: &str) -> String {
        text(self.sys.join("devices").join(path).join("power/wakeup"))
            .trim()
            .to_owned()
    }
}

#[test]
fn the_wake_sources_hook_leaves_only_keyboards_the_power_button_and_the_lid() {
    let scratch = Scratch::new("wake-sources");
    let sysfs = FakeSysfs::new(&scratch.0);
    let keyboard = "ID_INPUT=1\nID_INPUT_KEY=1\nID_INPUT_KEYBOARD=1";
    let mouse = "ID_INPUT=1\nID_INPUT_MOUSE=1";
    let consumer = "ID_INPUT=1\nID_INPUT_KEY=1";
    sysfs.device(
        "platform/i8042/serio0",
        Some("enabled"),
        Some("atkbd"),
        Some("serio"),
        &[],
    );
    sysfs.device(
        "platform/PNP0C0D:00",
        Some("enabled"),
        Some("acpi-button"),
        Some("platform"),
        &[],
    );
    sysfs.device(
        "platform/gpio-keys.1",
        Some("enabled"),
        Some("gpio-keys"),
        Some("platform"),
        &[],
    );
    sysfs.device(
        "platform/PNP0C0A:00",
        Some("enabled"),
        Some("acpi-battery"),
        Some("platform"),
        &[],
    );
    sysfs.device("platform/serial", Some(""), None, Some("platform"), &[]);
    sysfs.device(
        "pci/02.2",
        Some("enabled"),
        Some("pcieport"),
        Some("pci"),
        &[("class", "0x060400")],
    );
    sysfs.device(
        "pci/06.0",
        Some("enabled"),
        Some("xhci_hcd"),
        Some("pci"),
        &[("class", "0x0c0330")],
    );
    sysfs.usb("pci/06.0/usb3", "disabled", "09", &[]);
    sysfs.usb("pci/06.0/usb3/3-1", "enabled", "00", &[keyboard, mouse]);
    sysfs.usb("pci/06.0/usb3/3-2", "disabled", "00", &[keyboard, consumer]);
    sysfs.usb("pci/06.0/usb3/3-3", "disabled", "09", &[]);
    sysfs.usb("pci/06.0/usb3/3-3/3-3.1", "disabled", "00", &[keyboard]);
    sysfs.usb("pci/06.0/usb3/3-4", "enabled", "00", &[consumer]);
    sysfs.usb("pci/06.0/usb3/3-5", "enabled", "09", &[]);

    let bin = scratch.0.join("bin");
    fs::create_dir_all(&bin).unwrap();
    program(
        &bin.join("udevadm"),
        r#"for argument in "$@"; do
    case "$argument" in --path=*) path=${argument#--path=} ;; esac
done
cat "$INAYOS_WAKE_SYSFS$path/props""#,
    );
    let output = run(Command::new("bash")
        .arg(
            dotfiles().join("assets/wake-sources/usr/lib/systemd/system-sleep/inayos-wake-sources"),
        )
        .args(["pre", "suspend"])
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("INAYOS_WAKE_SYSFS", &sysfs.sys));
    let printed = String::from_utf8(output.stdout).unwrap();

    for (path, wanted) in [
        ("platform/i8042/serio0", "enabled"),
        ("platform/PNP0C0D:00", "enabled"),
        ("platform/gpio-keys.1", "enabled"),
        ("platform/PNP0C0A:00", "disabled"),
        ("platform/serial", ""),
        ("pci/02.2", "disabled"),
        ("pci/06.0", "enabled"),
        ("pci/06.0/usb3", "disabled"),
        ("pci/06.0/usb3/3-1", "disabled"),
        ("pci/06.0/usb3/3-2", "enabled"),
        ("pci/06.0/usb3/3-3", "enabled"),
        ("pci/06.0/usb3/3-3/3-3.1", "enabled"),
        ("pci/06.0/usb3/3-4", "disabled"),
        ("pci/06.0/usb3/3-5", "disabled"),
    ] {
        assert_eq!(sysfs.wakeup(path), wanted, "{path}\n{printed}");
    }
    assert!(
        printed.contains("inayos-wake-sources: disabled pci/06.0/usb3/3-1 (usb)"),
        "{printed}"
    );
}

#[test]
fn the_wake_sources_hook_changes_nothing_after_a_sleep() {
    let scratch = Scratch::new("wake-sources-post");
    let sysfs = FakeSysfs::new(&scratch.0);
    sysfs.device(
        "platform/PNP0C0A:00",
        Some("enabled"),
        Some("acpi-battery"),
        Some("platform"),
        &[],
    );
    run(Command::new("bash")
        .arg(
            dotfiles().join("assets/wake-sources/usr/lib/systemd/system-sleep/inayos-wake-sources"),
        )
        .args(["post", "suspend"])
        .env("INAYOS_WAKE_SYSFS", &sysfs.sys));
    assert_eq!(sysfs.wakeup("platform/PNP0C0A:00"), "enabled");
}
