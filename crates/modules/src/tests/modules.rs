//! What each module installs, writes, enables and refuses, against fake system programs.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use super::{Answers, Sandbox};
use crate::{
    Base, Cli, Context, Docker, Footprint, Ghostty, Handy, Kanata, KanataKde, Module, ModuleResult,
    Need, Package, PackageSystem, RunMode, Runtimes, ServiceScope, runner,
};

/// Fake system programs that keep their state under the sandbox, so modules run as on a
/// real machine without root: `sudo` resets the environment and moves `/etc`, `/usr`, `/boot`, `/efi`, `/var`, `/sys`, `/home` and `/proc` paths under
/// `system/`, groups live in `groups/`, `systemctl` remembers enabled system units in
/// `units/system/` and user units in `units/user/`, and KDE keys live in `kde/`. A unit is
/// active only while `<unit>.active` exists beside it: `start`, `restart` and
/// `enable --now` create that marker, and `stop` and `disable --now` remove it. A unit
/// fails to start while `<unit>.broken` exists.
fn system(sandbox: &Sandbox) {
    for directory in ["system", "groups", "units/system", "units/user", "kde"] {
        fs::create_dir_all(sandbox.root.join(directory)).unwrap();
    }
    sandbox.fake(
        "sudo",
        &format!(
            r#"printf 'sudo %s\n' "$*" >> "$INAYOS_TEST_LOG"
for argument in "$@"; do
    case "$argument" in
        /etc/* | /usr/* | /boot/* | /efi/* | /var/* | /sys/* | /home/* | /proc/*) set -- "$@" "$INAYOS_TEST_ROOT/system$argument" ;;
        *) set -- "$@" "$argument" ;;
    esac
    shift
done
{}"#,
            super::SUDO
        ),
    );
    sandbox.fake(
        "id",
        r#"case "$*" in
    -un) printf 'tester\n' ;;
    -u) printf '1000\n' ;;
    -Gn) printf 'users\n' ;;
    "-nG tester") printf 'users'; for group in "$INAYOS_TEST_ROOT"/groups/*; do
            [ -e "$group/tester" ] && printf ' %s' "$(basename "$group")"; done; printf '\n' ;;
    *) exit 1 ;;
esac"#,
    );
    sandbox.fake(
        "getent",
        r#"[ "$1" = group ] && [ -d "$INAYOS_TEST_ROOT/groups/$2" ]"#,
    );
    sandbox.fake(
        "groupadd",
        r#"for group; do :; done; mkdir -p "$INAYOS_TEST_ROOT/groups/$group""#,
    );
    sandbox.fake(
        "usermod",
        r#"printf 'usermod %s\n' "$*" >> "$INAYOS_TEST_LOG"
mkdir -p "$INAYOS_TEST_ROOT/groups/$2" && : > "$INAYOS_TEST_ROOT/groups/$2/$3""#,
    );
    sandbox.fake("gpasswd", r#"rm -f "$INAYOS_TEST_ROOT/groups/$3/$2""#);
    sandbox.fake("groupdel", r#"rm -rf "$INAYOS_TEST_ROOT/groups/$1""#);
    for program in ["modprobe", "udevadm"] {
        sandbox.fake(
            program,
            &format!("printf '{program} %s\\n' \"$*\" >> \"$INAYOS_TEST_LOG\""),
        );
    }
    sandbox.fake(
        "systemctl",
        r#"printf 'systemctl %s\n' "$*" >> "$INAYOS_TEST_LOG"
scope=system command= names= now=
for argument in "$@"; do
    case "$argument" in
        --user) scope=user ;;
        --now) now=1 ;;
        --*) ;;
        *) if [ -z "$command" ]; then command="$argument"; else names="$names $argument"; fi ;;
    esac
done
units="$INAYOS_TEST_ROOT/units/$scope"
start() { if [ -e "$units/$unit.broken" ]; then exit 1; fi; : > "$units/$unit.active"; }
for unit in $names; do
    case "$command" in
        enable) : > "$units/$unit"; [ -n "$now" ] && start ;;
        disable) rm -f "$units/$unit"; [ -n "$now" ] && rm -f "$units/$unit.active" ;;
        start | restart) start ;;
        stop) rm -f "$units/$unit.active" ;;
        is-enabled) if [ -e "$units/$unit" ]; then echo enabled; else echo disabled; exit 1; fi ;;
        is-active) if [ -e "$units/$unit.active" ]; then echo active; else echo inactive; exit 3; fi ;;
        show) cat "$INAYOS_TEST_ROOT/manager-pid" 2>/dev/null ;;
    esac
done
exit 0"#,
    );
    sandbox.fake("ps", r#"cat "$INAYOS_TEST_ROOT/manager-groups""#);
    sandbox.fake(
        "kwriteconfig6",
        r#"printf 'kwriteconfig6 %s\n' "$*" >> "$INAYOS_TEST_LOG"
path="$INAYOS_TEST_ROOT/kde"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --file | --group) path="$path/$2"; shift 2 ;;
        --key) key="$2"; shift 2 ;;
        --delete) rm -f "$path/$key"; exit 0 ;;
        *) value="$1"; shift ;;
    esac
done
mkdir -p "$(dirname "$path/$key")" && printf '%s' "$value" > "$path/$key""#,
    );
    sandbox.fake(
        "kreadconfig6",
        r#"path="$INAYOS_TEST_ROOT/kde"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --file) path="$path/${2##*/}"; shift 2 ;;
        --group) path="$path/$2"; shift 2 ;;
        --key) key="$2"; shift 2 ;;
        --default) default="$2"; shift 2 ;;
        *) shift ;;
    esac
done
if [ -e "$path/$key" ]; then cat "$path/$key"; else printf '%s' "$default"; fi"#,
    );
    sandbox.fake("python", r#"exec python3 "$@""#);
}

fn system_file(sandbox: &Sandbox, path: &str) -> Option<String> {
    fs::read_to_string(
        sandbox
            .root
            .join("system")
            .join(path.trim_start_matches('/')),
    )
    .ok()
}

fn member(sandbox: &Sandbox, group: &str) -> bool {
    sandbox
        .root
        .join("groups")
        .join(group)
        .join("tester")
        .exists()
}

fn kde(sandbox: &Sandbox, path: &str) -> Option<String> {
    fs::read_to_string(sandbox.root.join("kde").join(path)).ok()
}

/// The file that the fake `systemctl` keeps for `name`, a unit or one of its markers.
fn unit_file(sandbox: &Sandbox, scope: ServiceScope, name: &str) -> PathBuf {
    let folder = match scope {
        ServiceScope::System => "units/system",
        ServiceScope::User => "units/user",
    };
    sandbox.root.join(folder).join(name)
}

fn enabled(sandbox: &Sandbox, scope: ServiceScope, unit: &str) -> bool {
    unit_file(sandbox, scope, unit).exists()
}

fn active(sandbox: &Sandbox, scope: ServiceScope, unit: &str) -> bool {
    unit_file(sandbox, scope, &format!("{unit}.active")).exists()
}

fn install_everything(sandbox: &Sandbox, packages: &[&str]) {
    for package in packages {
        fs::write(sandbox.root.join("installed").join(package), "").unwrap();
    }
}

const MKINITCPIO: &str = "# vim:set ft=sh
MODULES=()
HOOKS=(base systemd autodetect microcode kms modconf block keyboard sd-vconsole filesystems fsck)
";
const REFIND_LINUX: &str = r#""Boot with standard options"    "rw root=UUID=31f4 resume=UUID=31f4 resume_offset=244412416 efi_pstore.pstore_disable=0"
"Boot to single-user mode"    "rw root=UUID=31f4 resume=UUID=31f4 resume_offset=244412416" single
"Boot with minimal options"   "ro root=/dev/nvme0n1p2 resume=UUID=31f4 resume_offset=244412416"
"#;

#[test]
fn packages_are_uninstalled_only_when_installed() {
    let sandbox = Sandbox::new("uninstall");
    install_everything(&sandbox, &["kitty", "konsole"]);
    let io = Answers::new(None);
    sandbox.with(&io, |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        let packages = [
            Package::Kitty,
            Package::Alacritty,
            Package::Wezterm,
            Package::Konsole,
        ];
        ctx.uninstall_packages(&packages).unwrap();
        ctx.uninstall_packages(&[Package::Alacritty, Package::Wezterm])
            .unwrap();
    });
    assert_eq!(sandbox.calls("pacman -Rs --noconfirm kitty konsole"), 1);
    assert_eq!(
        sandbox.calls("pacman -R"),
        1,
        "no removal runs when nothing is installed"
    );
}

#[test]
fn apt_packages_are_uninstalled_only_when_installed() {
    let sandbox = Sandbox::for_system("uninstall-apt", PackageSystem::Apt);
    install_everything(&sandbox, &["ripgrep", "btop"]);
    let io = Answers::new(None);
    sandbox.with(&io, |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        let packages = [
            Package::Ripgrep,
            Package::Jq,
            Package::Fastfetch,
            Package::Btop,
        ];
        ctx.uninstall_packages(&packages).unwrap();
        ctx.uninstall_packages(&[Package::Jq, Package::Fastfetch])
            .unwrap();
    });
    assert_eq!(sandbox.calls("apt-get purge -y ripgrep btop"), 1);
    assert_eq!(
        sandbox.calls("apt-get purge"),
        1,
        "no removal runs when nothing is installed"
    );
    assert!(!sandbox.installed("ripgrep") && !sandbox.installed("btop"));
}

/// Deploys one real config package.
struct Config(&'static str);

impl Module for Config {
    fn name(&self) -> &'static str {
        self.0
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.deploy_config(self.0)
    }
    fn verify(&self, ctx: &Context) -> ModuleResult {
        crate::deploy::verify_package(ctx, self.0)
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn deploying_over_a_linked_outside_folder_leaves_that_folder_untouched() {
    let sandbox = Sandbox::new("linked-folder");
    let outside = sandbox.root.join("outside-yazi");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("yazi.toml"), "outside\n").unwrap();
    fs::create_dir_all(sandbox.home.join(".config")).unwrap();
    std::os::unix::fs::symlink(&outside, sandbox.home.join(".config/yazi")).unwrap();
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&Config("yazi")], &[&Config("yazi")])
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(outside.join("yazi.toml")).unwrap(),
        "outside\n"
    );
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&Config("yazi")], &[&Config("yazi")], false)
        })
        .unwrap();
    assert_eq!(
        fs::read_link(sandbox.home.join(".config/yazi")).unwrap(),
        outside
    );
}

#[test]
fn ghostty_links_its_config_and_leaves_other_files_alone() {
    let sandbox = Sandbox::new("ghostty-config");
    let folder = sandbox.home.join(".config/ghostty");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("config.ghostty"), "").unwrap();
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&Config("ghostty")], &[&Config("ghostty")])
            })
            .unwrap();
        assert_eq!(
            fs::canonicalize(folder.join("config")).unwrap(),
            fs::canonicalize(sandbox.home.join("dotfiles/ghostty/.config/ghostty/config")).unwrap()
        );
        assert_eq!(
            fs::read_to_string(folder.join("config.ghostty")).unwrap(),
            ""
        );
    }
}

#[test]
fn folders_that_programs_write_into_stay_real_folders() {
    let sandbox = Sandbox::new("app-folders");
    let io = Answers::new(None);
    for (package, folder) in [
        ("plasma", ".local/share/applications"),
        ("plasma", ".local/share/kwin/scripts"),
        ("plasma", ".local/share/plasma/plasmoids"),
        ("plasma", ".config/fontconfig/conf.d"),
        (
            "ghostty",
            ".config/systemd/user/app-com.mitchellh.ghostty.service.d",
        ),
        ("vscodium", ".config/VSCodium/User"),
        ("t3code-desktop", ".t3/userdata"),
        ("ai", ".fx"),
        ("ai", ".config/opencode"),
    ] {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&Config(package)], &[&Config(package)])
            })
            .unwrap();
        let metadata = fs::symlink_metadata(sandbox.home.join(folder)).unwrap();
        assert!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "{folder} of {package} was folded"
        );
    }
}

#[test]
fn the_agent_package_keeps_bin_and_claude_as_real_folders() {
    let sandbox = Sandbox::new("ai-folders");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&Config("ai")], &[&Config("ai")])
        })
        .unwrap();
    for folder in [".local/bin", ".claude"] {
        let metadata = fs::symlink_metadata(sandbox.home.join(folder)).unwrap();
        assert!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "{folder} was folded"
        );
    }
    let helper = sandbox.home.join(".local/bin/claude-config-helper");
    assert!(fs::metadata(&helper).unwrap().permissions().mode() & 0o111 != 0);
    assert_eq!(
        fs::canonicalize(helper).unwrap(),
        fs::canonicalize(
            sandbox
                .home
                .join("dotfiles/ai/.local/bin/claude-config-helper")
        )
        .unwrap()
    );
}

#[test]
fn kanata_gets_input_access_and_waits_for_the_new_groups() {
    let sandbox = Sandbox::new("kanata");
    system(&sandbox);
    sandbox.fake(
        "kanata",
        r#"printf 'kanata %s\n' "$*" >> "$INAYOS_TEST_LOG""#,
    );
    let kanata = Kanata {
        start_at_login: true,
    };
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&kanata], &[&kanata]))
        .unwrap();
    assert!(sandbox.installed("kanata-bin"));
    assert_eq!(
        sandbox.calls("kanata --check --cfg"),
        2,
        "install and verify both check the config"
    );
    assert!(member(&sandbox, "input") && member(&sandbox, "uinput"));
    assert_eq!(sandbox.calls("sudo modprobe uinput"), 1);
    assert_eq!(
        system_file(&sandbox, "/etc/modules-load.d/inayos-kanata.conf").as_deref(),
        Some("uinput\n")
    );
    let rules = system_file(&sandbox, "/etc/udev/rules.d/99-inayos-kanata.rules").unwrap();
    assert!(rules.contains(
        r#"KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput""#
    ));
    assert!(rules.contains(r#"SUBSYSTEM=="input", KERNEL=="event*", MODE="0660", GROUP="input""#));
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-kanata.service"
    ));
    assert_eq!(
        sandbox.calls("restart inayos-kanata.service"),
        0,
        "the user's systemd manager started before the new groups"
    );
    // After a new login, the user's systemd manager has the groups, so a rerun starts Kanata.
    fs::write(sandbox.root.join("manager-pid"), "4242\n").unwrap();
    fs::write(sandbox.root.join("manager-groups"), "users,input,uinput\n").unwrap();
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&kanata], &[&kanata]))
        .unwrap();
    assert_eq!(sandbox.calls("restart inayos-kanata.service"), 1);

    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&kanata], &[&kanata], false)
        })
        .unwrap();
    assert!(!member(&sandbox, "uinput"));
    assert!(system_file(&sandbox, "/etc/udev/rules.d/99-inayos-kanata.rules").is_none());
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-kanata.service"
    ));
}

#[test]
fn kanata_started_by_the_kde_tray_is_not_enabled_on_its_own() {
    let sandbox = Sandbox::new("kanata-tray");
    system(&sandbox);
    sandbox.fake("kanata", "exit 0");
    sandbox.fake("plasmashell", "exit 0");
    let modules: [&dyn Module; 2] = [
        &Kanata {
            start_at_login: false,
        },
        &KanataKde,
    ];
    sandbox
        .with(&Answers::new(None), |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-kanata.service"
    ));
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-kanata-tray.service"
    ));
    assert_eq!(
        kde(&sandbox, "kglobalshortcutsrc/kwin/Overview").as_deref(),
        Some("Meta+W,Meta+W,Toggle Overview")
    );
    assert_eq!(
        sandbox.calls("stop inayos-kanata.service"),
        1,
        "the engine must not run without its tray"
    );
    // A later verify of either module passes on the same machine.
    let results = sandbox.with(&Answers::new(None), |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

#[test]
fn handy_configures_push_to_talk_and_keeps_your_settings() {
    let sandbox = Sandbox::new("handy");
    system(&sandbox);
    sandbox.fake("handy", "exit 0");
    let settings = sandbox
        .home
        .join(".config/com.pais.handy/settings_store.json");
    fs::create_dir_all(settings.parent().unwrap()).unwrap();
    fs::write(
        &settings,
        r#"{"settings":{"selected_model":"keep-model","unrelated":{"keep":true}}}"#,
    )
    .unwrap();
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Handy], &[&Handy]))
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(json["settings"]["keyboard_implementation"], "handy_keys");
    assert_eq!(json["settings"]["push_to_talk"], true);
    assert_eq!(
        json["settings"]["bindings"]["transcribe"]["current_binding"],
        "ctrl+space"
    );
    assert_eq!(json["settings"]["selected_model"], "keep-model");
    assert_eq!(json["settings"]["unrelated"]["keep"], true);
    assert_eq!(
        fs::metadata(&settings).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(system_file(&sandbox, "/etc/udev/rules.d/99-inayos-handy.rules").is_some());
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-handy.service"
    ));
    assert_eq!(sandbox.calls("stop inayos-handy.service"), 1);

    sandbox
        .with(&io, |ctx| runner::remove(ctx, &[&Handy], &[&Handy], false))
        .unwrap();
    assert_eq!(
        fs::read_to_string(&settings).unwrap(),
        r#"{"settings":{"selected_model":"keep-model","unrelated":{"keep":true}}}"#
    );
}

const DOCKER: Docker = Docker { setup_tool: false };

/// A `docker` command that keeps its contexts as files and reports a rootless daemon.
fn docker(sandbox: &Sandbox) {
    sandbox.fake(
        "docker",
        r#"printf 'docker %s\n' "$*" >> "$INAYOS_TEST_LOG"
contexts="$INAYOS_TEST_ROOT/docker"
mkdir -p "$contexts"
case "$1 $2" in
    "context inspect") [ -e "$contexts/$3" ] && cat "$contexts/$3" ;;
    "context create") printf '%s' "${5#host=}" > "$contexts/$3" ;;
    "context rm") rm -f "$contexts/$4" ;;
    "context use") printf '%s' "$3" > "$contexts/.current" ;;
    "context show") cat "$contexts/.current" 2>/dev/null || echo default ;;
    "info --format") echo '[name=seccomp,profile=builtin name=rootless name=cgroupns]' ;;
esac"#,
    );
}

#[test]
fn docker_refuses_a_docker_group_member_without_changing_anything() {
    let sandbox = Sandbox::new("docker-refused");
    system(&sandbox);
    fs::create_dir_all(sandbox.root.join("groups/docker")).unwrap();
    fs::write(sandbox.root.join("groups/docker/tester"), "").unwrap();
    let error = sandbox
        .with(&Answers::new(None), |ctx| DOCKER.install(ctx))
        .unwrap_err();
    assert!(error.to_string().contains("docker group"));
    assert_eq!(sandbox.calls("bootvine"), 0, "bootvine was installed");
    assert_eq!(sandbox.calls("systemctl"), 0);
    assert!(!enabled(&sandbox, ServiceScope::User, "docker.service"));
}

/// Makes the fake `docker` package enable and start the rootful daemon and create the
/// docker group when it installs, as Ubuntu's docker-ce package does.
fn docker_package_enables_the_rootful_daemon(sandbox: &Sandbox) {
    fs::rename(
        sandbox.root.join("bin/pacman"),
        sandbox.root.join("bin/pacman-base"),
    )
    .unwrap();
    sandbox.fake(
        "pacman",
        r#"if [ "$1" = -S ]; then for name in "$@"; do [ "$name" = docker ] && mkdir -p "$INAYOS_TEST_ROOT/groups/docker" && for unit in docker.service docker.socket; do : > "$INAYOS_TEST_ROOT/units/system/$unit"; : > "$INAYOS_TEST_ROOT/units/system/$unit.active"; done; done; fi
exec pacman-base "$@""#,
    );
}

#[test]
fn docker_runs_rootless_through_its_own_context_and_remove_restores_the_default() {
    let sandbox = Sandbox::new("docker");
    system(&sandbox);
    docker(&sandbox);
    docker_package_enables_the_rootful_daemon(&sandbox);
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| runner::install(ctx, &[&DOCKER], &[&DOCKER]))
            .unwrap();
    }
    for package in [
        "docker",
        "docker-buildx",
        "docker-compose",
        "docker-rootless-extras",
        "slirp4netns",
        "shadow",
    ] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    for unit in ["docker.service", "docker.socket"] {
        assert!(
            !enabled(&sandbox, ServiceScope::System, unit),
            "the rootful {unit} is still enabled"
        );
        assert!(
            !active(&sandbox, ServiceScope::System, unit),
            "the rootful {unit} still runs"
        );
    }
    assert!(enabled(&sandbox, ServiceScope::User, "docker.service"));
    assert_eq!(
        fs::read_to_string(sandbox.root.join("docker/rootless")).unwrap(),
        "unix:///run/user/1000/docker.sock"
    );
    assert_eq!(sandbox.calls("docker context create"), 1);
    assert_eq!(
        fs::read_to_string(sandbox.root.join("docker/.current")).unwrap(),
        "rootless"
    );
    assert!(sandbox.root.join("groups/docker").is_dir());
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&DOCKER], &[&DOCKER], false)
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("docker/.current")).unwrap(),
        "default"
    );
    assert!(!sandbox.root.join("docker/rootless").exists());
    assert!(!enabled(&sandbox, ServiceScope::User, "docker.service"));
    assert!(!sandbox.installed("docker"));
    assert!(
        !sandbox.root.join("groups/docker").exists(),
        "the docker group that the package created is still there"
    );
    for unit in ["docker.service", "docker.socket"] {
        assert!(
            !enabled(&sandbox, ServiceScope::System, unit),
            "remove turned on the rootful {unit}, which only the package install had turned on"
        );
        assert!(
            !active(&sandbox, ServiceScope::System, unit),
            "remove started the rootful {unit}, which only the package install had started"
        );
    }
}

#[test]
fn removing_docker_turns_back_on_a_rootful_daemon_that_was_on_before() {
    let sandbox = Sandbox::new("docker-rootful-before");
    system(&sandbox);
    docker(&sandbox);
    install_everything(&sandbox, &["docker"]);
    fs::create_dir_all(sandbox.root.join("groups/docker")).unwrap();
    for unit in ["docker.service", "docker.socket"] {
        fs::write(unit_file(&sandbox, ServiceScope::System, unit), "").unwrap();
        fs::write(
            unit_file(&sandbox, ServiceScope::System, &format!("{unit}.active")),
            "",
        )
        .unwrap();
    }
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&DOCKER], &[&DOCKER]))
        .unwrap();
    assert!(!enabled(&sandbox, ServiceScope::System, "docker.service"));
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&DOCKER], &[&DOCKER], false)
        })
        .unwrap();
    for unit in ["docker.service", "docker.socket"] {
        assert!(
            enabled(&sandbox, ServiceScope::System, unit),
            "the rootful {unit} was not enabled again"
        );
        assert!(
            active(&sandbox, ServiceScope::System, unit),
            "the rootful {unit} was not started again"
        );
    }
    assert!(
        sandbox.root.join("groups/docker").is_dir(),
        "remove deleted a docker group that was there before install"
    );
}

