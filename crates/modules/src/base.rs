//! The packages every other module builds on.
use crate::{Context, Footprint, Module, ModuleResult, Package};

pub struct Base {
    pub packages: &'static [Package],
    /// Distribution packages that conflict with this setup.
    pub unwanted: &'static [Package],
}

impl Base {
    pub const ARCH: &'static [Package] = &[
        Package::CaCertificates,
        Package::Sudo,
        Package::Git,
        Package::Curl,
        Package::Wget,
        Package::Rsync,
        Package::Stow,
        Package::Tar,
        Package::Unzip,
        Package::Zip,
        Package::Xz,
        Package::File,
        Package::ManDb,
        Package::ManPages,
        Package::BaseDevel,
        Package::Rustup,
        Package::Polkit,
    ];

    pub const UBUNTU: &'static [Package] = &[
        Package::CaCertificates,
        Package::Curl,
        Package::Git,
        Package::Rsync,
        Package::Stow,
    ];
}

impl Module for Base {
    fn name(&self) -> &'static str {
        "base"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: self.packages.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.uninstall_packages(self.unwanted)?;
        ctx.install_packages(self.packages)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        crate::support::verify_packages(ctx, self.packages)?;
        crate::support::verify_absent(ctx, self.unwanted)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
