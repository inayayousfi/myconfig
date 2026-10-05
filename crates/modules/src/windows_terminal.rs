//! Windows Terminal settings.
use std::path::{Path, PathBuf};

use embedded_dotfiles::DOTFILES;

use crate::{Context, Footprint, Module, ModuleResult, Package};

pub struct WindowsTerminal;

fn settings_path(ctx: &Context) -> ModuleResult<PathBuf> {
    let local = ctx
        .shell
        .var_os("LOCALAPPDATA")
        .ok_or("LOCALAPPDATA is unset")?;
    Ok(Path::new(&local)
        .join("Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState/settings.json"))
}

fn contents() -> &'static [u8] {
    DOTFILES
        .assets
        .windows
        .dotfiles
        .WindowsTerminal
        .settings_json
        .content
}

impl Module for WindowsTerminal {
    fn name(&self) -> &'static str {
        "windows-terminal"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::WindowsTerminal],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let path = settings_path(ctx)?;
        if std::fs::symlink_metadata(&path)
            .is_ok_and(|metadata| !metadata.is_file() || metadata.file_type().is_symlink())
        {
            return Err(format!(
                "Windows Terminal settings are not a regular file: {}",
                path.display()
            )
            .into());
        }
        ctx.write_file(&path, contents(), false)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let path = settings_path(ctx)?;
        if std::fs::read(&path)? != contents() {
            return Err(format!("{} differs from the repository", path.display()).into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
