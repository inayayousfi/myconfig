//! What each module installs, writes, enables and refuses, against fake system programs.
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

use super::{Answers, Sandbox};
use crate::{
    AgentConfigStowed, Base, CachyosSetup, Cli, Context, Docker, EnvironmentInventory, Footprint,
    Ghostty, Handy, Kanata, KanataKde, Module, ModuleResult, Need, Package, PackageSystem,
    Runtimes, ServiceScope, runner,
};

/// Fake system programs that keep their state under the sandbox, so modules run as on a
/// real machine without root: `sudo` moves `/etc`, `/usr`, `/boot` and `/efi` paths under
/// `system/`, groups live in `groups/`, `systemctl` remembers enabled system units in
/// `units/system/` and user units in `units/user/` and counts every unit active until
/// `disable --now` stops it, KDE keys live in `kde/`, and `ufw` prints its rules in ufw's
/// own word order.
fn system(sandbox: &Sandbox) {
    for directory in [
        "system",
        "groups",
        "units/system",
        "units/user",
        "kde",
        "ufw",
    ] {
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
scope=system command= names= now=
for argument in "$@"; do
    case "$argument" in
        --user) scope=user ;;
        --now) now=1 ;;
        --*) ;;
        *) if [ -z "$command" ]; then command="$argument"; else names="$names $argument"; fi ;;
    esac
done
units="$MYCONFIG_TEST_ROOT/units/$scope"
for unit in $names; do
    case "$command" in
        enable) : > "$units/$unit"; [ -n "$now" ] && rm -f "$units/$unit.stopped" ;;
        disable) rm -f "$units/$unit"; [ -n "$now" ] && : > "$units/$unit.stopped" ;;
        start) rm -f "$units/$unit.stopped" ;;
        is-enabled) if [ -e "$units/$unit" ]; then echo enabled; else echo disabled; exit 1; fi ;;
        is-active) case "$unit" in graphical-session.target) exit 1 ;; esac
            if [ -e "$units/$unit.stopped" ]; then echo inactive; exit 3; else echo active; fi ;;
    esac
done
exit 0"#,
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
mkdir -p "$(dirname "$path/$key")" && printf '%s' "$value" > "$path/$key""#,
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

