use std::process::ExitCode;

use inayos_interface::Profile;
use inayos_modules::*;

/// A personal Ubuntu machine for coding and long-running agents. Editors reach it over the
/// SSH access the machine already has, so the profile leaves SSH alone.
static MODULES: &[&dyn Module] = &[
    &Base {
        packages: Base::UBUNTU,
        unwanted: &[],
        needs: &[],
    },
    &Cli,
    &SystemMonitor,
    &Runtimes {
        bun_installer: true,
    },
    &Zsh {
        set_login_shell: true,
    },
    &TerminalTools,
    &Neovim {
        source: NeovimSource::Release,
    },
    &Tailscale,
    &AgentsPackages { extra: &[] },
    &Docker { setup_tool: true },
    &AgentConfigStowed { ydotool: false },
    &T3Code,
    &GitConfig { windows_ssh: false },
    &Login,
    &EnvironmentInventory::UBUNTU_DEVBOX,
];

const PACKAGE_SYSTEM: PackageSystem = PackageSystem::Apt;

fn main() -> ExitCode {
    let has_apt = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join("apt-get").is_file())
    });
    if std::env::consts::OS != "linux" || !has_apt {
        eprintln!("error: the Ubuntu Devbox installer requires an apt-based Linux system");
        return ExitCode::FAILURE;
    }
    inayos_interface::main(Profile {
        title: "Ubuntu Devbox",
        package_system: PACKAGE_SYSTEM,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use inayos_modules::{EnvironmentInventory, Package, runner::need_problems, unnamed_packages};

    use super::{MODULES, PACKAGE_SYSTEM};

    #[test]
    fn every_need_comes_earlier_and_every_hard_need_is_in_the_profile() {
        assert_eq!(need_problems(MODULES), Vec::<String>::new());
    }

    #[test]
    fn the_environment_inventory_describes_the_modules_of_the_profile() {
        assert_eq!(
            EnvironmentInventory::UBUNTU_DEVBOX.claim_problems(MODULES),
            Vec::<String>::new()
        );
    }

    #[test]
    fn every_package_has_a_name_on_the_package_system() {
        // Bun comes from its own installer here. agents-packages lists it so that remove
        // keeps it, and installs it from no package.
        assert_eq!(
            unnamed_packages(MODULES, PACKAGE_SYSTEM),
            [("agents-packages", Package::Bun)]
        );
    }

    #[test]
    fn the_profile_is_the_development_machine_without_ssh_or_a_desktop() {
        let names: Vec<_> = MODULES.iter().map(|module| module.name()).collect();
        assert_eq!(
            names,
            [
                "base",
                "cli",
                "system-monitor",
                "runtimes",
                "zsh",
                "terminal-tools",
                "neovim",
                "tailscale",
                "agents-packages",
                "docker",
                "agent-config",
                "t3code",
                "git-config",
                "login",
                "environment-inventory",
            ]
        );
    }
}
