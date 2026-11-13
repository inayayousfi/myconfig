use std::process::ExitCode;

use myconfig_interface::Profile;
use myconfig_modules::*;

static MODULES: &[&dyn Module] = &[
    &PlasmaVersion,
    &Base {
        packages: Base::ARCH,
        unwanted: &[Package::CachyUpdate],
    },
    &CachyosSetup,
    &Media,
    &Ssh,
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
    &AxidevOsk,
    &Tailscale,
    &AgentsPackages {
        extra: &[Package::Ydotool],
    },
    &AndroidPhone,
    &Refind,
    &Kanata {
        start_at_login: false,
    },
    &KdePlasma,
    &KanataKde,
    &Handy,
    &Pipewire,
    &Docker,
    &AgentConfigStowed { ydotool: true },
    &SshServer,
    &GitConfig { windows_ssh: false },
    &EnvironmentInventory::CACHYOS,
];

fn main() -> ExitCode {
    let id = std::fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find_map(|line| {
            line.strip_prefix("ID=")
                .map(|id| id.trim_matches('"').to_owned())
        });
    if id.as_deref() != Some("cachyos") {
        eprintln!(
            "error: the CachyOS installer requires CachyOS; detected {}",
            id.as_deref().unwrap_or("unknown")
        );
        return ExitCode::FAILURE;
    }
    myconfig_interface::main(Profile {
        title: "CachyOS",
        package_system: PackageSystem::Arch,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use super::MODULES;

    #[test]
    fn the_profile_is_the_full_desktop() {
        let names: Vec<_> = MODULES.iter().map(|module| module.name()).collect();
        for wanted in [
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
            "agent-config",
            "git-config",
        ] {
            assert!(names.contains(&wanted), "CachyOS lacks {wanted}");
        }
        assert_eq!(
            names[0], "plasma-version",
            "Plasma is checked before anything changes"
        );
    }
}
