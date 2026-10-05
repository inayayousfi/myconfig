use std::process::ExitCode;

use myconfig_interface::Profile;
use myconfig_modules::*;

static MODULES: &[&dyn Module] = &[
    &PackageGroup::BASE,
    &PackageGroup::DEV_TOOLS,
    &PackageGroup::ART,
    &PackageGroup::SUPPLEMENTARY,
    &ArchWsl,
    &PowerShellProfile,
    &OhMyPosh,
    &WindowsTerminal,
    &EmacsCopied(EmacsOptions {
        browser_terminal_firewall: false,
    }),
    &AutoHotkey,
    &AgentConfigCopied,
    &PsReadLine,
    &OhMyPoshFont,
    &LlvmPath,
    &RegistryTweaks,
    &TaskbarAutoHide,
    &SharedDesktop,
];

fn main() -> ExitCode {
    if !cfg!(windows) {
        eprintln!("error: the Windows Workstation installer requires Windows");
        return ExitCode::FAILURE;
    }
    myconfig_interface::main(Profile {
        title: "Windows Workstation",
        package_system: PackageSystem::Winget,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use super::MODULES;

    #[test]
    fn the_base_packages_come_first() {
        // Every later module uses PowerShell 7 or another base package.
        assert_eq!(MODULES[0].name(), "base-packages");
    }
}