fn enabled(sandbox: &Sandbox, scope: ServiceScope, unit: &str) -> bool {
    let folder = match scope {
        ServiceScope::System => "units/system",
        ServiceScope::User => "units/user",
    };
    sandbox.root.join(folder).join(unit).exists()
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
    let arch: [&dyn Module; 5] = [
        &Base {
            packages: Base::ARCH,
            unwanted: &[Package::CachyUpdate],
            needs: &[],
        },
        &CachyosSetup,
        &Cli,
        &Runtimes {
            bun_installer: false,
        },
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
        &Docker { setup_tool: false },
        &AgentConfigStowed { ydotool: true },
    ];
    sandbox.with(&Answers::new(None), |ctx| {
        for module in arch.iter().chain(more.iter()) {
            ctx.specs(&module.footprint(ctx).packages)
                .unwrap_or_else(|error| panic!("{}: {error}", module.name()));
        }
    });
    let apt = |package| myconfig_utils::resolve_package(package, PackageSystem::Apt);
    let ubuntu: [&dyn Module; 12] = [
        &Base {
            packages: Base::UBUNTU,
            unwanted: &[],
            needs: &[],
        },
        &Cli,
        &crate::SystemMonitor,
        &Runtimes {
            bun_installer: true,
        },
        &crate::Zsh {
            set_login_shell: true,
        },
        &crate::TerminalTools,
        &crate::Tailscale,
        &crate::AgentsPackages { extra: &[] },
        &Docker { setup_tool: true },
        &AgentConfigStowed { ydotool: false },
        &crate::T3Code,
        &crate::GitConfig { windows_ssh: false },
    ];
    sandbox.with(&Answers::new(None), |ctx| {
        for module in ubuntu {
            // Bun comes from its own installer on Ubuntu.
            for package in module.footprint(ctx).packages {
                assert!(
                    package == Package::Bun || apt(package).is_some(),
                    "{}: {package:?} has no APT name",
                    module.name()
                );
            }
        }
    });
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

const MKINITCPIO: &str = "# vim:set ft=sh
MODULES=()
HOOKS=(base systemd autodetect microcode kms modconf block keyboard sd-vconsole plymouth filesystems fsck)
";
const REFIND_LINUX: &str = r#""Boot with standard options"    "quiet nowatchdog splash rw root=UUID=31f4 resume=UUID=31f4 resume_offset=244412416 efi_pstore.pstore_disable=0"
"Boot to single-user mode"    "quiet nowatchdog splash rw root=UUID=31f4 resume=UUID=31f4 resume_offset=244412416" single
"Boot with minimal options"   "ro root=/dev/nvme0n1p2 resume=UUID=31f4 resume_offset=244412416"
"#;

/// A CachyOS boot with the Plymouth splash, and rEFInd when `refind` is set.
fn cachyos_boot(sandbox: &Sandbox, refind: bool) {
    system(sandbox);
    let etc = sandbox.root.join("system/etc");
    fs::create_dir_all(&etc).unwrap();
    fs::write(etc.join("mkinitcpio.conf"), MKINITCPIO).unwrap();
    if refind {
        let boot = sandbox.root.join("system/boot");
        fs::create_dir_all(&boot).unwrap();
        fs::write(boot.join("refind_linux.conf"), REFIND_LINUX).unwrap();
    }
    sandbox.fake(
        "mkinitcpio",
        r#"printf 'mkinitcpio %s\n' "$*" >> "$MYCONFIG_TEST_LOG""#,
    );
}

#[test]
fn cachyos_removes_the_boot_splash_and_rebuilds_the_initramfs_once() {
    let sandbox = Sandbox::new("cachyos-splash");
    cachyos_boot(&sandbox, true);
    let splash = [
        "plymouth",
        "plymouth-kcm",
        "cachyos-plymouth-theme",
        "cachyos-plymouth-bootanimation",
    ];
    install_everything(&sandbox, &splash);
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&CachyosSetup], &[&CachyosSetup])
            })
            .unwrap();
    }
    assert_eq!(
        system_file(&sandbox, "/etc/mkinitcpio.conf").unwrap(),
        MKINITCPIO.replace(" plymouth", "")
    );
    assert_eq!(
        system_file(&sandbox, "/boot/refind_linux.conf").unwrap(),
        REFIND_LINUX.replace("quiet nowatchdog splash ", "")
    );
    for package in splash {
        assert!(!sandbox.installed(package), "{package} is still installed");
    }
    assert_eq!(sandbox.calls("sudo mkinitcpio -P"), 1);
    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let hook = calls.find("mkinitcpio.conf").unwrap();
    let packages = calls.find("pacman -R").unwrap();
    assert!(hook < packages, "the packages left before the hook");
}

#[test]
fn removing_cachyos_setup_brings_the_splash_back_and_rebuilds_the_initramfs_last() {
    let sandbox = Sandbox::new("cachyos-splash-remove");
    cachyos_boot(&sandbox, true);
    let splash = [
        "plymouth",
        "plymouth-kcm",
        "cachyos-plymouth-theme",
        "cachyos-plymouth-bootanimation",
    ];
    install_everything(&sandbox, &splash);
    sandbox
        .with(&Answers::new(None), |ctx| {
            runner::install(ctx, &[&CachyosSetup], &[&CachyosSetup])
        })
        .unwrap();
    fs::remove_file(sandbox.root.join("calls")).unwrap();

    let io = Answers::new(Some(true));
    let report = sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&CachyosSetup], &[&CachyosSetup], false)
        })
        .unwrap();
    assert!(!report.unanswered);
    assert_eq!(
        system_file(&sandbox, "/etc/mkinitcpio.conf").unwrap(),
        MKINITCPIO
    );
    assert_eq!(
        system_file(&sandbox, "/boot/refind_linux.conf").unwrap(),
        REFIND_LINUX
    );
    for package in splash {
        assert!(sandbox.installed(package), "{package} was not reinstalled");
    }
    assert_eq!(sandbox.calls("sudo mkinitcpio -P"), 1);
    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    let rebuild = calls.find("sudo mkinitcpio -P").unwrap();
    assert!(
        calls.find("/etc/mkinitcpio.conf").unwrap() < rebuild,
        "the initramfs was rebuilt before the plymouth hook came back"
    );
    assert!(
        calls.find("pacman -S").unwrap() < rebuild,
        "the initramfs was rebuilt before plymouth came back"
    );
}