#[test]
fn docker_creates_the_user_service_with_the_setup_tool_and_remove_undoes_what_the_tool_did() {
    let sandbox = Sandbox::new("docker-setup-tool");
    system(&sandbox);
    docker(&sandbox);
    // Like Docker's tool, it also enables the service and creates and selects the context.
    sandbox.fake(
        "dockerd-rootless-setuptool.sh",
        r#"printf 'setuptool %s PATH=%s\n' "$*" "$PATH" >> "$INAYOS_TEST_LOG"
mkdir -p "$HOME/.config/systemd/user" && : > "$HOME/.config/systemd/user/docker.service"
: > "$INAYOS_TEST_ROOT/units/user/docker.service"
mkdir -p "$INAYOS_TEST_ROOT/docker"
printf 'unix:///run/user/1000/docker.sock' > "$INAYOS_TEST_ROOT/docker/rootless"
printf rootless > "$INAYOS_TEST_ROOT/docker/.current""#,
    );
    let docker = Docker { setup_tool: true };
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| runner::install(ctx, &[&docker], &[&docker]))
            .unwrap();
    }
    assert_eq!(
        sandbox.calls(
            "setuptool install PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
        ),
        1
    );
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&docker], &[&docker], false)
        })
        .unwrap();
    assert!(!enabled(&sandbox, ServiceScope::User, "docker.service"));
    assert!(!sandbox.root.join("docker/rootless").exists());
    assert_eq!(
        fs::read_to_string(sandbox.root.join("docker/.current")).unwrap(),
        "default"
    );
    assert!(
        !sandbox
            .home
            .join(".config/systemd/user/docker.service")
            .exists()
    );
}

#[test]
fn ghostty_becomes_the_kde_terminal_until_removed() {
    let sandbox = Sandbox::new("ghostty-kde");
    system(&sandbox);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Ghostty], &[&Ghostty]))
        .unwrap();
    assert!(sandbox.installed("ghostty"));
    assert_eq!(
        kde(&sandbox, "kdeglobals/General/TerminalService").as_deref(),
        Some("com.mitchellh.ghostty.desktop")
    );
    assert_eq!(
        kde(&sandbox, "kdeglobals/General/TerminalApplication").as_deref(),
        Some("/usr/bin/ghostty --gtk-single-instance=true")
    );
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&Ghostty], &[&Ghostty], false)
        })
        .unwrap();
    assert_eq!(kde(&sandbox, "kdeglobals/General/TerminalService"), None);
    assert_eq!(
        kde(&sandbox, "kdeglobals/General/TerminalApplication"),
        None
    );
}

/// Axidev's release server for `configure`: GitHub's redirect names the release in the
/// `latest` file, and the downloaded installer puts the application at `app`, its lifecycle
/// installer at `lifecycle` and a 0.19.0 payload at `payload` in place.
fn axidev_releases(sandbox: &Sandbox, app: &Path, lifecycle: &Path, payload: &Path) {
    sandbox.fake(
        "curl",
        &format!(
            r#"printf 'curl %s\n' "$*" >> "$INAYOS_TEST_LOG"
case " $* " in *" --write-out "*)
    printf 'https://github.com/axide-dev/axidev-osk/releases/tag/v%s' "$(cat "$INAYOS_TEST_ROOT/latest")"
    exit 0 ;;
esac
while [ "$#" -gt 1 ]; do [ "$1" = --output ] && output="$2"; shift; done
cat > "$output" <<'SCRIPT'
#!/bin/sh
printf 'installer %s\n' "$*" >> "$INAYOS_TEST_LOG"
cat > {app} <<'APP'
#!/bin/sh
printf 'app %s\n' "$*" >> "$INAYOS_TEST_LOG"
case "$*" in "linux setup-permissions"*)
    mkdir -p "$INAYOS_TEST_ROOT/groups/uinput" "$INAYOS_TEST_ROOT/system/etc/udev/rules.d"
    : > "$INAYOS_TEST_ROOT/groups/uinput/tester"
    ln -sf /dev/null "$INAYOS_TEST_ROOT/system/etc/udev/rules.d/70-axidev-io-uinput.rules" ;;
esac
APP
printf '#!/bin/sh\nprintf "lifecycle %%s\\n" "$*" >> "$INAYOS_TEST_LOG"\n' > {lifecycle}
chmod +x {app} {lifecycle}
mkdir -p {payload}/lib/python/axidev_osk-0.19.0.dist-info
SCRIPT"#,
            app = app.display(),
            lifecycle = lifecycle.display(),
            payload = payload.display()
        ),
    );
}

#[test]
fn axidev_installs_once_upgrades_to_a_newer_release_and_remove_uninstalls_what_it_installed() {
    let sandbox = Sandbox::new("axidev");
    system(&sandbox);
    let lifecycle = sandbox.root.join("bin/axidev-osk-install");
    let app = sandbox.root.join("bin/axidev-osk");
    let payload = sandbox.root.join("opt/axidev-osk");
    fs::write(sandbox.root.join("latest"), "0.19.0").unwrap();
    axidev_releases(&sandbox, &app, &lifecycle, &payload);
    let io = Answers::new(None);
    let run = || {
        sandbox.with(&io, |ctx| {
            ctx.state()
                .borrow_mut()
                .begin(crate::state::Action::Install)
                .unwrap();
            ctx.set_module("axidev-osk");
            crate::axidev_osk::configure(ctx, &lifecycle, &app, &payload).unwrap();
        })
    };
    run();
    assert_eq!(sandbox.calls("installer install --user tester"), 1);
    for step in [
        "setup-permissions --user tester",
        "setup-autostart --user tester",
        "setup-greeter --manager plasma-login",
    ] {
        assert_eq!(sandbox.calls(&format!("app linux {step}")), 1, "{step}");
    }
    run();
    assert_eq!(
        sandbox.calls("lifecycle upgrade"),
        0,
        "the installed release was upgraded again"
    );
    assert_eq!(
        sandbox.calls("releases/latest/download/axidev-osk-install"),
        1,
        "a rerun must not download the installer again"
    );
    fs::write(sandbox.root.join("latest"), "0.20.0").unwrap();
    run();
    assert_eq!(sandbox.calls("lifecycle upgrade --user tester"), 1);
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::AxidevOsk], &[&crate::AxidevOsk], false)
        })
        .unwrap();
    assert_eq!(sandbox.calls("lifecycle uninstall --user tester"), 1);
    // Axidev's own uninstall leaves the uinput group, the membership in it and the masked
    // udev rule. The group did not exist before the install, so remove deletes it.
    assert!(!sandbox.root.join("groups/uinput").exists());
    assert!(
        fs::symlink_metadata(
            sandbox
                .root
                .join("system/etc/udev/rules.d/70-axidev-io-uinput.rules")
        )
        .is_err()
    );
}

#[test]
fn axidev_older_than_the_offline_setup_changes_nothing() {
    let sandbox = Sandbox::new("axidev-old");
    system(&sandbox);
    let lifecycle = sandbox.root.join("bin/axidev-osk-install");
    let app = sandbox.root.join("bin/axidev-osk");
    let payload = sandbox.root.join("opt/axidev-osk");
    fs::write(sandbox.root.join("latest"), "0.18.1").unwrap();
    axidev_releases(&sandbox, &app, &lifecycle, &payload);
    let io = Answers::new(None);
    let result = sandbox.with(&io, |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        ctx.set_module("axidev-osk");
        crate::axidev_osk::configure(ctx, &lifecycle, &app, &payload)
    });
    let error = result.unwrap_err().to_string();
    assert!(error.contains("0.19.0"), "{error}");
    assert_eq!(sandbox.calls("releases/latest/download"), 0);
    assert_eq!(sandbox.calls("pacman -S"), 0);
    assert!(!app.exists());
}

#[test]
fn agent_links_share_one_instruction_file_and_leave_other_skill_links() {
    let sandbox = Sandbox::new("agent-links");
    let home = &sandbox.home;
    fs::create_dir_all(home.join(".agents/skills/demo")).unwrap();
    fs::create_dir_all(home.join(".claude/skills")).unwrap();
    fs::write(
        home.join(".agents/AGENTS.md"),
        "# Test global instructions\n",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        "../../.agents/skills/stale",
        home.join(".claude/skills/stale"),
    )
    .unwrap();
    fs::create_dir_all(home.join(".fx")).unwrap();
    fs::write(
        home.join(".fx/mcp.json"),
        r#"{"mcp":{"existing-server":{"type":"http","url":"https://example.test/mcp"}}}"#,
    )
    .unwrap();
    sandbox.with(&Answers::new(None), |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        crate::agent_config::link_agent_config(ctx).unwrap();
        crate::agent_config::configure_fx_playwright(ctx).unwrap();
    });
    for link in [
        ".config/opencode/AGENTS.md",
        ".fx/AGENTS.md",
        ".pi/agent/AGENTS.md",
    ] {
        assert_eq!(
            fs::read_link(home.join(link)).unwrap(),
            home.join(".agents/AGENTS.md")
        );
    }
    assert!(
        fs::symlink_metadata(home.join(".claude/skills/demo"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
    // Install never undoes an earlier run, so a link to a skill that is gone stays.
    assert!(fs::symlink_metadata(home.join(".claude/skills/stale")).is_ok());
    let json: serde_json::Value =
        serde_json::from_slice(&fs::read(home.join(".fx/mcp.json")).unwrap()).unwrap();
    assert_eq!(
        json["mcp"]["existing-server"]["url"],
        "https://example.test/mcp"
    );
    assert_eq!(json["mcp"]["playwright"]["type"], "stdio");
    assert_eq!(json["mcp"]["playwright"]["enabled"], true);
    assert_eq!(
        json["mcp"]["playwright"]["command"],
        serde_json::json!([home.join(".bun/bin/playwright-mcp"), "--headless"])
    );
    assert_eq!(
        fs::metadata(home.join(".fx/mcp.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[test]
fn paru_is_built_from_its_prebuilt_aur_package() {
    let sandbox = Sandbox::new("paru-aur");
    fs::remove_file(sandbox.root.join("installed/paru")).unwrap();
    sandbox.fake(
        "git",
        r#"printf 'git %s\n' "$*" >> "$INAYOS_TEST_LOG"; mkdir -p "$3""#,
    );
    sandbox.fake(
        "makepkg",
        r#"printf 'makepkg %s\n' "$*" >> "$INAYOS_TEST_LOG"
[ "$1" = --packagelist ] && printf '%s/paru-bin.pkg.tar.zst\n' "$PWD"; exit 0"#,
    );
    sandbox.with(&Answers::new(None), |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        crate::package_manager::ensure_paru(ctx).unwrap();
    });
    assert_eq!(
        sandbox.calls("git clone https://aur.archlinux.org/paru-bin.git"),
        1
    );
    assert_eq!(
        sandbox.calls("local "),
        1,
        "the built package is installed with pacman -U"
    );
    assert_eq!(
        sandbox.calls("rustup"),
        0,
        "paru-bin needs no Rust toolchain"
    );
}

#[test]
fn the_gtk_cursor_size_changes_without_touching_other_settings() {
    let sandbox = Sandbox::new("gtk-cursor");
    let files = [
        ".gtkrc-2.0",
        ".config/gtk-3.0/settings.ini",
        ".config/gtk-4.0/settings.ini",
    ];
    for file in files {
        let path = sandbox.home.join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "gtk-cursor-theme-size=32\nother-setting=preserved\n").unwrap();
    }
    let xsettings = sandbox.home.join(".config/xsettingsd/xsettingsd.conf");
    fs::create_dir_all(xsettings.parent().unwrap()).unwrap();
    fs::write(&xsettings, "Gtk/CursorThemeSize 32\nOther/Setting 1\n").unwrap();
    sandbox.with(&Answers::new(None), |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        crate::kde_plasma::set_gtk_cursor_size(ctx).unwrap();
    });
    for file in files {
        assert_eq!(
            fs::read_to_string(sandbox.home.join(file)).unwrap(),
            "gtk-cursor-theme-size=24\nother-setting=preserved\n"
        );
    }
    assert_eq!(
        fs::read_to_string(xsettings).unwrap(),
        "Gtk/CursorThemeSize 24\nOther/Setting 1\n"
    );
}

#[test]
fn the_refind_banner_keeps_its_historical_colors_and_size() {
    let sandbox = Sandbox::new("refind-images");
    let output = sandbox.root.join("theme");
    fs::create_dir_all(&output).unwrap();
    sandbox.with(&Answers::new(None), |ctx| {
        crate::refind::generate_images(ctx, &output).unwrap()
    });
    let tool = if Path::new("/usr/bin/magick").exists() {
        "magick"
    } else {
        "convert"
    };
    let info = std::process::Command::new(tool)
        .arg(output.join("banner.png"))
        .args([
            "-format",
            "%wx%h|%[pixel:p{0,0}]|%[pixel:p{0,1079}]",
            "info:",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&info.stdout),
        "1920x1080|srgb(0,0,0)|srgb(255,78,173)"
    );
    for name in ["selection_big.png", "selection_small.png"] {
        assert!(
            fs::metadata(output.join(name)).unwrap().len() > 0,
            "{name} is empty"
        );
    }

    // A later install writes the same bytes, even a second later.
    let again = sandbox.root.join("theme-again");
    fs::create_dir_all(&again).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100));
    sandbox.with(&Answers::new(None), |ctx| {
        crate::refind::generate_images(ctx, &again).unwrap()
    });
    for name in ["banner.png", "selection_big.png", "selection_small.png"] {
        assert_eq!(
            fs::read(output.join(name)).unwrap(),
            fs::read(again.join(name)).unwrap(),
            "{name} changed between two runs"
        );
    }
}

#[test]
fn kde_keys_in_this_module_are_the_approved_values() {
    // These values come from the approved desktop design; changing one changes the desktop.
    let sandbox = Sandbox::new("kde-keys");
    system(&sandbox);
    sandbox.with(&Answers::new(None), |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        for (setting, value) in crate::kde_plasma::kde_settings() {
            ctx.set(setting, value).unwrap();
        }
    });
    for (path, value) in [
        ("kwinrc/Windows/ElectricBorderPushbackPixels", "0"),
        ("kwinrc/EdgeBarrier/CornerBarrier", "false"),
        ("kwinrc/EdgeBarrier/EdgeBarrier", "0"),
        ("kwinrc/Effect-overview/BorderActivate", "9"),
        ("kwinrc/Windows/PerOutputVirtualDesktops", "true"),
        ("kcminputrc/Mouse/cursorSize", "24"),
        (
            "kdeglobals/General/font",
            "IBM Plex Sans,12,-1,5,50,0,0,0,0,0",
        ),
        (
            "kdeglobals/General/fixed",
            "BlexMono Nerd Font Mono,12,-1,5,50,0,0,0,0,0",
        ),
        ("kdeglobals/KDE/AnimationDurationFactor", "0.25"),
        ("kdeglobals/KDE/SingleClick", "true"),
        ("kwinrc/Plugins/inayos-panelsEnabled", "true"),
        ("plasmanotifyrc/DoNotDisturb/WhenFullscreen", "false"),
        (
            "kglobalshortcutsrc/services/org.kde.krunner.desktop/_launch",
            "Search\tAlt+F2\tMeta+Space",
        ),
        ("baloofilerc/Basic Settings/Indexing-Enabled", "false"),
        ("plasmaparc/General/RaiseMaximumVolume", "true"),
        ("powerdevilrc/AC/SuspendAndShutdown/LidAction", "0"),
        ("powerdevilrc/Battery/SuspendAndShutdown/LidAction", "1"),
        (
            "powerdevilrc/LowBattery/SuspendAndShutdown/InhibitLidActionWhenExternalMonitorPresent",
            "false",
        ),
        (
            "kcminputrc/Libinput/12771/4657/Wooting Wooting Two HE (ARM) Mouse/ScrollMethod",
            "0",
        ),
        (
            "kcminputrc/Libinput/Defaults/Pointer/PointerAcceleration",
            "1.000",
        ),
        (
            "kcminputrc/Libinput/Defaults/Pointer/PointerAccelerationProfile",
            "1",
        ),
        (
            "kcminputrc/Libinput/Defaults/Touchpad/PointerAcceleration",
            "1.000",
        ),
        (
            "kcminputrc/Libinput/Defaults/Touchpad/PointerAccelerationProfile",
            "1",
        ),
        (
            "kcminputrc/Libinput/Defaults/Touchpad/NaturalScroll",
            "true",
        ),
        ("kcminputrc/Libinput/Defaults/Touchpad/TapDragLock", "true"),
        ("kcminputrc/Libinput/Defaults/Touchpad/ClickMethod", "2"),
    ] {
        assert_eq!(kde(&sandbox, path).as_deref(), Some(value), "{path}");
    }
}

/// Stand-ins for the KDE programs the KDE Plasma module runs. Plasma is not running, so
/// the layout script stops with its "not active" status.
fn plasma(sandbox: &Sandbox) {
    system(sandbox);
    sandbox.fake("plasmashell", "exit 0");
    sandbox.fake(
        "plasma-apply-colorscheme",
        r#"printf 'plasma-apply-colorscheme %s\n' "$*" >> "$INAYOS_TEST_LOG""#,
    );
    sandbox.fake(
        "plasma-apply-lookandfeel",
        r#"printf 'plasma-apply-lookandfeel %s\n' "$*" >> "$INAYOS_TEST_LOG"
mkdir -p "$INAYOS_TEST_ROOT/kde/kdeglobals/KDE"
printf '%s' "$2" > "$INAYOS_TEST_ROOT/kde/kdeglobals/KDE/LookAndFeelPackage""#,
    );
    sandbox.fake(
        "plasma-apply-cursortheme",
        r#"printf 'plasma-apply-cursortheme %s\n' "$*" >> "$INAYOS_TEST_LOG"
for theme; do :; done
mkdir -p "$INAYOS_TEST_ROOT/kde/kcminputrc/Mouse"
printf '%s' "$theme" > "$INAYOS_TEST_ROOT/kde/kcminputrc/Mouse/cursorTheme""#,
    );
    sandbox.fake(
        "fc-match",
        r#"case "$3" in
    sans-serif) printf 'IBM Plex Sans' ;;
    serif) printf 'IBM Plex Serif' ;;
    monospace) printf 'BlexMono Nerd Font Mono' ;;
esac"#,
    );
    sandbox.fake("qdbus6", "exit 1");
    let runtime = sandbox.root.join("runtime");
    fs::create_dir_all(&runtime).unwrap();
    sandbox.shell.set_var("XDG_RUNTIME_DIR", runtime);
}

#[test]
fn a_rule_file_that_kwin_rewrote_keeps_its_order_list() {
    let sandbox = Sandbox::new("kde-plasma-order");
    plasma(&sandbox);
    let rules = sandbox.root.join("kde/kwinrulesrc/General");
    fs::create_dir_all(&rules).unwrap();
    fs::write(rules.join("Order"), "user-rule").unwrap();
    let modules: [&dyn Module; 1] = [&crate::KdePlasma];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    assert_eq!(
        kde(&sandbox, "kwinrulesrc/General/Order").as_deref(),
        Some("user-rule,inayos-picture-in-picture")
    );
    assert_eq!(kde(&sandbox, "kwinrulesrc/General/rules"), None);
    assert_eq!(kde(&sandbox, "kwinrulesrc/General/count"), None);
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

#[test]
fn kde_plasma_applies_breeze_dark_with_the_pink_accent_and_remove_puts_it_back() {
    let sandbox = Sandbox::new("kde-plasma");
    plasma(&sandbox);
    // The fake `kreadconfig6` reads its own store, and the module reads the theme tools'
    // changes from the configuration files. `kwriteconfig6` therefore writes to both.
    fs::rename(
        sandbox.root.join("bin/kwriteconfig6"),
        sandbox.root.join("bin/kwriteconfig6-store"),
    )
    .unwrap();
    sandbox.fake(
        "kwriteconfig6",
        r#"parse() {
    while [ "$#" -gt 0 ]; do
        case "$1" in
            --file) file="$2"; shift 2 ;;
            --group) groups="$groups[$2]"; shift 2 ;;
            --key) key="$2"; shift 2 ;;
            --delete) return 1 ;;
            --*) shift ;;
            *) value="$1"; shift ;;
        esac
    done
}
if parse "$@"; then printf '%s\n%s=%s\n' "$groups" "$key" "$value" >> "$XDG_CONFIG_HOME/$file"; fi
exec kwriteconfig6-store "$@""#,
    );
    let rules = sandbox.root.join("kde/kwinrulesrc/General");
    fs::create_dir_all(&rules).unwrap();
    fs::write(rules.join("rules"), "user-rule").unwrap();
    fs::write(rules.join("count"), "1").unwrap();
    // The theme tools change keys beyond the theme, here the colors derived from the
    // accent and the window decoration, and create the Plasma style file.
    let config = sandbox.home.join(".config");
    fs::create_dir_all(&config).unwrap();
    let kdeglobals = "[Colors:View]\nBackgroundNormal=35,38,41\n";
    let kwinrc = "[org.kde.kdecoration2]\ntheme=Oxygen\n";
    fs::write(config.join("kdeglobals"), kdeglobals).unwrap();
    fs::write(config.join("kwinrc"), kwinrc).unwrap();
    let applets = config.join("plasma-org.kde.plasma.desktop-appletsrc");
    fs::write(&applets, "default panels\n").unwrap();
    sandbox.fake(
        "plasma-apply-colorscheme",
        r#"printf 'plasma-apply-colorscheme %s\n' "$*" >> "$INAYOS_TEST_LOG"
printf '[Colors:View]\nBackgroundNormal=255,78,173\n' >> "$XDG_CONFIG_HOME/kdeglobals""#,
    );
    sandbox.fake(
        "plasma-apply-lookandfeel",
        r#"printf 'plasma-apply-lookandfeel %s\n' "$*" >> "$INAYOS_TEST_LOG"
mkdir -p "$INAYOS_TEST_ROOT/kde/kdeglobals/KDE"
printf '%s' "$2" > "$INAYOS_TEST_ROOT/kde/kdeglobals/KDE/LookAndFeelPackage"
printf '[org.kde.kdecoration2]\ntheme=Breeze\n' >> "$XDG_CONFIG_HOME/kwinrc"
printf '[Theme]\nname=default\n' > "$XDG_CONFIG_HOME/plasmarc"
mkdir -p "$INAYOS_TEST_ROOT/kde/plasmarc/Theme"
printf 'default' > "$INAYOS_TEST_ROOT/kde/plasmarc/Theme/name""#,
    );
    // GSettings keeps only the values the user set in dconf, and `get` would show the
    // schema default for the others.
    sandbox.fake(
        "gsettings",
        r#"printf 'gsettings %s\n' "$*" >> "$INAYOS_TEST_LOG"
store="$INAYOS_TEST_ROOT/dconf"
case "$1" in
    list-keys) printf 'cursor-size\n' ;;
    list-schemas) printf 'org.gnome.desktop.interface /org/gnome/desktop/interface/\n' ;;
    set) mkdir -p "$store" && printf '%s' "$4" > "$store/$3" ;;
    reset) rm -f "$store/$3" ;;
