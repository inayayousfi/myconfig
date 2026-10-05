//! Yazi and the tools it uses for previews, with the Yazi config.
use crate::{
    Context, Footprint, Module, ModuleResult, Package, deploy::verify_package,
    support::verify_packages,
};

pub struct TerminalTools;

const PACKAGES: &[Package] = &[
    Package::Yazi,
    Package::Ffmpeg,
    Package::SevenZip,
    Package::Poppler,
    Package::Resvg,
    Package::Imagemagick,
];

impl Module for TerminalTools {
    fn name(&self) -> &'static str {
        "terminal-tools"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)?;
        ctx.deploy_config("yazi")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "yazi")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
