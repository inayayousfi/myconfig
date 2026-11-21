use std::process::ExitCode;

use inayos_interface::Profile;
use inayos_modules::*;

static MODULES: &[&dyn Module] = &[
    &PackageGroup::BASE,
    &PackageGroup::DEV_TOOLS,
    &PackageGroup::ART,
    &PackageGroup::SUPPLEMENTARY,
    &ArchWsl,
    &PowerShellProfile,
    &OhMyPosh,
    &WindowsTerminal,
    &AutoHotkey,
    &AgentConfigCopied,
    &WindowsT3Code,
    &OhMyPoshFont,
    &PlexSerifFont,
    &LlvmPath,
    &WindowsEditor,
    &RegistryTweaks,
    &TaskbarAutoHide,
    &SharedDesktop,
];

const PACKAGE_SYSTEM: PackageSystem = PackageSystem::Winget;

fn main() -> ExitCode {
    if !cfg!(windows) {
        eprintln!("error: the Windows Workstation installer requires Windows");
        return ExitCode::FAILURE;
    }
    inayos_interface::main(Profile {
        title: "Windows Workstation",
        package_system: PACKAGE_SYSTEM,
        modules: MODULES,
    })
}

#[cfg(test)]
mod tests {
    use inayos_modules::{runner::need_problems, unnamed_packages};

    use super::{MODULES, PACKAGE_SYSTEM};

    #[test]
    fn every_need_comes_earlier_and_every_hard_need_is_in_the_profile() {
        assert_eq!(need_problems(MODULES), Vec::<String>::new());
    }

    #[test]
    fn every_package_has_a_name_on_the_package_system() {
        assert_eq!(unnamed_packages(MODULES, PACKAGE_SYSTEM), []);
    }

    #[test]
    fn the_base_packages_come_first() {
        // Every later module uses PowerShell 7 or another base package.
        assert_eq!(MODULES[0].name(), "base-packages");
    }
}