esac"#,
    );
    sandbox.fake(
        "dconf",
        r#"[ "$1" = read ] && cat "$INAYOS_TEST_ROOT/dconf/${2##*/}" 2>/dev/null; exit 0"#,
    );
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&crate::KdePlasma], &[&crate::KdePlasma])
            })
            .unwrap();
    }
    assert_ne!(
        fs::read_to_string(config.join("kdeglobals")).unwrap(),
        kdeglobals
    );
    // Another module, or the person in System Settings, writes to the same file later.
    let later = "[General]\nTerminalApplication=ghostty\n";
    let mut themed = fs::read_to_string(config.join("kdeglobals")).unwrap();
    themed.push_str(later);
    fs::write(config.join("kdeglobals"), &themed).unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("dconf/cursor-size")).ok(),
        Some("24".to_owned())
    );
    for package in ["otf-ibm-plex", "ttf-ibmplex-mono-nerd", "libinput"] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    for plugin in [
        "90-inayos-pointer-sensitivity.lua",
        "90-inayos-super-ctrl-right-click.lua",
    ] {
        assert!(
            system_file(&sandbox, &format!("/etc/libinput/plugins/{plugin}")).is_some(),
            "{plugin} was not installed"
        );
    }
    assert!(sandbox.calls("plasma-apply-lookandfeel --apply inayos.desktop") > 0);
    assert!(sandbox.calls("plasma-apply-colorscheme --accent-color #ff4ead") > 0);
    assert!(sandbox.calls("plasma-apply-cursortheme --size 24 breeze_cursors") > 0);
    for key in ["AccentColor", "LastUsedCustomAccentColor"] {
        assert_eq!(
            kde(&sandbox, &format!("kdeglobals/General/{key}")).as_deref(),
            Some("255,78,173")
        );
    }
    // The picture-in-picture rule joins the rules already there, once.
    assert_eq!(
        kde(&sandbox, "kwinrulesrc/General/rules").as_deref(),
        Some("user-rule,inayos-picture-in-picture")
    );
    assert_eq!(
        kde(&sandbox, "kwinrulesrc/General/count").as_deref(),
        Some("2")
    );
    assert_eq!(
        kde(
            &sandbox,
            "kwinrulesrc/inayos-picture-in-picture/excludefromcapture"
        )
        .as_deref(),
        Some("true")
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-plasma-layout.service"
    ));

    // The layout script rewrote the panels of the running Plasma shell, which writes the
    // panels it holds when it stops and reads the panel file when it starts.
    fs::write(&applets, "inayos panels\n").unwrap();
    fs::write(
        sandbox
            .root
            .join("units/user/plasma-plasmashell.service.active"),
        "",
    )
    .unwrap();
    fs::rename(
        sandbox.root.join("bin/systemctl"),
        sandbox.root.join("bin/systemctl-units"),
    )
    .unwrap();
    sandbox.fake(
        "systemctl",
        r#"applets="$HOME/.config/plasma-org.kde.plasma.desktop-appletsrc"
case "$*" in
    "--user stop plasma-plasmashell.service") printf 'inayos panels\n' > "$applets" ;;
    "--user start plasma-plasmashell.service") cp "$applets" "$INAYOS_TEST_ROOT/shown-panels" ;;
esac
exec systemctl-units "$@""#,
    );
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::KdePlasma], &[&crate::KdePlasma], false)
        })
        .unwrap();
    assert_eq!(fs::read_to_string(&applets).unwrap(), "default panels\n");
    assert_eq!(
        fs::read_to_string(sandbox.root.join("shown-panels")).unwrap(),
        "default panels\n",
        "the restarted Plasma shell did not read the restored panels"
    );
    assert_eq!(
        kde(&sandbox, "kwinrulesrc/General/rules").as_deref(),
        Some("user-rule")
    );
    assert_eq!(kde(&sandbox, "kdeglobals/General/AccentColor"), None);
    // Only the keys the theme tools changed go back, each through `kwriteconfig6`, and
    // the file keeps what was written after install.
    for restore in [
        "kwriteconfig6 --notify --file kdeglobals --group Colors:View --key BackgroundNormal 35,38,41\n",
        "kwriteconfig6 --notify --file kwinrc --group org.kde.kdecoration2 --key theme Oxygen\n",
        "kwriteconfig6 --file plasmarc --group Theme --key name --delete\n",
    ] {
        assert_eq!(sandbox.calls(restore), 1, "{restore}");
    }
    assert_eq!(sandbox.calls("--key TerminalApplication"), 0);
    assert!(
        fs::read_to_string(config.join("kdeglobals"))
            .unwrap()
            .contains(later)
    );
    assert!(
        !sandbox.root.join("dconf/cursor-size").exists(),
        "remove stored the schema default instead of resetting the key"
    );
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-plasma-layout.service"
    ));
    assert!(
        system_file(
            &sandbox,
            "/etc/libinput/plugins/90-inayos-pointer-sensitivity.lua"
        )
        .is_none()
    );
}

/// Records the KDE Plasma panels, then rewrites the panel file as the layout script does.
struct PlasmaPanelsRewrite;

impl Module for PlasmaPanelsRewrite {
    fn name(&self) -> &'static str {
        "plasma-panels-rewrite"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        let runtime = ctx.home.join("runtime");
        ctx.record_setting(crate::Setting::PlasmaPanels {
            runtime: runtime.to_string_lossy().into_owned(),
        })?;
        fs::write(
            ctx.home
                .join(".config/plasma-org.kde.plasma.desktop-appletsrc"),
            "inayos panels\n",
        )?;
        Ok(())
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn a_failed_panel_restore_starts_the_stopped_plasma_shell_again() {
    let sandbox = Sandbox::new("plasma-panels-failure");
    let applets = sandbox
        .home
        .join(".config/plasma-org.kde.plasma.desktop-appletsrc");
    fs::create_dir_all(applets.parent().unwrap()).unwrap();
    fs::write(&applets, "default panels\n").unwrap();
    // The Plasma shell runs. Something takes the panel file's place while it is stopped,
    // so the restore cannot write it.
    sandbox.fake(
        "systemctl",
        r#"printf 'systemctl %s\n' "$*" >> "$INAYOS_TEST_LOG"
applets="$HOME/.config/plasma-org.kde.plasma.desktop-appletsrc"
case "$*" in
    "--user stop plasma-plasmashell.service") rm -f "$applets" && mkdir "$applets" ;;
esac
exit 0"#,
    );
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&PlasmaPanelsRewrite], &[&PlasmaPanelsRewrite])
        })
        .unwrap();
    let removed = sandbox.with(&io, |ctx| {
        runner::remove(ctx, &[&PlasmaPanelsRewrite], &[&PlasmaPanelsRewrite], false)
    });
    assert!(
        removed.is_err(),
        "the failed panel restore was not reported"
    );
    assert_eq!(
        sandbox.calls("systemctl --user stop plasma-plasmashell.service"),
        1
    );
    assert_eq!(
        sandbox.calls("systemctl --user start plasma-plasmashell.service"),
        1,
        "the Plasma shell stayed stopped"
    );
}

/// The shared system fakes with sshd.service active or not, and failing to start or not.
fn sshd(sandbox: &Sandbox, active: bool, broken: bool) {
    system(sandbox);
    if active {
        fs::write(
            unit_file(sandbox, ServiceScope::System, "sshd.service.active"),
            "",
        )
        .unwrap();
    }
    if broken {
        fs::write(
            unit_file(sandbox, ServiceScope::System, "sshd.service.broken"),
            "",
        )
        .unwrap();
    }
}

#[test]
fn a_stopped_ssh_server_stays_off_unless_you_turn_it_on() {
    let without_terminal = Sandbox::new("ssh-server-off");
    sshd(&without_terminal, false, false);
    let io = Answers::new(None);
    without_terminal
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::SshServer], &[&crate::SshServer])
        })
        .unwrap();
    assert!(!enabled(
        &without_terminal,
        ServiceScope::System,
        "sshd.service"
    ));
    assert!(
        io.notes
            .borrow()
            .iter()
            .any(|note| note.contains("Run: sudo systemctl enable --now sshd.service"))
    );

    let declined = Sandbox::new("ssh-server-declined");
    sshd(&declined, false, false);
    declined
        .with(&Answers::new(Some(false)), |ctx| {
            runner::install(ctx, &[&crate::SshServer], &[&crate::SshServer])
        })
        .unwrap();
    assert!(!enabled(&declined, ServiceScope::System, "sshd.service"));

    let accepted = Sandbox::new("ssh-server-on");
    sshd(&accepted, false, false);
    accepted
        .with(&Answers::new(Some(true)), |ctx| {
            runner::install(ctx, &[&crate::SshServer], &[&crate::SshServer])
        })
        .unwrap();
    assert!(enabled(&accepted, ServiceScope::System, "sshd.service"));
    assert!(active(&accepted, ServiceScope::System, "sshd.service"));
    assert_eq!(accepted.calls("sudo systemctl start sshd.service"), 1);
}

#[test]
fn a_failed_ssh_server_start_warns_without_stopping_the_install() {
    let sandbox = Sandbox::new("ssh-server-broken");
    sshd(&sandbox, false, true);
    let io = Answers::new(Some(true));
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::SshServer], &[&crate::SshServer])
        })
        .unwrap();
    assert!(
        io.notes
            .borrow()
            .iter()
            .any(|note| note.starts_with("WARNING: the OpenSSH server did not start"))
    );
}

#[test]
fn a_running_ssh_server_restarts_without_a_question() {
    let sandbox = Sandbox::new("ssh-server-running");
    sshd(&sandbox, true, false);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::SshServer], &[&crate::SshServer])
        })
        .unwrap();
    assert!(io.questions.borrow().is_empty());
    assert_eq!(sandbox.calls("sudo systemctl restart sshd.service"), 1);
}

#[test]
fn media_makes_vlc_the_player_and_loupe_the_image_viewer() {
    let sandbox = Sandbox::new("media");
    system(&sandbox);
    let applications = sandbox.root.join("share/applications");
    fs::create_dir_all(&applications).unwrap();
    fs::write(
        applications.join("vlc.desktop"),
        "[Desktop Entry]\nName=VLC\nMimeType=application/x-iso9660-image;audio/mpeg;video/mp4;x-content/video-dvd;\n",
    )
    .unwrap();
    fs::write(
        applications.join("org.gnome.Loupe.desktop"),
        "[Desktop Entry]\nName=Loupe\nMimeType=image/png;image/jpeg;\n",
    )
    .unwrap();
    sandbox
        .shell
        .set_var("XDG_DATA_DIRS", sandbox.root.join("share"));
    let defaults = sandbox.root.join("kde/mimeapps.list/Default Applications");
    fs::create_dir_all(defaults.join("text")).unwrap();
    fs::write(defaults.join("text/plain"), "other.desktop").unwrap();
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::Media], &[&crate::Media])
        })
        .unwrap();
    for package in ["vlc", "loupe"] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    let default = |kind: &str| {
        kde(
            &sandbox,
            &format!("mimeapps.list/Default Applications/{kind}"),
        )
    };
    assert_eq!(default("audio/mpeg").as_deref(), Some("vlc.desktop"));
    assert_eq!(default("video/mp4").as_deref(), Some("vlc.desktop"));
    assert_eq!(default("application/x-iso9660-image"), None);
    assert_eq!(default("x-content/video-dvd"), None);
    assert_eq!(
        default("image/png").as_deref(),
        Some("org.gnome.Loupe.desktop")
    );
    assert_eq!(
        default("image/jpeg").as_deref(),
        Some("org.gnome.Loupe.desktop")
    );
    assert_eq!(default("text/plain").as_deref(), Some("other.desktop"));
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &[&crate::Media]));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

#[test]
fn flatpak_adds_flathub_once_and_remove_deletes_it() {
    let sandbox = Sandbox::new("flatpak");
    system(&sandbox);
    sandbox.fake(
        "flatpak",
        r#"printf 'flatpak %s\n' "$*" >> "$INAYOS_TEST_LOG"
remotes="$INAYOS_TEST_ROOT/flatpak"
mkdir -p "$remotes"
case "$1" in
    remotes) ls "$remotes" ;;
    remote-add) for name; do :; done; : > "$remotes/$4" ;;
    remote-delete) rm -f "$remotes/$3" ;;
esac"#,
    );
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&crate::Flatpak], &[&crate::Flatpak])
            })
            .unwrap();
    }
    assert!(sandbox.installed("flatpak"));
    assert!(sandbox.root.join("flatpak/flathub").exists());
    assert_eq!(
        sandbox.calls(
            "sudo flatpak remote-add --system --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo"
        ),
        1
    );
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::Flatpak], &[&crate::Flatpak], false)
        })
        .unwrap();
    assert!(!sandbox.root.join("flatpak/flathub").exists());
    assert!(!sandbox.installed("flatpak"));
}

#[test]
fn runtimes_install_bun_with_its_installer_once_when_asked() {
    let sandbox = Sandbox::new("bun-installer");
    sandbox.fake(
        "rustup",
        r#"[ "$#" -eq 1 ] && echo 'stable-x86_64-unknown-linux-gnu (default)'; exit 0"#,
    );
    fs::write(
        sandbox.root.join("bun-installer"),
        r#"printf 'installer SHELL=%s\n' "$SHELL" >> "$INAYOS_TEST_LOG"
mkdir -p "$BUN_INSTALL/bin" && : > "$BUN_INSTALL/bin/bun" && chmod +x "$BUN_INSTALL/bin/bun"
"#,
    )
    .unwrap();
    sandbox.fake(
        "curl",
        r#"printf 'curl %s\n' "$*" >> "$INAYOS_TEST_LOG"
cat "$INAYOS_TEST_ROOT/bun-installer""#,
    );
    let runtimes = Runtimes {
        bun_installer: true,
    };
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| runner::install(ctx, &[&runtimes], &[&runtimes]))
            .unwrap();
    }
    assert!(sandbox.home.join(".bun/bin/bun").is_file());
    assert!(!sandbox.installed("bun"), "the Bun package was installed");
    assert_eq!(sandbox.calls("curl -fsSL https://bun.sh/install"), 1);
    assert_eq!(sandbox.calls("installer SHELL=/bin/sh"), 1);

    // ~/.bun also holds the global packages people add later, so remove asks first.
    let no = Answers::new(Some(false));
    sandbox
        .with(&no, |ctx| {
            runner::remove(ctx, &[&runtimes], &[&runtimes], false)
        })
        .unwrap();
    let bun = sandbox.home.join(".bun");
    assert!(
        no.questions
            .borrow()
            .iter()
            .any(|question| question.contains(&bun.display().to_string()))
    );
    assert!(bun.join("bin/bun").is_file());
}

#[test]
fn vscodium_installs_its_theme_and_extensions_and_remove_uninstalls_only_those() {
    let sandbox = Sandbox::new("vscodium");
    system(&sandbox);
    // Keeps the installed extensions in a file, and names the packaged theme as VSCodium does.
    sandbox.fake(
        "codium",
        r#"printf 'codium %s\n' "$*" >> "$INAYOS_TEST_LOG"
installed="$INAYOS_TEST_ROOT/extensions"
touch "$installed"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --list-extensions) cat "$installed" ;;
        --install-extension)
            case "$2" in *.vsix) id=inayos.blacknpink-theme ;; *) id="$2" ;; esac
            grep -Fxq "$id" "$installed" || printf '%s\n' "$id" >> "$installed"; shift ;;
        --uninstall-extension) grep -Fxq "$2" "$installed" || { echo "Extension '$2' is not installed." >&2; exit 1; }
            grep -Fxv "$2" "$installed" > "$installed.new"; mv "$installed.new" "$installed"; shift ;;
    esac
    shift
done"#,
    );
    // Installed before, so remove keeps it.
    fs::write(sandbox.root.join("extensions"), "rust-lang.rust-analyzer\n").unwrap();
    sandbox.fake(
        "bsdtar",
        r#"printf 'bsdtar %s\n' "$*" >> "$INAYOS_TEST_LOG"
[ "$3" = -cf ] && : > "$4""#,
    );
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::Vscodium], &[&crate::Vscodium])
        })
        .unwrap();
    for folder in [".config/VSCodium", ".config/VSCodium/User"] {
        let metadata = fs::symlink_metadata(sandbox.home.join(folder)).unwrap();
        assert!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "{folder} was folded"
        );
    }
    for kind in ["text/plain", "text/markdown", "application/json"] {
        assert_eq!(
            kde(
                &sandbox,
                &format!("mimeapps.list/Default Applications/{kind}")
            )
            .as_deref(),
            Some("codium.desktop")
        );
    }
    assert_eq!(sandbox.calls("bsdtar --format zip -cf "), 1);
    assert_eq!(sandbox.calls("codium --force --install-extension "), 1);

    // What VSCodium leaves after installing the theme: its folder with a `__metadata`
    // block added to package.json, and an entry in extensions.json.
    let theme = sandbox
        .home
        .join("dotfiles/vscodium/.config/VSCodium/blacknpink-theme");
    let extensions = sandbox.home.join(".vscode-oss/extensions");
    let installed = extensions.join("inayos.blacknpink-theme-1.0.0");
    fs::create_dir_all(installed.join("themes")).unwrap();
    fs::copy(
        theme.join("themes/blacknpink-color-theme.json"),
        installed.join("themes/blacknpink-color-theme.json"),
    )
    .unwrap();
    let mut package: serde_json::Value =
        serde_json::from_slice(&fs::read(theme.join("package.json")).unwrap()).unwrap();
    assert_eq!(package["version"], "1.0.0");
    package["__metadata"] = serde_json::json!({"installedTimestamp": 1792860770308_u64});
    fs::write(installed.join("package.json"), package.to_string()).unwrap();
    fs::write(
        extensions.join("extensions.json"),
        r#"[{"identifier":{"id":"inayos.blacknpink-theme"},"version":"1.0.0","relativeLocation":"inayos.blacknpink-theme-1.0.0","metadata":{"installedTimestamp":1792860770299}}]"#,
    )
    .unwrap();
    let install = || {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&crate::Vscodium], &[&crate::Vscodium])
            })
            .unwrap()
    };
    install();
    assert_eq!(
        sandbox.calls("codium --force --install-extension "),
        1,
        "a rerun reinstalled the unchanged theme"
    );
    fs::write(installed.join("themes/blacknpink-color-theme.json"), "{}").unwrap();
    install();
    assert_eq!(
        sandbox.calls("codium --force --install-extension "),
        2,
        "a changed installed theme was not replaced"
    );
    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let extensions = calls
        .lines()
        .find(|line| line.starts_with("codium --install-extension jeanp413.open-remote-ssh"))
        .expect("the extensions were not installed");
    let listed = fs::read_to_string(
        sandbox
            .home
            .join("dotfiles/vscodium/.config/VSCodium/extensions.txt"),
    )
    .unwrap();
    for extension in listed.lines().filter(|line| !line.trim().is_empty()) {
        assert!(
            extensions.contains(&format!("--install-extension {}", extension.trim())),
            "{extension} was not installed"
        );
    }

    // An extension that install added and the person uninstalled since has nothing left
    // to undo.
    let installed = fs::read_to_string(sandbox.root.join("extensions")).unwrap();
    fs::write(
        sandbox.root.join("extensions"),
        installed.replace("jeanp413.open-remote-ssh\n", ""),
    )
    .unwrap();
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::Vscodium], &[&crate::Vscodium], false)
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("extensions")).unwrap(),
        "rust-lang.rust-analyzer\n",
        "remove kept an extension that install added, or removed one that was there"
    );
}

/// An Arch boot with the systemd initramfs hooks and rEFInd, which bootvine can join.
fn bootvine_boot(sandbox: &Sandbox) {
    system(sandbox);
    let etc = sandbox.root.join("system/etc");
    fs::create_dir_all(&etc).unwrap();
    fs::write(etc.join("mkinitcpio.conf"), MKINITCPIO).unwrap();
    let boot = sandbox.root.join("system/boot");
    fs::create_dir_all(&boot).unwrap();
    fs::write(boot.join("refind_linux.conf"), REFIND_LINUX).unwrap();
    sandbox.fake(
        "mkinitcpio",
        r#"printf 'mkinitcpio %s\n' "$*" >> "$INAYOS_TEST_LOG""#,
    );
}

#[test]
fn bootvine_installs_its_package_then_joins_the_initramfs_and_the_first_boot_entry() {
    let sandbox = Sandbox::new("bootvine");
    bootvine_boot(&sandbox);
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&crate::Bootvine], &[&crate::Bootvine])
            })
            .unwrap();
    }
    assert_eq!(
        sandbox.calls("sudo mkinitcpio -P"),
        1,
        "the initramfs was not rebuilt exactly once, when HOOKS gained the hook"
    );
    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let hooks_written = calls
        .lines()
        .position(|line| line.starts_with("sudo install -m") && line.ends_with("mkinitcpio.conf"))
        .unwrap();
    let package_installed = calls
        .lines()
        .position(|line| line == "pacman -S --needed --noconfirm bootvine")
        .unwrap();
    assert!(
        package_installed < hooks_written,
        "HOOKS named the hook before its package was installed"
    );
    assert!(
        system_file(&sandbox, "/etc/mkinitcpio.conf")
            .unwrap()
            .contains("HOOKS=(base systemd sd-bootvine autodetect")
    );
    let colors = "vt.default_red=0,248,74,250,96,255,34,208,42,252,134,253,147,255,103,240 \
                  vt.default_grn=0,113,222,204,165,78,211,214,45,165,239,224,197,133,232,242 \
                  vt.default_blu=0,113,128,21,250,173,238,224,54,165,172,71,253,200,249,247";
    assert_eq!(
        system_file(&sandbox, "/boot/refind_linux.conf").unwrap(),
        REFIND_LINUX
            .replacen("\"rw root", "\"quiet rw root", 1)
            .replace(
                "efi_pstore.pstore_disable=0\"",
                &format!("efi_pstore.pstore_disable=0 {colors}\"")
            )
            .replace(
                "resume_offset=244412416\" single",
                &format!("resume_offset=244412416 {colors}\" single")
            )
            .replace(
                "resume=UUID=31f4 resume_offset=244412416\"\n",
                &format!("resume=UUID=31f4 resume_offset=244412416 {colors}\"\n")
            ),
        "every entry did not get the Black & Pink console colors"
    );
    assert_eq!(
        system_file(&sandbox, "/etc/bootvine.toml").unwrap(),
        String::from_utf8_lossy(embedded_dotfiles::BOOTVINE_ASSETS.bootvine_toml.content)
    );
    for service in ["bootvine.service", "bootvine-shutdown.service"] {
        assert!(
            enabled(&sandbox, ServiceScope::System, service),
            "{service} is not enabled"
        );
    }
}

