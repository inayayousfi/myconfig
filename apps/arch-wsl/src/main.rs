use std::process::ExitCode;

use inayos_interface::Profile;
use inayos_modules::*;

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
    &Ssh { keys_only: true },
    &Cli,
    &SystemMonitor,
    &Runtimes {
        bun_installer: false,
    },
    &Zsh {
        set_login_shell: false,
    },
    &TerminalTools,
    &Neovim {
        source: NeovimSource::Package,
    },
    &HerdrCore {
        source: HerdrCoreSource::Package,
    },
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

const PACKAGE_SYSTEM: PackageSystem = PackageSystem::Arch;

fn main() -> ExitCode {
    if std::env::consts::OS != "linux" || !running_in_wsl() {
        eprintln!("error: the Arch WSL installer requires Linux inside WSL");
        return ExitCode::FAILURE;
    }
    inayos_interface::main(Profile {
        title: "Arch WSL",
        package_system: PACKAGE_SYSTEM,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use inayos_modules::{EnvironmentInventory, runner::need_problems, unnamed_packages};

    use super::{MODULES, PACKAGE_SYSTEM};

    #[test]
    fn every_need_comes_earlier_and_every_hard_need_is_in_the_profile() {
        assert_eq!(need_problems(MODULES), Vec::<String>::new());
    }

    #[test]
    fn the_environment_inventory_describes_the_modules_of_the_profile() {
        assert_eq!(
            EnvironmentInventory::ARCH_WSL.claim_problems(MODULES),
            Vec::<String>::new()
        );
    }

    #[test]
    fn every_package_has_a_name_on_the_package_system() {
        assert_eq!(unnamed_packages(MODULES, PACKAGE_SYSTEM), []);
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
            "herdr",
        ] {
            assert!(!names.contains(&absent), "Arch WSL includes {absent}");
        }
        for wanted in [
            "zsh",
            "neovim",
            "herdr-core",
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
