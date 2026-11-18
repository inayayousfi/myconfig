//! The check that KDE Plasma is a supported version. In the desktop profile, `base`
//! needs it, so no module changes the machine on an unsupported Plasma.
use xshell::cmd;

use crate::{Context, Footprint, Module, ModuleResult, Need, support::require_programs};

pub struct PlasmaVersion;

impl PlasmaVersion {
    /// The module name, for the soft needs that the compiler checks.
    pub const NAME: &'static str = "plasma-version";
}

/// KDE Plasma 6.7 through 6.x, which the panels and the layout script support. On a system
/// without Plasma, such as a new plain Arch system before `kde-plasma` runs, the check reads
/// the version that pacman would install.
pub(crate) fn require_supported_plasma(ctx: &Context) -> ModuleResult {
    require_programs(ctx, &["pacman"])?;
    if ctx.succeeds(cmd!(ctx.shell, "pacman -Q plasma-workspace"))? {
        return check_plasma_version(&ctx.read(cmd!(ctx.shell, "pacman -Q plasma-workspace"))?);
    }
    check_plasma_version(&repository_version(
        &ctx.read(cmd!(ctx.shell, "pacman -Si plasma-workspace"))?,
    )?)
}

/// Turns the `Version` field of `pacman -Si` output into the `name version` shape of
/// `pacman -Q`.
fn repository_version(output: &str) -> ModuleResult<String> {
    output
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            (key.trim() == "Version").then(|| format!("plasma-workspace {}", value.trim()))
        })
        .ok_or_else(|| "pacman -Si plasma-workspace shows no version".into())
}

/// Accepts `pacman -Q plasma-workspace` output for KDE Plasma 6.7 through 6.x, which the
/// panels and the layout script support.
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
        Self::NAME
    }

    fn needs(&self) -> &'static [Need] {
        &[]
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
    use super::{check_plasma_version, repository_version};

    #[test]
    fn the_repository_version_reads_like_an_installed_one() {
        let output = "Repository      : extra\nName            : plasma-workspace\nVersion         : 1:6.8.3-1\n";
        assert_eq!(
            repository_version(output).unwrap(),
            "plasma-workspace 1:6.8.3-1"
        );
        assert!(repository_version("Name : plasma-workspace\n").is_err());
    }

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
