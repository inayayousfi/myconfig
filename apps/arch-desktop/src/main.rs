use std::process::ExitCode;

use inayos_interface::Profile;
use inayos_modules::*;

static MODULES: &[&dyn Module] = &[
    &PlasmaVersion,
    &Base {
        packages: Base::ARCH,
        unwanted: &[],
        needs: &[Need::Hard("plasma-version")],
    },
    // Before the modules that install the inayos package from it.
    &InayosRepository,
    &SystemTuning,
    &WakeSources,
    &Snapshots,
    // Before refind and bootvine, which find the unified kernel image it builds, and before
    // secure-boot, which signs it. The InayOS installer skips it without encryption.
    &Encryption,
    &Hibernation,
    // Before the modules that set KDE defaults, because it installs Plasma and its
    // kreadconfig6 and kwriteconfig6.
    &KdePlasma,
    &Media,
    &Ssh { keys_only: false },
    &Cli,
    &Runtimes {
        bun_installer: false,
    },
    &Flatpak,
    &AppImage,
    &Zsh {
        set_login_shell: true,
    },
    &TerminalTools,
    &Ghostty,
    &Tailscale,
    &AgentsPackages {
        extra: &[Package::Ydotool],
    },
    &AndroidPhone,
    &Vscodium,
    &Chrome,
    &Math,
    &Refind,
    &SecureBoot,
    &Bootvine,
    &BootGuard,
    &BootRescue,
    &Maintenance,
    &Kanata {
        start_at_login: false,
    },
    &AxidevOsk,
    &Printing,
    &KanataKde,
    &Handy,
    &Pipewire,
    &Docker { setup_tool: false },
    &AgentConfigStowed { ydotool: true },
    &T3CodeDesktop,
    &SshServer,
    &GitConfig { windows_ssh: false },
    &Login,
    &EnvironmentInventory::ARCH_DESKTOP,
];

const PACKAGE_SYSTEM: PackageSystem = PackageSystem::Arch;

fn running_in_wsl() -> bool {
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .to_ascii_lowercase();
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || kernel.contains("microsoft")
        || kernel.contains("wsl")
}

fn main() -> ExitCode {
    let id = std::fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.strip_prefix("ID=")
                .map(|id| id.trim_matches('"').to_owned())
        });
    if id.as_deref() != Some("arch") || running_in_wsl() {
        eprintln!(
            "error: the InayOS desktop installer requires Arch Linux outside WSL; detected {}",
            id.as_deref().unwrap_or("unknown")
        );
        return ExitCode::FAILURE;
    }
    inayos_interface::main(Profile {
        title: "InayOS desktop",
        package_system: PACKAGE_SYSTEM,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use inayos_modules::{EnvironmentInventory, Need, runner::need_problems, unnamed_packages};

    use super::{MODULES, PACKAGE_SYSTEM};

    #[test]
    fn every_need_comes_earlier_and_every_hard_need_is_in_the_profile() {
        assert_eq!(need_problems(MODULES), Vec::<String>::new());
    }

    #[test]
    fn the_environment_inventory_describes_the_modules_of_the_profile() {
        assert_eq!(
            EnvironmentInventory::ARCH_DESKTOP.claim_problems(MODULES),
            Vec::<String>::new()
        );
    }

    #[test]
    fn every_package_has_a_name_on_the_package_system() {
        assert_eq!(unnamed_packages(MODULES, PACKAGE_SYSTEM), []);
    }

    /// Whether `name` needs the Plasma version check, directly or through other needs.
    fn waits_for_plasma(name: &str) -> bool {
        if name == "plasma-version" {
            return true;
        }
        let module = MODULES
            .iter()
            .find(|module| module.name() == name)
            .unwrap_or_else(|| panic!("the profile lacks {name}"));
        module.needs().iter().any(|need| match need {
            Need::Hard(needed) => waits_for_plasma(needed),
            // The check comes first, so it is one of the earlier modules.
            Need::EveryEarlier => true,
            Need::Soft(_) => false,
        })
    }

    #[test]
    fn no_module_runs_when_the_plasma_version_check_fails() {
        for module in MODULES {
            assert!(
                waits_for_plasma(module.name()),
                "{} runs even when the Plasma version is unsupported",
                module.name()
            );
        }
    }

    #[test]
    fn the_profile_is_the_full_desktop() {
        let names: Vec<_> = MODULES.iter().map(|module| module.name()).collect();
        for wanted in [
            "inayos-repository",
            "system-tuning",
            "wake-sources",
            "axidev-osk",
            "ghostty",
            "kde-plasma",
            "printing",
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
            "chrome",
            "math",
            "bootvine",
            "snapshots",
            "encryption",
            "hibernation",
            "secure-boot",
            "boot-guard",
            "maintenance",
            "t3code-desktop",
            "agent-config",
            "git-config",
            "login",
        ] {
            assert!(names.contains(&wanted), "the InayOS desktop lacks {wanted}");
        }
        assert_eq!(
            names[0], "plasma-version",
            "Plasma is checked before anything changes"
        );
    }
}
