//! The runtime that AppImages need to start.
use crate::{Context, Footprint, Module, ModuleResult, Need, Package, support::verify_packages};

pub struct AppImage;

/// Most AppImages mount themselves through libfuse2, which fuse3 does not provide.
const PACKAGES: &[Package] = &[Package::Fuse2];

impl Module for AppImage {
    fn name(&self) -> &'static str {
        "appimage"
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
