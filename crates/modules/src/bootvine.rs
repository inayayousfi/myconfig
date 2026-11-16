//! bootvine, which shows the boot as a live tree of systemd jobs, built from the
//! package recipe in this repository.
use std::{fs, path::PathBuf};

use embedded_dotfiles::BOOTVINE_ASSETS;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, ServiceScope,
    boot_config::{
        MKINITCPIO_CONFIG, REFIND_LINUX_CONFIG, add_initramfs_hook, has_hook, with_quiet_default,
    },
    support::{edit_system_file, read_system_file, with_temporary_directory},
};

pub struct Bootvine;

const SERVICES: [&str; 2] = ["bootvine.service", "bootvine-shutdown.service"];

/// The `version-release` that the recipe builds.
fn recipe_version(recipe: &str) -> ModuleResult<String> {
    let field = |name: &str| {
        recipe
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .map(|value| value.trim().trim_matches(['\'', '"']).to_owned())
            .ok_or_else(|| format!("the bootvine recipe has no {name}"))
    };
    Ok(format!("{}-{}", field("pkgver")?, field("pkgrel")?))
}

/// Builds the package when the installed version differs, and returns its files.
fn build(ctx: &Context, directory: &std::path::Path) -> ModuleResult<Vec<PathBuf>> {
    let sh = ctx.shell;
    let recipe = BOOTVINE_ASSETS.PKGBUILD.content;
    let version = recipe_version(&String::from_utf8_lossy(recipe))?;
    let (_, installed) = ctx.read_unchecked(cmd!(sh, "pacman -Q bootvine"))?;
    if installed == format!("bootvine {version}") {
        return Ok(Vec::new());
    }
    fs::write(directory.join("PKGBUILD"), recipe)?;
    let _cwd = sh.push_dir(directory);
    ctx.run(cmd!(sh, "makepkg --noconfirm"))?;
    let packages: Vec<PathBuf> = ctx
        .read(cmd!(sh, "makepkg --packagelist"))?
        .lines()
        .map(|path| directory.join(path))
        .collect();
    if packages.is_empty() {
        return Err("makepkg did not produce the bootvine package".into());
    }
    Ok(packages)
}

impl Module for Bootvine {
    fn name(&self) -> &'static str {
        "bootvine"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    /// bootvine is a package built here, so the catalog footprint cannot list it.
    fn changes_packages(&self, _ctx: &Context) -> bool {
        true
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let config = read_system_file(ctx, MKINITCPIO_CONFIG)?
            .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?;
        if !has_hook(&config, "systemd") {
            return Err(format!("bootvine needs the systemd hook in {MKINITCPIO_CONFIG}").into());
        }
        let first_install = !ctx.succeeds(cmd!(sh, "pacman -Q bootvine"))?;
        with_temporary_directory("bootvine", |directory| {
            let packages = build(ctx, directory)?;
            // Recorded before the hook, so remove takes the hook out before the package.
            if first_install && !packages.is_empty() {
                ctx.local_package_installed("bootvine")?;
            }
            // HOOKS names sd-bootvine only once its package is built, and before the
            // package installs, so the rebuild that pacman runs includes it.
            let hook_changed = add_initramfs_hook(ctx, "sd-bootvine", "systemd")?;
            if !packages.is_empty() {
                ctx.run(cmd!(sh, "sudo pacman -U --noconfirm {packages...}"))
            } else if hook_changed {
                ctx.run(cmd!(sh, "sudo mkinitcpio -P"))
            } else {
                Ok(())
            }
        })?;
        for service in SERVICES {
            ctx.enable_service(service, ServiceScope::System)?;
        }
        // bootvine silences the console only once it runs, so quiet hides the kernel
        // messages before it.
        if edit_system_file(ctx, REFIND_LINUX_CONFIG, with_quiet_default)?.is_none() {
            ctx.note(&format!(
                "Skipping the kernel options because {REFIND_LINUX_CONFIG} was not found"
            ));
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        if !ctx.succeeds(cmd!(sh, "pacman -Q bootvine"))? {
            return Err("bootvine is not installed".into());
        }
        let config = read_system_file(ctx, MKINITCPIO_CONFIG)?
            .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?;
        if !has_hook(&config, "sd-bootvine") {
            return Err(format!("{MKINITCPIO_CONFIG} does not run the sd-bootvine hook").into());
        }
        for service in SERVICES {
            if !ctx.succeeds(cmd!(sh, "systemctl --quiet is-enabled {service}"))? {
                return Err(format!("{service} is not enabled").into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_recipe_version_is_version_and_release() {
        assert_eq!(
            super::recipe_version("pkgname=bootvine\npkgver=0.1.3\npkgrel=1\n").unwrap(),
            "0.1.3-1"
        );
    }
}
