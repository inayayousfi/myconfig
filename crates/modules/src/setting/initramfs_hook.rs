//! A hook in the `HOOKS` list of `/etc/mkinitcpio.conf`, whose value is the hook right
//! before it, or empty when it comes first.
use std::path::Path;

use super::Kind;
use crate::{
    Context, ModuleResult,
    boot_config::{MKINITCPIO_CONFIG, hook_before, with_hook_after, without_hook},
    support::{install_system_file, read_system_file},
};

pub(super) struct InitramfsHook<'a> {
    pub hook: &'a str,
}

fn config(ctx: &Context) -> ModuleResult<String> {
    Ok(read_system_file(ctx, MKINITCPIO_CONFIG)?
        .ok_or_else(|| format!("{MKINITCPIO_CONFIG} was not found"))?)
}

fn replace(ctx: &Context, contents: &str) -> ModuleResult {
    install_system_file(
        ctx,
        Path::new(MKINITCPIO_CONFIG),
        contents.as_bytes(),
        "0644",
    )
}

impl Kind for InitramfsHook<'_> {
    fn describe(&self) -> String {
        format!("initramfs hook {}", self.hook)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        Ok(hook_before(&config(ctx)?, self.hook))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        replace(ctx, &with_hook_after(&config(ctx)?, self.hook, value)?)
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        replace(ctx, &without_hook(&config(ctx)?, self.hook))
    }
}
