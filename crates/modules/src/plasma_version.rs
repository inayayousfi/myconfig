//! Stops a CachyOS run early when KDE Plasma is not a supported version.
use xshell::cmd;

use crate::{Context, Footprint, Module, ModuleResult, support::require_programs};

pub struct PlasmaVersion;

/// KDE Plasma 6.7 through 6.x, which the panels, Glass and the layout script support.
pub(crate) fn require_supported_plasma(ctx: &Context) -> ModuleResult {
    require_programs(ctx, &["plasmashell", "pacman"])?;
    check_plasma_version(&ctx.read(cmd!(ctx.shell, "pacman -Q plasma-workspace"))?)
}

/// Accepts `pacman -Q plasma-workspace` output for KDE Plasma 6.7 through 6.x, which the
/// panels, Glass and the layout script support.
fn check_plasma_version(output: &str) -> ModuleResult {
    let version = output
        .split_whitespace()
        .nth(1)
        .ok_or("could not read the Plasma version")?;
    let no_epoch = version.rsplit(':').next().unwrap_or(version);
    let mut components = no_epoch.split('.');
    let major: u32 = components
        .next()
        .ok_or("Plasma has no major version")?
        .parse()?;
    let minor: u32 = components
        .next()
        .ok_or("Plasma has no minor version")?
        .parse()?;
    if major != 6 || minor < 7 {
        return Err(format!("KDE Plasma 6.7 through 6.x is required; found {version}").into());
    }
    Ok(())
}

impl Module for PlasmaVersion {
    fn name(&self) -> &'static str {
        "plasma-version"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        require_supported_plasma(ctx)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        require_supported_plasma(ctx)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::check_plasma_version;

    #[test]
    fn only_plasma_6_7_through_6_x_is_supported() {
        assert!(check_plasma_version("plasma-workspace 6.7.0-1").is_ok());
        assert!(check_plasma_version("plasma-workspace 6.99.4-2").is_ok());
        assert!(check_plasma_version("plasma-workspace 1:6.8.1-1").is_ok());
        assert!(check_plasma_version("plasma-workspace 6.6.5-1").is_err());
        assert!(check_plasma_version("plasma-workspace 7.0.0-1").is_err());
        assert!(check_plasma_version("unknown version").is_err());
    }
}