#[test]
fn cachyos_skips_the_kernel_options_without_refind() {
    let sandbox = Sandbox::new("cachyos-no-refind");
    cachyos_boot(&sandbox, false);
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&CachyosSetup], &[&CachyosSetup])
        })
        .unwrap();
    assert!(
        io.notes.borrow().iter().any(|note| note
            == "Skipping the kernel options because /boot/refind_linux.conf was not found")
    );
    assert!(system_file(&sandbox, "/boot/refind_linux.conf").is_none());
}

#[test]
fn cachyos_replaces_its_defaults() {
    let sandbox = Sandbox::new("cachyos-packages");
    cachyos_boot(&sandbox, false);
    install_everything(
        &sandbox,
        &["konsole", "firefox", "fish", "kate", "cachy-update", "vim"],
    );
    let modules: [&dyn Module; 4] = [
        &Base {
            packages: Base::ARCH,
            unwanted: &[Package::CachyUpdate],
            needs: &[],
        },
        &CachyosSetup,
        &Cli,
        &Runtimes {
            bun_installer: false,
        },
    ];
    sandbox.fake(
        "rustup",
        r#"[ "$#" -eq 1 ] && echo 'stable-x86_64-unknown-linux-gnu (default)'; exit 0"#,
    );
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &modules, &modules))
        .unwrap();
    for removed in ["konsole", "firefox", "fish", "kate", "cachy-update", "vim"] {
        assert!(!sandbox.installed(removed), "{removed} is still installed");
    }
    for kept in [
        "linux-cachyos",
        "cachyos-kernel-manager",
        "noto-fonts-cjk",
        "ripgrep",
        "jq",
        "tokei",
        "jdk-openjdk",
        "maven",
        "python",
        "bun",
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
        ("plasma", ".local/share/kwin/scripts"),
        ("plasma", ".local/share/plasma/plasmoids"),
        ("plasma", ".local/share/kio/servicemenus"),
        ("plasma", ".config/fontconfig/conf.d"),
        (
            "ghostty",
            ".config/systemd/user/app-com.mitchellh.ghostty.service.d",
        ),
        ("vscodium", ".config/VSCodium/User"),
        ("t3code-desktop", ".t3/userdata"),
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
        r#"printf 'kanata %s\n' "$*" >> "$MYCONFIG_TEST_LOG""#,
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
        system_file(&sandbox, "/etc/modules-load.d/myconfig-kanata.conf").as_deref(),
        Some("uinput\n")
    );
    let rules = system_file(&sandbox, "/etc/udev/rules.d/99-myconfig-kanata.rules").unwrap();
    assert!(rules.contains(
        r#"KERNEL=="uinput", MODE="0660", GROUP="uinput", OPTIONS+="static_node=uinput""#
    ));
    assert!(rules.contains(r#"SUBSYSTEM=="input", KERNEL=="event*", MODE="0660", GROUP="input""#));
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
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
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
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
        .with(&Answers::new(None), |ctx| {
            runner::install(ctx, &modules, &modules)
        })
        .unwrap();
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-kanata.service"
    ));
    assert!(fs::symlink_metadata(wants.join("myconfig-kanata.service")).is_err());
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-kanata-tray.service"
    ));
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
    assert!(system_file(&sandbox, "/etc/udev/rules.d/99-myconfig-handy.rules").is_some());
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-handy.service"
    ));
    assert_eq!(sandbox.calls("stop myconfig-handy.service"), 1);

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
        r#"printf 'docker %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
contexts="$MYCONFIG_TEST_ROOT/docker"
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
    assert_eq!(sandbox.calls("pacman -S"), 0);
    assert_eq!(sandbox.calls("systemctl"), 0);
    assert!(!enabled(&sandbox, ServiceScope::User, "docker.service"));
}

