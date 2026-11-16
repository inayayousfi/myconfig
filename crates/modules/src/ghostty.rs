//! Ghostty, set as the KDE Plasma terminal.
use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting, deploy::verify_package,
    support::verify_packages,
};

pub struct Ghostty;

const TERMINAL: &str = "/usr/bin/ghostty --gtk-single-instance=true";
const SERVICE: &str = "com.mitchellh.ghostty.desktop";

/// The KDE keys that name the terminal, with Ghostty's values.
fn terminal_settings() -> [(Setting, &'static str); 2] {
    [
        (
            Setting::kde("kdeglobals", &["General"], "TerminalApplication"),
            TERMINAL,
        ),
        (
            Setting::kde("kdeglobals", &["General"], "TerminalService"),
            SERVICE,
        ),
    ]
}

impl Module for Ghostty {
    fn name(&self) -> &'static str {
        "ghostty"
    }

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Hard("plasma-version")]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Ghostty],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&[Package::Ghostty])?;
        for (setting, value) in terminal_settings() {
            ctx.set(setting, value)?;
        }
        ctx.deploy_config("ghostty")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Ghostty])?;
        for (setting, value) in terminal_settings() {
            if setting.read(ctx)?.as_deref() != Some(value) {
                return Err("Ghostty is not the KDE Plasma terminal".into());
            }
        }
        verify_package(ctx, "ghostty")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
