//! The PowerShell 7 profile that loads Oh My Posh.
use std::path::PathBuf;

use embedded_dotfiles::WINDOWS_ASSETS;
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Need, Package, Setting,
    support::{powershell, powershell_7},
};

pub struct PowerShellProfile;

/// The profile paths of PowerShell 7 and of Windows PowerShell 5.1, which differ.
pub(crate) fn profile_paths(ctx: &Context) -> ModuleResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let pwsh7 = powershell(ctx, "$PROFILE")?;
    if pwsh7.is_empty() {
        return Err("PowerShell 7 did not return its profile path".into());
    }
    paths.push(PathBuf::from(pwsh7));
    let windows_powershell = ctx.read(cmd!(
        ctx.shell,
        "powershell.exe -NoProfile -Command $PROFILE"
    ))?;
    if windows_powershell.is_empty() {
        return Err("Windows PowerShell did not return its profile path".into());
    }
    paths.push(PathBuf::from(windows_powershell));
    Ok(paths)
}

fn contents() -> &'static [u8] {
    WINDOWS_ASSETS
        .dotfiles
        .PowerShell
        .Microsoft_PowerShell_profile_ps1
        .content
}

/// Defender's Controlled Folder Access blocks unknown programs from writing to Documents,
/// where PowerShell keeps its profile. This program writes the profile, so it goes on the
/// allowed-apps list.
fn allow_through_controlled_folder_access(ctx: &Context) -> ModuleResult {
    let pwsh = powershell_7(ctx)?;
    let writer = std::env::current_exe()?;
    let check = "$p = Get-MpPreference -ErrorAction SilentlyContinue; if (-not $p -or $p.EnableControlledFolderAccess -ne 1) { 'off' } elseif ($p.ControlledFolderAccessAllowedApplications -contains $env:INAYOS_APP) { 'allowed' } else { 'blocked' }";
    let state =
        ctx.read(cmd!(ctx.shell, "{pwsh} -NoProfile -Command {check}").env("INAYOS_APP", &writer))?;
    if state != "blocked" {
        return Ok(());
    }
    ctx.note(&format!(
        "Controlled Folder Access is on; allowing {} to write to protected folders",
        writer.display()
    ));
    let app = Setting::ControlledFolderAccessApp {
        path: writer.display().to_string(),
    };
    ctx.add(app).map_err(|error| {
        format!(
            "could not allow {} through Controlled Folder Access; it needs administrator privileges, and Defender may be managed by company policy: {error}",
            writer.display()
        )
        .into()
    })
}

impl Module for PowerShellProfile {
    fn name(&self) -> &'static str {
        "powershell-profile"
    }

    fn needs(&self) -> &'static [Need] {
        &[]
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Powershell, Package::OhMyPosh],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        allow_through_controlled_folder_access(ctx)?;
        for profile in profile_paths(ctx)? {
            if std::fs::symlink_metadata(&profile)
                .is_ok_and(|metadata| !metadata.is_file() || metadata.file_type().is_symlink())
            {
                return Err(format!(
                    "PowerShell profile is not a regular file: {}",
                    profile.display()
                )
                .into());
            }
            ctx.write_file(&profile, contents(), false)?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for profile in profile_paths(ctx)? {
            if std::fs::read(&profile)? != contents() {
                return Err(format!("{} differs from the repository", profile.display()).into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
