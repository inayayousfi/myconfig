//! What each module installs, writes, enables and refuses, against fake system programs.
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use super::{Answers, Sandbox};
use crate::{
    AgentConfigStowed, Base, CachyosSetup, Cli, Context, Docker, EmacsOptions, EmacsStowed,
    EnvironmentInventory, Footprint, Ghostty, Handy, Kanata, KanataKde, Module, ModuleResult,
    Package, PackageSystem, Runtimes, runner,
};

/// Fake system programs that keep their state under the sandbox, so modules run as on a
/// real machine without root: `sudo` moves `/etc`, `/usr`, `/boot` and `/efi` paths under
/// `system/`, groups live in `groups/`, `systemctl` remembers enabled units, KDE keys
/// live in `kde/`, and `ufw` prints its rules in ufw's own word order.
fn system(sandbox: &Sandbox) {
    for directory in ["system", "groups", "units", "kde", "ufw"] {
        fs::create_dir_all(sandbox.root.join(directory)).unwrap();
    }
    sandbox.fake(
        "sudo",
        r#"printf 'sudo %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
for argument in "$@"; do
    case "$argument" in
        /etc/* | /usr/* | /boot/* | /efi/*) set -- "$@" "$MYCONFIG_TEST_ROOT/system$argument" ;;
        *) set -- "$@" "$argument" ;;
    esac
    shift
done
exec "$@""#,
    );
    sandbox.fake(
        "id",
        r#"case "$*" in
    -un) printf 'tester\n' ;;
    -u) printf '1000\n' ;;
    -Gn) printf 'users\n' ;;
    "-nG tester") printf 'users'; for group in "$MYCONFIG_TEST_ROOT"/groups/*; do
            [ -e "$group/tester" ] && printf ' %s' "$(basename "$group")"; done; printf '\n' ;;
    *) exit 1 ;;
esac"#,
    );
    sandbox.fake(
        "getent",
        r#"[ "$1" = group ] && [ -d "$MYCONFIG_TEST_ROOT/groups/$2" ]"#,
    );
    sandbox.fake(
        "groupadd",
        r#"for group; do :; done; mkdir -p "$MYCONFIG_TEST_ROOT/groups/$group""#,
    );
    sandbox.fake(
        "usermod",
        r#"printf 'usermod %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
mkdir -p "$MYCONFIG_TEST_ROOT/groups/$2" && : > "$MYCONFIG_TEST_ROOT/groups/$2/$3""#,
    );
    sandbox.fake("gpasswd", r#"rm -f "$MYCONFIG_TEST_ROOT/groups/$3/$2""#);
    sandbox.fake("groupdel", r#"rm -rf "$MYCONFIG_TEST_ROOT/groups/$1""#);
    for program in ["modprobe", "udevadm"] {
        sandbox.fake(
            program,
            &format!("printf '{program} %s\\n' \"$*\" >> \"$MYCONFIG_TEST_LOG\""),
        );
    }
    sandbox.fake(
        "systemctl",
        r#"printf 'systemctl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
[ "$1" = --user ] && shift
[ "$1" = --quiet ] && shift
units="$MYCONFIG_TEST_ROOT/units"
case "$1" in
    enable) : > "$units/$2" ;;
    disable) [ "$2" = --now ] && shift; rm -f "$units/$2" ;;
    is-enabled) if [ -e "$units/$2" ]; then echo enabled; else echo disabled; exit 1; fi ;;
    is-active) case "$2" in graphical-session.target) exit 1 ;; *) echo active ;; esac ;;
esac"#,
    );
    sandbox.fake(
        "kwriteconfig6",
        r#"printf 'kwriteconfig6 %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
path="$MYCONFIG_TEST_ROOT/kde"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --file | --group) path="$path/$2"; shift 2 ;;
        --key) key="$2"; shift 2 ;;
        --delete) rm -f "$path/$key"; exit 0 ;;
        *) value="$1"; shift ;;
    esac
done
mkdir -p "$path" && printf '%s' "$value" > "$path/$key""#,
    );
    sandbox.fake(
        "kreadconfig6",
        r#"path="$MYCONFIG_TEST_ROOT/kde"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --file | --group) path="$path/$2"; shift 2 ;;
        --key) key="$2"; shift 2 ;;
        --default) default="$2"; shift 2 ;;
        *) shift ;;
    esac
done
if [ -e "$path/$key" ]; then cat "$path/$key"; else printf '%s' "$default"; fi"#,
    );
    sandbox.fake("python", r#"exec python3 "$@""#);
    sandbox.fake(
        "ufw",
        r#"printf 'ufw %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
rules="$MYCONFIG_TEST_ROOT/ufw/rules"
case "$1" in
    show) [ -e "$rules" ] && cat "$rules"; exit 0 ;;
    status) if [ -e "$MYCONFIG_TEST_ROOT/ufw/active" ]; then echo 'Status: active'; else echo 'Status: inactive'; fi; exit 0 ;;
    --force) : > "$MYCONFIG_TEST_ROOT/ufw/active"; exit 0 ;;
    disable) rm -f "$MYCONFIG_TEST_ROOT/ufw/active"; exit 0 ;;
    delete) shift; delete=1 ;;
esac
# Print the rule as ufw lists it: action, from, to, port, proto, comment.
action="$1"; shift
while [ "$#" -gt 0 ]; do
    case "$1" in
        in) shift ;;
        from | to | port | proto | comment) eval "$1=\"\$2\""; shift 2 ;;
        *) shift ;;
    esac
done
line="ufw $action from $from to $to port $port proto $proto comment '$comment'"
touch "$rules"
if [ -n "${delete:-}" ]; then grep -Fxv "$line" "$rules" > "$rules.new"; mv "$rules.new" "$rules"
else printf '%s\n' "$line" >> "$rules"; fi"#,
    );
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

fn enabled(sandbox: &Sandbox, unit: &str) -> bool {
    sandbox.root.join("units").join(unit).exists()
}

fn install_everything(sandbox: &Sandbox, packages: &[&str]) {
    for package in packages {
        fs::write(sandbox.root.join("installed").join(package), "").unwrap();
    }
}

#[test]
fn every_linux_module_package_has_a_name_on_its_package_manager() {
    // A package without a name fails the install before the module changes anything.
    let sandbox = Sandbox::new("package-names");
    let arch: [&dyn Module; 6] = [
        &Base {
            packages: Base::ARCH,
            unwanted: &[Package::CachyUpdate],
        },
        &CachyosSetup,
        &Cli {
            retired_config: &[],
        },
        &Runtimes,
        &EmacsStowed(EmacsOptions {
            browser_terminal_firewall: true,
        }),
        &crate::KdePlasma,
    ];
    let more: [&dyn Module; 13] = [
        &crate::Ssh,
        &crate::TerminalTools,
        &Ghostty,
        &crate::AxidevOsk,
        &crate::Tailscale,
        &crate::AgentsPackages {
            extra: &[Package::Ydotool, Package::WslSshAgent],
        },
        &crate::AndroidPhone,
        &Kanata {
            start_at_login: true,
        },
        &KanataKde,
        &Handy,
        &crate::Pipewire,
        &Docker,
        &AgentConfigStowed { ydotool: true },
    ];
    sandbox.with(&Answers::new(None), |ctx| {
        for module in arch.iter().chain(more.iter()) {
            ctx.specs(&module.footprint(ctx).packages)
                .unwrap_or_else(|error| panic!("{}: {error}", module.name()));
        }
    });
    let apt = |package| myconfig_utils::resolve_package(package, PackageSystem::Apt);
    for package in Base::UBUNTU
        .iter()
        .chain(&[Package::Zsh, Package::Git, Package::Curl])
    {
        assert!(apt(*package).is_some(), "{package:?} has no APT name");
    }
    let arch_name = |package| {
        myconfig_utils::resolve_package(package, PackageSystem::Arch)
            .unwrap()
            .name
    };
    assert_eq!(arch_name(Package::Kanata), "kanata-bin");
    assert_eq!(arch_name(Package::Handy), "handy-bin");
    assert_eq!(arch_name(Package::GithubCli), "github-cli-git");
    assert_eq!(arch_name(Package::IosevkaFont), "ttf-iosevka-nerd");
    assert_eq!(apt(Package::Fd).unwrap().name, "fd-find");
}

#[test]
fn cachyos_replaces_its_defaults_and_the_cli_retires_old_tools() {
    let sandbox = Sandbox::new("cachyos-packages");
    install_everything(
        &sandbox,
        &[
            "konsole",
            "firefox",
            "fish",
            "kate",
            "fd",
            "neovim",
            "tmux",
            "cachy-update",
            "vim",
        ],
    );
    let modules: [&dyn Module; 4] = [
        &Base {
            packages: Base::ARCH,
            unwanted: &[Package::CachyUpdate],
        },
        &CachyosSetup,
        &Cli {
            retired_config: &[],
        },
        &Runtimes,
    ];
    sandbox.fake(
        "rustup",
        r#"[ "$#" -eq 1 ] && echo 'stable-x86_64-unknown-linux-gnu (default)'; exit 0"#,
    );
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules))
        .unwrap();
    for removed in [
        "konsole",
        "firefox",
        "fish",
        "kate",
        "fd",
        "neovim",
        "tmux",
        "cachy-update",
        "vim",
    ] {
        assert!(!sandbox.installed(removed), "{removed} is still installed");
    }
    for kept in [
        "linux-cachyos",
        "cachyos-kernel-manager",
        "noto-fonts-cjk",
        "ripgrep",
        "jq",
        "btop",
        "tokei",
        "jdk-openjdk",
        "maven",
    ] {
        assert!(sandbox.installed(kept), "{kept} was not installed");
    }
}

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
    assert_eq!(sandbox.calls("pacman -R --noconfirm kitty konsole"), 1);
    assert_eq!(
        sandbox.calls("pacman -R"),
        1,
        "no removal runs when nothing is installed"
    );
}

/// Deploys one real config package.
struct Config(&'static str);

impl Module for Config {
    fn name(&self) -> &'static str {
        self.0
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
        .with(&io, |ctx| runner::install(ctx, &[&Config("yazi")]))
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
            .with(&io, |ctx| runner::install(ctx, &[&Config("ghostty")]))
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
fn the_agent_package_keeps_bin_and_claude_as_real_folders() {
    let sandbox = Sandbox::new("ai-folders");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Config("ai")]))
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
fn the_cli_unlinks_retired_config_and_remove_can_link_it_again() {
    let sandbox = Sandbox::new("retired");
    let retired = sandbox.home.join("dotfiles/nvim/.config/nvim");
    fs::create_dir_all(&retired).unwrap();
    fs::write(retired.join("init.lua"), "retired\n").unwrap();
    fs::create_dir_all(sandbox.home.join(".config")).unwrap();
    std::process::Command::new("stow")
        .args(["--dir", "dotfiles", "--target", ".", "nvim"])
        .current_dir(&sandbox.home)
        .status()
        .unwrap();
    assert!(sandbox.home.join(".config/nvim/init.lua").exists());
    let cli = Cli {
        retired_config: &["nvim"],
    };
    let io = Answers::new(Some(true));
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&cli]))
        .unwrap();
    assert!(fs::symlink_metadata(sandbox.home.join(".config/nvim")).is_err());
    sandbox
        .with(&io, |ctx| runner::remove(ctx, &[&cli], &[&cli], false))
        .unwrap();
    assert!(sandbox.home.join(".config/nvim/init.lua").exists());
}

#[test]
fn emacs_opens_the_browser_terminal_only_to_private_networks() {
    let sandbox = Sandbox::new("emacs-firewall");
    system(&sandbox);
    fs::create_dir_all(sandbox.home.join(".emacs.d")).unwrap();
    fs::write(sandbox.home.join(".emacs.d/legacy"), "old\n").unwrap();
    let emacs = EmacsStowed(EmacsOptions {
        browser_terminal_firewall: true,
    });
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&emacs]))
        .unwrap();
    for package in ["emacs-wayland", "sshfs", "ttf-iosevka-nerd", "ufw"] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    let rules = fs::read_to_string(sandbox.root.join("ufw/rules")).unwrap();
    for subnet in ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"] {
        assert!(rules.contains(&format!("from {subnet} to any port 18080,18081 proto tcp")));
    }
    assert_eq!(rules.lines().count(), 3);
    assert!(sandbox.root.join("ufw/active").exists());
    assert!(
        !sandbox.home.join(".emacs.d").exists(),
        "the old ~/.emacs.d would shadow the config"
    );

    sandbox
        .with(&io, |ctx| runner::remove(ctx, &[&emacs], &[&emacs], false))
        .unwrap();
    assert_eq!(
        fs::read_to_string(sandbox.root.join("ufw/rules")).unwrap(),
        ""
    );
    assert!(!sandbox.root.join("ufw/active").exists());
    assert_eq!(
        fs::read_to_string(sandbox.home.join(".emacs.d/legacy")).unwrap(),
        "old\n"
    );
}

#[test]
fn kanata_gets_input_access_and_waits_for_the_new_groups() {
    let sandbox = Sandbox::new("kanata");
    system(&sandbox);
    sandbox.fake(
        "kanata",
        r#"printf 'kanata %s\n' "$*" >> "$MYCONFIG_TEST_LOG""#,
    );
    let kanata = Kanata {
        start_at_login: true,
    };
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&kanata]))
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
        system_file(&sandbox, "/etc/modules-load.d/myconfig-kanata.conf").as_deref(),
        Some("uinput\n")
    );
    let rules = system_file(&sandbox, "/etc/udev/rules.d/99-myconfig-kanata.rules").unwrap();
    assert!(rules.contains(
        r#"KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput""#
    ));
    assert!(rules.contains(r#"SUBSYSTEM=="input", KERNEL=="event*", MODE="0660", GROUP="input""#));
    assert!(enabled(&sandbox, "myconfig-kanata.service"));
    assert_eq!(
        sandbox.calls("restart myconfig-kanata.service"),
        0,
        "the user's session lacks the new groups"
    );

    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&kanata], &[&kanata], false)
        })
        .unwrap();
    assert!(!member(&sandbox, "uinput"));
    assert!(system_file(&sandbox, "/etc/udev/rules.d/99-myconfig-kanata.rules").is_none());
    assert!(!enabled(&sandbox, "myconfig-kanata.service"));
}

#[test]
fn kanata_started_by_the_kde_tray_is_not_enabled_on_its_own() {
    let sandbox = Sandbox::new("kanata-tray");
    system(&sandbox);
    sandbox.fake("kanata", "exit 0");
    sandbox.fake("plasmashell", "exit 0");
    let wants = sandbox
        .home
        .join(".config/systemd/user/default.target.wants");
    fs::create_dir_all(&wants).unwrap();
    std::os::unix::fs::symlink(
        "../myconfig-kanata.service",
        wants.join("myconfig-kanata.service"),
    )
    .unwrap();
    let modules: [&dyn Module; 2] = [
        &Kanata {
            start_at_login: false,
        },
        &KanataKde,
    ];
    sandbox
        .with(&Answers::new(None), |ctx| runner::install(ctx, &modules))
        .unwrap();
    assert!(!enabled(&sandbox, "myconfig-kanata.service"));
    assert!(fs::symlink_metadata(wants.join("myconfig-kanata.service")).is_err());
    assert!(enabled(&sandbox, "myconfig-kanata-tray.service"));
    assert_eq!(
        kde(&sandbox, "kglobalshortcutsrc/kwin/Overview").as_deref(),
        Some("Meta+W,Meta+W,Toggle Overview")
    );
    assert_eq!(
        sandbox.calls("stop myconfig-kanata.service"),
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
        .with(&io, |ctx| runner::install(ctx, &[&Handy]))
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
    assert!(system_file(&sandbox, "/etc/udev/rules.d/99-myconfig-handy.rules").is_some());
    assert!(enabled(&sandbox, "myconfig-handy.service"));
    assert_eq!(sandbox.calls("stop myconfig-handy.service"), 1);

    sandbox
        .with(&io, |ctx| runner::remove(ctx, &[&Handy], &[&Handy], false))
        .unwrap();
    assert_eq!(
        fs::read_to_string(&settings).unwrap(),
        r#"{"settings":{"selected_model":"keep-model","unrelated":{"keep":true}}}"#
    );
}

#[test]
fn docker_refuses_a_docker_group_member_without_changing_anything() {
    let sandbox = Sandbox::new("docker-refused");
    system(&sandbox);
    fs::create_dir_all(sandbox.root.join("groups/docker")).unwrap();
    fs::write(sandbox.root.join("groups/docker/tester"), "").unwrap();
    let error = sandbox
        .with(&Answers::new(None), |ctx| Docker.install(ctx))
        .unwrap_err();
    assert!(error.to_string().contains("docker group"));
    assert_eq!(sandbox.calls("pacman -S"), 0);
    assert!(!enabled(&sandbox, "docker.service"));

    let sandbox = Sandbox::new("docker");
    system(&sandbox);
    sandbox
        .with(&Answers::new(None), |ctx| runner::install(ctx, &[&Docker]))
        .unwrap();
    for package in ["docker", "docker-buildx", "docker-compose"] {
        assert!(sandbox.installed(package));
    }
    assert!(enabled(&sandbox, "docker.service"));
    assert_eq!(sandbox.calls("sudo systemctl start docker.service"), 1);
}

#[test]
fn ghostty_becomes_the_kde_terminal_when_plasma_is_present() {
    let sandbox = Sandbox::new("ghostty-kde");
    system(&sandbox);
    sandbox.fake("plasmashell", "exit 0");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Ghostty]))
        .unwrap();
    assert!(sandbox.installed("ghostty"));
    assert_eq!(
        kde(&sandbox, "kdeglobals/General/TerminalService").as_deref(),
        Some("com.mitchellh.ghostty.desktop")
    );
    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&Ghostty], &[&Ghostty], false)
        })
        .unwrap();
    assert_eq!(kde(&sandbox, "kdeglobals/General/TerminalService"), None);
}

#[test]
fn axidev_installs_once_then_upgrades_through_its_lifecycle_installer() {
    let sandbox = Sandbox::new("axidev");
    system(&sandbox);
    let lifecycle = sandbox.root.join("bin/axidev-osk-install");
    let app = sandbox.root.join("bin/axidev-osk");
    // The downloaded installer puts the application and its lifecycle installer in place.
    sandbox.fake(
        "curl",
        &format!(
            r#"printf 'curl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
while [ "$#" -gt 1 ]; do [ "$1" = --output ] && output="$2"; shift; done
cat > "$output" <<'SCRIPT'
#!/bin/sh
printf 'installer %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
printf '#!/bin/sh\nprintf "app %%s\\n" "$*" >> "$MYCONFIG_TEST_LOG"\n' > {app}
printf '#!/bin/sh\nprintf "lifecycle %%s\\n" "$*" >> "$MYCONFIG_TEST_LOG"\n' > {lifecycle}
chmod +x {app} {lifecycle}
SCRIPT"#,
            app = app.display(),
            lifecycle = lifecycle.display()
        ),
    );
    let io = Answers::new(None);
    let run = || {
        sandbox.with(&io, |ctx| {
            ctx.state()
                .borrow_mut()
                .begin(crate::state::Action::Install)
                .unwrap();
            crate::axidev_osk::configure(ctx, &lifecycle, &app).unwrap();
        })
    };
    run();
    assert_eq!(sandbox.calls("installer install --user tester"), 1);
    for step in [
        "setup-permissions --user tester",
        "setup-autostart --user tester",
        "setup-greeter",
    ] {
        assert_eq!(sandbox.calls(&format!("app linux {step}")), 1, "{step}");
    }
    run();
    assert_eq!(sandbox.calls("lifecycle upgrade --user tester"), 1);
    assert_eq!(
        sandbox.calls("curl "),
        1,
        "a rerun must not download the installer again"
    );
    // Without a terminal, the module stops before installing anything.
    let error = sandbox
        .with(&io, |ctx| crate::AxidevOsk.install(ctx))
        .unwrap_err();
    assert!(error.to_string().contains("requires a terminal"));
}

#[test]
fn agent_links_share_one_instruction_file_and_prune_old_skills() {
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
    assert!(fs::symlink_metadata(home.join(".claude/skills/stale")).is_err());
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
fn the_inventory_describes_only_what_the_profile_installs() {
    let cachyos = EnvironmentInventory::CACHYOS.template();
    for line in [
        "- **Terminal tools**: Yazi, ripgrep, jq, and btop.",
        "http://HOSTNAME.local:18080",
        "Axidev OSK with desktop and login-screen startup",
        "Black & Pink panels and application dock for KDE Plasma 6.7 through 6.x",
        "Kanata keyboard remapping with a KDE tray profile selector",
        "Handy offline push-to-talk dictation on Ctrl+Space",
        "ydotool with a persistent user service",
    ] {
        assert!(cachyos.contains(line), "CachyOS inventory lacks: {line}");
    }
    let arch_wsl = EnvironmentInventory::ARCH_WSL.template();
    assert!(arch_wsl.contains("This profile does not install an editor."));
    for absent in ["Axidev OSK", "KDE Plasma", "Kanata", "Handy", "ydotool"] {
        assert!(
            !arch_wsl.contains(absent),
            "Arch WSL inventory mentions {absent}"
        );
    }
}

#[test]
fn paru_comes_from_a_repository_or_else_from_the_prebuilt_aur_package() {
    let sandbox = Sandbox::new("paru-repository");
    fs::remove_file(sandbox.root.join("installed/paru")).unwrap();
    fs::create_dir_all(sandbox.root.join("repository")).unwrap();
    fs::write(sandbox.root.join("repository/paru"), "").unwrap();
    sandbox.with(&Answers::new(None), |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        crate::support::ensure_paru(ctx).unwrap();
    });
    assert!(sandbox.installed("paru"));
    assert_eq!(sandbox.calls("git clone"), 0);

    let sandbox = Sandbox::new("paru-aur");
    fs::remove_file(sandbox.root.join("installed/paru")).unwrap();
    sandbox.fake(
        "git",
        r#"printf 'git %s\n' "$*" >> "$MYCONFIG_TEST_LOG"; mkdir -p "$3""#,
    );
    sandbox.fake(
        "makepkg",
        r#"printf 'makepkg %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
[ "$1" = --packagelist ] && printf '%s/paru-bin.pkg.tar.zst\n' "$PWD"; exit 0"#,
    );
    sandbox.with(&Answers::new(None), |ctx| {
        ctx.state()
            .borrow_mut()
            .begin(crate::state::Action::Install)
            .unwrap();
        crate::support::ensure_paru(ctx).unwrap();
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
            "gtk-cursor-theme-size=40\nother-setting=preserved\n"
        );
    }
    assert_eq!(
        fs::read_to_string(xsettings).unwrap(),
        "Gtk/CursorThemeSize 40\nOther/Setting 1\n"
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
        ("kcminputrc/Mouse/cursorSize", "40"),
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
