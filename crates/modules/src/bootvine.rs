//! bootvine, which shows the boot as a live tree of systemd jobs, from the distribution's
//! package, in the Black & Pink colors.
use std::path::Path;

use embedded_dotfiles::BOOTVINE_ASSETS;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    boot_config::{
        MKINITCPIO_CONFIG, REFIND_LINUX_CONFIG, add_initramfs_hook, has_hook, with_console_colors,
        with_quiet_default,
    },
    support::{edit_system_file, read_system_bytes, read_system_file, verify_packages},
};

pub struct Bootvine;

const PACKAGES: &[Package] = &[Package::Bootvine];
const SERVICES: [&str; 2] = ["bootvine.service", "bootvine-shutdown.service"];
const CONFIG: &str = "/etc/bootvine.toml";
/// The Ghostty palette, colors 0 to 15, which the console shows instead of its own.
const PALETTE: [&str; 16] = [
    "000000", "f87171", "4ade80", "facc15", "60a5fa", "ff4ead", "22d3ee", "d0d6e0", "2a2d36",
    "fca5a5", "86efac", "fde047", "93c5fd", "ff85c8", "67e8f9", "f0f2f7",
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

    fn needs(&self) -> &'static [Need] {
        &[Need::Hard("base")]
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
        // The initramfs holds a copy of the colors file, so it is rebuilt for either change.
        let hook_changed = add_initramfs_hook(ctx, "sd-bootvine", "systemd")?;
        if hook_changed || config_changed {
            ctx.run(cmd!(sh, "sudo mkinitcpio -P"))?;
        }
        for service in SERVICES {
            ctx.enable_service(service, ServiceScope::System)?;
        }
        // bootvine silences the console only once it runs, so quiet hides the kernel
        // messages before it.
        let colors = console_colors();
        let edit = |config: &str| with_console_colors(&with_quiet_default(config), &colors);
        if edit_system_file(ctx, REFIND_LINUX_CONFIG, edit)?.is_none() {
            ctx.note(&format!(
                "Skipping the kernel options because {REFIND_LINUX_CONFIG} was not found"
            ));
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
        if !config_is_current(ctx)? {
            return Err(format!("{CONFIG} does not hold the Black & Pink colors").into());
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
        assert!(colors[0].starts_with("vt.default_red=0,248,74,250,96,255,"));
        assert!(colors[1].starts_with("vt.default_grn=0,113,222,204,165,78,"));
        assert!(colors[2].starts_with("vt.default_blu=0,113,128,21,250,173,"));
        assert!(colors.iter().all(|option| option.split(',').count() == 16));
    }
}