/// Makes the fake `docker` package enable the rootful daemon when it installs, as
/// Ubuntu's docker-ce package does.
fn docker_package_enables_the_rootful_daemon(sandbox: &Sandbox) {
    fs::rename(
        sandbox.root.join("bin/pacman"),
        sandbox.root.join("bin/pacman-base"),
    )
    .unwrap();
    sandbox.fake(
        "pacman",
        r#"if [ "$1" = -S ]; then for name in "$@"; do [ "$name" = docker ] && : > "$MYCONFIG_TEST_ROOT/units/system/docker.service" && : > "$MYCONFIG_TEST_ROOT/units/system/docker.socket"; done; fi
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
    let stopped = |unit: &str| sandbox.root.join(format!("units/system/{unit}.stopped"));
    for unit in ["docker.service", "docker.socket"] {
        assert!(
            !enabled(&sandbox, ServiceScope::System, unit),
            "the rootful {unit} is still enabled"
        );
        assert!(stopped(unit).exists(), "the rootful {unit} still runs");
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
    for unit in ["docker.service", "docker.socket"] {
        assert!(
            !enabled(&sandbox, ServiceScope::System, unit),
            "remove turned on the rootful {unit}, which only the package install had turned on"
        );
    }
}

#[test]
fn removing_docker_turns_back_on_a_rootful_daemon_that_was_on_before() {
    let sandbox = Sandbox::new("docker-rootful-before");
    system(&sandbox);
    docker(&sandbox);
    install_everything(&sandbox, &["docker"]);
    for unit in ["docker.service", "docker.socket"] {
        fs::write(sandbox.root.join("units/system").join(unit), "").unwrap();
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
    let stopped = |unit: &str| sandbox.root.join(format!("units/system/{unit}.stopped"));
    for unit in ["docker.service", "docker.socket"] {
        assert!(
            enabled(&sandbox, ServiceScope::System, unit),
            "the rootful {unit} was not enabled again"
        );
        assert!(
            !stopped(unit).exists(),
            "the rootful {unit} was not started again"
        );
    }
}

#[test]
fn docker_creates_the_user_service_with_the_setup_tool_and_the_system_path() {
    let sandbox = Sandbox::new("docker-setup-tool");
    system(&sandbox);
    docker(&sandbox);
    sandbox.fake(
        "dockerd-rootless-setuptool.sh",
        r#"printf 'setuptool %s PATH=%s\n' "$*" "$PATH" >> "$MYCONFIG_TEST_LOG"
mkdir -p "$HOME/.config/systemd/user" && : > "$HOME/.config/systemd/user/docker.service""#,
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
}

#[test]
fn ghostty_becomes_the_kde_terminal_when_plasma_is_present() {
    let sandbox = Sandbox::new("ghostty-kde");
    system(&sandbox);
    sandbox.fake("plasmashell", "exit 0");
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| runner::install(ctx, &[&Ghostty], &[&Ghostty]))
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
fn the_inventory_describes_only_what_the_profile_installs() {
    let cachyos = EnvironmentInventory::CACHYOS.template();
    for line in [
        "- **Terminal tools**: Yazi, ripgrep, and jq.",
        "- **Coding agents**: Claude Code, installed by Anthropic's installer",
        "the desktop app (AUR package `t3code-bin`)",
        "VSCodium with the Black & Pink theme and Open Remote SSH",
        "Axidev OSK with desktop and login-screen startup",
        "Breeze Dark with a pink accent color",
        "Kanata keyboard remapping with a KDE tray profile selector",
        "Handy offline push-to-talk dictation on Ctrl+Space",
        "ydotool with a persistent user service",
    ] {
        assert!(cachyos.contains(line), "CachyOS inventory lacks: {line}");
    }
    let arch_wsl = EnvironmentInventory::ARCH_WSL.template();
    for line in [
        "This profile does not install an editor.",
        "- **Terminal tools**: Yazi, ripgrep, jq, and btop.",
        "user service `t3code.service`",
    ] {
        assert!(arch_wsl.contains(line), "Arch WSL inventory lacks: {line}");
    }
    let devbox = EnvironmentInventory::UBUNTU_DEVBOX.template();
    for line in [
        "remote SSH extensions",
        "the `rootless` Docker context is selected",
        "user service `t3code.service`",
    ] {
        assert!(
            devbox.contains(line),
            "Ubuntu Devbox inventory lacks: {line}"
        );
    }
    for (name, inventory) in [("Arch WSL", &arch_wsl), ("Ubuntu Devbox", &devbox)] {
        for absent in ["Axidev OSK", "KDE Plasma", "Kanata", "Handy", "ydotool"] {
            assert!(
                !inventory.contains(absent),
                "{name} inventory mentions {absent}"
            );
        }
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
        ("kwinrc/Plugins/myconfig-panelsEnabled", "true"),
        ("plasmanotifyrc/DoNotDisturb/WhenFullscreen", "false"),
        ("dolphinrc/General/BrowseThroughArchives", "true"),
        (
            "kglobalshortcutsrc/services/org.kde.krunner.desktop/_launch",
            "Search\tAlt+F2\tMeta+Space",
        ),
        ("baloofilerc/Basic Settings/Indexing-Enabled", "false"),
        ("plasmaparc/General/RaiseMaximumVolume", "true"),
        ("powerdevilrc/AC/SuspendAndShutdown/LidAction", "1"),
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
        r#"printf 'plasma-apply-colorscheme %s\n' "$*" >> "$MYCONFIG_TEST_LOG""#,
    );
    sandbox.fake(
        "plasma-apply-lookandfeel",
        r#"printf 'plasma-apply-lookandfeel %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
mkdir -p "$MYCONFIG_TEST_ROOT/kde/kdeglobals/KDE"
printf '%s' "$2" > "$MYCONFIG_TEST_ROOT/kde/kdeglobals/KDE/LookAndFeelPackage""#,
    );
    sandbox.fake(
        "plasma-apply-cursortheme",
        r#"printf 'plasma-apply-cursortheme %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
for theme; do :; done
mkdir -p "$MYCONFIG_TEST_ROOT/kde/kcminputrc/Mouse"
printf '%s' "$theme" > "$MYCONFIG_TEST_ROOT/kde/kcminputrc/Mouse/cursorTheme""#,
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
    sandbox.fake("kbuildsycoca6", "exit 0");
    let applications = sandbox.root.join("share/applications");
    fs::create_dir_all(&applications).unwrap();
    fs::write(
        applications.join("assistant.desktop"),
        "[Desktop Entry]\nName=Qt Assistant\n",
    )
    .unwrap();
    sandbox
        .shell
        .set_var("XDG_DATA_DIRS", sandbox.root.join("share"));
}

#[test]
fn kde_plasma_applies_breeze_dark_with_the_pink_accent_and_remove_puts_it_back() {
    let sandbox = Sandbox::new("kde-plasma");
    plasma(&sandbox);
    let rules = sandbox.root.join("kde/kwinrulesrc/General");
    fs::create_dir_all(&rules).unwrap();
    fs::write(rules.join("rules"), "user-rule").unwrap();
    fs::write(rules.join("count"), "1").unwrap();
    let io = Answers::new(None);
    for _ in 0..2 {
        sandbox
            .with(&io, |ctx| {
                runner::install(ctx, &[&crate::KdePlasma], &[&crate::KdePlasma])
            })
            .unwrap();
    }
    for package in ["otf-ibm-plex", "ttf-ibmplex-mono-nerd", "libinput"] {
        assert!(sandbox.installed(package), "{package} was not installed");
    }
    for plugin in [
        "90-myconfig-pointer-sensitivity.lua",
        "90-myconfig-super-ctrl-right-click.lua",
    ] {
        assert!(
            system_file(&sandbox, &format!("/etc/libinput/plugins/{plugin}")).is_some(),
            "{plugin} was not installed"
        );
    }
    assert!(sandbox.calls("plasma-apply-lookandfeel --apply org.kde.breezedark.desktop") > 0);
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
        Some("user-rule,myconfig-picture-in-picture")
    );
    assert_eq!(
        kde(&sandbox, "kwinrulesrc/General/count").as_deref(),
        Some("2")
    );
    assert_eq!(
        kde(
            &sandbox,
            "kwinrulesrc/myconfig-picture-in-picture/excludefromcapture"
        )
        .as_deref(),
        Some("true")
    );
    assert!(enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-plasma-layout.service"
    ));
    let hidden = sandbox
        .home
        .join(".local/share/applications/assistant.desktop");
    assert!(
        fs::read_to_string(&hidden)
            .unwrap()
            .contains("NoDisplay=true")
    );

    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::KdePlasma], &[&crate::KdePlasma], false)
        })
        .unwrap();
    assert_eq!(
        kde(&sandbox, "kwinrulesrc/General/rules").as_deref(),
        Some("user-rule")
    );
    assert_eq!(kde(&sandbox, "kdeglobals/General/AccentColor"), None);
    assert!(!enabled(
        &sandbox,
        ServiceScope::User,
        "myconfig-plasma-layout.service"
    ));
    assert!(!hidden.exists());
    assert!(
        system_file(
            &sandbox,
            "/etc/libinput/plugins/90-myconfig-pointer-sensitivity.lua"
        )
        .is_none()
    );
}

