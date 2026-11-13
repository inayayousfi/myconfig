use std::process::ExitCode;

use myconfig_interface::Profile;
use myconfig_modules::*;

fn running_in_wsl() -> bool {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .to_ascii_lowercase();
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || kernel.contains("microsoft")
        || kernel.contains("wsl")
}

static MODULES: &[&dyn Module] = &[
    &Base {
        packages: Base::ARCH,
        unwanted: &[],
        needs: &[],
    },
    &Ssh,
    &Cli,
    &SystemMonitor,
    &Runtimes {
        bun_installer: false,
    },
    &Zsh {
        set_login_shell: false,
    },
    &TerminalTools,
    &Tailscale,
    &AgentsPackages {
        extra: &[Package::WslSshAgent],
    },
    &AgentConfigStowed { ydotool: false },
    &T3Code,
    &SshServer,
    &GitConfig { windows_ssh: true },
    &Login,
    &EnvironmentInventory::ARCH_WSL,
];

fn main() -> ExitCode {
    if std::env::consts::OS != "linux" || !running_in_wsl() {
        eprintln!("error: the Arch WSL installer requires Linux inside WSL");
        return ExitCode::FAILURE;
    }
    myconfig_interface::main(Profile {
        title: "Arch WSL",
        package_system: PackageSystem::Arch,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use myconfig_modules::runner::need_problems;

    use super::MODULES;

    #[test]
    fn every_need_comes_earlier_and_every_hard_need_is_in_the_profile() {
        assert_eq!(need_problems(MODULES), Vec::<String>::new());
    }

    #[test]
    fn the_profile_has_no_desktop_modules() {
        let names: Vec<_> = MODULES.iter().map(|module| module.name()).collect();
        for absent in [
            "axidev-osk",
            "ghostty",
            "kde-plasma",
            "refind",
            "kanata",
            "kanata-kde",
            "handy",
            "pipewire",
            "docker",
            "media",
            "flatpak",
            "appimage",
            "vscodium",
            "bootvine",
            "t3code-desktop",
        ] {
            assert!(!names.contains(&absent), "Arch WSL includes {absent}");
        }
        for wanted in [
            "zsh",
            "system-monitor",
            "agent-config",
            "git-config",
            "tailscale",
            "t3code",
            "login",
        ] {
            assert!(names.contains(&wanted), "Arch WSL lacks {wanted}");
        }
    }
}
