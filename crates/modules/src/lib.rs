use std::path::Path;

use myconfig_utils::PackageSystem;
use xshell::Shell;

pub mod linux;
pub mod windows;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Profile {
    Cachyos,
    ArchWsl,
    UbuntuServer,
    Windows,
}

/// Inputs formerly obtained from profile globals and filesystem paths.
pub struct ModuleContext<'a> {
    pub profile: Profile,
    pub package_system: PackageSystem,
    pub shell: &'a Shell,
    pub home: &'a Path,
}

pub type ModuleResult = Result<(), Box<dyn std::error::Error>>;
