//! The btop terminal system monitor.
use crate::{Context, Footprint, Module, ModuleResult, Package, support::verify_packages};

pub struct SystemMonitor;

impl Module for SystemMonitor {
    fn name(&self) -> &'static str {
        "system-monitor"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Btop],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Btop])
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Btop])
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