#[test]
fn removing_bootvine_takes_its_hook_out_of_the_initramfs_before_the_package() {
    let sandbox = Sandbox::new("bootvine-remove");
    bootvine_boot(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::Bootvine];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    fs::remove_file(sandbox.root.join("calls")).unwrap();

    let report = sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &modules, &[&crate::Bootvine], false)
        })
        .unwrap();
    assert!(!report.unanswered);
    assert_eq!(
        system_file(&sandbox, "/etc/mkinitcpio.conf").unwrap(),
        MKINITCPIO,
        "remove did not take out exactly the bootvine hook"
    );
    assert_eq!(sandbox.calls("sudo mkinitcpio -P"), 1);
    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let rebuild = calls.find("sudo mkinitcpio -P").unwrap();
    assert!(
        calls.find("sudo install -m 0644").unwrap() < rebuild,
        "the initramfs was rebuilt while HOOKS still named sd-bootvine"
    );
    assert!(
        rebuild < calls.find("pacman -Rs --noconfirm bootvine").unwrap(),
        "the package left before the initramfs stopped using its hook"
    );
    for service in ["bootvine.service", "bootvine-shutdown.service"] {
        assert!(!enabled(&sandbox, ServiceScope::System, service));
    }
}

#[test]
fn bootvine_needs_the_systemd_hook() {
    let sandbox = Sandbox::new("bootvine-busybox");
    bootvine_boot(&sandbox);
    fs::write(
        sandbox.root.join("system/etc/mkinitcpio.conf"),
        "HOOKS=(base udev autodetect block filesystems fsck)\n",
    )
    .unwrap();
    let io = Answers::new(None);
    let error = sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::Bootvine], &[&crate::Bootvine])
        })
        .unwrap_err();
    assert!(
        error.to_string().contains("needs the systemd hook"),
        "{error}"
    );
    assert_eq!(sandbox.calls("bootvine"), 0, "bootvine was installed");
}

/// The new system an installer-mode run builds, before rEFInd: the EFI system partition
/// mounted at `/efi`, `/boot` inside the Btrfs root subvolume, and the firmware's boot
/// entries reachable. `findmnt` answers from `mounts/` and `fstab-root`, and gives the
/// partition GUID of the EFI system partition, `efibootmgr`
/// keeps the entries in `nvram`, and `refind-install` copies what the real one copies for
/// a Btrfs `/boot` and adds its entry unless the firmware has it.
fn unbooted_disk(sandbox: &Sandbox) {
    bootvine_boot(sandbox);
    fs::remove_file(sandbox.root.join("system/boot/refind_linux.conf")).unwrap();
    let mounts = sandbox.root.join("mounts");
    fs::create_dir_all(&mounts).unwrap();
    fs::write(mounts.join("efi"), "vfat").unwrap();
    fs::write(mounts.join("boot"), "btrfs").unwrap();
    fs::create_dir_all(mounts.join("sys/firmware/efi")).unwrap();
    fs::write(mounts.join("sys/firmware/efi/efivars"), "efivarfs").unwrap();
    fs::create_dir_all(sandbox.root.join("system/efi")).unwrap();
    fs::write(
        sandbox.root.join("fstab-root"),
        "UUID=5e2a btrfs rw,relatime,compress=zstd:3,ssd,subvolid=256,subvol=/@\n",
    )
    .unwrap();
    fs::write(
        sandbox.root.join("nvram"),
        "Boot0001* Windows Boot Manager\tHD(1,GPT)\n",
    )
    .unwrap();
    sandbox.fake(
        "findmnt",
        r#"printf 'findmnt %s\n' "$*" >> "$INAYOS_TEST_LOG"
case " $* " in
    *" --fstab "*) cat "$INAYOS_TEST_ROOT/fstab-root" ;;
    *" --output PARTUUID --mountpoint /efi "*) printf '2f5a0c1e-0000-4000-8000-000000000001\n' ;;
    *" --mountpoint "*) for mount; do :; done; cat "$INAYOS_TEST_ROOT/mounts/${mount#/}" 2>/dev/null ;;
    *" --target /boot "*) cat "$INAYOS_TEST_ROOT/mounts/boot" ;;
    *) exit 1 ;;
esac"#,
    );
    sandbox.fake(
        "efibootmgr",
        r#"printf 'efibootmgr %s\n' "$*" >> "$INAYOS_TEST_LOG"
nvram="$INAYOS_TEST_ROOT/nvram"
case "$*" in
    "" | -v) printf 'BootOrder: 0001\n'; cat "$nvram" ;;
    *--delete-bootnum*) grep -v "^Boot$3" "$nvram" > "$nvram.new"; mv "$nvram.new" "$nvram" ;;
esac"#,
    );
    sandbox.fake(
        "refind-install",
        r#"printf 'refind-install %s\n' "$*" >> "$INAYOS_TEST_LOG"
refind="$INAYOS_TEST_ROOT/system/efi/EFI/refind"
mkdir -p "$refind/drivers_x64" "$INAYOS_TEST_ROOT/system/efi/EFI/tools"
: > "$refind/refind_x64.efi"; : > "$refind/drivers_x64/btrfs_x64.efi"
printf 'timeout 20\n' > "$refind/refind.conf"
grep -q 'rEFInd Boot Manager' "$INAYOS_TEST_ROOT/nvram" ||
    printf 'Boot0003* rEFInd Boot Manager\tHD(1,GPT)/\\EFI\\refind\\refind_x64.efi\n' >> "$INAYOS_TEST_ROOT/nvram""#,
    );
}

const BOOT_MODULES: [&dyn Module; 2] = [&crate::Refind, &crate::Bootvine];

#[test]
fn installer_mode_installs_refind_with_entries_for_the_new_root_and_bootvine_joins_them() {
    let sandbox = Sandbox::new("refind-installer");
    unbooted_disk(&sandbox);
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with_mode(&io, RunMode::Installer, |ctx| {
                runner::install(ctx, &BOOT_MODULES, &BOOT_MODULES)
            })
            .unwrap();
    }
    assert!(sandbox.installed("refind"));
    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let written = calls
        .lines()
        .position(|line| line.contains(" install -m") && line.ends_with("refind_linux.conf"))
        .unwrap();
    let installed = calls
        .lines()
        .position(|line| line == "refind-install --yes")
        .unwrap();
    assert!(
        written < installed,
        "refind-install ran before the entries existed, so it would copy the installer's own kernel options"
    );
    let root = "root=UUID=5e2a rw rootflags=subvol=/@";
    let config = system_file(&sandbox, "/boot/refind_linux.conf").unwrap();
    let entries: Vec<&str> = config.lines().collect();
    assert_eq!(entries.len(), 2);
    assert!(
        entries[0].contains(&format!("\"quiet {root} vt.default_red=")),
        "{config}"
    );
    assert!(
        entries[1].contains(&format!("\"{root} single vt.default_red=")),
        "{config}"
    );
    assert!(
        system_file(&sandbox, "/efi/EFI/refind/refind.conf")
            .unwrap()
            .ends_with(INAYOS_REFIND_BLOCK)
    );
    // The block's include of the boot guard's file never points at a missing file.
    assert!(
        system_file(&sandbox, "/efi/EFI/refind/inayos-guard.conf")
            .unwrap()
            .starts_with('#')
    );
    assert_eq!(
        fs::read_to_string(sandbox.root.join("nvram"))
            .unwrap()
            .matches("rEFInd Boot Manager")
            .count(),
        1
    );
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| {
        runner::verify(ctx, &BOOT_MODULES)
    });
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    // The firmware still lists Windows, but its files are gone, and the fallback loader
    // is a copy of rEFInd, so rEFInd is the only boot manager left.
    let efi = sandbox.root.join("system/efi/EFI");
    fs::create_dir_all(efi.join("BOOT")).unwrap();
    fs::write(efi.join("BOOT/BOOTX64.EFI"), "rEFInd").unwrap();
    fs::write(efi.join("BOOT/refind.conf"), "").unwrap();
    let error = sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &BOOT_MODULES, &[&crate::Refind], false)
        })
        .unwrap_err();
    assert!(error.to_string().contains("no boot manager"), "{error}");
    assert!(sandbox.installed("refind"));
    assert!(efi.join("refind/refind_x64.efi").exists());
    assert!(
        fs::read_to_string(sandbox.root.join("nvram"))
            .unwrap()
            .contains("rEFInd Boot Manager")
    );
    fs::remove_dir_all(efi.join("BOOT")).unwrap();

    // With Windows Boot Manager back on the partition, remove takes out only rEFInd.
    fs::create_dir_all(efi.join("Microsoft/Boot")).unwrap();
    fs::write(efi.join("Microsoft/Boot/bootmgfw.efi"), "Windows").unwrap();
    fs::write(
        sandbox.root.join("nvram"),
        fs::read_to_string(sandbox.root.join("nvram"))
            .unwrap()
            .replace(
                "Windows Boot Manager\tHD(1,GPT)",
                "Windows Boot Manager\tHD(1,GPT)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi",
            ),
    )
    .unwrap();
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &BOOT_MODULES, &BOOT_MODULES, false)
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("nvram")).unwrap(),
        "Boot0001* Windows Boot Manager\tHD(1,GPT)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi\n",
        "remove did not take out only rEFInd's firmware entry"
    );
    assert!(
        efi.join("Microsoft/Boot/bootmgfw.efi").exists(),
        "remove deleted Windows Boot Manager"
    );
    for path in [
        "/efi/EFI/refind",
        "/efi/EFI/tools",
        "/boot/refind_linux.conf",
    ] {
        assert_eq!(
            fs::symlink_metadata(
                sandbox
                    .root
                    .join("system")
                    .join(path.trim_start_matches('/'))
            )
            .ok()
            .map(|_| path),
            None
        );
    }
    assert!(!sandbox.installed("refind"));
}

#[test]
fn refind_shows_windows_only_through_its_firmware_boot_entry() {
    let sandbox = Sandbox::new("refind-beside-windows");
    unbooted_disk(&sandbox);
    let efi = sandbox.root.join("system/efi/EFI");
    fs::create_dir_all(efi.join("Microsoft/Boot")).unwrap();
    fs::write(efi.join("Microsoft/Boot/bootmgfw.efi"), "Windows").unwrap();
    fs::create_dir_all(efi.join("BOOT")).unwrap();
    fs::write(efi.join("BOOT/BOOTX64.EFI"), "Windows").unwrap();
    let io = Answers::new(None);
    sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &[&crate::Refind], &[&crate::Refind])
        })
        .unwrap();
    let managed = system_file(&sandbox, "/efi/EFI/refind/managed.conf").unwrap();
    assert!(
        managed.starts_with(
            std::str::from_utf8(embedded_dotfiles::REFIND_ASSETS.refind.global_conf.content)
                .unwrap()
                .trim_end()
        )
    );
    assert!(
        managed.ends_with(
            "scanfor internal,external,optical,manual\n\
             dont_scan_files +,EFI/Microsoft/Boot/bootmgfw.efi,2f5a0c1e-0000-4000-8000-000000000001:EFI/BOOT/bootx64.efi\n\
             \n\
             menuentry \"Windows\" {\n    icon EFI/refind/icons/os_win.png\n    firmware_bootnum 0001\n}\n"
        ),
        "{managed}"
    );
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| {
        runner::verify(ctx, &[&crate::Refind])
    });
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    // Windows gave its boot manager another firmware entry, so the stanza is stale.
    fs::write(
        sandbox.root.join("nvram"),
        fs::read_to_string(sandbox.root.join("nvram"))
            .unwrap()
            .replace("Boot0001* Windows", "Boot0005* Windows"),
    )
    .unwrap();
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| {
        runner::verify(ctx, &[&crate::Refind])
    });
    assert!(
        results.iter().any(|(_, result)| result
            .as_ref()
            .is_err_and(|error| error.to_string().contains("managed settings"))),
        "{results:?}"
    );
}

#[test]
fn installer_mode_changes_nothing_when_refind_could_not_boot_the_new_system() {
    for (name, break_disk, reason) in [
        (
            "no-esp",
            (|root: &Path| fs::remove_file(root.join("mounts/efi")).unwrap()) as fn(&Path),
            "no EFI system partition",
        ),
        (
            "no-efivars",
            |root: &Path| fs::remove_file(root.join("mounts/sys/firmware/efi/efivars")).unwrap(),
            "efivarfs is not mounted",
        ),
        (
            "device-root",
            |root: &Path| {
                fs::write(root.join("fstab-root"), "/dev/vda2 btrfs rw,subvol=/@\n").unwrap()
            },
            "not a UUID",
        ),
    ] {
        let sandbox = Sandbox::new(&format!("refind-installer-{name}"));
        unbooted_disk(&sandbox);
        break_disk(&sandbox.root);
        let io = Answers::new(None);
        let error = sandbox
            .with_mode(&io, RunMode::Installer, |ctx| {
                runner::install(ctx, &[&crate::Refind], &[&crate::Refind])
            })
            .unwrap_err();
        assert!(error.to_string().contains(reason), "{name}: {error}");
        assert!(!sandbox.installed("refind"), "{name}");
        assert_eq!(sandbox.calls("refind-install"), 0, "{name}");
        assert_eq!(
            system_file(&sandbox, "/boot/refind_linux.conf"),
            None,
            "{name}"
        );
    }
}

#[test]
fn installer_mode_bootvine_fails_without_the_boot_entries_and_a_running_system_skips_them() {
    let sandbox = Sandbox::new("bootvine-no-entries");
    bootvine_boot(&sandbox);
    fs::remove_file(sandbox.root.join("system/boot/refind_linux.conf")).unwrap();
    let io = Answers::new(None);
    let error = sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &[&crate::Bootvine], &[&crate::Bootvine])
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("/boot/refind_linux.conf was not found"),
        "{error}"
    );

    let sandbox = Sandbox::new("bootvine-live-no-entries");
    bootvine_boot(&sandbox);
    fs::remove_file(sandbox.root.join("system/boot/refind_linux.conf")).unwrap();
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::Bootvine], &[&crate::Bootvine])
        })
        .unwrap();
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &[&crate::Bootvine]));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
    assert!(io.notes.borrow().iter().any(|note| {
        note.contains("Skipping the kernel options because /boot/refind_linux.conf was not found")
    }));
}

#[test]
fn a_running_system_without_refind_gets_no_boot_manager_and_no_theme() {
    let sandbox = Sandbox::new("refind-live-absent");
    unbooted_disk(&sandbox);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::Refind], &[&crate::Refind])
        })
        .unwrap();
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &[&crate::Refind]));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
    assert_eq!(sandbox.calls("refind-install"), 0);
    assert!(!sandbox.installed("refind"));
    assert!(io.notes.borrow().iter().any(
        |note| note == "Skipping the rEFInd theme because a rEFInd installation was not found"
    ));
}

/// A system where NetworkManager runs, with the programs that apply kernel settings.
fn tunable_system(sandbox: &Sandbox) {
    system(sandbox);
    sandbox.fake(
        "sysctl",
        r#"printf 'sysctl %s\n' "$*" >> "$INAYOS_TEST_LOG""#,
    );
    sandbox.fake(
        "nmcli",
        r#"printf 'nmcli %s\n' "$*" >> "$INAYOS_TEST_LOG"
[ "$*" = "-t -f RUNNING general" ] && echo running
exit 0"#,
    );
}

const TUNING_FILES: [&str; 7] = [
    "/etc/systemd/zram-generator.conf",
    "/etc/NetworkManager/conf.d/inayos-system-tuning.conf",
    "/etc/sysctl.d/99-inayos-system-tuning.conf",
    "/etc/systemd/system.conf.d/inayos-system-tuning.conf",
    "/etc/systemd/user.conf.d/inayos-system-tuning.conf",
    "/etc/modules-load.d/inayos-system-tuning.conf",
    "/etc/tmpfiles.d/inayos-system-tuning.conf",
];

/// The `[section]` and `key = value` lines of a configuration file, without comments.
fn settings(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| match line.split_once('=') {
            Some((key, value)) => format!("{}={}", key.trim(), value.trim()),
            None => line.to_owned(),
        })
        .collect()
}

/// The first line of the call log at or after `from` that contains `pattern`.
fn call_position(calls: &[&str], from: usize, pattern: &str) -> usize {
    from + calls[from..]
        .iter()
        .position(|line| line.contains(pattern))
        .unwrap_or_else(|| panic!("no call contains {pattern}"))
}

#[test]
fn system_tuning_installs_the_inayos_settings_and_hands_dns_to_a_running_resolved() {
    let sandbox = Sandbox::new("system-tuning");
    tunable_system(&sandbox);
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&crate::SystemTuning], &[&crate::SystemTuning])
            })
            .unwrap();
    }
    for package in ["zram-generator", "systemd-resolvconf", "networkmanager"] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    let file = |path: &str| settings(&system_file(&sandbox, path).unwrap());
    assert_eq!(
        file(TUNING_FILES[0]),
        [
            "[zram0]",
            "zram-size=ram",
            "compression-algorithm=zstd",
            "swap-priority=100"
        ]
    );
    assert_eq!(file(TUNING_FILES[1]), ["[main]", "dns=systemd-resolved"]);
    assert_eq!(
        file(TUNING_FILES[2]),
        [
            "vm.swappiness=100",
            "vm.page-cluster=0",
            "kernel.printk=3 3 3 3"
        ]
    );
    for manager in [TUNING_FILES[3], TUNING_FILES[4]] {
        assert_eq!(
            file(manager),
            ["[Manager]", "DefaultLimitNOFILE=2048:2097152"]
        );
    }
    assert_eq!(file(TUNING_FILES[5]), ["ntsync"]);
    // At every boot, systemd-tmpfiles writes N to the zswap switch and points
    // /etc/resolv.conf at resolved's stub file.
    assert_eq!(
        system_file(&sandbox, TUNING_FILES[6])
            .unwrap()
            .lines()
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect::<Vec<_>>(),
        [
            "w /sys/module/zswap/parameters/enabled - - - - N",
            "L+ /etc/resolv.conf - - - - ../run/systemd/resolve/stub-resolv.conf"
        ]
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::System,
        "systemd-resolved.service"
    ));
    assert!(active(
        &sandbox,
        ServiceScope::System,
        "systemd-resolved.service"
    ));
    assert_eq!(
        sandbox.calls("sudo install -m"),
        TUNING_FILES.len(),
        "the second install wrote a file again"
    );

    let log = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let calls: Vec<&str> = log.lines().collect();
    let started = call_position(&calls, 0, "systemctl start systemd-resolved.service");
    let handed = call_position(&calls, 0, "NetworkManager/conf.d/inayos-system-tuning.conf");
    let handed = call_position(&calls, handed, "sudo install -m");
    assert!(
        started < handed,
        "NetworkManager was told to use resolved before resolved ran"
    );
    let applied = call_position(&calls, handed, "sysctl --load");
    assert_eq!(
        calls[applied],
        "sudo sysctl --load /etc/sysctl.d/99-inayos-system-tuning.conf"
    );
    call_position(&calls, handed, "modprobe ntsync");
    call_position(&calls, handed, "nmcli general reload");

    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &[&crate::SystemTuning]));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

#[test]
fn removing_system_tuning_gives_dns_back_to_network_manager_before_resolved_stops() {
    for mode in [RunMode::Live, RunMode::Installer] {
        removing_system_tuning_puts_back_dns(mode);
    }
}

/// Installs system tuning in `mode`, boots, which links `/etc/resolv.conf` to resolved's
/// stub file, then removes it on the running system.
fn removing_system_tuning_puts_back_dns(mode: RunMode) {
    let sandbox = Sandbox::new(&format!("system-tuning-remove-{mode:?}"));
    tunable_system(&sandbox);
    let resolv = sandbox.root.join("system/etc/resolv.conf");
    fs::create_dir_all(resolv.parent().unwrap()).unwrap();
    fs::write(&resolv, "nameserver 192.168.1.1\n").unwrap();
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with_mode(&io, mode, |ctx| {
                runner::install(ctx, &[&crate::SystemTuning], &[&crate::SystemTuning])
            })
            .unwrap();
    }
    // The install leaves the file to the rule, which applies at boot.
    assert_eq!(
        fs::read_to_string(&resolv).unwrap(),
        "nameserver 192.168.1.1\n"
    );
    fs::remove_file(&resolv).unwrap();
    std::os::unix::fs::symlink("../run/systemd/resolve/stub-resolv.conf", &resolv).unwrap();
    fs::remove_file(sandbox.root.join("calls")).unwrap();

    let report = sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::SystemTuning], &[&crate::SystemTuning], false)
        })
        .unwrap();
    assert!(!report.unanswered);
    for path in TUNING_FILES {
        assert_eq!(system_file(&sandbox, path), None, "{path} is still there");
    }
    for package in ["zram-generator", "systemd-resolvconf"] {
        assert!(!sandbox.installed(package), "{package} is still installed");
    }
    assert!(!enabled(
        &sandbox,
        ServiceScope::System,
        "systemd-resolved.service"
    ));
    let log = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let calls: Vec<&str> = log.lines().collect();
    let deleted = call_position(
        &calls,
        0,
        "rm -f -- /etc/NetworkManager/conf.d/inayos-system-tuning.conf",
    );
    let reloaded = call_position(&calls, deleted, "nmcli general reload");
    // Only the live install started resolved.
    let stopped = match mode {
        RunMode::Live => "systemctl stop systemd-resolved.service",
        RunMode::Installer => "systemctl disable systemd-resolved.service",
    };
    call_position(&calls, reloaded, stopped);
    let rule_deleted = call_position(
        &calls,
        0,
        "rm -f -- /etc/tmpfiles.d/inayos-system-tuning.conf",
    );
    let restored = call_position(&calls, 0, "/etc/resolv.conf");
    assert!(
        rule_deleted < restored,
        "{mode:?}: resolv.conf came back while the rule could still replace it"
    );
    assert!(
        !fs::symlink_metadata(&resolv).unwrap().is_symlink(),
        "{mode:?}"
    );
    assert_eq!(
        fs::read_to_string(&resolv).unwrap(),
        "nameserver 192.168.1.1\n",
        "{mode:?}"
    );
}

