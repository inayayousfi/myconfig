//! The Black & Pink Oh My Posh theme next to the PowerShell profile.
use std::path::PathBuf;

use embedded_dotfiles::DOTFILES;

use crate::{Context, Footprint, Module, ModuleResult, Package, powershell_profile::profile_paths};

pub struct OhMyPosh;

/// The theme sits next to each PowerShell profile, where the profile looks for it.
fn theme_paths(ctx: &Context) -> ModuleResult<Vec<PathBuf>> {
    profile_paths(ctx)?
        .iter()
        .map(|profile| {
            Ok(profile
                .parent()
                .ok_or("PowerShell profile has no parent directory")?
                .join("black-pink.omp.json"))
        })
        .collect()
}

fn contents() -> &'static [u8] {
    DOTFILES
        .assets
        .windows
        .dotfiles
        .PowerShell
        .black_pink_omp_json
        .content
}

impl Module for OhMyPosh {
    fn name(&self) -> &'static str {
        "oh-my-posh"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::OhMyPosh],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        for path in theme_paths(ctx)? {
            ctx.write_file(&path, contents(), false)?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for path in theme_paths(ctx)? {
            if std::fs::read(&path)? != contents() {
                return Err(format!("{} differs from the repository", path.display()).into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
