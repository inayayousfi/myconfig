//! Neovim with its config, which needs Neovim 0.12 for its built-in plugin manager.
use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    deploy::verify_package,
    release_archive::{Build, ReleaseArchive},
    support::verify_packages,
};

pub struct Neovim {
    pub source: NeovimSource,
}

/// Where Neovim comes from.
pub enum NeovimSource {
    /// The system's package, when it is at least 0.12.
    Package,
    /// The official release archive, for systems whose package is older.
    Release,
}

const PACKAGES: &[Package] = &[Package::Neovim];

const RELEASE: ReleaseArchive = ReleaseArchive {
    folder: "nvim-0.12.6",
    builds: &[
        Build {
            machine: "x86_64",
            url: "https://github.com/neovim/neovim/releases/download/v0.12.6/nvim-linux-x86_64.tar.gz",
            sha256: "474430d53e6264f6d6dd18db42d6dc9df3a1b56ca9e88a325bbf860e1a811d87",
            strip: 1,
        },
        Build {
            machine: "aarch64",
            url: "https://github.com/neovim/neovim/releases/download/v0.12.6/nvim-linux-arm64.tar.gz",
            sha256: "8f1f64a0bdb97247034038c3823c6cbad5bdf9ecd5751b85494b71c3ee04c815",
            strip: 1,
        },
    ],
    programs: &[("nvim", "bin/nvim")],
};

impl Module for Neovim {
    fn name(&self) -> &'static str {
        "neovim"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        let packages = match self.source {
            NeovimSource::Package => PACKAGES.to_vec(),
            NeovimSource::Release => Vec::new(),
        };
        Footprint {
            packages,
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        match self.source {
            NeovimSource::Package => ctx.install_packages(PACKAGES)?,
            NeovimSource::Release => RELEASE.install(ctx)?,
        }
        ctx.deploy_config("nvim")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        match self.source {
            NeovimSource::Package => verify_packages(ctx, PACKAGES)?,
            NeovimSource::Release => RELEASE.verify(ctx)?,
        }
        verify_package(ctx, "nvim")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