/// A systemctl stand-in whose sshd.service is enabled while `units/system/sshd.service`
/// exists, is active only while `units/sshd-active` exists, and fails to start while
/// `units/sshd-broken` exists.
fn sshd(sandbox: &Sandbox, active: bool, broken: bool) {
    system(sandbox);
    let units = sandbox.root.join("units");
    if active {
        fs::write(units.join("sshd-active"), "").unwrap();
    }
    if broken {
        fs::write(units.join("sshd-broken"), "").unwrap();
    }
    sandbox.fake(
        "systemctl",
        r#"printf 'systemctl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
units="$MYCONFIG_TEST_ROOT/units"
case "$*" in
    "is-active --quiet sshd.service") [ -e "$units/sshd-active" ] ;;
    "is-enabled sshd.service") if [ -e "$units/system/sshd.service" ]; then echo enabled; else echo disabled; exit 1; fi ;;
    "enable sshd.service") : > "$units/system/sshd.service" ;;
    "disable sshd.service") rm -f "$units/system/sshd.service" ;;
    "start sshd.service" | "restart sshd.service") [ ! -e "$units/sshd-broken" ] ;;
esac"#,
    );
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
        r#"printf 'flatpak %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
remotes="$MYCONFIG_TEST_ROOT/flatpak"
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
        r#"printf 'installer SHELL=%s\n' "$SHELL" >> "$MYCONFIG_TEST_LOG"
