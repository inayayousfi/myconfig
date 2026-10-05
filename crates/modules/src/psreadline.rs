//! The PSReadLine PowerShell module, when PowerShell does not already have it.
use std::path::PathBuf;

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package,
    support::{powershell, powershell_7},
};

pub struct PsReadLine;

fn available(ctx: &Context) -> ModuleResult<bool> {
    let count = powershell(
        ctx,
        "(Get-Module -ListAvailable -Name PSReadLine | Measure-Object).Count",
    )?;
    Ok(count.trim().parse::<u32>()? > 0)
}

impl Module for PsReadLine {
    fn name(&self) -> &'static str {
        "psreadline"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Powershell],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        if available(ctx)? {
            return Ok(());
        }
        let pwsh = powershell_7(ctx)?;
        let script = "$ErrorActionPreference = 'Stop'; Install-Module -Name PSReadLine -AllowPrerelease -Force -Scope CurrentUser; if (-not (Get-Module -ListAvailable -Name PSReadLine)) { throw 'PSReadLine was not installed' }";
        ctx.run(cmd!(ctx.shell, "{pwsh} -NoProfile -Command {script}"))?;
        let base = powershell(
            ctx,
            "(Get-Module -ListAvailable -Name PSReadLine | Select-Object -First 1).ModuleBase",
        )?;
        // ModuleBase is the version directory; the module directory above it holds every version.
        let module = PathBuf::from(base);
        let module = module.parent().map(PathBuf::from).unwrap_or(module);
        ctx.created(&module, false)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if !available(ctx)? {
            return Err("PSReadLine is not available".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
