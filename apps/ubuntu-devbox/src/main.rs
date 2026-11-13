use std::process::ExitCode;

use myconfig_interface::Profile;
use myconfig_modules::*;

/// A personal Ubuntu machine for coding and long-running agents. Editors reach it over the
/// SSH access the machine already has, so the profile leaves SSH alone.
static MODULES: &[&dyn Module] = &[
    &Base {
        packages: Base::UBUNTU,
        unwanted: &[],
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
    &Tailscale,
    &AgentsPackages { extra: &[] },
    &Docker { setup_tool: true },
    &AgentConfigStowed { ydotool: false },
    &T3Code,
    &GitConfig { windows_ssh: false },
    &EnvironmentInventory::UBUNTU_DEVBOX,
];

fn main() -> ExitCode {
    let has_apt = std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| directory.join("apt-get").is_file())
    });
    if std::env::consts::OS != "linux" || !has_apt {
        eprintln!("error: the Ubuntu Devbox installer requires an apt-based Linux system");
        return ExitCode::FAILURE;
    }
    myconfig_interface::main(Profile {
        title: "Ubuntu Devbox",
        package_system: PackageSystem::Apt,
        modules: MODULES,
        keep_going: false,
    })
}

#[cfg(test)]
mod tests {
    use super::MODULES;

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
                "tailscale",
                "agents-packages",
                "docker",
                "agent-config",
                "t3code",
                "git-config",
                "environment-inventory",
            ]
        );
    }
}
