//! Ghostty, set as the KDE Plasma terminal when Plasma is present.
use crate::{
    Context, Footprint, Module, ModuleResult, Package, Setting, deploy::verify_package,
    support::verify_packages,
};

pub struct Ghostty;

const TERMINAL: &str = "/usr/bin/ghostty --gtk-single-instance=true";
const SERVICE: &str = "com.mitchellh.ghostty.desktop";

fn kde_present(ctx: &Context) -> bool {
    ctx.find_program("plasmashell").is_ok() && ctx.find_program("kwriteconfig6").is_ok()
}

impl Module for Ghostty {
    fn name(&self) -> &'static str {
        "ghostty"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Ghostty],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Ghostty])?;
        if kde_present(ctx) {
            ctx.set(
                Setting::kde("kdeglobals", &["General"], "TerminalApplication"),
                TERMINAL,
            )?;
            ctx.set(
                Setting::kde("kdeglobals", &["General"], "TerminalService"),
                SERVICE,
            )?;
        } else {
            ctx.note("KDE Plasma not found; leaving its default terminal unchanged");
        }
        ctx.deploy_config("ghostty")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Ghostty])?;
        if kde_present(ctx)
            && Setting::kde("kdeglobals", &["General"], "TerminalService")
                .read(ctx)?
                .as_deref()
                != Some(SERVICE)
        {
            return Err("Ghostty is not the KDE Plasma terminal".into());
        }
        verify_package(ctx, "ghostty")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
