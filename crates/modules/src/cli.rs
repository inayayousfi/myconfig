//! Command-line tools.
use crate::{Context, Footprint, Module, ModuleResult, Package, support::verify_packages};

pub struct Cli;

const PACKAGES: &[Package] = &[
    Package::Ripgrep,
    Package::Jq,
    Package::Fastfetch,
    Package::Tokei,
    Package::GithubCli,
];

impl Module for Cli {
    fn name(&self) -> &'static str {
        "cli"
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
