//! The editor that Git and other programs open files with: Neovim inside Arch WSL, through
//! a wrapper that turns Windows paths into WSL paths.
use std::path::PathBuf;

use embedded_dotfiles::WINDOWS_ASSETS;

use crate::{Context, Footprint, Module, ModuleResult, Need, Setting};

pub struct WindowsEditor;

/// `~/.local/bin` is not on the Windows PATH, so programs get the wrapper's full path.
const WRAPPER: &str = ".local/bin/nvim.cmd";

fn files() -> [(&'static str, &'static [u8]); 2] {
    let bin = &WINDOWS_ASSETS.dotfiles.bin;
    [
        (".local/bin/nvim.cmd", bin.nvim_cmd.content),
        (".local/bin/nvim.ps1", bin.nvim_ps1.content),
    ]
}

fn wrapper(ctx: &Context) -> PathBuf {
    ctx.home.join(WRAPPER)
}

/// The wrapper's path with forward slashes, which Git and the other programs all accept.
fn command(ctx: &Context) -> String {
    wrapper(ctx).display().to_string().replace('\\', "/")
}

fn git_editor() -> Setting {
    Setting::GitConfig {
        key: "core.editor".to_owned(),
    }
}

fn environment(name: &str) -> Setting {
    Setting::UserEnvironment {
        name: name.to_owned(),
    }
}

fn settings() -> [Setting; 3] {
    [environment("EDITOR"), environment("VISUAL"), git_editor()]
}

impl Module for WindowsEditor {
    fn name(&self) -> &'static str {
        "editor"
    }

    fn needs(&self) -> &'static [Need] {
        // Neovim and its config come from the Arch WSL profile, installed in the distro.
        &[Need::Hard(crate::ArchWsl::NAME)]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: Vec::new(),
            settings: settings().to_vec(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        for (path, contents) in files() {
            ctx.write_file(&ctx.home.join(path), contents, false)?;
        }
        let command = command(ctx);
        for setting in settings() {
            ctx.set(setting, &command)?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for (path, contents) in files() {
            let path = ctx.home.join(path);
            if std::fs::read(&path).ok().as_deref() != Some(contents) {
                return Err(format!("{} is missing or changed", path.display()).into());
            }
        }
        let command = command(ctx);
        for setting in settings() {
            if setting.read(ctx)?.as_deref() != Some(command.as_str()) {
                return Err(format!("{} is not {command}", setting.describe()).into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