mkdir -p "$BUN_INSTALL/bin" && : > "$BUN_INSTALL/bin/bun" && chmod +x "$BUN_INSTALL/bin/bun"
"#,
    )
    .unwrap();
    sandbox.fake(
        "curl",
        r#"printf 'curl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
cat "$MYCONFIG_TEST_ROOT/bun-installer""#,
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
}

#[test]
fn vscodium_installs_its_theme_and_extensions_and_remove_uninstalls_only_those() {
    let sandbox = Sandbox::new("vscodium");
    system(&sandbox);
    // Keeps the installed extensions in a file, and names the packaged theme as VSCodium does.
    sandbox.fake(
        "codium",
        r#"printf 'codium %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
installed="$MYCONFIG_TEST_ROOT/extensions"
touch "$installed"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --list-extensions) cat "$installed" ;;
        --install-extension)
            case "$2" in *.vsix) id=myconfig.blacknpink-theme ;; *) id="$2" ;; esac
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
        r#"printf 'bsdtar %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
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
            Some("vscodium.desktop")
        );
    }
    assert_eq!(sandbox.calls("bsdtar --format zip -cf "), 1);
    assert_eq!(sandbox.calls("codium --force --install-extension "), 1);
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

/// A boot that bootvine can join, with pacman reporting bootvine at the version it
/// last installed.
fn bootvine_boot(sandbox: &Sandbox) {
    cachyos_boot(sandbox, true);
    let config = sandbox.root.join("system/etc/mkinitcpio.conf");
    fs::write(&config, MKINITCPIO.replace(" plymouth", "")).unwrap();
    fs::write(
        sandbox.root.join("system/boot/refind_linux.conf"),
        REFIND_LINUX.replace("quiet nowatchdog splash ", ""),
    )
    .unwrap();
    fs::rename(
        sandbox.root.join("bin/pacman"),
        sandbox.root.join("bin/pacman-base"),
    )
    .unwrap();
    sandbox.fake(
        "pacman",
        r#"version="$MYCONFIG_TEST_ROOT/bootvine-version"
case "$1 $2" in
    "-Q bootvine") [ -e "$version" ] && printf 'bootvine %s\n' "$(cat "$version")" ;;
    -U*) printf '%s\n' "$BOOTVINE_BUILT" > "$version"; exec pacman-base "$@" ;;
    *) exec pacman-base "$@" ;;
esac"#,
    );
    sandbox.fake(
        "makepkg",
        r#"printf 'makepkg %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
[ "$1" = --packagelist ] && printf 'bootvine-0.1.3-1-x86_64.pkg.tar.zst\n'; exit 0"#,
    );
    sandbox.shell.set_var("BOOTVINE_BUILT", "0.1.3-1");
}

