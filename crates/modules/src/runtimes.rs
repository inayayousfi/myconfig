//! Language runtimes and build tools.
use crate::{Context, Footprint, Module, ModuleResult, Package, Setting, support::verify_packages};

pub struct Runtimes;

const PACKAGES: &[Package] = &[
    Package::Go,
    Package::Bun,
    Package::Python,
    Package::Jdk,
    Package::Maven,
    Package::Llvm,
    Package::Make,
    Package::Cmake,
    Package::Nodejs,
    Package::Npm,
    Package::NodeGyp,
];

impl Module for Runtimes {
    fn name(&self) -> &'static str {
        "runtimes"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: [PACKAGES, &[Package::Rustup]].concat(),
            settings: vec![Setting::RustupDefault],
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)?;
        ctx.set(Setting::RustupDefault, "stable")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        if Setting::RustupDefault.read(ctx)?.is_none() {
            return Err("rustup has no default toolchain".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