#[test]
fn tailscale_makes_the_user_its_operator_once_and_remove_clears_it() {
    let sandbox = Sandbox::new("tailscale");
    system(&sandbox);
    sandbox.fake(
        "tailscale",
        r#"printf 'tailscale %s\n' "$*" >> "$INAYOS_TEST_LOG"
operator="$INAYOS_TEST_ROOT/tailscale-operator"
case "$1" in
    debug) printf '{"OperatorUser":"%s","WantRunning":true}\n' "$(cat "$operator" 2>/dev/null)" ;;
    set) printf '%s' "${2#--operator=}" > "$operator" ;;
esac"#,
    );
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&crate::Tailscale], &[&crate::Tailscale])
            })
            .unwrap();
    }
    assert_eq!(sandbox.calls("sudo tailscale set --operator=tester"), 1);
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &[&crate::Tailscale]));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::Tailscale], &[&crate::Tailscale], false)
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("tailscale-operator")).unwrap(),
        ""
    );
}

#[test]
fn an_operator_unset_before_any_tailscale_login_is_pending_and_a_wrong_one_fails() {
    let sandbox = Sandbox::new("tailscale-operator-pending");
    system(&sandbox);
    sandbox.fake(
        "tailscale",
        r#"case "$1" in
    debug) printf '{"OperatorUser":"%s"}\n' "$(cat "$INAYOS_TEST_ROOT/tailscale-operator" 2>/dev/null)" ;;
    status) [ -e "$INAYOS_TEST_ROOT/tailscale-login" ] ;;
esac"#,
    );
    fs::write(sandbox.root.join("installed/tailscale"), "").unwrap();
    let units = sandbox.root.join("units/system");
    fs::write(units.join("tailscaled"), "").unwrap();
    let verify = |mode| {
        let io = Answers::new(None);
        let mut results =
            sandbox.with_mode(&io, mode, |ctx| runner::verify(ctx, &[&crate::Tailscale]));
        let pending = io
            .notes
            .borrow()
            .iter()
            .any(|note| note.contains("sudo tailscale set --operator=tester"));
        (
            results.remove(0).1.map_err(|error| error.to_string()),
            pending,
        )
    };
    // The installed system has not started tailscaled yet.
    assert_eq!(verify(RunMode::Installer), (Ok(()), true));

    fs::write(units.join("tailscaled.active"), "").unwrap();
    assert_eq!(verify(RunMode::Live), (Ok(()), true));
    fs::write(sandbox.root.join("tailscale-login"), "").unwrap();
    assert_eq!(
        verify(RunMode::Live),
        (
            Err("tailscale: tester is not the Tailscale operator".to_owned()),
            false
        )
    );
    fs::remove_file(sandbox.root.join("tailscale-login")).unwrap();
    fs::write(sandbox.root.join("tailscale-operator"), "someone").unwrap();
    assert_eq!(
        verify(RunMode::Live),
        (
            Err("tailscale: someone is the Tailscale operator instead of tester".to_owned()),
            false
        )
    );
}

#[test]
fn agents_install_claude_code_once_and_the_bun_tools_and_remove_takes_them_out() {
    let sandbox = Sandbox::new("agents");
    fs::write(
        sandbox.root.join("claude-installer"),
        r#"printf 'claude installer\n' >> "$INAYOS_TEST_LOG"
mkdir -p "$HOME/.local/bin" && : > "$HOME/.local/bin/claude" && chmod +x "$HOME/.local/bin/claude"
"#,
    )
    .unwrap();
    sandbox.fake(
        "curl",
        r#"printf 'curl %s\n' "$*" >> "$INAYOS_TEST_LOG"
cat "$INAYOS_TEST_ROOT/claude-installer""#,
    );
    sandbox.fake(
        "bun",
        r#"printf 'bun %s\n' "$*" >> "$INAYOS_TEST_LOG"
modules="$BUN_INSTALL/install/global/node_modules"
action="$1"; shift 2
for package; do
    package="${package%@latest}"
    case "$action" in
        add) mkdir -p "$modules/$package" "$BUN_INSTALL/bin" "$modules/.bin" ;;
        remove) rm -rf "$modules/$package" ;;
    esac
done
if [ "$action" = add ]; then
    for command in playwright-mcp pi; do
        printf '#!/bin/sh\n' > "$BUN_INSTALL/bin/$command"; chmod +x "$BUN_INSTALL/bin/$command"
    done
    printf '#!/bin/sh\n' > "$modules/.bin/playwright"; chmod +x "$modules/.bin/playwright"
fi"#,
    );
    let agents = crate::AgentsPackages { extra: &[] };
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| runner::install(ctx, &[&agents], &[&agents]))
            .unwrap();
    }
    assert_eq!(sandbox.calls("claude installer"), 1);
    assert_eq!(
        sandbox
            .calls("bun add --global @playwright/mcp@latest @mariozechner/pi-coding-agent@latest"),
        2
    );
    let modules = sandbox.home.join(".bun/install/global/node_modules");
    assert!(modules.join("@mariozechner/pi-coding-agent").is_dir());
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&agents], &[&agents], false)
        })
        .unwrap();
    assert_eq!(
        sandbox.calls("bun remove --global @playwright/mcp @mariozechner/pi-coding-agent"),
        1
    );
    assert!(!sandbox.home.join(".local/bin/claude").exists());
}

#[test]
fn t3code_repairs_the_server_through_the_deployed_t3ctl_and_remove_undoes_what_it_did() {
    let sandbox = Sandbox::new("t3code");
    system(&sandbox);
    sandbox.fake("npm", "exit 0");
    let io = Answers::new(None);
    // The deployed t3ctl runs as itself, so its own commands are faked through PATH.
    sandbox.fake(
        "loginctl",
        r#"printf 'loginctl %s\n' "$*" >> "$INAYOS_TEST_LOG"
linger="$INAYOS_TEST_ROOT/linger"
case "$1" in
    show-user) if [ -e "$linger" ]; then echo yes; else echo no; fi ;;
    enable-linger) : > "$linger" ;;
    disable-linger) rm -f "$linger" ;;
esac"#,
    );
    sandbox.fake("curl", "exit 0");
    // ~/.t3 is missing before install. The started server writes its runtime file there.
    let t3_home = sandbox.home.join(".t3");
    let t3 = sandbox.home.join(".local/bin/t3");
    fs::create_dir_all(t3.parent().unwrap()).unwrap();
    super::program(
        &t3,
        &format!(
            r#"printf 't3 %s\n' "$*" >> "$INAYOS_TEST_LOG"
if [ "$*" = "service install" ]; then
    mkdir -p '{userdata}' && echo '{{}}' > '{userdata}/server-runtime.json'
fi"#,
            userdata = t3_home.join("userdata").display()
        ),
    );
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::T3Code], &[&crate::T3Code])
        })
        .unwrap();
    assert!(sandbox.installed("qrencode"));
    assert_eq!(sandbox.calls("t3 service install"), 1);
    assert!(sandbox.root.join("linger").exists());
    assert!(sandbox.home.join(".t3/service.env").exists());
    let t3ctl = sandbox.home.join(".local/bin/t3ctl");
    assert_eq!(
        fs::canonicalize(&t3ctl).unwrap(),
        fs::canonicalize(sandbox.home.join("dotfiles/t3code/.local/bin/t3ctl")).unwrap()
    );

    let yes = Answers::new(Some(true));
    sandbox
        .with(&yes, |ctx| {
            runner::remove(ctx, &[&crate::T3Code], &[&crate::T3Code], false)
        })
        .unwrap();
    assert_eq!(sandbox.calls("t3 service uninstall"), 1);
    // Lingering was off and ~/.t3 was missing before install. Remove asks before
    // deleting ~/.t3, because the server keeps its data there.
    assert!(!sandbox.root.join("linger").exists());
    assert!(
        yes.questions
            .borrow()
            .iter()
            .any(|question| question.contains(&t3_home.display().to_string()))
    );
    assert!(!t3_home.exists());
    assert!(!sandbox.installed("qrencode"));
    assert!(fs::symlink_metadata(&t3ctl).is_err());
}

#[test]
fn t3code_desktop_installs_the_app_and_links_its_client_settings() {
    let sandbox = Sandbox::new("t3code-desktop");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::T3CodeDesktop], &[&crate::T3CodeDesktop])
        })
        .unwrap();
    assert!(sandbox.installed("t3code-bin"));
    let settings = sandbox.home.join(".t3/userdata/client-settings.json");
    assert!(
        fs::symlink_metadata(&settings)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    let userdata = fs::symlink_metadata(sandbox.home.join(".t3/userdata")).unwrap();
    assert!(!userdata.file_type().is_symlink());
}

#[test]
fn vendor_apt_repositories_are_added_once_with_their_keys() {
    let sandbox = Sandbox::new("apt-repositories");
    system(&sandbox);
    sandbox.fake(
        "curl",
        r#"printf 'curl %s\n' "$*" >> "$INAYOS_TEST_LOG"
case "$*" in
    *docker*) printf -- '-----BEGIN PGP PUBLIC KEY BLOCK-----\nkey\n' ;;
    *) printf '\231\001binary key %s' "$2" ;;
esac"#,
    );
    sandbox.fake("dpkg", "echo amd64");
    sandbox.fake(
        "apt-get",
        r#"printf 'apt-get %s\n' "$*" >> "$INAYOS_TEST_LOG""#,
    );
    let packages: Vec<_> = [
        Package::Tailscale,
        Package::Docker,
        Package::DockerCompose,
        Package::Ripgrep,
    ]
    .into_iter()
    .map(|package| inayos_utils::resolve_package(package, PackageSystem::Apt).unwrap())
    .collect();
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox.with(&io, |ctx| {
            ctx.state()
                .borrow_mut()
                .begin(crate::state::Action::Install)
                .unwrap();
            ctx.set_module("base");
            crate::apt_repository::add_repositories(ctx, &packages, "noble").unwrap();
        });
    }
    assert_eq!(sandbox.calls("sudo apt-get update"), 1);
    assert_eq!(
        system_file(&sandbox, "/etc/apt/sources.list.d/inayos-docker.sources").unwrap(),
        "Types: deb\nURIs: https://download.docker.com/linux/ubuntu\nSuites: noble\nComponents: stable\nArchitectures: amd64\nSigned-By: /etc/apt/keyrings/inayos-docker.asc\n"
    );
    assert!(system_file(&sandbox, "/etc/apt/keyrings/inayos-docker.asc").is_some());
    // A binary key is not text, and the second run must still compare it unchanged.
    assert_eq!(
        fs::read(
            sandbox
                .root
                .join("system/etc/apt/keyrings/inayos-tailscale.gpg")
        )
        .unwrap(),
        b"\x99\x01binary key https://pkgs.tailscale.com/stable/ubuntu/noble.noarmor.gpg"
    );
    assert!(system_file(&sandbox, "/etc/apt/sources.list.d/inayos-github.sources").is_none());
    assert!(system_file(&sandbox, "/etc/apt/sources.list.d/inayos-yazi.sources").is_none());
}

/// A module that notes its name when it installs, then succeeds or fails in both install
/// and verify.
struct Step(&'static str, bool, &'static [Need]);

impl Module for Step {
    fn name(&self) -> &'static str {
        self.0
    }
    fn needs(&self) -> &'static [Need] {
        self.2
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.note(self.0);
        self.verify(ctx)
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        if self.1 { Ok(()) } else { Err("broken".into()) }
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

/// A module that installs one package, like most modules do.
struct PackageStep;

impl Module for PackageStep {
    fn name(&self) -> &'static str {
        "package"
    }
    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Btop],
            settings: vec![],
        }
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Btop])
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn a_failed_package_refresh_skips_only_the_modules_that_change_packages() {
    let sandbox = Sandbox::new("failed-refresh");
    sandbox.fake(
        "pacman",
        r#"printf 'pacman %s\n' "$*" >> "$INAYOS_TEST_LOG"
[ "$1" = -Syu ] && exit 1
exit 0"#,
    );
    let modules: [&dyn Module; 2] = [&PackageStep, &Step("config", true, &[])];
    let io = Answers::new(None);
    let report = incomplete(sandbox.with(&io, |ctx| runner::install(ctx, &modules, &modules)));
    assert_eq!(*io.notes.borrow(), ["config"]);
    assert!(report.failed.is_empty());
    let skipped: Vec<_> = report
        .skipped
        .iter()
        .map(|skipped| skipped.module)
        .collect();
    assert_eq!(skipped, ["package"]);
    assert!(matches!(
        report.skipped[0].reason,
        runner::SkipReason::PackageDatabase(_)
    ));
    assert_eq!(sandbox.calls("pacman -S "), 0);
}

fn incomplete(result: Result<(), runner::InstallError>) -> runner::InstallReport {
    match result {
        Err(runner::InstallError::Incomplete(report)) => report,
        other => panic!("expected an incomplete install, got {other:?}"),
    }
}

#[test]
fn install_goes_on_after_a_failure_and_skips_the_modules_that_need_it() {
    let sandbox = Sandbox::new("keep-going");
    let modules: [&dyn Module; 7] = [
        &Step("first", false, &[]),
        &Step("second", true, &[]),
        &Step("third", true, &[Need::Soft("first")]),
        &Step("fourth", true, &[Need::Hard("third")]),
        &Step("fifth", true, &[Need::Soft("absent"), Need::Hard("second")]),
        &Step("sixth", false, &[]),
        &Step("summary", true, &[Need::EveryEarlier]),
    ];
    let io = Answers::new(None);
    let report = incomplete(sandbox.with(&io, |ctx| runner::install(ctx, &modules, &modules)));
    assert_eq!(*io.notes.borrow(), ["first", "second", "fifth", "sixth"]);
    let failed: Vec<_> = report.failed.iter().map(|failed| failed.module).collect();
    assert_eq!(failed, ["first", "sixth"]);
    let skipped: Vec<_> = report
        .skipped
        .iter()
        .map(|skipped| (skipped.module, skipped.reason.clone()))
        .collect();
    assert_eq!(
        skipped,
        [
            (
                "third",
                runner::SkipReason::Needs(vec![("first", runner::Unmet::Failed)])
            ),
            (
                "fourth",
                runner::SkipReason::Needs(vec![("third", runner::Unmet::Skipped)])
            ),
            (
                "summary",
                runner::SkipReason::Needs(vec![
                    ("first", runner::Unmet::Failed),
                    ("third", runner::Unmet::Skipped),
                    ("fourth", runner::Unmet::Skipped),
                    ("sixth", runner::Unmet::Failed),
                ])
            ),
        ]
    );
    assert!(!report.only_unanswered());
}

#[test]
fn a_hard_need_left_out_of_the_run_must_verify() {
    let sandbox = Sandbox::new("hard-need");
    let dependent = Step("dependent", true, &[Need::Hard("needed")]);
    let run: [&dyn Module; 1] = [&dependent];

    let broken: [&dyn Module; 2] = [&Step("needed", false, &[]), &dependent];
    let io = Answers::new(None);
    let report = incomplete(sandbox.with(&io, |ctx| runner::install(ctx, &broken, &run)));
    assert!(io.notes.borrow().is_empty(), "nothing installs");
    assert_eq!(report.skipped.len(), 1);
    assert_eq!(
        report.skipped[0].reason,
        runner::SkipReason::Needs(vec![(
            "needed",
            runner::Unmet::NotInstalled("broken".to_owned())
        )])
    );

    let working: [&dyn Module; 2] = [&Step("needed", true, &[]), &dependent];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &working, &run))
        .unwrap();
    assert_eq!(*io.notes.borrow(), ["dependent"]);
}

/// Fake PowerShells where Defender keeps its allowed apps in a file, and only an elevated
/// pwsh changes them. With `refused`, the person refuses every administrator prompt.
fn defender(sandbox: &Sandbox, refused: bool) {
    let elevate = if refused { "exit 1" } else { ":" };
    sandbox.fake(
        "pwsh.exe",
        &format!(
            r#"allowed="$INAYOS_TEST_ROOT/allowed"
case "$3" in
    '$PROFILE') printf '%s\n' "$HOME/Documents/PowerShell/Microsoft.PowerShell_profile.ps1" ;;
    *Get-MpPreference*) if [ -e "$allowed" ]; then echo allowed; else echo blocked; fi ;;
    *-Verb\ RunAs*)
        printf 'defender %s\n' "$INAYOS_ELEVATED_SCRIPT" >> "$INAYOS_TEST_LOG"; {elevate}
        case "$INAYOS_ELEVATED_SCRIPT" in
            *Add-MpPreference*) : > "$allowed" ;;
            *Remove-MpPreference*) rm -f "$allowed" ;;
        esac ;;
esac"#
        ),
    );
    sandbox.fake(
        "powershell.exe",
        r#"printf '%s\n' "$HOME/Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1""#,
    );
}

#[test]
fn a_refused_administrator_prompt_leaves_nothing_for_remove_to_ask_again() {
    let sandbox = Sandbox::new("powershell-profile-refused");
    defender(&sandbox, true);
    let modules: [&dyn Module; 1] = [&crate::PowerShellProfile];
    let io = Answers::new(None);
    let report = incomplete(sandbox.with(&io, |ctx| runner::install(ctx, &modules, &modules)));
    assert_eq!(report.failed.len(), 1);
    sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert_eq!(
        sandbox.calls("Remove-MpPreference"),
        0,
        "remove showed an administrator prompt for an entry that was never added"
    );
}

#[test]
fn the_powershell_profile_allows_the_installer_through_controlled_folder_access_until_removed() {
    let sandbox = Sandbox::new("powershell-profile");
    defender(&sandbox, false);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(
                ctx,
                &[&crate::PowerShellProfile],
                &[&crate::PowerShellProfile],
            )
        })
        .unwrap();
    assert!(sandbox.root.join("allowed").exists());
    // The installer itself writes the profile, so it is the program Defender must allow.
    let installer = std::env::current_exe().unwrap();
    assert_eq!(
        sandbox.calls(&format!(
            "Add-MpPreference -ControlledFolderAccessAllowedApplications '{}'",
            installer.display()
        )),
        1
    );

    sandbox
        .with(&io, |ctx| {
            runner::remove(
                ctx,
                &[&crate::PowerShellProfile],
                &[&crate::PowerShellProfile],
                false,
            )
        })
        .unwrap();
    assert!(!sandbox.root.join("allowed").exists());
    assert_eq!(
        sandbox.calls(&format!(
            "Remove-MpPreference -ControlledFolderAccessAllowedApplications '{}'",
            installer.display()
        )),
        1
    );
    assert!(!sandbox.home.join("Documents").exists());
}

/// Fake `gh` and `tailscale` that count as logged in after their login command, and log
/// every call.
fn logins(sandbox: &Sandbox) {
    sandbox.fake(
        "gh",
        r#"printf 'gh %s\n' "$*" >> "$INAYOS_TEST_LOG"
case "$*" in
    "auth status") [ -e "$INAYOS_TEST_ROOT/gh-login" ] ;;
    "auth login") : > "$INAYOS_TEST_ROOT/gh-login" ;;
esac"#,
    );
    sandbox.fake(
        "tailscale",
        r#"printf 'tailscale %s\n' "$*" >> "$INAYOS_TEST_LOG"
case "$*" in
    status) [ -e "$INAYOS_TEST_ROOT/tailscale-login" ] ;;
    up) : > "$INAYOS_TEST_ROOT/tailscale-login" ;;
esac"#,
    );
}

#[test]
fn login_asks_only_when_logged_out_and_otherwise_prints_the_command() {
    let sandbox = Sandbox::new("login");
    logins(&sandbox);
    let install = |io: &Answers| {
        sandbox
            .with(io, |ctx| {
                runner::install(ctx, &[&crate::Login], &[&crate::Login])
            })
            .unwrap();
    };

    for io in [Answers::new(None), Answers::new(Some(false))] {
        install(&io);
        assert_eq!(io.questions.borrow().len(), 2);
        assert_eq!(
            *io.notes.borrow(),
            [
                "GitHub CLI is not authenticated. Run: gh auth login",
                "Tailscale is not authenticated. Run: sudo tailscale up",
            ]
        );
    }
    assert_eq!(sandbox.calls("gh auth login"), 0);
    assert_eq!(sandbox.calls("tailscale up"), 0);

    let accepted = Answers::new(Some(true));
    install(&accepted);
    assert_eq!(
        *accepted.questions.borrow(),
        [
            "GitHub CLI is not authenticated. Log in now?",
            "Tailscale is not authenticated. Log in now?",
        ]
    );
    assert_eq!(sandbox.calls("gh auth login"), 1);
    assert_eq!(sandbox.calls("tailscale up"), 1);

    let logged_in = Answers::new(Some(true));
    install(&logged_in);
    assert!(logged_in.questions.borrow().is_empty());

    sandbox
        .with(&logged_in, |ctx| {
            runner::remove(ctx, &[&crate::Login], &[&crate::Login], false)
        })
        .unwrap();
    assert_eq!(sandbox.calls("logout"), 0, "remove keeps the logins");
    assert_eq!(
        sandbox.calls("tailscale down"),
        0,
        "remove keeps the logins"
    );
}

#[test]
fn a_declined_windows_t3code_verifies_and_is_asked_again() {
    let sandbox = Sandbox::new("windows-t3code-declined");
    let modules: [&dyn Module; 1] = [&crate::WindowsT3Code];
    for _ in 0..2 {
        let io = Answers::new(Some(false));
        sandbox
            .with(&io, |ctx| runner::install(ctx, &modules, &modules))
            .unwrap();
        assert_eq!(io.questions.borrow().len(), 1);
    }
    assert!(!sandbox.home.join(".local/bin/t3ctl.ps1").exists());
    let io = Answers::new(None);
    let verified = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    assert!(verified[0].1.is_ok());
    assert!(
        io.events
            .borrow()
            .iter()
            .any(|event| matches!(event, crate::Event::Declined { module } if module == "t3code")),
        "the screen would show the declined module as installed"
    );
    assert!(sandbox.with(&io, |ctx| !runner::installed(ctx, &crate::WindowsT3Code)));
}

