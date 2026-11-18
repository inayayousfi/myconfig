//! bootvine, which shows the boot as a live tree of systemd jobs, from the distribution's
//! package.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, ServiceScope,
    boot_config::{
        MKINITCPIO_CONFIG, REFIND_LINUX_CONFIG, add_initramfs_hook, has_hook, with_quiet_default,
    },
    support::{edit_system_file, read_system_file, verify_packages},
};

pub struct Bootvine;

const PACKAGES: &[Package] = &[Package::Bootvine];
const SERVICES: [&str; 2] = ["bootvine.service", "bootvine-shutdown.service"];

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
        if add_initramfs_hook(ctx, "sd-bootvine", "systemd")? {
            ctx.run(cmd!(sh, "sudo mkinitcpio -P"))?;
        }
        for service in SERVICES {
            ctx.enable_service(service, ServiceScope::System)?;
        }
        // bootvine silences the console only once it runs, so quiet hides the kernel
        // messages before it.
        if edit_system_file(ctx, REFIND_LINUX_CONFIG, with_quiet_default)?.is_none() {
            ctx.note(&format!(
                "Skipping the kernel options because {REFIND_LINUX_CONFIG} was not found"
            ));
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, PACKAGES)?;
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
