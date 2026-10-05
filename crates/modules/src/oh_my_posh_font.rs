//! The Iosevka Nerd Font, installed for the current user by Oh My Posh.
use std::{collections::HashSet, fs, path::PathBuf};

use xshell::cmd;

use crate::{Context, Footprint, Module, ModuleResult, Package, Setting};

pub struct OhMyPoshFont;

const FONTS_KEY: &str = r"HKEY_CURRENT_USER\Software\Microsoft\Windows NT\CurrentVersion\Fonts";

fn font_directory(ctx: &Context) -> ModuleResult<PathBuf> {
    let local = ctx
        .shell
        .var_os("LOCALAPPDATA")
        .ok_or("LOCALAPPDATA is unset")?;
    Ok(PathBuf::from(local).join("Microsoft/Windows/Fonts"))
}

fn font_files(ctx: &Context) -> ModuleResult<HashSet<PathBuf>> {
    let directory = font_directory(ctx)?;
    if !directory.is_dir() {
        return Ok(HashSet::new());
    }
    Ok(fs::read_dir(directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect())
}

fn registered_fonts(ctx: &Context) -> ModuleResult<HashSet<String>> {
    let (_, output) = ctx.read_unchecked(cmd!(ctx.shell, "reg.exe query {FONTS_KEY}"))?;
    Ok(Setting::registry_value_names(&output).into_iter().collect())
}

impl Module for OhMyPoshFont {
    fn name(&self) -> &'static str {
        "oh-my-posh-font"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::OhMyPosh],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let Ok(program) = ctx.find_program("oh-my-posh.exe") else {
            ctx.note("Oh My Posh is not installed; skipping the Iosevka font");
            return Ok(());
        };
        // Oh My Posh copies the files and registers them itself, so record what appeared.
        let files_before = font_files(ctx)?;
        let names_before = registered_fonts(ctx)?;
        let installed = ctx.run(cmd!(ctx.shell, "{program} font install Iosevka"));
        for file in font_files(ctx)?.difference(&files_before) {
            ctx.created(file, false)?;
        }
        for name in registered_fonts(ctx)?.difference(&names_before) {
            ctx.record_setting_previous(
                Setting::RegistryValue {
                    key: FONTS_KEY.to_owned(),
                    name: name.clone(),
                },
                None,
            )?;
        }
        installed
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if ctx.find_program("oh-my-posh.exe").is_err() {
            return Ok(());
        }
        if !registered_fonts(ctx)?
            .iter()
            .any(|name| name.contains("Iosevka"))
        {
            return Err("the Iosevka font is not registered for this user".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
