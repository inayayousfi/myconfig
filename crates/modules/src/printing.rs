//! Printing: the CUPS print server, started by its socket when something prints, with
//! KDE's printer settings and the helper that picks a printer's driver.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope, support::verify_packages,
};

pub struct Printing;

const PACKAGES: &[Package] = &[
    Package::Cups,
    Package::SystemConfigPrinter,
    Package::PrintManager,
];
const SOCKET: &str = "cups.socket";

impl Module for Printing {
    fn name(&self) -> &'static str {
        "printing"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("plasma-version")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(PACKAGES)?;
        ctx.enable_service(SOCKET, ServiceScope::System)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        if !ctx.succeeds(cmd!(ctx.shell, "systemctl --quiet is-enabled {SOCKET}"))? {
            return Err(format!("{SOCKET} is not enabled").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
