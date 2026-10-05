//! Replaces CachyOS defaults with the tools this setup uses instead.
use crate::{Context, Footprint, Module, ModuleResult, Package};

pub struct CachyosSetup;

const INSTALLED: &[Package] = &[
    Package::CachyosKernelManager,
    Package::LinuxCachyos,
    Package::NotoFontsCjk,
];

const UNWANTED: &[Package] = &[
    Package::Konsole,
    Package::Alacritty,
    Package::CachyosHello,
    Package::CachyosZshConfig,
    Package::Vim,
    Package::Fish,
    Package::CachyosFishConfig,
    Package::FishAutopair,
    Package::FishPurePrompt,
    Package::Fisher,
    Package::Firefox,
    Package::FirefoxI18nFr,
    Package::MesloFont,
    Package::CachyosEmeraldKdeTheme,
    Package::CachyosIridescentKde,
    Package::CachyosNordKdeTheme,
    Package::Kate,
    Package::Micro,
    Package::CachyosMicroSettings,
    Package::Nano,
    Package::NanoSyntaxHighlighting,
    Package::Meld,
    Package::Glances,
    Package::Duf,
    Package::Tealdeer,
    Package::Filelight,
    Package::Pavucontrol,
    Package::Kcalc,
    Package::Shelly,
    Package::CachyosPackageinstaller,
    Package::Expac,
    Package::CachyosWallpapers,
    Package::Hwdetect,
    Package::Qtscrcpy,
];

impl Module for CachyosSetup {
    fn name(&self) -> &'static str {
        "cachyos-setup"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: INSTALLED.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(INSTALLED)?;
        ctx.uninstall_packages(UNWANTED)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        crate::support::verify_packages(ctx, INSTALLED)?;
        crate::support::verify_absent(ctx, UNWANTED)
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