#[test]
fn an_unanswered_windows_t3code_is_skipped() {
    let sandbox = Sandbox::new("windows-t3code-unanswered");
    let modules: [&dyn Module; 1] = [&crate::WindowsT3Code];
    let report = incomplete(sandbox.with(&Answers::new(None), |ctx| {
        runner::install(ctx, &modules, &modules)
    }));
    assert!(report.failed.is_empty());
    assert_eq!(report.skipped.len(), 1);
    assert!(matches!(
        report.skipped[0].reason,
        runner::SkipReason::Unanswered(_)
    ));
    assert!(
        report.only_unanswered(),
        "the command line exits with code 3 instead of reporting a failure"
    );
    assert!(!sandbox.home.join(".local/bin/t3ctl.ps1").exists());
    let verified = sandbox.with(&Answers::new(None), |ctx| runner::verify(ctx, &modules));
    assert!(verified[0].1.is_err(), "nothing records a skip");
}

/// What `AsksThenFails` does after nobody answers its question.
#[derive(Clone, Copy)]
enum AfterQuestion {
    /// Returns the `Unanswered` error unchanged.
    PassesItOn,
    /// Fails for another reason, the way a failed move does after a skipped question.
    FailsAnyway,
    /// Asked an optional follow-up, then fails.
    FailsAfterOptional,
}

/// Asks a question during install that nobody answers, then fails as `after` says.
struct AsksThenFails {
    name: &'static str,
    after: AfterQuestion,
}

impl Module for AsksThenFails {
    fn name(&self) -> &'static str {
        self.name
    }
    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        match self.after {
            AfterQuestion::PassesItOn => {
                ctx.confirm("Install?")?;
                Ok(())
            }
            AfterQuestion::FailsAnyway => {
                let _ = ctx.confirm("Replace it?");
                Err("could not move the item".into())
            }
            AfterQuestion::FailsAfterOptional => {
                ctx.confirm_optional("Log in now?");
                Err("broken".into())
            }
        }
    }
    fn verify(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn a_missing_answer_skips_the_module_only_when_it_is_the_only_problem() {
    let sandbox = Sandbox::new("unanswered-or-failed");
    let passes = AsksThenFails {
        name: "passes",
        after: AfterQuestion::PassesItOn,
    };
    let modules: [&dyn Module; 1] = [&passes];
    let report = incomplete(sandbox.with(&Answers::new(None), |ctx| {
        runner::install(ctx, &modules, &modules)
    }));
    assert!(report.failed.is_empty());
    assert_eq!(
        report.skipped[0].reason,
        runner::SkipReason::Unanswered("Install?".to_owned())
    );
    assert!(
        report.only_unanswered(),
        "the command line exits with code 3 instead of reporting a failure"
    );

    // A failure after a missing answer is a failure, and its error is reported.
    let io = Answers::new(None);
    let fails = AsksThenFails {
        name: "fails",
        after: AfterQuestion::FailsAnyway,
    };
    let modules: [&dyn Module; 1] = [&fails];
    let report = incomplete(sandbox.with(&io, |ctx| runner::install(ctx, &modules, &modules)));
    assert!(report.skipped.is_empty());
    assert!(report.failed[0].error.contains("could not move the item"));
    assert!(io.events.borrow().iter().any(|event| matches!(
        event,
        crate::Event::Finished { module, error: Some(error), .. }
            if module == "fails" && error.contains("could not move the item")
    )));

    // So does a failure after a missing answer to an optional question.
    let optional = AsksThenFails {
        name: "optional",
        after: AfterQuestion::FailsAfterOptional,
    };
    let modules: [&dyn Module; 1] = [&optional];
    let report = incomplete(sandbox.with(&Answers::new(None), |ctx| {
        runner::install(ctx, &modules, &modules)
    }));
    assert_eq!(report.failed.len(), 1);
    assert!(report.skipped.is_empty());
}

/// Asks before installing, the way `WindowsT3Code` does, and verifies that it installed
/// unless the person declined.
struct Asks;

impl Module for Asks {
    fn name(&self) -> &'static str {
        "asks"
    }
    fn needs(&self) -> &'static [Need] {
        &[]
    }
    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }
    fn install(&self, ctx: &Context) -> ModuleResult {
        if !ctx.confirm("Install?")? {
            return ctx.record_declined();
        }
        ctx.record_accepted()
    }
    fn verify(&self, ctx: &Context) -> ModuleResult {
        if ctx.declined() {
            Ok(())
        } else {
            Err("not installed".into())
        }
    }
    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[test]
fn accepting_after_a_decline_makes_verify_check_the_install_again() {
    let sandbox = Sandbox::new("accepted-after-decline");
    let modules: [&dyn Module; 1] = [&Asks];
    sandbox
        .with(&Answers::new(Some(false)), |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    let report = incomplete(sandbox.with(&Answers::new(Some(true)), |ctx| {
        runner::install(ctx, &modules, &modules)
    }));
    assert_eq!(
        report.failed.len(),
        1,
        "verify no longer passes on the decline"
    );
}

#[test]
fn remove_unregisters_the_arch_distribution_once_whatever_its_records_say() {
    let sandbox = Sandbox::new("arch-wsl-remove");
    sandbox.fake(
        "wsl.exe",
        r#"registered="$INAYOS_TEST_ROOT/distributions"
case "$1" in
    --list) cat "$registered" ;;
    --unregister) printf 'wsl %s\n' "$*" >> "$INAYOS_TEST_LOG"
        grep -Fxq "$2" "$registered" || exit 1
        grep -Fxv "$2" "$registered" > "$registered.new"; mv "$registered.new" "$registered" ;;
esac"#,
    );
    fs::write(
        sandbox.root.join("distributions"),
        "Ubuntu\nhost-subsystem\n",
    )
    .unwrap();
    // Two installs that each recorded the distribution, as installs did before the record
    // was kept to one.
    let io = Answers::new(Some(true));
    for _ in 0..2 {
        sandbox.with(&io, |ctx| {
            ctx.state()
                .borrow_mut()
                .begin(crate::state::Action::Install)
                .unwrap();
            ctx.set_module(crate::ArchWsl::NAME);
            ctx.record(crate::state::Change::WslDistribution {
                name: "host-subsystem".to_owned(),
            })
            .unwrap();
        });
    }
    let modules: [&dyn Module; 1] = [&crate::ArchWsl];
    sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert_eq!(sandbox.calls("wsl --unregister host-subsystem"), 1);
    assert_eq!(
        fs::read_to_string(sandbox.root.join("distributions")).unwrap(),
        "Ubuntu\n"
    );
    assert!(
        sandbox.with(&io, |ctx| !runner::installed(ctx, &crate::ArchWsl)),
        "the record of the distribution that was already gone still blocks remove"
    );
}

/// The rEFInd block that myconfig added, and the one that InayOS adds.
const MYCONFIG_REFIND_BLOCK: &str = "\n# BEGIN MYCONFIG REFIND\ninclude managed.conf\ninclude themes/black-pink/theme.conf\n# END MYCONFIG REFIND\n";
const INAYOS_REFIND_BLOCK: &str = "\n# BEGIN INAYOS REFIND\ninclude managed.conf\ninclude themes/black-pink/theme.conf\ninclude inayos-guard.conf\n# END INAYOS REFIND\n";
const REFIND_CONF: &str = "/boot/EFI/refind/refind.conf";

fn write_kde(sandbox: &Sandbox, path: &str, value: &str) {
    let path = sandbox.root.join("kde").join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, value).unwrap();
}

/// A machine as myconfig, the earlier name of InayOS, left it, recorded in its format 1
/// state: the Kanata package deployed under its earlier unit name and linked with Stow,
/// that unit enabled and running, Kanata's system files, a libinput plugin, an APT source
/// with its key, the rEFInd block, and the KDE keys of the earlier window rule, listed
/// after another rule, and of the earlier panel script.
fn myconfig_machine(sandbox: &Sandbox) {
    system(sandbox);
    sandbox.fake(
        "kanata",
        r#"printf 'kanata %s\n' "$*" >> "$INAYOS_TEST_LOG""#,
    );
    let home = &sandbox.home;
    let (_, files) = embedded_dotfiles::CONFIG_PACKAGES
        .iter()
        .find(|(name, _)| *name == "kanata")
        .unwrap();
    for file in files.files() {
        let path = home
            .join("dotfiles/kanata")
            .join(file.path_from_root.replace("inayos-", "myconfig-"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, file.content).unwrap();
    }
    fs::create_dir_all(home.join(".config/systemd/user")).unwrap();
    let dotfiles = home.join("dotfiles");
    sandbox
        .shell
        .cmd("stow")
        .args(["--dir".as_ref(), dotfiles.as_os_str()])
        .args(["--target".as_ref(), home.as_os_str()])
        .arg("kanata")
        .run()
        .unwrap();
    assert!(
        home.join(".config/systemd/user/myconfig-kanata.service")
            .is_file()
    );
    for unit in ["myconfig-kanata.service", "myconfig-kanata.service.active"] {
        fs::write(unit_file(sandbox, ServiceScope::User, unit), "").unwrap();
    }
    for (path, contents) in [
        ("etc/modules-load.d/myconfig-kanata.conf", "uinput\n"),
        (
            "etc/udev/rules.d/99-myconfig-kanata.rules",
            "KERNEL==\"uinput\"\n",
        ),
        (
            "etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua",
            "-- x4\n",
        ),
        (
            "etc/apt/sources.list.d/myconfig-docker.sources",
            "Types: deb\n",
        ),
        ("etc/apt/keyrings/myconfig-docker.asc", "key\n"),
        (
            &REFIND_CONF[1..],
            &format!("timeout 5\n{MYCONFIG_REFIND_BLOCK}"),
        ),
    ] {
        let path = sandbox.root.join("system").join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    for (path, value) in [
        ("kwinrulesrc/myconfig-picture-in-picture/title", "picture"),
        ("kwinrc/Plugins/myconfig-panelsEnabled", "true"),
        (
            "kwinrulesrc/General/rules",
            "other-rule,myconfig-picture-in-picture",
        ),
        ("kwinrulesrc/General/count", "2"),
    ] {
        write_kde(sandbox, path, value);
    }
    let system = |path: &str| serde_json::json!({"file": {"path": path, "system": true, "previous": "missing"}});
    let kde = |file: &str, group: &str, key: &str| serde_json::json!({"setting": {"setting": {"KdeKey": {"file": file, "groups": [group], "key": key}}, "previous": null}});
    let state = serde_json::json!({
        "format": 1,
        "written_by": "0.9.0",
        "snapshots": [{"id": 1, "action": "install", "started": 1, "modules": {
            "kanata": [
                {"stowed": {"package": "kanata"}},
                system("/etc/modules-load.d/myconfig-kanata.conf"),
                system("/etc/udev/rules.d/99-myconfig-kanata.rules"),
                {"service": {"unit": "myconfig-kanata.service", "scope": "user", "was_enabled": false}}
            ],
            "kde-plasma": [
                system("/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua"),
                kde("kwinrulesrc", "myconfig-picture-in-picture", "title"),
                kde("kwinrulesrc", "General", "rules"),
                kde("kwinrulesrc", "General", "count"),
                kde("kwinrc", "Plugins", "myconfig-panelsEnabled")
            ],
            "refind": [
                {"file": {"path": REFIND_CONF, "system": true, "previous": {"file": {"contents": "dGltZW91dCA1Cg==", "executable": false}}}}
            ],
            "docker": [
                system("/etc/apt/keyrings/myconfig-docker.asc"),
                system("/etc/apt/sources.list.d/myconfig-docker.sources")
            ]
        }}]
    });
    fs::create_dir_all(sandbox.root.join("state")).unwrap();
    fs::write(
        sandbox.root.join("state/state.json"),
        serde_json::to_vec(&state).unwrap(),
    )
    .unwrap();
}

/// Whether the earlier unit, its system files, its plugin, its APT source and key, and its
/// KDE keys are gone.
fn earlier_names_are_gone(sandbox: &Sandbox) {
    assert!(!enabled(
        sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
    assert!(!active(
        sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
    for path in [
        "/etc/modules-load.d/myconfig-kanata.conf",
        "/etc/udev/rules.d/99-myconfig-kanata.rules",
        "/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua",
        "/etc/apt/sources.list.d/myconfig-docker.sources",
        "/etc/apt/keyrings/myconfig-docker.asc",
    ] {
        assert!(
            system_file(sandbox, path).is_none(),
            "{path} is still there"
        );
    }
    assert!(kde(sandbox, "kwinrulesrc/myconfig-picture-in-picture/title").is_none());
    assert!(kde(sandbox, "kwinrc/Plugins/myconfig-panelsEnabled").is_none());
}

/// The rEFInd configuration and the KWin rule list with its count.
fn refind_and_rules(sandbox: &Sandbox) -> (String, String, String) {
    (
        system_file(sandbox, REFIND_CONF).unwrap(),
        kde(sandbox, "kwinrulesrc/General/rules").unwrap(),
        kde(sandbox, "kwinrulesrc/General/count").unwrap(),
    )
}

#[test]
fn removing_the_myconfig_leftovers_after_the_renamed_install_keeps_the_new_install() {
    let sandbox = Sandbox::new("leftovers-after");
    myconfig_machine(&sandbox);
    let kanata = Kanata {
        start_at_login: true,
    };
    let profile: [&dyn Module; 1] = [&kanata];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &profile, &profile))
        .unwrap();
    // Install leaves what the earlier name installed in place.
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
    assert!(active(
        &sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
    assert!(
        system_file(
            &sandbox,
            "/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua"
        )
        .is_some()
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-kanata.service"
    ));

    // The renamed rEFInd and KDE Plasma modules add their own block and rule.
    fs::write(
        sandbox.root.join("system").join(&REFIND_CONF[1..]),
        format!("timeout 5\n{MYCONFIG_REFIND_BLOCK}{INAYOS_REFIND_BLOCK}"),
    )
    .unwrap();
    write_kde(
        &sandbox,
        "kwinrulesrc/General/rules",
        "other-rule,myconfig-picture-in-picture,inayos-picture-in-picture",
    );
    write_kde(&sandbox, "kwinrulesrc/General/count", "3");
    // Links that Stow did not reach, a file and links that only share the earlier name.
    let units = sandbox.home.join(".config/systemd/user");
    std::os::unix::fs::symlink(
        "../../../dotfiles/kanata/.config/systemd/user/myconfig-kanata-tray.service",
        units.join("myconfig-kanata-tray.service"),
    )
    .unwrap();
    fs::write(units.join("myconfig-own.service"), "[Unit]\n").unwrap();
    std::os::unix::fs::symlink(
        "/nowhere/myconfig-x",
        units.join("myconfig-elsewhere.service"),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        "../../../dotfiles/kanata/.config/kanata/config.kbd",
        units.join("myconfig-live.service"),
    )
    .unwrap();

    sandbox
        .with(&io, |ctx| {
            runner::remove_with_leftovers(ctx, &profile, &[], false)
        })
        .unwrap();
    earlier_names_are_gone(&sandbox);
    let cleaned = (
        format!("timeout 5\n{INAYOS_REFIND_BLOCK}"),
        "other-rule,inayos-picture-in-picture".to_owned(),
        "2".to_owned(),
    );
    assert_eq!(refind_and_rules(&sandbox), cleaned);
    assert!(fs::symlink_metadata(units.join("myconfig-kanata-tray.service")).is_err());
    assert!(units.join("myconfig-own.service").is_file());
    assert!(fs::symlink_metadata(units.join("myconfig-elsewhere.service")).is_ok());
    assert!(units.join("myconfig-live.service").is_file());
    // The renamed install is whole: its unit, its system files and its links.
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-kanata.service"
    ));
    assert!(system_file(&sandbox, "/etc/modules-load.d/inayos-kanata.conf").is_some());
    assert!(member(&sandbox, "uinput"));
    for (_, result) in sandbox.with(&io, |ctx| runner::verify(ctx, &profile)) {
        result.unwrap();
    }

    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    sandbox
        .with(&io, |ctx| {
            runner::remove_with_leftovers(ctx, &profile, &[], false)
        })
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("calls")).unwrap(),
        calls,
        "a second remove of the leftovers ran a command"
    );
    assert_eq!(refind_and_rules(&sandbox), cleaned);
    assert!(sandbox.with(&io, |ctx| runner::installed(ctx, &kanata)));
}

#[test]
fn removing_the_myconfig_leftovers_before_the_renamed_install_leaves_no_earlier_name() {
    let sandbox = Sandbox::new("leftovers-before");
    myconfig_machine(&sandbox);
    let kanata = Kanata {
        start_at_login: true,
    };
    let profile: [&dyn Module; 1] = [&kanata];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::remove_with_leftovers(ctx, &profile, &[], false)
        })
        .unwrap();
    earlier_names_are_gone(&sandbox);
    assert_eq!(
        refind_and_rules(&sandbox),
        (
            "timeout 5\n".to_owned(),
            "other-rule".to_owned(),
            "1".to_owned()
        )
    );
    let unit = sandbox
        .home
        .join(".config/systemd/user/myconfig-kanata.service");
    assert!(
        unit.is_file(),
        "a live link of the deployed package went away"
    );

    sandbox
        .with(&io, |ctx| runner::install(ctx, &profile, &profile))
        .unwrap();
    assert!(
        fs::symlink_metadata(&unit).is_err(),
        "the renamed install left the link of the earlier unit"
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-kanata.service"
    ));
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
}

#[test]
fn an_earlier_refind_block_and_rule_already_gone_stay_untouched() {
    let sandbox = Sandbox::new("leftovers-gone");
    myconfig_machine(&sandbox);
    let refind = format!("timeout 5\n{INAYOS_REFIND_BLOCK}");
    fs::write(sandbox.root.join("system").join(&REFIND_CONF[1..]), &refind).unwrap();
    write_kde(
        &sandbox,
        "kwinrulesrc/General/rules",
        "other-rule,inayos-picture-in-picture",
    );
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::remove_with_leftovers(ctx, &[], &[], false)
        })
        .unwrap();
    assert_eq!(
        refind_and_rules(&sandbox),
        (
            refind,
            "other-rule,inayos-picture-in-picture".to_owned(),
            "2".to_owned()
        )
    );
    assert_eq!(sandbox.calls("sudo install"), 0);
    assert_eq!(sandbox.calls("--group General"), 0);
}

#[test]
fn removing_the_myconfig_leftovers_of_a_new_machine_changes_nothing() {
    let sandbox = Sandbox::new("leftovers-none");
    system(&sandbox);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::remove_with_leftovers(ctx, &[], &[], false)
        })
        .unwrap();
    assert!(!sandbox.root.join("calls").exists());
    assert!(io.questions.borrow().is_empty());
}

/// The InayOS desktop profile's modules that touch services, the session or the kernel, in
/// profile order. The others only install packages and write files, and `login` stays
/// out of installer runs.
fn installer_profile() -> Vec<&'static dyn Module> {
    vec![
        &crate::PlasmaVersion,
        &Base {
            packages: Base::ARCH,
            unwanted: &[],
            needs: &[Need::Hard("plasma-version")],
        },
        &crate::SystemTuning,
        &crate::Ssh { keys_only: false },
        &Cli,
        &crate::Tailscale,
        &Kanata {
            start_at_login: false,
        },
        &crate::KdePlasma,
        &crate::Printing,
        &KanataKde,
        &Handy,
        &crate::Pipewire,
        &DOCKER,
        &crate::T3CodeDesktop,
        &crate::SshServer,
    ]
}

/// A never-booted system as `arch-chroot -u` gives it to the installer: the fake programs
/// of `plasma`, with every program that acts on a running system logging its call, and
/// `systemctl` logging whether it got `SYSTEMD_OFFLINE`.
fn never_booted(sandbox: &Sandbox) {
    plasma(sandbox);
    docker(sandbox);
    fs::create_dir_all(sandbox.root.join("system/etc/ssh")).unwrap();
    for program in [
        "sshd",
        "ssh-keygen",
        "handy",
        "kanata",
        "pactl",
        "tailscale",
        "sysctl",
        "nmcli",
    ] {
        sandbox.fake(
            program,
            &format!("printf '{program} %s\\n' \"$*\" >> \"$INAYOS_TEST_LOG\""),
        );
    }
    for program in ["plasmashell", "qdbus6", "loginctl"] {
        sandbox.fake(
            program,
            &format!("printf '{program} %s\\n' \"$*\" >> \"$INAYOS_TEST_LOG\"; exit 1"),
        );
    }
    fs::rename(
        sandbox.root.join("bin/systemctl"),
        sandbox.root.join("bin/systemctl-units"),
    )
    .unwrap();
    sandbox.fake(
        "systemctl",
        r#"printf 'offline=%s\n' "$SYSTEMD_OFFLINE" >> "$INAYOS_TEST_LOG"
exec systemctl-units "$@""#,
    );
    sandbox.fake(
        "dbus-run-session",
        r#"printf 'dbus-run-session config=%s\n' "$XDG_CONFIG_HOME" >> "$INAYOS_TEST_LOG"
shift; exec "$@""#,
    );
    sandbox.fake(
        "plasma-apply-colorscheme",
        r#"printf 'plasma-apply-colorscheme %s dirs=%s\n' "$*" "$XDG_CONFIG_DIRS" >> "$INAYOS_TEST_LOG""#,
    );
    sandbox.fake(
        "gsettings",
        r#"printf 'gsettings %s\n' "$*" >> "$INAYOS_TEST_LOG"
store="$INAYOS_TEST_ROOT/dconf"
case "$1" in
    list-keys) printf 'cursor-size\n' ;;
    list-schemas) printf 'org.gnome.desktop.interface /org/gnome/desktop/interface/\n' ;;
    set) mkdir -p "$store" && printf '%s' "$4" > "$store/$3" ;;
esac"#,
    );
    sandbox.fake(
        "dconf",
        r#"[ "$1" = read ] && cat "$INAYOS_TEST_ROOT/dconf/${2##*/}" 2>/dev/null; exit 0"#,
    );
    // `-u` clears the environment, and the modules read an empty value as unset.
    sandbox.shell.set_var("XDG_RUNTIME_DIR", "");
    sandbox.shell.set_var("DBUS_SESSION_BUS_ADDRESS", "");
    sandbox.shell.set_var("XDG_CONFIG_DIRS", "");
}