#[test]
fn bootvine_builds_once_and_joins_the_initramfs_and_the_first_boot_entry() {
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
    assert_eq!(sandbox.calls("makepkg --noconfirm"), 1);
    assert_eq!(sandbox.calls("local "), 1, "pacman -U ran more than once");
    assert_eq!(
        sandbox.calls("sudo mkinitcpio -P"),
        0,
        "pacman rebuilds the initramfs when the package installs"
    );
    let calls = fs::read_to_string(sandbox.root.join("calls")).unwrap();
    assert!(
        calls.find("mkinitcpio.conf").unwrap() < calls.find("local ").unwrap(),
        "the package installed before HOOKS named its hook"
    );
    assert!(
        system_file(&sandbox, "/etc/mkinitcpio.conf")
            .unwrap()
            .contains("HOOKS=(base systemd sd-bootvine autodetect")
    );
    assert_eq!(
        system_file(&sandbox, "/boot/refind_linux.conf").unwrap(),
        REFIND_LINUX
            .replace("quiet nowatchdog splash ", "")
            .replacen("\"rw root", "\"quiet rw root", 1)
    );
    for service in ["bootvine.service", "bootvine-shutdown.service"] {
        assert!(
            enabled(&sandbox, ServiceScope::System, service),
            "{service} is not enabled"
        );
    }
}

#[test]
fn bootvine_rebuilds_the_initramfs_when_only_the_hook_was_missing() {
    let sandbox = Sandbox::new("bootvine-hook");
    bootvine_boot(&sandbox);
    fs::write(sandbox.root.join("bootvine-version"), "0.1.3-1\n").unwrap();
    let io = Answers::new(None);
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::Bootvine], &[&crate::Bootvine])
        })
        .unwrap();
    assert_eq!(sandbox.calls("makepkg"), 0);
    assert_eq!(sandbox.calls("sudo mkinitcpio -P"), 1);
}

#[test]
fn removing_bootvine_takes_its_hook_out_of_the_initramfs_before_the_package() {
    let sandbox = Sandbox::new("bootvine-remove");
    bootvine_boot(&sandbox);
    // The CachyOS run changes the same file first, in the same run.
    fs::write(sandbox.root.join("system/etc/mkinitcpio.conf"), MKINITCPIO).unwrap();
    let modules: [&dyn Module; 2] = [&CachyosSetup, &crate::Bootvine];
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
        MKINITCPIO.replace(" plymouth", ""),
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
        rebuild < calls.find("pacman -R --noconfirm bootvine").unwrap(),
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
    assert_eq!(sandbox.calls("makepkg"), 0);
}

