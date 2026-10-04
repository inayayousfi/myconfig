use std::{error::Error, path::PathBuf};

use myconfig_modules::{
    ModuleContext, Profile,
    linux::{
        AgentsConfigureModule, AgentsPackagesModule, AndroidPhoneModule, AuthenticationModule,
        AxidevOskModule, BaseModule, CachyosAgentsConfigure, CachyosAgentsPackages,
        CachyosAndroidPhone, CachyosAuthentication, CachyosAxidevOsk, CachyosBase, CachyosCli,
        CachyosCursorTheme, CachyosDocker, CachyosDotfiles, CachyosEmacs,
        CachyosEnvironmentInventory, CachyosGhostty, CachyosHandy, CachyosKanata, CachyosKanataKde,
        CachyosKdePlasma, CachyosKdePlasmaValidate, CachyosModule, CachyosPipewire, CachyosRefind,
        CachyosRuntimes, CachyosSetup, CachyosSsh, CachyosTailscale, CachyosTerminalTools,
        CachyosZsh, CliModule, CursorThemeModule, DockerModule, DotfilesModule, EmacsModule,
        EnvironmentInventoryModule, GhosttyModule, HandyModule, KanataKdeModule, KanataModule,
        KdePlasmaModule, KdePlasmaValidateModule, PipewireModule, RefindModule, RuntimesModule,
        SshModule, TailscaleModule, TerminalToolsModule, ZshModule,
    },
};
use myconfig_utils::{LinuxSession, PackageSystem};
use xshell::Shell;

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::consts::OS != "linux" {
        return Err("the CachyOS installer requires Linux".into());
    }
    let sh = Shell::new()?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?);
    let context = ModuleContext {
        profile: Profile::Cachyos,
        package_system: PackageSystem::Arch,
        shell: &sh,
        home: &home,
    };
    let _session = LinuxSession::prepare(&sh, PackageSystem::Arch)?;
    CachyosKdePlasmaValidate.install(&context)?;

    CachyosBase.install(&context)?;
    CachyosSetup.install(&context)?;
    CachyosSsh.install(&context)?;
    CachyosCli.install(&context)?;
    CachyosRuntimes.install(&context)?;
    CachyosZsh.install(&context)?;
    CachyosTerminalTools.install(&context)?;
    CachyosGhostty.install(&context)?;
    CachyosAxidevOsk.install(&context)?;
    CachyosTailscale.install(&context)?;
    CachyosAgentsPackages.install(&context)?;
    CachyosDotfiles.install(&context)?;
    CachyosAndroidPhone.install(&context)?;
    CachyosEmacs.install(&context)?;
    CachyosCursorTheme.install(&context)?;
    CachyosRefind.install(&context)?;
    CachyosKanata.install(&context)?;
    CachyosKdePlasma.install(&context)?;
    CachyosKanataKde.install(&context)?;
    CachyosHandy.install(&context)?;
    CachyosPipewire.install(&context)?;
    CachyosDocker.install(&context)?;
    CachyosAgentsConfigure.install(&context)?;
    CachyosAuthentication.install(&context)?;
    CachyosEnvironmentInventory.install(&context)?;
    println!("CachyOS profile completed successfully");
    Ok(())
}
