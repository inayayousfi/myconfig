//! The T3 Code server on Windows, started at boot by the deployed `t3ctl.ps1`, with
//! Tailscale for its remote route. Install asks first.
use embedded_dotfiles::DOTFILES;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package,
    support::{powershell_7, refresh_windows_path},
};

pub struct WindowsT3Code;

const T3CTL: &str = ".local/bin/t3ctl.ps1";

const QUESTION: &str = "Install the T3 Code server, started at boot, with Tailscale, Node.js when npm is missing, and t3ctl?";

fn contents() -> &'static [u8] {
    DOTFILES.assets.windows.dotfiles.T3Code.t3ctl_ps1.content
}

impl Module for WindowsT3Code {
    fn name(&self) -> &'static str {
        "t3code"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Tailscale, Package::NodejsLts],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        if !ctx.confirm(QUESTION)? {
            ctx.note("T3 Code server not installed; the next install asks again");
            return ctx.record_declined();
        }
        ctx.record_accepted()?;
        ctx.install_packages(&[Package::Tailscale])?;
        refresh_windows_path(ctx)?;
        // nvm for Windows owns node and npm where it is installed; a second Node from
        // winget would compete with it on PATH.
        if ctx.find_program("npm").is_err() {
            ctx.install_packages(&[Package::NodejsLts])?;
            refresh_windows_path(ctx)?;
        }
        ctx.find_program("npm")
            .map_err(|_| "npm is still missing after installing Node.js")?;
        ctx.find_program("tailscale")
            .map_err(|_| "tailscale is missing after installing Tailscale")?;
        let t3ctl = ctx.home.join(T3CTL);
        ctx.write_file(&t3ctl, contents(), false)?;
        let pwsh = powershell_7(ctx)?;
        ctx.run(cmd!(
            ctx.shell,
            "{pwsh} -NoProfile -ExecutionPolicy Bypass -File {t3ctl} repair"
        ))
        .map_err(|error| format!("t3ctl could not install the T3 Code server: {error}"))?;
        ctx.note("T3 Code server installed; t3ctl remote turns its routes on");
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        if ctx.declined() {
            return Ok(());
        }
        let t3ctl = ctx.home.join(T3CTL);
        if std::fs::read(&t3ctl)? != contents() {
            return Err(format!("{} differs from the repository", t3ctl.display()).into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
