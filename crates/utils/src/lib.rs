use package_catalog::{AptRepository, PACKAGES, Package, Platform, apt_source};

mod files;
mod programs;

pub use files::{ExistingFilePolicy, install_embedded_file, install_embedded_file_with_policy};
pub use programs::find_program;

/// Package identifier and the tool that will eventually install it.
/// On Arch, official packages use Pacman and AUR packages use Paru.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PackageSpec {
    pub tool: PackageTool,
    pub name: &'static str,
    /// The vendor repository that APT installs the package from, when Ubuntu lacks it.
    pub apt_repository: Option<&'static AptRepository>,
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
        let (_, identifier) = PACKAGES
            .iter()
            .find(|((candidate, platform), _)| *candidate == package && platform == source)?;
        let identifier = (*identifier)?;
        let (apt_repository, name) = match source {
            Platform::Apt => apt_source(identifier)?,
            _ => (None, identifier),
        };
        Some(PackageSpec {
            tool: match source {
                Platform::Pacman => PackageTool::Pacman,
                Platform::Aur => PackageTool::Paru,
                Platform::Apt => PackageTool::Apt,
                Platform::Winget => PackageTool::Winget,
            },
            name,
            apt_repository,
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
                name: "yazi-git",
                apt_repository: None,
            })
        );
        assert_eq!(
            resolve_package(Package::Git, PackageSystem::Arch),
            Some(PackageSpec {
                tool: PackageTool::Pacman,
                name: "git",
                apt_repository: None,
            })
        );
    }

    #[test]
    fn apt_packages_from_a_vendor_carry_its_repository() {
        let spec = resolve_package(Package::Docker, PackageSystem::Apt).unwrap();
        assert_eq!(spec.name, "docker-ce");
        assert_eq!(
            spec.apt_repository.map(|repository| repository.id),
            Some("docker")
        );
        let spec = resolve_package(Package::Ripgrep, PackageSystem::Apt).unwrap();
        assert_eq!(spec.name, "ripgrep");
        assert_eq!(spec.apt_repository, None);
    }

    #[test]
    fn unavailable_mappings_are_not_invented() {
        assert_eq!(resolve_package(Package::Yazi, PackageSystem::Winget), None);
    }
}