#[test]
fn tailscale_makes_the_user_its_operator_once_and_remove_clears_it() {
    let sandbox = Sandbox::new("tailscale");
    system(&sandbox);
    sandbox.fake(
        "tailscale",
        r#"printf 'tailscale %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
operator="$MYCONFIG_TEST_ROOT/tailscale-operator"
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
fn agents_install_claude_code_once_and_the_bun_tools_and_remove_takes_them_out() {
    let sandbox = Sandbox::new("agents");
    fs::write(
        sandbox.root.join("claude-installer"),
        r#"printf 'claude installer\n' >> "$MYCONFIG_TEST_LOG"
mkdir -p "$HOME/.local/bin" && : > "$HOME/.local/bin/claude" && chmod +x "$HOME/.local/bin/claude"
"#,
    )
    .unwrap();
    sandbox.fake(
        "curl",
        r#"printf 'curl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
cat "$MYCONFIG_TEST_ROOT/claude-installer""#,
    );
    sandbox.fake(
        "bun",
        r#"printf 'bun %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
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
fn t3code_repairs_the_server_through_the_deployed_t3ctl_and_remove_uninstalls_its_service() {
    let sandbox = Sandbox::new("t3code");
    system(&sandbox);
    sandbox.fake("npm", "exit 0");
    let io = Answers::new(None);
    // The deployed t3ctl runs as itself, so its own commands are faked through PATH.
    sandbox.fake(
        "loginctl",
        r#"printf 'loginctl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"; [ "$1" = show-user ] && echo yes"#,
    );
    sandbox.fake("curl", "exit 0");
    let runtime = sandbox.home.join(".t3/userdata/server-runtime.json");
    fs::create_dir_all(runtime.parent().unwrap()).unwrap();
    fs::write(&runtime, "{}").unwrap();
    let t3 = sandbox.home.join(".local/bin/t3");
    fs::create_dir_all(t3.parent().unwrap()).unwrap();
    fs::write(
        &t3,
        "#!/bin/sh\nprintf 't3 %s\\n' \"$*\" >> \"$MYCONFIG_TEST_LOG\"\n",
    )
    .unwrap();
    fs::set_permissions(&t3, fs::Permissions::from_mode(0o755)).unwrap();
    sandbox
        .with(&io, |ctx| {
            runner::install(ctx, &[&crate::T3Code], &[&crate::T3Code])
        })
        .unwrap();
    assert!(sandbox.installed("qrencode"));
    assert_eq!(sandbox.calls("t3 service install"), 1);
    let t3ctl = sandbox.home.join(".local/bin/t3ctl");
    assert_eq!(
        fs::canonicalize(&t3ctl).unwrap(),
        fs::canonicalize(sandbox.home.join("dotfiles/t3code/.local/bin/t3ctl")).unwrap()
    );

    sandbox
        .with(&io, |ctx| {
            runner::remove(ctx, &[&crate::T3Code], &[&crate::T3Code], false)
        })
        .unwrap();
    assert_eq!(sandbox.calls("t3 service uninstall"), 1);
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
        r#"printf 'curl %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
case "$*" in
    *docker*) printf -- '-----BEGIN PGP PUBLIC KEY BLOCK-----\nkey\n' ;;
    *) printf 'binary key %s' "$2" ;;
esac"#,
    );
    sandbox.fake("dpkg", "echo amd64");
    sandbox.fake(
        "apt-get",
        r#"printf 'apt-get %s\n' "$*" >> "$MYCONFIG_TEST_LOG""#,
    );
    let packages: Vec<_> = [
        Package::Tailscale,
        Package::Docker,
        Package::DockerCompose,
        Package::Ripgrep,
    ]
    .into_iter()
    .map(|package| myconfig_utils::resolve_package(package, PackageSystem::Apt).unwrap())
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
        system_file(&sandbox, "/etc/apt/sources.list.d/myconfig-docker.sources").unwrap(),
        "Types: deb\nURIs: https://download.docker.com/linux/ubuntu\nSuites: noble\nComponents: stable\nArchitectures: amd64\nSigned-By: /etc/apt/keyrings/myconfig-docker.asc\n"
    );
    assert!(system_file(&sandbox, "/etc/apt/keyrings/myconfig-docker.asc").is_some());
    assert_eq!(
        system_file(&sandbox, "/etc/apt/keyrings/myconfig-tailscale.gpg").unwrap(),
        "binary key https://pkgs.tailscale.com/stable/ubuntu/noble.noarmor.gpg"
    );
    assert!(system_file(&sandbox, "/etc/apt/sources.list.d/myconfig-github.sources").is_none());
    assert!(system_file(&sandbox, "/etc/apt/sources.list.d/myconfig-yazi.sources").is_none());
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
        r#"printf 'pacman %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
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
            r#"allowed="$MYCONFIG_TEST_ROOT/allowed"
case "$3" in
    '$PROFILE') printf '%s\n' "$HOME/Documents/PowerShell/Microsoft.PowerShell_profile.ps1" ;;
    *Get-MpPreference*) if [ -e "$allowed" ]; then echo allowed; else echo blocked; fi ;;
    *Add-MpPreference*-Verb\ RunAs*) printf 'defender %s\n' "$3" >> "$MYCONFIG_TEST_LOG"; {elevate}; : > "$allowed" ;;
    *Remove-MpPreference*-Verb\ RunAs*) printf 'defender %s\n' "$3" >> "$MYCONFIG_TEST_LOG"; {elevate}; rm -f "$allowed" ;;
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
fn the_powershell_profile_allows_pwsh_through_controlled_folder_access_until_removed() {
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
    let pwsh = sandbox.root.join("bin/pwsh.exe");
    assert_eq!(
        sandbox.calls(&format!(
            "Add-MpPreference -ControlledFolderAccessAllowedApplications ''{}''",
            pwsh.display()
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
            "Remove-MpPreference -ControlledFolderAccessAllowedApplications ''{}''",
            pwsh.display()
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
        r#"printf 'gh %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
case "$*" in
    "auth status") [ -e "$MYCONFIG_TEST_ROOT/gh-login" ] ;;
    "auth login") : > "$MYCONFIG_TEST_ROOT/gh-login" ;;
esac"#,
    );
    sandbox.fake(
        "tailscale",
        r#"printf 'tailscale %s\n' "$*" >> "$MYCONFIG_TEST_LOG"
case "$*" in
    status) [ -e "$MYCONFIG_TEST_ROOT/tailscale-login" ] ;;
    up) : > "$MYCONFIG_TEST_ROOT/tailscale-login" ;;
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
