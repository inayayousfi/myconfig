//! Flatpak with the Flathub remote.
use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting, support::verify_packages,
};

pub struct Flatpak;

fn flathub() -> Setting {
    Setting::FlatpakRemote {
        name: "flathub".to_owned(),
        from: "https://dl.flathub.org/repo/flathub.flatpakrepo".to_owned(),
    }
}

impl Module for Flatpak {
    fn name(&self) -> &'static str {
        "flatpak"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Flatpak],
            settings: vec![flathub()],
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Flatpak])?;
        ctx.add(flathub())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Flatpak])?;
        if flathub().read(ctx)?.is_none() {
            return Err("the Flathub remote is missing".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