#[test]
fn installer_mode_enables_the_profile_without_touching_a_running_system() {
    let sandbox = Sandbox::new("installer-mode");
    never_booted(&sandbox);
    let profile = installer_profile();
    // The answers source turns the SSH server on.
    let io = Answers::new(Some(true));
    sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &profile, &profile)
        })
        .unwrap();

    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    for line in calls.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if let Some(("systemctl", arguments)) = words.split_first().map(|(a, b)| (*a, b)) {
            let action = arguments
                .iter()
                .find(|word| !word.starts_with("--"))
                .unwrap();
            assert!(
                ["enable", "disable", "is-enabled"].contains(action)
                    && !arguments.contains(&"--now"),
                "installer mode ran `{line}`"
            );
        }
        for program in [
            "loginctl",
            "modprobe",
            "udevadm",
            "qdbus6",
            "plasmashell",
            "sysctl",
            "nmcli",
        ] {
            assert_ne!(words.first(), Some(&program), "installer mode ran `{line}`");
            assert!(
                !line.contains(&format!("sudo {program}")),
                "installer mode ran `{line}`"
            );
        }
        if let Some(offline) = line.strip_prefix("offline=") {
            assert_eq!(offline, "1", "systemctl ran without SYSTEMD_OFFLINE=1");
        }
    }
    assert!(sandbox.calls("offline=1") > 0);
    // Plasma builds the top bar and dock from the global theme's default layout only
    // when it starts without a panel file.
    assert!(
        !sandbox
            .home
            .join(".config/plasma-org.kde.plasma.desktop-appletsrc")
            .exists()
    );
    assert_eq!(
        kde(&sandbox, "kdeglobals/KDE/LookAndFeelPackage").as_deref(),
        Some("inayos.desktop")
    );

    for unit in [
        "inayos-plasma-layout.service",
        "inayos-kanata-tray.service",
        "inayos-handy.service",
        "inayos-pipewire-tray.service",
        "docker.service",
    ] {
        assert!(enabled(&sandbox, ServiceScope::User, unit), "{unit}");
    }
    for unit in [
        "tailscaled",
        "cups.socket",
        "sshd.service",
        "systemd-resolved.service",
        "NetworkManager.service",
        "plasmalogin.service",
    ] {
        assert!(enabled(&sandbox, ServiceScope::System, unit), "{unit}");
    }

    // The settings that save through a session bus save through a bus of their own, into
    // the user's configuration folder.
    assert_eq!(
        sandbox.calls(&format!(
            "dbus-run-session config={}\ngsettings set org.gnome.desktop.interface cursor-size 24",
            sandbox.home.join(".config").display()
        )),
        1
    );
    assert_eq!(
        fs::read_to_string(sandbox.root.join("dconf/cursor-size")).ok(),
        Some("24".to_owned())
    );
    assert_eq!(
        sandbox.calls(&format!(
            "plasma-apply-colorscheme --accent-color #ff4ead dirs={}:/etc/xdg",
            sandbox.home.join(".config/kdedefaults").display()
        )),
        1
    );

    // Docker's client contexts need no daemon; `docker info` does.
    assert_eq!(
        fs::read_to_string(sandbox.root.join("docker/.current")).unwrap(),
        "rootless"
    );
    assert_eq!(sandbox.calls("docker info"), 0);
    // Only a running tailscaled stores its operator, so the person sets it after login.
    assert_eq!(sandbox.calls("tailscale set"), 0);
    assert!(
        io.notes
            .borrow()
            .iter()
            .any(|note| note.contains("sudo tailscale set --operator=tester"))
    );

    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &profile));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

/// A Btrfs system as the installer mounts it: the `@snapshots` subvolume at `/.snapshots`,
/// and snapper's shipped `/etc/conf.d/snapper`, which lists no configuration.
fn snapshot_system(sandbox: &Sandbox) {
    bootvine_boot(sandbox);
    let mounts = sandbox.root.join("mounts");
    fs::create_dir_all(&mounts).unwrap();
    fs::write(mounts.join(".snapshots"), "btrfs").unwrap();
    sandbox.fake(
        "findmnt",
        r#"for mount; do :; done; cat "$INAYOS_TEST_ROOT/mounts/${mount#/}" 2>/dev/null"#,
    );
    let conf = sandbox.root.join("system/etc/conf.d");
    fs::create_dir_all(&conf).unwrap();
    fs::write(conf.join("snapper"), SHIPPED_SNAPPER_CONFIGS).unwrap();
    // The image holds the sd-volatile program only when mkinitcpio built it with the
    // drop-in in place.
    fs::write(sandbox.root.join("system/boot/initramfs-linux.img"), "").unwrap();
    sandbox.fake(
        "mkinitcpio",
        r#"printf 'mkinitcpio %s\n' "$*" >> "$INAYOS_TEST_LOG"
image="$INAYOS_TEST_ROOT/system/boot/initramfs-linux.img"
if [ -e "$INAYOS_TEST_ROOT/system/etc/mkinitcpio.conf.d/inayos-volatile.conf" ]; then
    printf 'usr/lib/systemd/systemd-volatile-root\n' > "$image"
else
    : > "$image"
fi"#,
    );
    sandbox.fake("lsinitcpio", r#"cat "$1""#);
    sandbox.fake(
        "snapper",
        r#"[ -e "$INAYOS_TEST_ROOT/system/etc/snapper/configs/root" ]"#,
    );
}

const SHIPPED_SNAPPER_CONFIGS: &str =
    "## Path: System/Snapper\n\n# List of snapper configurations.\nSNAPPER_CONFIGS=\"\"\n";
const SNAPSHOT_FILES: [&str; 2] = [
    "/etc/snapper/configs/root",
    "/etc/mkinitcpio.conf.d/inayos-volatile.conf",
];

#[test]
fn snapshots_configure_snapper_for_the_system_and_add_the_overlay_hook_until_removed() {
    let sandbox = Sandbox::new("snapshots");
    snapshot_system(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::Snapshots];
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with_mode(&io, RunMode::Installer, |ctx| {
                runner::install(ctx, &modules, &modules)
            })
            .unwrap();
    }
    for package in ["snapper", "snap-pac"] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    let config = settings(&system_file(&sandbox, SNAPSHOT_FILES[0]).unwrap());
    for wanted in [
        "SUBVOLUME=\"/\"",
        "FSTYPE=\"btrfs\"",
        "NUMBER_CLEANUP=\"yes\"",
        "NUMBER_LIMIT=\"10\"",
        "NUMBER_LIMIT_IMPORTANT=\"5\"",
        "TIMELINE_CREATE=\"no\"",
    ] {
        assert!(config.contains(&wanted.to_owned()), "{wanted}: {config:?}");
    }
    assert_eq!(
        settings(&system_file(&sandbox, SNAPSHOT_FILES[1]).unwrap()),
        ["HOOKS+=(sd-volatile)"]
    );
    assert_eq!(
        system_file(&sandbox, "/etc/conf.d/snapper").unwrap(),
        SHIPPED_SNAPPER_CONFIGS.replace("SNAPPER_CONFIGS=\"\"", "SNAPPER_CONFIGS=\"root\"")
    );
    // The hooks that other modules add to mkinitcpio.conf stay there.
    assert_eq!(
        system_file(&sandbox, "/etc/mkinitcpio.conf").unwrap(),
        MKINITCPIO
    );
    assert_eq!(
        program_calls(&sandbox, "mkinitcpio -P"),
        1,
        "the initramfs was not rebuilt exactly once, while its image lacked the hook"
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::System,
        "snapper-cleanup.timer"
    ));
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    // A run that stopped after writing the drop-in left an image without the hook: verify
    // names it, and the next install rebuilds it although the drop-in is current.
    fs::write(sandbox.root.join("system/boot/initramfs-linux.img"), "").unwrap();
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    let error = results[0].1.as_ref().unwrap_err().to_string();
    assert!(error.contains("/boot/initramfs-linux.img"), "{error}");
    sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    assert_eq!(program_calls(&sandbox, "mkinitcpio -P"), 2);
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    fs::remove_file(sandbox.root.join("calls")).unwrap();
    let report = sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert!(!report.unanswered);
    for path in SNAPSHOT_FILES {
        assert_eq!(system_file(&sandbox, path), None, "{path} is still there");
    }
    assert_eq!(
        system_file(&sandbox, "/etc/conf.d/snapper").unwrap(),
        SHIPPED_SNAPPER_CONFIGS
    );
    let log = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let calls: Vec<&str> = log.lines().collect();
    let deleted = call_position(&calls, 0, "inayos-volatile.conf");
    call_position(&calls, deleted, "sudo mkinitcpio -P");
    assert!(!enabled(
        &sandbox,
        ServiceScope::System,
        "snapper-cleanup.timer"
    ));
    for package in ["snapper", "snap-pac"] {
        assert!(!sandbox.installed(package), "{package} is still installed");
    }
}

#[test]
fn snapshots_need_the_snapshot_subvolume_on_the_new_system_and_skip_without_it_on_a_running_one() {
    let modules: [&dyn Module; 1] = [&crate::Snapshots];
    let io = Answers::new(None);
    let sandbox = Sandbox::new("snapshots-no-subvolume");
    snapshot_system(&sandbox);
    fs::remove_file(sandbox.root.join("mounts/.snapshots")).unwrap();
    let error = sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap_err();
    assert!(error.to_string().contains("/.snapshots"), "{error}");
    assert!(!sandbox.installed("snapper"));
    assert_eq!(system_file(&sandbox, SNAPSHOT_FILES[0]), None);

    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    assert!(!sandbox.installed("snapper"));
    assert!(
        io.notes
            .borrow()
            .iter()
            .any(|note| note.contains("Skipping the snapshots"))
    );
}

/// rEFInd on the EFI system partition at `/efi` with its Btrfs driver, the refind
/// package's copies in `/usr/share/refind`, one kernel in /boot, and a fake `sbctl` that
/// keeps its keys, Setup Mode, saved files and signatures under `sbctl/`. The firmware
/// starts in Setup Mode with no keys.
fn secure_boot_system(sandbox: &Sandbox) {
    system(sandbox);
    let system_root = sandbox.root.join("system");
    for folder in ["efi/EFI/refind", "usr/share/refind"] {
        let folder = system_root.join(folder);
        fs::create_dir_all(folder.join("drivers_x64")).unwrap();
        fs::write(folder.join("refind_x64.efi"), "rEFInd").unwrap();
        fs::write(folder.join("drivers_x64/btrfs_x64.efi"), "btrfs").unwrap();
    }
    fs::write(
        system_root.join("efi/EFI/refind/refind.conf"),
        "timeout 5\n",
    )
    .unwrap();
    fs::create_dir_all(system_root.join("boot")).unwrap();
    fs::write(system_root.join("boot/vmlinuz-linux"), "kernel").unwrap();
    let state = sandbox.root.join("sbctl");
    fs::create_dir_all(&state).unwrap();
    fs::write(state.join("setup-mode"), "").unwrap();
    sandbox.fake(
        "sbctl",
        r#"printf 'sbctl %s\n' "$*" >> "$INAYOS_TEST_LOG"
state="$INAYOS_TEST_ROOT/sbctl"
strip() { printf '%s' "${1#"$INAYOS_TEST_ROOT"/system}"; }
flag() { if [ -e "$state/$1" ]; then printf true; else printf false; fi; }
command="$1"; shift
case "$command" in
    status) printf '{"installed":%s,"guid":"","setup_mode":%s,"secure_boot":%s,"vendors":[]}\n' \
        "$(flag keys)" "$(flag setup-mode)" "$(flag secure-boot)" ;;
    create-keys) : > "$state/keys" ;;
    enroll-keys) [ -e "$state/setup-mode" ] && [ -e "$state/keys" ] || exit 1
        rm "$state/setup-mode"; printf '%s\n' "$*" > "$state/enrolled" ;;
    sign) [ -e "$state/keys" ] || exit 1
        save= output=
        while [ "$#" -gt 0 ]; do
            case "$1" in
                -s) save=1 ;;
                -o) output="$2"; shift ;;
                *) file="$1" ;;
            esac
            shift
        done
        [ -n "$output" ] || output="$file"
        strip "$output" >> "$state/signed"; echo >> "$state/signed"
        if [ -n "$save" ] && ! grep -qxF "$(strip "$file")" "$state/files" 2>/dev/null; then
            strip "$file" >> "$state/files"; echo >> "$state/files"
        fi ;;
    list-files) [ -e "$state/keys" ] || exit 1
        printf '['; separator=
        if [ -e "$state/files" ]; then while read -r file; do
            printf '%s{"file":"%s","output_file":"%s","is_signed":true}' "$separator" "$file" "$file"; separator=,
        done < "$state/files"; fi
        printf ']\n' ;;
    remove-file) grep -vxF "$(strip "$1")" "$state/files" > "$state/files.new"; mv "$state/files.new" "$state/files" ;;
    verify) shift; printf '['; separator=
        for file; do
            name="$(strip "$file")"; signed=0
            grep -qxF "$name" "$state/signed" 2>/dev/null && signed=1
            printf '%s{"file_name":"%s","is_signed":%s}' "$separator" "$name" "$signed"; separator=,
        done
        printf ']\n' ;;
esac"#,
    );
}

/// How many times a fake program logged a call that starts with `call`. sudo logs its own
/// line, with the options it got, before the program logs this one.
fn program_calls(sandbox: &Sandbox, call: &str) -> usize {
    fs::read_to_string(sandbox.root.join("calls"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.starts_with(call))
        .count()
}

fn sbctl_state(sandbox: &Sandbox, name: &str) -> Option<String> {
    fs::read_to_string(sandbox.root.join("sbctl").join(name)).ok()
}

#[test]
fn secure_boot_on_the_new_system_signs_every_boot_file_and_leaves_enrolling_to_the_installer() {
    let sandbox = Sandbox::new("secure-boot");
    secure_boot_system(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::SecureBoot];
    let io = Answers::new(None);
    sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    assert!(sandbox.installed("sbctl"));
    assert!(sbctl_state(&sandbox, "keys").is_some());
    assert_eq!(program_calls(&sandbox, "sbctl enroll-keys"), 0);
    assert!(io.questions.borrow().is_empty(), "{:?}", io.questions);
    // rEFInd and its driver are signed from the package's copies, so sbctl's hook signs
    // them again after a rEFInd update; the kernel is signed in place.
    assert_eq!(
        sbctl_state(&sandbox, "files").unwrap(),
        "/usr/share/refind/drivers_x64/btrfs_x64.efi\n/usr/share/refind/refind_x64.efi\n/boot/vmlinuz-linux\n"
    );
    assert_eq!(
        sbctl_state(&sandbox, "signed").unwrap(),
        "/efi/EFI/refind/drivers_x64/btrfs_x64.efi\n/efi/EFI/refind/refind_x64.efi\n/boot/vmlinuz-linux\n"
    );

    // On a running system, a firmware still in Setup Mode fails the check.
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    let error = results[0].1.as_ref().unwrap_err().to_string();
    assert!(error.contains("Setup Mode"), "{error}");

    // The installer enrolls once the install passed; the check then passes, and the next
    // run keeps the keys.
    fs::remove_file(sandbox.root.join("sbctl/setup-mode")).unwrap();
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    assert_eq!(program_calls(&sandbox, "sbctl create-keys"), 1);
    assert_eq!(program_calls(&sandbox, "sbctl enroll-keys"), 0);

    // A kernel that nothing signed fails the check.
    fs::write(sandbox.root.join("system/boot/vmlinuz-linux-zen"), "kernel").unwrap();
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    let error = results[0].1.as_ref().unwrap_err().to_string();
    assert!(error.contains("/boot/vmlinuz-linux-zen"), "{error}");
}

#[test]
fn secure_boot_on_a_running_system_asks_then_enrolls_after_the_last_signature() {
    let sandbox = Sandbox::new("secure-boot-live");
    secure_boot_system(&sandbox);
    let efivars = sandbox.root.join("system/sys/firmware/efi/efivars");
    fs::create_dir_all(&efivars).unwrap();
    fs::write(
        efivars.join("dbDefault-8be4df61-93ca-11d2-aa0d-00e098032b8c"),
        "db",
    )
    .unwrap();
    let modules: [&dyn Module; 1] = [&crate::SecureBoot];
    let io = Answers::new(Some(true));
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    assert_eq!(io.questions.borrow().len(), 1);
    assert_eq!(
        sbctl_state(&sandbox, "enrolled").as_deref(),
        Some("--microsoft --firmware-builtin\n"),
        "the keys were not enrolled with Microsoft's and the firmware's keys"
    );
    let log = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let calls: Vec<&str> = log.lines().collect();
    let created = call_position(&calls, 0, "sbctl create-keys");
    let last_sign = calls
        .iter()
        .rposition(|line| line.starts_with("sbctl sign"))
        .unwrap();
    let enrolled = call_position(&calls, 0, "sbctl enroll-keys");
    assert!(created < last_sign && last_sign < enrolled, "{log}");
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }
}

#[test]
fn secure_boot_declined_on_a_running_system_creates_and_signs_nothing() {
    let sandbox = Sandbox::new("secure-boot-declined");
    secure_boot_system(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::SecureBoot];
    let io = Answers::new(Some(false));
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    assert_eq!(sandbox.calls("sbctl create-keys"), 0);
    assert_eq!(sandbox.calls("sbctl sign"), 0);
    assert_eq!(sandbox.calls("sbctl enroll-keys"), 0);
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    // Nobody to answer: the run skips the module instead of enrolling.
    let sandbox = Sandbox::new("secure-boot-unanswered");
    secure_boot_system(&sandbox);
    let io = Answers::new(None);
    let error = sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap_err();
    assert!(format!("{error:?}").contains("Unanswered"), "{error:?}");
    assert_eq!(sandbox.calls("sbctl create-keys"), 0);
    assert_eq!(sandbox.calls("sbctl enroll-keys"), 0);
}

#[test]
fn secure_boot_without_setup_mode_or_keys_asks_for_setup_mode_and_signs_nothing() {
    let sandbox = Sandbox::new("secure-boot-no-setup");
    secure_boot_system(&sandbox);
    fs::remove_file(sandbox.root.join("sbctl/setup-mode")).unwrap();
    let modules: [&dyn Module; 1] = [&crate::SecureBoot];
    let io = Answers::new(Some(true));
    let error = sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("turn on Setup Mode in the firmware settings"),
        "{error}"
    );
    // A running system goes on without Secure Boot, and says why.
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    assert!(results[0].1.is_ok(), "{:?}", results[0].1);
    assert!(
        io.notes
            .borrow()
            .iter()
            .any(|note| note.contains("turn on Setup Mode in the firmware settings"))
    );
    assert_eq!(sandbox.calls("sbctl create-keys"), 0);
    assert_eq!(sandbox.calls("sbctl sign"), 0);
    assert_eq!(sandbox.calls("sbctl enroll-keys"), 0);
}

#[test]
fn removing_secure_boot_waits_for_secure_boot_off_and_keeps_the_keys() {
    let sandbox = Sandbox::new("secure-boot-remove");
    secure_boot_system(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::SecureBoot];
    let io = Answers::new(Some(true));
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    fs::write(sandbox.root.join("sbctl/secure-boot"), "").unwrap();
    let error = sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap_err();
    assert!(
        error.to_string().contains("Turn off Secure Boot"),
        "{error}"
    );
    assert!(sandbox.installed("sbctl"));
    assert_eq!(sbctl_state(&sandbox, "files").unwrap().lines().count(), 3);

    fs::remove_file(sandbox.root.join("sbctl/secure-boot")).unwrap();
    let report = sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert!(!report.unanswered);
    assert_eq!(sbctl_state(&sandbox, "files").as_deref(), Some(""));
    assert!(!sandbox.installed("sbctl"));
    assert!(
        sbctl_state(&sandbox, "keys").is_some(),
        "remove deleted the keys that the firmware trusts"
    );
}

/// rEFInd on `/efi`, and the guard program and its hook where the `inayos` package puts
/// them. The fake guard writes `guard-status` as its status file and a default entry next
/// to `refind.conf`.
fn guarded_system(sandbox: &Sandbox) {
    system(sandbox);
    let system_root = sandbox.root.join("system");
    fs::create_dir_all(system_root.join("efi/EFI/refind")).unwrap();
    fs::write(
        system_root.join("efi/EFI/refind/refind.conf"),
        "timeout 5\n",
    )
    .unwrap();
    fs::create_dir_all(system_root.join("usr/bin")).unwrap();
    fs::create_dir_all(system_root.join("usr/share/libalpm/hooks")).unwrap();
    fs::write(
        system_root.join("usr/share/libalpm/hooks/zzz-inayos-guard.hook"),
        include_str!("../../../../packaging/inayos/zzz-inayos-guard.hook"),
    )
    .unwrap();
    super::program(
        &system_root.join("usr/bin/inayos-guard"),
        r#"printf 'inayos-guard %s\n' "$*" >> "$INAYOS_TEST_LOG"
mkdir -p "$INAYOS_TEST_ROOT/system/var/lib/inayos-guard"
cp "$INAYOS_TEST_ROOT/guard-status" "$INAYOS_TEST_ROOT/system/var/lib/inayos-guard/status.json"
printf 'default_selection "Boot with standard options"\n' > "$INAYOS_TEST_ROOT/system/efi/EFI/refind/inayos-guard.conf""#,
    );
    fs::write(
        sandbox.root.join("guard-status"),
        r#"{"time":"2026-11-19T10:00:00Z","passed":true,"failures":[]}"#,
    )
    .unwrap();
}

const GUARD_FILES: [&str; 4] = [
    "/etc/systemd/system/inayos-guard-boot.service",
    "/etc/systemd/system/fwupd.service.d/inayos-guard.conf",
    "/etc/systemd/user/inayos-guard-notify.path",
    "/etc/systemd/user/inayos-guard-notify.service",
];

