//! Command-line tools, with lazygit's config.
use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, deploy::verify_package,
    support::verify_packages,
};

pub struct Cli;

const PACKAGES: &[Package] = &[
    Package::Ripgrep,
    Package::Jq,
    Package::Fastfetch,
    Package::Tokei,
    Package::GithubCli,
    Package::Fd,
    Package::Fzf,
    Package::Zoxide,
    Package::Eza,
    Package::Bat,
    Package::Lazygit,
];

impl Module for Cli {
    fn name(&self) -> &'static str {
        "cli"
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
        ctx.install_packages(PACKAGES)?;
        ctx.deploy_config("lazygit")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        verify_package(ctx, "lazygit")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
