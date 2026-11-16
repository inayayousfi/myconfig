//! Ubuntu: APT, with the vendor repositories that the package catalog names.
use std::collections::BTreeSet;

use myconfig_utils::{PackageSpec, PackageTool};
use xshell::cmd;

use super::{PackageManager, names};
use crate::{Context, ModuleResult};

pub(super) struct Apt;

/// The packages that `apt-get autoremove` would remove: installed as dependencies, and
/// that no installed package depends on, recommends or suggests.
fn autoremovable(ctx: &Context) -> ModuleResult<BTreeSet<String>> {
    let output = ctx.read(cmd!(ctx.shell, "apt-get --simulate autoremove"))?;
    Ok(output
        .lines()
        .filter_map(|line| line.strip_prefix("Remv "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(str::to_owned)
        .collect())
}

impl PackageManager for Apt {
    fn refresh(&self, ctx: &Context) -> ModuleResult {
        ctx.run(cmd!(ctx.shell, "sudo apt-get update"))
    }

    fn installed(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult<bool> {
        let name = spec.name;
        let format = "-f=${Status}";
        let (_, status) = ctx.read_unchecked(cmd!(ctx.shell, "dpkg-query -W {format} {name}"))?;
        Ok(status.ends_with("install ok installed"))
    }

    /// A package from a vendor repository is installed again, so that a copy from
    /// Ubuntu's own archive moves to the vendor's newer version.
    fn moves_installed(&self, spec: PackageSpec) -> bool {
        spec.apt_repository.is_some()
    }

    fn install(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult {
        let names = names(specs, PackageTool::Apt)?;
        if names.is_empty() {
            return Ok(());
        }
        if specs.iter().any(|spec| spec.apt_repository.is_some()) {
            let codename = crate::apt_repository::codename()?;
            crate::apt_repository::add_repositories(ctx, specs, &codename)?;
        }
        ctx.run(cmd!(ctx.shell, "sudo apt-get install -y {names...}"))
    }

    fn uninstall(&self, ctx: &Context, specs: &[PackageSpec]) -> ModuleResult {
        let names = names(specs, PackageTool::Apt)?;
        if names.is_empty() {
            return Ok(());
        }
        // Purging deletes the configuration files too, as pacman does. The dependencies that
        // only these packages needed go with them, but not the ones that were already unneeded.
        let unneeded = autoremovable(ctx)?;
        ctx.run(cmd!(ctx.shell, "sudo apt-get purge -y {names...}"))?;
        let orphans: Vec<String> = autoremovable(ctx)?
            .into_iter()
            .filter(|name| !unneeded.contains(name))
            .collect();
        if orphans.is_empty() {
            return Ok(());
        }
        ctx.run(cmd!(ctx.shell, "sudo apt-get purge -y {orphans...}"))
    }

    /// APT's purge already removes the installed packages that depend on the purged ones.
    fn uninstall_with_dependents(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult {
        self.uninstall(ctx, &[spec])
    }

    fn dependents(&self, ctx: &Context, spec: PackageSpec) -> ModuleResult<Vec<String>> {
        let name = spec.name;
        let output = ctx.read(cmd!(ctx.shell, "apt-cache rdepends --installed {name}"))?;
        Ok(output
            .lines()
            .skip_while(|line| !line.starts_with("Reverse Depends:"))
            .skip(1)
            .map(|line| line.trim().trim_start_matches('|').to_owned())
            .filter(|line| !line.is_empty() && line != name)
            .collect())
    }

    fn running_kernel(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let sh = ctx.shell;
        let release = ctx.read(cmd!(sh, "uname -r"))?;
        let image = format!("/boot/vmlinuz-{release}");
        let (found, output) = ctx.read_unchecked(cmd!(sh, "dpkg -S {image}"))?;
        let name = output.split(':').next().unwrap_or_default();
        Ok((found && !name.is_empty()).then(|| name.to_owned()))
    }

    fn uninstall_local(&self, _ctx: &Context, name: &str) -> ModuleResult {
        Err(
            format!("{name} is a package built from this repository, which only Arch installs")
                .into(),
        )
    }
}
