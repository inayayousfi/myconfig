//! AutoHotkey scripts, started at logon when the compiled script is present.
use std::path::Path;

use embedded_dotfiles::WINDOWS_ASSETS;
use typed_fs_rs::EmbeddedDirectory;

use crate::{
    Context, Footprint, Module, ModuleResult, Need,
    support::{
        StartupShortcut, copy_embedded, startup_directory, verify_embedded, write_startup_shortcut,
    },
};

pub struct AutoHotkey;

const SOURCE: &str = "dotfiles/AutoHotkey";
const SHORTCUT: &str = "inayos-autohotkey.lnk";

impl Module for AutoHotkey {
    fn name(&self) -> &'static str {
        "autohotkey"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let destination = ctx.home.join("AutoHotkey");
        let files = WINDOWS_ASSETS.dotfiles.AutoHotkey.files();
        if copy_embedded(ctx, files, Path::new(SOURCE), &destination)? == 0 {
            return Err("AutoHotkey resources are missing from the binary".into());
        }
        let executable = destination.join("inayos.exe");
        if !executable.is_file() {
            return Ok(());
        }
        write_startup_shortcut(
            ctx,
            &StartupShortcut {
                name: SHORTCUT,
                target: &executable,
                arguments: "",
                working_directory: &destination,
                description: "",
                minimized: false,
            },
        )
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let destination = ctx.home.join("AutoHotkey");
        verify_embedded(
            WINDOWS_ASSETS.dotfiles.AutoHotkey.files(),
            Path::new(SOURCE),
            &destination,
        )?;
        if destination.join("inayos.exe").is_file()
            && !startup_directory(ctx)?.join(SHORTCUT).is_file()
        {
            return Err("the AutoHotkey startup shortcut is missing".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
