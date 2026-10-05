//! Command-line tools, replacing the retired editor and multiplexer setup.
use crate::{
    Context, Footprint, Module, ModuleResult, Package,
    support::{verify_absent, verify_packages},
};

pub struct Cli {
    /// Retired config packages to unlink from the home directory.
    pub retired_config: &'static [&'static str],
}

const PACKAGES: &[Package] = &[
    Package::Ripgrep,
    Package::Jq,
    Package::Fastfetch,
    Package::Btop,
    Package::Tokei,
    Package::GithubCli,
];

const RETIRED: &[Package] = &[
    Package::Fd,
    Package::Fzf,
    Package::Zoxide,
    Package::Eza,
    Package::Bat,
    Package::Hunk,
    Package::Neovim,
    Package::Lazygit,
    Package::Tmux,
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
        for package in self.retired_config {
            ctx.unstow_retired(package)?;
        }
        ctx.uninstall_packages(RETIRED)?;
        ctx.install_packages(PACKAGES)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        verify_absent(ctx, RETIRED)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
