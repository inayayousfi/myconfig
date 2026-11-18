//! Google Chrome from the AUR, the browser that the dock pins.
use crate::{Context, Footprint, Module, ModuleResult, Need, Package, support::verify_packages};

pub struct Chrome;

const PACKAGES: &[Package] = &[Package::GoogleChrome];

impl Module for Chrome {
    fn name(&self) -> &'static str {
        "chrome"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
