use std::{error::Error, path::PathBuf};

use myconfig_modules::{
    ModuleContext, Profile,
    linux::{
        AgentsConfigureModule, AgentsPackagesModule, ArchWslAgentsConfigure, ArchWslAgentsPackages,
        ArchWslAuthentication, ArchWslBase, ArchWslCli, ArchWslDotfiles,
        ArchWslEnvironmentInventory, ArchWslRuntimes, ArchWslSsh, ArchWslTailscale,
        ArchWslTerminalTools, ArchWslZsh, AuthenticationModule, BaseModule, CliModule,
        DotfilesModule, EnvironmentInventoryModule, RuntimesModule, SshModule, TailscaleModule,
        TerminalToolsModule, ZshModule,
    },
};
use myconfig_utils::{LinuxSession, PackageSystem};
use xshell::Shell;

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::consts::OS != "linux" {
        return Err("the Arch WSL installer requires Linux".into());
    }
    let wsl_kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if std::env::var_os("WSL_DISTRO_NAME").is_none()
        && !wsl_kernel.contains("microsoft")
        && !wsl_kernel.contains("wsl")
    {
        return Err("the Arch WSL installer requires WSL".into());
    }

    let sh = Shell::new()?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?);
    let _session = LinuxSession::prepare(&sh, PackageSystem::Arch)?;
    let context = ModuleContext {
        profile: Profile::ArchWsl,
        package_system: PackageSystem::Arch,
        shell: &sh,
        home: &home,
    };

    ArchWslBase.install(&context)?;
    ArchWslSsh.install(&context)?;
    ArchWslCli.install(&context)?;
    ArchWslRuntimes.install(&context)?;
    ArchWslZsh.install(&context)?;
    ArchWslTerminalTools.install(&context)?;
    ArchWslTailscale.install(&context)?;
    ArchWslAgentsPackages.install(&context)?;
    ArchWslDotfiles.install(&context)?;
    ArchWslAgentsConfigure.install(&context)?;
    ArchWslAuthentication.install(&context)?;
    ArchWslEnvironmentInventory.install(&context)?;
    println!("Arch WSL profile completed successfully");
    Ok(())
}
