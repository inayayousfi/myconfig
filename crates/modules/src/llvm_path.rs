//! Adds LLVM to the machine PATH when the dev-tools packages installed it.
use std::path::{Path, PathBuf};

use crate::{Context, Footprint, Module, ModuleResult, Package, Setting};

pub struct LlvmPath;

fn llvm_bin(ctx: &Context) -> ModuleResult<PathBuf> {
    let program_files = ctx
        .shell
        .var_os("ProgramFiles")
        .ok_or("ProgramFiles is unset")?;
    Ok(Path::new(&program_files).join("LLVM/bin"))
}

fn entry(ctx: &Context) -> ModuleResult<Setting> {
    Ok(Setting::MachinePathEntry {
        entry: llvm_bin(ctx)?
            .to_str()
            .ok_or("LLVM path is not UTF-8")?
            .to_owned(),
    })
}

impl Module for LlvmPath {
    fn name(&self) -> &'static str {
        "llvm-path"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Llvm],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let llvm = llvm_bin(ctx)?;
        if !llvm.is_dir() {
            ctx.note(&format!(
                "LLVM bin directory not found; skipping machine PATH setup: {}",
                llvm.display()
            ));
            return Ok(());
        }
        let setting = entry(ctx)?;
        ctx.set(setting.clone(), "present")?;
        if setting.read(ctx)?.is_none() {
            return Err("LLVM was not added to the machine PATH".into());
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if llvm_bin(ctx)?.is_dir() && entry(ctx)?.read(ctx)?.is_none() {
            return Err("LLVM is not on the machine PATH".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
