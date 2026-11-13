//! Replaces CachyOS defaults with the tools this setup uses instead, and leaves the boot
//! as a plain Arch installation shows it.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package,
    boot_config::{
        MKINITCPIO_CONFIG, REFIND_LINUX_CONFIG, edit_system_file, has_hook, read_system_file,
        without_hook, without_options,
    },
};

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
    Package::Fzf,
    Package::Bat,
    Package::Eza,
    Package::Btop,
    Package::Haruna,
    Package::Mpvqt,
    Package::Mpv,
    Package::Kdeconnect,
    Package::Partitionmanager,
    Package::Ark,
    Package::Gwenview,
];

/// Removed after the plymouth hook leaves mkinitcpio, so no initramfs build looks for a
/// missing hook.
const SPLASH: &[Package] = &[
    Package::PlymouthKcm,
    Package::CachyosPlymouthBootanimation,
    Package::CachyosPlymouthTheme,
    Package::Plymouth,
];

/// Kernel options that the CachyOS installer adds to a plain Arch boot.
const SPLASH_OPTIONS: &[&str] = &["quiet", "nowatchdog", "splash"];

fn remove_boot_splash(ctx: &Context) -> ModuleResult {
    let hooks_changed = edit_system_file(ctx, MKINITCPIO_CONFIG, |config| {
        without_hook(config, "plymouth")
    })?
    .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?;
    ctx.uninstall_packages(SPLASH)?;
    if edit_system_file(ctx, REFIND_LINUX_CONFIG, |config| {
        without_options(config, SPLASH_OPTIONS)
    })?
    .is_none()
    {
        ctx.note(&format!(
            "Skipping the kernel options because {REFIND_LINUX_CONFIG} was not found"
        ));
    }
    if hooks_changed {
        ctx.run(cmd!(ctx.shell, "sudo mkinitcpio -P"))?;
    }
    Ok(())
}

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
        ctx.uninstall_packages(UNWANTED)?;
        remove_boot_splash(ctx)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        crate::support::verify_packages(ctx, INSTALLED)?;
        crate::support::verify_absent(ctx, UNWANTED)?;
        crate::support::verify_absent(ctx, SPLASH)?;
        let config = read_system_file(ctx, MKINITCPIO_CONFIG)?
            .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?;
        if has_hook(&config, "plymouth") {
            return Err(format!("{MKINITCPIO_CONFIG} still runs the plymouth hook").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
