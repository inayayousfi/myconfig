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
    },
    &Ssh,
    &Cli {
        retired_config: &["hunk", "lazygit", "nvim", "tmux"],
    },
    &Runtimes,
    &Zsh {
        set_login_shell: false,
    },
    &TerminalTools,
    &Tailscale,
    &AgentsPackages {
        extra: &[Package::WslSshAgent],
    },
    &AgentConfigStowed { ydotool: false },
    &GitConfig { windows_ssh: true },
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
    use super::MODULES;

    #[test]
    fn the_profile_has_no_desktop_modules() {
        let names: Vec<_> = MODULES.iter().map(|module| module.name()).collect();
        for absent in [
            "axidev-osk",
            "ghostty",
            "emacs",
            "kde-plasma",
            "cursor-theme",
            "refind",
            "kanata",
            "kanata-kde",
            "handy",
            "pipewire",
            "docker",
        ] {
            assert!(!names.contains(&absent), "Arch WSL includes {absent}");
        }
        for wanted in ["zsh", "agent-config", "git-config", "tailscale"] {
            assert!(names.contains(&wanted), "Arch WSL lacks {wanted}");
        }
    }
}