#[test]
fn the_boot_guard_hooks_into_pacman_checks_once_at_install_and_reports_a_failed_check() {
    let sandbox = Sandbox::new("boot-guard");
    guarded_system(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::BootGuard];
    let io = Answers::new(None);
    sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    assert!(sandbox.installed("libnotify"));
    assert!(sandbox.installed("inayos"));
    for path in GUARD_FILES {
        assert!(system_file(&sandbox, path).is_some(), "{path} is missing");
    }
    // fwupd's start only records the Secure Boot variables; the guard adds its TPM slot
    // when fwupd stops after changing them or staging a capsule.
    let fwupd = settings(&system_file(&sandbox, GUARD_FILES[1]).unwrap());
    assert_eq!(fwupd.len(), 3, "{fwupd:?}");
    assert_eq!(fwupd[0], "[Service]");
    assert!(
        fwupd[1].starts_with("ExecStartPre=-+") && !fwupd[1].contains("inayos-guard "),
        "{fwupd:?}"
    );
    assert!(
        fwupd[2].starts_with("ExecStopPost=+")
            && fwupd[2].contains("fwupd-*.cap")
            && fwupd[2].contains("exec /usr/bin/inayos-guard prepare-firmware-update"),
        "{fwupd:?}"
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::System,
        "inayos-guard-boot.service"
    ));
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-guard-notify.path"
    ));
    assert_eq!(program_calls(&sandbox, "inayos-guard check-update"), 1);
    assert!(system_file(&sandbox, "/efi/EFI/refind/inayos-guard.conf").is_some());
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    fs::write(
        sandbox.root.join("system/var/lib/inayos-guard/status.json"),
        r#"{"time":"2026-11-19T10:05:00Z","passed":false,"failures":["the initramfs of 7.3.4-arch1-2 holds no kernel modules"]}"#,
    )
    .unwrap();
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    let error = results[0].1.as_ref().unwrap_err().to_string();
    assert!(error.contains("holds no kernel modules"), "{error}");

    let report = sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert!(!report.unanswered);
    for path in GUARD_FILES.into_iter().chain([
        "/efi/EFI/refind/inayos-guard.conf",
        "/var/lib/inayos-guard/status.json",
    ]) {
        assert_eq!(system_file(&sandbox, path), None, "{path} is still there");
    }
    assert!(!enabled(
        &sandbox,
        ServiceScope::System,
        "inayos-guard-boot.service"
    ));
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-guard-notify.path"
    ));
}

#[test]
fn an_inayos_package_without_the_guard_fails_before_any_unit_is_written() {
    let sandbox = Sandbox::new("boot-guard-missing");
    guarded_system(&sandbox);
    fs::remove_file(sandbox.root.join("system/usr/bin/inayos-guard")).unwrap();
    let modules: [&dyn Module; 1] = [&crate::BootGuard];
    let io = Answers::new(None);
    let error = sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("/usr/bin/inayos-guard is missing"),
        "{error}"
    );
    for path in GUARD_FILES {
        assert_eq!(system_file(&sandbox, path), None, "{path} was written");
    }
    assert_eq!(program_calls(&sandbox, "inayos-guard check-update"), 0);
}

const RESCUE_FILES: [&str; 4] = [
    "/usr/local/libexec/inayos-boot-rescue",
    "/etc/systemd/system/inayos-boot-rescue.service",
    "/etc/inayos-boot-rescue.conf",
    "/etc/sudoers.d/20-inayos-boot-rescue",
];

/// A system whose fake `visudo` accepts a rule from standard input only when `accepts`.
fn rescue_system(sandbox: &Sandbox, accepts: bool) {
    system(sandbox);
    let status = if accepts { 0 } else { 1 };
    sandbox.fake(
        "visudo",
        &format!(
            r#"printf 'visudo %s\n' "$*" >> "$INAYOS_TEST_LOG"
cat > /dev/null
exit {status}"#
        ),
    );
}

#[test]
fn the_boot_rescue_watches_each_boot_and_lets_its_user_run_sudo_without_a_password() {
    let sandbox = Sandbox::new("boot-rescue");
    rescue_system(&sandbox, true);
    let modules: [&dyn Module; 1] = [&crate::BootRescue];
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    for path in RESCUE_FILES {
        assert!(system_file(&sandbox, path).is_some(), "{path} is missing");
    }
    assert!(sandbox.installed("curl"));
    assert!(sandbox.installed("networkmanager"));
    assert_eq!(program_calls(&sandbox, "visudo -cqf -"), 1);
    assert_eq!(
        system_file(&sandbox, RESCUE_FILES[3]).unwrap(),
        "tester ALL=(ALL:ALL) NOPASSWD: ALL\n"
    );
    assert_eq!(
        system_file(&sandbox, RESCUE_FILES[2]).unwrap(),
        "RESCUE_USER=tester\n"
    );
    let program = sandbox
        .root
        .join("system/usr/local/libexec/inayos-boot-rescue");
    assert_eq!(
        fs::metadata(program).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::System,
        "inayos-boot-rescue.service"
    ));
    let results = sandbox.with(&io, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    let report = sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert!(!report.unanswered);
    for path in RESCUE_FILES {
        assert_eq!(system_file(&sandbox, path), None, "{path} is still there");
    }
    assert!(!enabled(
        &sandbox,
        ServiceScope::System,
        "inayos-boot-rescue.service"
    ));
}

#[test]
fn a_sudo_rule_that_visudo_refuses_is_never_written() {
    let sandbox = Sandbox::new("boot-rescue-refused");
    rescue_system(&sandbox, false);
    let modules: [&dyn Module; 1] = [&crate::BootRescue];
    let io = Answers::new(None);
    let error = sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("visudo refused the sudo rule for tester"),
        "{error}"
    );
    for path in RESCUE_FILES {
        assert_eq!(system_file(&sandbox, path), None, "{path} was written");
    }
}

/// The system that the InayOS installer leaves: Arch's `pacman.conf`, and both InayOS
/// packages installed with a keyring package that trusts `ABCD1234`. The fake `pacman-key`
/// keeps the keys of pacman's keyring as files under `keys/`.
fn system_with_inayos_packages(sandbox: &Sandbox) {
    system(sandbox);
    let system_root = sandbox.root.join("system");
    fs::create_dir_all(system_root.join("etc/pacman.d")).unwrap();
    fs::write(
        system_root.join("etc/pacman.conf"),
        "[options]\nSigLevel = Required DatabaseOptional\n\n[core]\nInclude = /etc/pacman.d/mirrorlist\n",
    )
    .unwrap();
    fs::create_dir_all(system_root.join("usr/share/pacman/keyrings")).unwrap();
    fs::write(
        system_root.join("usr/share/pacman/keyrings/inayos-trusted"),
        "ABCD1234:4:\n",
    )
    .unwrap();
    for package in ["inayos", "inayos-keyring"] {
        fs::write(sandbox.root.join("installed").join(package), "").unwrap();
    }
    fs::create_dir_all(sandbox.root.join("keys")).unwrap();
    sandbox.fake(
        "pacman-key",
        r#"printf 'pacman-key %s\n' "$*" >> "$INAYOS_TEST_LOG"
keys="$INAYOS_TEST_ROOT/keys"
case "$1" in
    --list-keys) [ -e "$keys/$2" ] ;;
    --populate) while IFS=: read -r key _; do [ -n "$key" ] && : > "$keys/$key"; done < "$INAYOS_TEST_ROOT/system/usr/share/pacman/keyrings/inayos-trusted" ;;
    --delete) rm "$keys/$2" ;;
    *) exit 1 ;;
esac"#,
    );
}

#[test]
fn the_inayos_repository_joins_pacman_with_its_key_and_leaves_with_it() {
    let sandbox = Sandbox::new("inayos-repository");
    system_with_inayos_packages(&sandbox);
    let shipped = system_file(&sandbox, "/etc/pacman.conf").unwrap();
    let modules: [&dyn Module; 1] = [&crate::InayosRepository];
    let io = Answers::new(None);
    sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    let conf = system_file(&sandbox, "/etc/pacman.conf").unwrap();
    assert!(conf.starts_with(&shipped), "{conf}");
    assert_eq!(
        settings(&conf).last().map(String::as_str),
        Some("Include=/etc/pacman.d/inayos.conf")
    );
    // This binary carries the placeholder key, so the releases are not signed with the key
    // that the keyring trusts, and pacman reads no InayOS repository.
    let repository = system_file(&sandbox, "/etc/pacman.d/inayos.conf").unwrap();
    assert!(
        repository.lines().all(|line| line.starts_with('#')),
        "{repository}"
    );
    assert!(sandbox.root.join("keys/ABCD1234").exists());
    // Both packages were there, so only the run's own refresh read the databases, and
    // nothing was installed.
    assert_eq!(program_calls(&sandbox, "pacman -Syu"), 1);
    assert_eq!(program_calls(&sandbox, "pacman -S "), 0);
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    assert!(results[0].1.is_ok(), "{:?}", results[0].1);

    // A second install changes nothing more.
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    assert_eq!(program_calls(&sandbox, "pacman-key --populate"), 1);
    assert_eq!(system_file(&sandbox, "/etc/pacman.conf").unwrap(), conf);

    sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert_eq!(system_file(&sandbox, "/etc/pacman.conf").unwrap(), shipped);
    assert_eq!(system_file(&sandbox, "/etc/pacman.d/inayos.conf"), None);
    assert!(!sandbox.root.join("keys/ABCD1234").exists());
    // The installer installed the packages, so they stay.
    assert!(sandbox.installed("inayos"));
}

/// The system that the InayOS installer leaves for an encrypted install: the EFI system
/// partition at `/efi`, the crypttab and kernel options it writes, and a fake mkinitcpio that
/// builds the unified kernel image, and puts systemd-cryptsetup into the initramfs only with
/// the encryption drop-in in place.
fn encrypted_system(sandbox: &Sandbox) {
    bootvine_boot(sandbox);
    let mounts = sandbox.root.join("mounts");
    fs::create_dir_all(&mounts).unwrap();
    fs::write(mounts.join("efi"), "vfat").unwrap();
    sandbox.fake(
        "findmnt",
        r#"for mount; do :; done; cat "$INAYOS_TEST_ROOT/mounts/${mount#/}" 2>/dev/null"#,
    );
    let system = sandbox.root.join("system");
    fs::create_dir_all(system.join("etc/kernel")).unwrap();
    fs::write(
        system.join("etc/crypttab.initramfs"),
        "system UUID=1111 none tpm2-device=auto,discard\nhome UUID=3333 none tpm2-device=auto,discard\n",
    )
    .unwrap();
    fs::write(
        system.join("etc/kernel/cmdline"),
        "root=/dev/mapper/system rootflags=subvol=@ rw\n",
    )
    .unwrap();
    fs::create_dir_all(system.join("efi/EFI")).unwrap();
    fs::write(system.join("boot/initramfs-linux.img"), "").unwrap();
    sandbox.fake(
        "mkinitcpio",
        r#"printf 'mkinitcpio %s\n' "$*" >> "$INAYOS_TEST_LOG"
root="$INAYOS_TEST_ROOT/system"
if [ -e "$root/etc/mkinitcpio.conf.d/inayos-encryption.conf" ]; then
    printf 'usr/lib/systemd/systemd-cryptsetup\nusr/lib/systemd/systemd-hibernate-resume\n' > "$root/boot/initramfs-linux.img"
    [ -d "$root/efi/EFI/Linux" ] && cp "$root/etc/kernel/cmdline" "$root/efi/EFI/Linux/inayos-linux.efi"
else
    printf 'usr/lib/systemd/systemd-hibernate-resume\n' > "$root/boot/initramfs-linux.img"
fi
exit 0"#,
    );
    sandbox.fake("lsinitcpio", r#"cat "$1""#);
}

const ENCRYPTION_FILES: [&str; 3] = [
    "/etc/mkinitcpio.conf.d/inayos-encryption.conf",
    "/etc/mkinitcpio.d/linux.preset",
    "/etc/systemd/user/inayos-recovery-key-reminder.service",
];

#[test]
fn encryption_builds_the_unified_kernel_image_that_opens_the_volumes_once() {
    let sandbox = Sandbox::new("encryption");
    encrypted_system(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::Encryption];
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with_mode(&io, RunMode::Installer, |ctx| {
                runner::install(ctx, &modules, &modules)
            })
            .unwrap();
    }
    for package in ["systemd-ukify", "tpm2-tss", "libnotify"] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    let preset = settings(&system_file(&sandbox, ENCRYPTION_FILES[1]).unwrap());
    for wanted in [
        "PRESETS=('default')",
        "default_image=\"/boot/initramfs-linux.img\"",
        "default_uki=\"/efi/EFI/Linux/inayos-linux.efi\"",
    ] {
        assert!(preset.contains(&wanted.to_owned()), "{wanted}: {preset:?}");
    }
    // The image takes the kernel options that the installer wrote.
    assert_eq!(
        system_file(&sandbox, "/efi/EFI/Linux/inayos-linux.efi").as_deref(),
        Some("root=/dev/mapper/system rootflags=subvol=@ rw\n")
    );
    assert_eq!(program_calls(&sandbox, "mkinitcpio -P"), 1);
    let reminder = settings(&system_file(&sandbox, ENCRYPTION_FILES[2]).unwrap());
    assert!(reminder.contains(&"ConditionPathExists=%h/inayos-recovery-key.txt".to_owned()));
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "inayos-recovery-key-reminder.service"
    ));
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    // A kernel option that points the root elsewhere fails the check.
    fs::write(
        sandbox.root.join("system/etc/kernel/cmdline"),
        "root=UUID=31f4 rw\n",
    )
    .unwrap();
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    let error = results[0].1.as_ref().unwrap_err().to_string();
    assert!(error.contains("root="), "{error}");

    // Remove refuses while the root is encrypted, and changes nothing.
    let error = sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap_err();
    assert!(error.to_string().contains("crypttab"), "{error}");
    assert!(system_file(&sandbox, ENCRYPTION_FILES[1]).is_some());
}

#[test]
fn encryption_needs_the_installers_volumes_on_the_new_system_and_skips_without_them() {
    let sandbox = Sandbox::new("encryption-plain");
    encrypted_system(&sandbox);
    fs::remove_file(sandbox.root.join("system/etc/crypttab.initramfs")).unwrap();
    let modules: [&dyn Module; 1] = [&crate::Encryption];
    let io = Answers::new(None);
    let error = sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap_err();
    assert!(error.to_string().contains("crypttab"), "{error}");
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    assert!(!sandbox.installed("systemd-ukify"));
    assert_eq!(system_file(&sandbox, ENCRYPTION_FILES[1]), None);
}

/// The hooks that mkinitcpio ends up with: `/etc/mkinitcpio.conf`, then its drop-ins in
/// name order, read by bash as mkinitcpio reads them.
fn hooks_after_drop_ins(drop_ins: &[&[u8]]) -> String {
    let folder = std::env::temp_dir().join(format!("inayos-hooks-{}", std::process::id()));
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("mkinitcpio.conf"), MKINITCPIO).unwrap();
    let mut script = format!(". {}/mkinitcpio.conf\n", folder.display());
    for (index, drop_in) in drop_ins.iter().enumerate() {
        let path = folder.join(format!("{index}.conf"));
        fs::write(&path, drop_in).unwrap();
        script.push_str(&format!(". {}\n", path.display()));
    }
    script.push_str("printf '%s\\n' \"${HOOKS[*]}\"\n");
    let output = std::process::Command::new("bash")
        .args(["-c", &script])
        .output()
        .unwrap();
    fs::remove_dir_all(&folder).unwrap();
    assert!(output.status.success(), "{output:?}");
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[test]
fn the_encryption_and_overlay_drop_ins_compose_with_the_hooks_of_mkinitcpio_conf() {
    let encryption = embedded_dotfiles::ENCRYPTION_ASSETS
        .etc
        .mkinitcpio_conf_d
        .inayos_encryption_conf
        .content;
    let volatile = embedded_dotfiles::SNAPSHOTS_ASSETS
        .etc
        .mkinitcpio_conf_d
        .inayos_volatile_conf
        .content;
    // `inayos-encryption.conf` sorts before `inayos-volatile.conf`.
    assert_eq!(
        hooks_after_drop_ins(&[encryption, volatile]),
        "base systemd autodetect microcode kms modconf block keyboard sd-vconsole sd-encrypt filesystems fsck sd-volatile"
    );
    assert_eq!(
        hooks_after_drop_ins(&[encryption, encryption]),
        "base systemd autodetect microcode kms modconf block keyboard sd-vconsole sd-encrypt filesystems fsck"
    );
}

/// An ext4 home at `/home`, 1 GiB of memory, and fake swap tools: `mkswap` creates the file
/// of the size it is given and marks it as swap, which `blkid` reads back.
fn hibernation_system(sandbox: &Sandbox) {
    encrypted_system(sandbox);
    fs::write(sandbox.root.join("mounts/home"), "ext4").unwrap();
    let system = sandbox.root.join("system");
    fs::create_dir_all(system.join("home")).unwrap();
    fs::create_dir_all(system.join("proc")).unwrap();
    fs::write(system.join("proc/meminfo"), "MemTotal:        1000000 kB\n").unwrap();
    fs::write(
        system.join("proc/swaps"),
        "Filename Type Size Used Priority\n",
    )
    .unwrap();
    // The systemd hook puts the resume program into every image.
    fs::write(
        system.join("boot/initramfs-linux.img"),
        "usr/lib/systemd/systemd-hibernate-resume\n",
    )
    .unwrap();
    sandbox.fake(
        "mkswap",
        r#"printf 'mkswap %s\n' "$*" >> "$INAYOS_TEST_LOG"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --size) size="$2"; shift ;;
        --file) ;;
        *) file="$1" ;;
    esac
    shift
done
truncate -s "$size" "$file" && : > "$file.swap""#,
    );
    sandbox.fake(
        "blkid",
        r#"for file; do :; done; [ -e "$file.swap" ] && echo swap"#,
    );
    sandbox.fake(
        "stat",
        r#"for file; do :; done; printf '%s:%s:root\n' "$(/usr/bin/stat -c %s "$file")" "$(/usr/bin/stat -c %a "$file")""#,
    );
    sandbox.fake("chown", r#"printf 'chown %s\n' "$*" >> "$INAYOS_TEST_LOG""#);
}

#[test]
fn hibernation_creates_a_swap_file_as_large_as_the_memory_and_its_unit_until_removed() {
    let sandbox = Sandbox::new("hibernation");
    hibernation_system(&sandbox);
    let modules: [&dyn Module; 1] = [&crate::Hibernation];
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with_mode(&io, RunMode::Installer, |ctx| {
                runner::install(ctx, &modules, &modules)
            })
            .unwrap();
    }
    let swap_file = sandbox.root.join("system/home/swapfile");
    let metadata = fs::metadata(&swap_file).unwrap();
    // 1,000,000 kB of memory rounds up to 1 GiB.
    assert_eq!(metadata.len(), 1 << 30);
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    assert_eq!(program_calls(&sandbox, "mkswap"), 1);
    let unit = settings(&system_file(&sandbox, "/etc/systemd/system/home-swapfile.swap").unwrap());
    for wanted in ["What=/home/swapfile", "Priority=0", "WantedBy=swap.target"] {
        assert!(unit.contains(&wanted.to_owned()), "{wanted}: {unit:?}");
    }
    assert!(enabled(
        &sandbox,
        ServiceScope::System,
        "home-swapfile.swap"
    ));
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    for (name, result) in results {
        assert!(result.is_ok(), "{name}: {result:?}");
    }

    // More memory: the check names the size, and install keeps the file it made earlier
    // and names `remove`, which deletes it, so the next install makes it again.
    fs::write(
        sandbox.root.join("system/proc/meminfo"),
        "MemTotal:        2000000 kB\n",
    )
    .unwrap();
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    let error = results[0].1.as_ref().unwrap_err().to_string();
    assert!(error.contains("/home/swapfile"), "{error}");
    let error = sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap_err();
    assert!(
        error.to_string().contains("inayos remove hibernation"),
        "{error}"
    );
    assert_eq!(fs::metadata(&swap_file).unwrap().len(), 1 << 30);
    sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert!(!swap_file.exists());
    sandbox
        .with_mode(&io, RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    assert_eq!(fs::metadata(&swap_file).unwrap().len(), 2 << 30);

    // An initramfs that cannot resume fails the check.
    fs::write(sandbox.root.join("system/boot/initramfs-linux.img"), "").unwrap();
    let results = sandbox.with_mode(&io, RunMode::Installer, |ctx| runner::verify(ctx, &modules));
    let error = results[0].1.as_ref().unwrap_err().to_string();
    assert!(error.contains("systemd-hibernate-resume"), "{error}");

    let report = sandbox
        .with(&io, |ctx| runner::remove(ctx, &modules, &modules, false))
        .unwrap();
    assert!(!report.unanswered);
    assert!(!swap_file.exists());
    assert_eq!(
        system_file(&sandbox, "/etc/systemd/system/home-swapfile.swap"),
        None
    );
    assert!(!enabled(
        &sandbox,
        ServiceScope::System,
        "home-swapfile.swap"
    ));
}

#[test]
fn hibernation_never_deletes_a_swap_file_it_did_not_make() {
    let sandbox = Sandbox::new("hibernation-on");
    hibernation_system(&sandbox);
    let swap_file = sandbox.root.join("system/home/swapfile");
    fs::write(&swap_file, "old").unwrap();
    fs::write(
        sandbox.root.join("system/proc/swaps"),
        "Filename Type Size Used Priority\n/home/swapfile file 3 0 0\n",
    )
    .unwrap();
    let modules: [&dyn Module; 1] = [&crate::Hibernation];
    let error = sandbox
        .with(&Answers::new(None), |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap_err();
    assert!(error.to_string().contains("swapoff"), "{error}");
    assert_eq!(fs::read_to_string(&swap_file).unwrap(), "old");
}

#[test]
fn secure_boot_signs_the_unified_kernel_image_of_an_encrypted_system() {
    let sandbox = Sandbox::new("secure-boot-uki");
    secure_boot_system(&sandbox);
    let images = sandbox.root.join("system/efi/EFI/Linux");
    fs::create_dir_all(&images).unwrap();
    fs::write(images.join("inayos-linux.efi"), "image").unwrap();
    let modules: [&dyn Module; 1] = [&crate::SecureBoot];
    sandbox
        .with_mode(&Answers::new(None), RunMode::Installer, |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    // Saved in sbctl's list, so its hooks sign it again after updates.
    assert!(
        sbctl_state(&sandbox, "files")
            .unwrap()
            .lines()
            .any(|file| file == "/efi/EFI/Linux/inayos-linux.efi")
    );
}

#[test]
fn bootvine_joins_the_kernel_options_of_the_unified_kernel_image_and_rebuilds_it() {
    let sandbox = Sandbox::new("bootvine-cmdline");
    bootvine_boot(&sandbox);
    let cmdline = sandbox.root.join("system/etc/kernel/cmdline");
    fs::create_dir_all(cmdline.parent().unwrap()).unwrap();
    fs::write(&cmdline, "root=/dev/mapper/system rootflags=subvol=@ rw\n").unwrap();
    let modules: [&dyn Module; 1] = [&crate::Bootvine];
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with_mode(&io, RunMode::Installer, |ctx| {
                runner::install(ctx, &modules, &modules)
            })
            .unwrap();
    }
    let options = fs::read_to_string(&cmdline).unwrap();
    assert!(
        options.starts_with("root=/dev/mapper/system rootflags=subvol=@ rw quiet vt.default_red="),
        "{options}"
    );
    assert_eq!(options.matches("quiet").count(), 1);
    assert_eq!(program_calls(&sandbox, "mkinitcpio -P"), 1);
}
