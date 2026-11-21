//! What each package system does with packages. Each system is one implementation of
//! `PackageManager`, and the rest of the crate reaches it through `Context::packages`.
mod apt;
mod pacman;
mod winget;

use inayos_utils::{PackageSpec, PackageSystem};

use crate::{Context, Error, ModuleResult};

#[cfg(all(test, unix))]
pub(crate) use pacman::ensure_paru;

/// The operations that installing and removing modules need from a package system.
/// Installing and uninstalling here records nothing; `Context::install_packages` and
/// `Context::uninstall_packages` record around them.
pub(crate) trait PackageManager {
    /// Refreshes the package database so installs get current versions.
    fn refresh(&self, ctx: &Context) -> ModuleResult;

    fn installed(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult<bool>;

    /// Whether `install` must run for this package even when it is installed, so that
    /// it moves to the source the catalog names.
    fn moves_installed(&self, _spec: PackageSpec) -> bool {
        false
    }

    fn install(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult;

    /// Reacts to a package that is still missing after `install`.
    fn still_missing(&self, _ctx: &Context, spec: PackageSpec) -> ModuleResult {
        Err(format!("{} is not installed after installation", spec.name).into())
    }

    fn uninstall(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult;

    /// Puts back the configuration files of `spec` that an earlier uninstall saved aside
    /// because they had been edited. Only pacman saves them.
    fn restore_saved_configuration(&self, _ctx: &Context, _spec: PackageSpec) -> ModuleResult {
        Ok(())
    }

    /// Uninstalls `spec` together with the installed packages that depend on it.
    fn uninstall_with_dependents(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult;

    /// Installed packages that depend on `spec`. Empty when the system cannot tell.
    fn dependents(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult<Vec<String>>;

    /// The package that owns the running kernel, which `remove` never uninstalls.
    fn running_kernel(&self, ctx: &Context) -> ModuleResult<Option<String>>;

    /// Uninstalls a package built from this repository, when it is installed.
    fn uninstall_local(&self, ctx: &Context, name: &str) -> ModuleResult;
}

pub(crate) fn for_system(system: PackageSystem) -> &'static dyn PackageManager {
    match system {
        PackageSystem::Arch => &pacman::Pacman,
        PackageSystem::Apt => &apt::Apt,
        PackageSystem::Winget => &winget::Winget,
    }
}

/// The names of `specs`, which must all come from `tool`.
fn names(specs: &[PackageSpec], tool: inayos_utils::PackageTool) -> ModuleResult<Vec<&str>> {
    specs
        .iter()
        .map(|spec| {
            if spec.tool == tool {
                Ok(spec.name)
            } else {
                Err(foreign(*spec))
            }
        })
        .collect()
}

fn foreign(spec: PackageSpec) -> Error {
    format!(
        "{} is a {:?} package, which this package system does not install",
        spec.name, spec.tool
    )
    .into()
}
