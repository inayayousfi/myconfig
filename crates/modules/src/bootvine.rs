//! bootvine, which shows the boot as a live tree of systemd jobs, from the distribution's
//! package, in the Lilas colors.
use std::path::Path;

use embedded_dotfiles::BOOTVINE_ASSETS;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    boot_config::{
        KERNEL_CMDLINE, MKINITCPIO_CONFIG, REFIND_LINUX_CONFIG, add_initramfs_hook, cmdline_with,
        has_hook, with_console_colors, with_quiet_default,
    },
    support::{edit_system_file, read_system_bytes, read_system_file, verify_packages},
};

pub struct Bootvine;

const PACKAGES: &[Package] = &[Package::Bootvine];
const SERVICES: [&str; 2] = ["bootvine.service", "bootvine-shutdown.service"];
const CONFIG: &str = "/etc/bootvine.toml";
/// The Ghostty palette, colors 0 to 15, which the console shows instead of its own.
const PALETTE: [&str; 16] = [
    "1c1528", "ff6b8b", "9ee6c8", "ffe39a", "9cc8f0", "ff4ead", "8fe3ef", "f1e8f8", "7a6a92",
    "ff9ab0", "c4f2de", "fff0c4", "c3defa", "ff85c8", "bff1f7", "fbf6fe",
];

/// The kernel options that set the console's 16 colors to the palette.
fn console_colors() -> Vec<String> {
    ["red", "grn", "blu"]
        .iter()
        .enumerate()
        .map(|(channel, name)| {
            let values: Vec<String> = PALETTE
                .iter()
                .map(|color| {
                    let digits = &color[channel * 2..channel * 2 + 2];
                    u8::from_str_radix(digits, 16).unwrap_or(0).to_string()
                })
                .collect();
            format!("vt.default_{name}={}", values.join(","))
        })
        .collect()
}

fn config_is_current(ctx: &Context) -> ModuleResult<bool> {
    Ok(read_system_bytes(ctx, CONFIG)?.as_deref() == Some(BOOTVINE_ASSETS.bootvine_toml.content))
}

impl Module for Bootvine {
    fn name(&self) -> &'static str {
        "bootvine"
    }

    /// The kernel options go into the boot entries that the refind module writes on the
    /// system an installer-mode run builds.
    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base"), Need::Soft(crate::Refind::NAME)]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let config = read_system_file(ctx, MKINITCPIO_CONFIG)?
            .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?;
        if !has_hook(&config, "systemd") {
            return Err(format!("bootvine needs the systemd hook in {MKINITCPIO_CONFIG}").into());
        }
        let entries = read_system_file(ctx, REFIND_LINUX_CONFIG)?.is_some();
        if !entries {
            ctx.missing_part(REFIND_LINUX_CONFIG, "the kernel options")?;
        }
        // The package goes in before HOOKS names its hook, so a failed install never
        // leaves an initramfs configuration that the next kernel update cannot build.
        // Recorded first, so remove takes the hook out before the package.
        ctx.install_packages(PACKAGES)?;
        let config_changed = !config_is_current(ctx)?;
        if config_changed {
            ctx.write_system_file(
                Path::new(CONFIG),
                BOOTVINE_ASSETS.bootvine_toml.content,
                "0644",
            )?;
        }
        // bootvine silences the console only once it runs, so quiet hides the kernel
        // messages before it.
        let colors = console_colors();
        // On an encrypted system the unified kernel image holds the kernel options of
        // /etc/kernel/cmdline, so the rebuild below takes them in.
        let mut options = vec!["quiet".to_owned()];
        options.extend(colors.iter().cloned());
        let cmdline_changed =
            edit_system_file(ctx, KERNEL_CMDLINE, |line| cmdline_with(line, &options))?
                == Some(true);
        // The initramfs holds a copy of the colors file, so it is rebuilt for either change.
        let hook_changed = add_initramfs_hook(ctx, "sd-bootvine", "systemd")?;
        if hook_changed || config_changed || cmdline_changed {
            ctx.run(cmd!(sh, "sudo mkinitcpio -P"))?;
        }
        for service in SERVICES {
            ctx.enable_service(service, ServiceScope::System)?;
        }
        let edit = |config: &str| with_console_colors(&with_quiet_default(config), &colors);
        if entries {
            edit_system_file(ctx, REFIND_LINUX_CONFIG, edit)?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        if !config_is_current(ctx)? {
            return Err(format!("{CONFIG} does not hold the Lilas colors").into());
        }
        let config = read_system_file(ctx, MKINITCPIO_CONFIG)?
            .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?;
        if !has_hook(&config, "sd-bootvine") {
            return Err(format!("{MKINITCPIO_CONFIG} does not run the sd-bootvine hook").into());
        }
        for service in SERVICES {
            if !ctx.succeeds(cmd!(sh, "systemctl --quiet is-enabled {service}"))? {
                return Err(format!("{service} is not enabled").into());
            }
        }
        if read_system_file(ctx, REFIND_LINUX_CONFIG)?.is_none() {
            ctx.missing_part(REFIND_LINUX_CONFIG, "the kernel options check")?;
        }
        if let Some(cmdline) = read_system_file(ctx, KERNEL_CMDLINE)?
            && !cmdline.split_whitespace().any(|option| option == "quiet")
        {
            return Err(format!("{KERNEL_CMDLINE} lacks the quiet option").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{PALETTE, console_colors};

    #[test]
    fn the_console_palette_is_the_ghostty_palette() {
        let config = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../dotfiles/ghostty/.config/ghostty/config"
        ))
        .unwrap();
        let ghostty: Vec<String> = config
            .lines()
            .filter_map(|line| line.strip_prefix("palette = "))
            .map(|entry| entry.split_once("=#").unwrap().1.to_lowercase())
            .collect();
        assert_eq!(ghostty, PALETTE);
    }

    #[test]
    fn the_console_colors_split_each_color_into_its_channels() {
        let colors = console_colors();
        assert!(colors[0].starts_with("vt.default_red=28,255,158,255,156,255,"));
        assert!(colors[1].starts_with("vt.default_grn=21,107,230,227,200,78,"));
        assert!(colors[2].starts_with("vt.default_blu=40,139,200,154,240,173,"));
        assert!(colors.iter().all(|option| option.split(',').count() == 16));
    }
}
