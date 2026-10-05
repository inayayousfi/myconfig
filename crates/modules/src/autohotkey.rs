//! AutoHotkey scripts, started at logon when the compiled script is present.
use std::path::Path;

use embedded_dotfiles::DOTFILES;
use typed_fs_rs::EmbeddedDirectory;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult,
    support::{copy_embedded, powershell_7, startup_directory, verify_embedded},
};

pub struct AutoHotkey;

const SOURCE: &str = "assets/windows/dotfiles/AutoHotkey";

impl Module for AutoHotkey {
    fn name(&self) -> &'static str {
        "autohotkey"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let destination = ctx.home.join("AutoHotkey");
        let files = DOTFILES.assets.windows.dotfiles.AutoHotkey.files();
        if copy_embedded(ctx, files, Path::new(SOURCE), &destination)? == 0 {
            return Err("AutoHotkey resources are missing from the binary".into());
        }
        let executable = destination.join("myconfig.exe");
        if !executable.is_file() {
            return Ok(());
        }
        let shortcut = startup_directory(ctx)?.join("myconfig-autohotkey.lnk");
        if shortcut.exists() {
            ctx.record_path(&shortcut)?;
        } else {
            ctx.created(&shortcut, false)?;
        }
        let pwsh = powershell_7(ctx)?;
        let script = "$ErrorActionPreference = 'Stop'; $shell = New-Object -ComObject WScript.Shell; $link = $shell.CreateShortcut($env:MYCONFIG_AHK_SHORTCUT); $link.TargetPath = $env:MYCONFIG_AHK_EXE; $link.WorkingDirectory = $env:MYCONFIG_AHK_WORKDIR; $link.Save(); $saved = $shell.CreateShortcut($env:MYCONFIG_AHK_SHORTCUT); if ($saved.TargetPath -ine $env:MYCONFIG_AHK_EXE) { throw 'AutoHotkey shortcut target does not match the installed executable' }";
        ctx.run(
            cmd!(ctx.shell, "{pwsh} -NoProfile -Command {script}")
                .env("MYCONFIG_AHK_SHORTCUT", &shortcut)
                .env("MYCONFIG_AHK_EXE", executable)
                .env("MYCONFIG_AHK_WORKDIR", &destination),
        )?;
        if !shortcut.is_file() {
            return Err("AutoHotkey startup shortcut was not created".into());
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let destination = ctx.home.join("AutoHotkey");
        verify_embedded(
            DOTFILES.assets.windows.dotfiles.AutoHotkey.files(),
            Path::new(SOURCE),
            &destination,
        )?;
        if destination.join("myconfig.exe").is_file()
            && !startup_directory(ctx)?
                .join("myconfig-autohotkey.lnk")
                .is_file()
        {
            return Err("the AutoHotkey startup shortcut is missing".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
