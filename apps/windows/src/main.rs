use std::{error::Error, path::PathBuf};

use myconfig_modules::{
    ModuleContext, Profile,
    windows::{
        AhkScriptsModule, AiConfigModule, EmacsConfigModule, IosevkaMonoFontModule, LlvmPathModule,
        OhMyPoshConfigModule, PowerShellProfileModule, PsReadLineModule, RegistryTweaksModule,
        SharedDesktopModule, TaskbarAutoHideModule, WindowsAhkScripts, WindowsAiConfig,
        WindowsEmacsConfig, WindowsIosevkaMonoFont, WindowsLlvmPath, WindowsOhMyPoshConfig,
        WindowsPowerShellProfile, WindowsPsReadLine, WindowsRegistryTweaks, WindowsSharedDesktop,
        WindowsTaskbarAutoHide, WindowsTerminalConfig, WindowsTerminalConfigModule,
        WindowsWingetPackages, WingetPackagesModule,
    },
};
use myconfig_utils::PackageSystem;
use xshell::Shell;

fn main() -> Result<(), Box<dyn Error>> {
    if !cfg!(windows) {
        return Err("the Windows Workstation installer requires Windows".into());
    }
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.is_empty() && args.as_slice() != ["--move-shared-desktop"] {
        return Err("unknown Windows installer argument".into());
    }
    let sh = Shell::new()?;
    let home = PathBuf::from(std::env::var_os("USERPROFILE").ok_or("USERPROFILE is unset")?);
    let context = ModuleContext {
        profile: Profile::Windows,
        package_system: PackageSystem::Winget,
        shell: &sh,
        home: &home,
    };
    if !args.is_empty() {
        return WindowsSharedDesktop.install(&context);
    }

    let selected = WindowsWingetPackages.install(&context)?;
    WindowsPowerShellProfile.install(&context)?;
    WindowsOhMyPoshConfig.install(&context)?;
    WindowsTerminalConfig.install(&context)?;
    WindowsEmacsConfig.install(&context)?;
    WindowsAhkScripts.install(&context)?;
    WindowsAiConfig.install(&context)?;
    WindowsPsReadLine.install(&context)?;
    WindowsIosevkaMonoFont.install(&context)?;
    if selected.dev_tools {
        WindowsLlvmPath.install(&context)?;
    }
    WindowsRegistryTweaks.install(&context)?;
    WindowsTaskbarAutoHide.install(&context)?;
    WindowsSharedDesktop.install(&context)?;
    println!("Windows Workstation profile completed successfully");
    Ok(())
}
