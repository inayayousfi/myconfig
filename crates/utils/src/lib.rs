use package_catalog::{PACKAGES, Package, Platform};

mod files;
mod linux_session;
mod packages;
mod programs;

pub use files::{ExistingFilePolicy, install_embedded_file, install_embedded_file_with_policy};
pub use linux_session::LinuxSession;
pub use packages::{PackageInstallError, install_packages, remove_arch_packages};
pub use programs::{emacs_home, find_program};

/// Package identifier and the tool that will eventually install it.
/// On Arch, official packages use Pacman and AUR packages use Paru.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageSpec {
    pub tool: PackageTool,
    pub name: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageTool {
    Pacman,
    Paru,
    Apt,
    Winget,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageSystem {
    Arch,
    Apt,
    Winget,
}

/// Resolve an existing catalog mapping without executing a package manager.
pub fn resolve_package(package: Package, system: PackageSystem) -> Option<PackageSpec> {
    let sources: &[Platform] = match system {
        PackageSystem::Arch => &[Platform::Pacman, Platform::Aur],
        PackageSystem::Apt => &[Platform::Apt],
        PackageSystem::Winget => &[Platform::Winget],
    };
    sources.iter().find_map(|source| {
        let (_, name) = PACKAGES
            .iter()
            .find(|((candidate, platform), _)| *candidate == package && platform == source)?;
        Some(PackageSpec {
            tool: match source {
                Platform::Pacman => PackageTool::Pacman,
                Platform::Aur => PackageTool::Paru,
                Platform::Apt => PackageTool::Apt,
                Platform::Winget => PackageTool::Winget,
            },
            name: (*name)?,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_selects_paru_for_aur_packages() {
        assert_eq!(
            resolve_package(Package::Yazi, PackageSystem::Arch),
            Some(PackageSpec {
                tool: PackageTool::Paru,
                name: "yazi-git"
            })
        );
        assert_eq!(
            resolve_package(Package::Git, PackageSystem::Arch),
            Some(PackageSpec {
                tool: PackageTool::Pacman,
                name: "git"
            })
        );
    }

    #[test]
    fn unavailable_mappings_are_not_invented() {
        assert_eq!(resolve_package(Package::Yazi, PackageSystem::Winget), None);
    }
}
