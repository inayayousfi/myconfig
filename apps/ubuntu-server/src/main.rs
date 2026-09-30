use std::{error::Error, path::PathBuf};

use myconfig_modules::{
    ModuleContext, Profile,
    linux::{
        BaseModule, DotfilesModule, UbuntuServerBase, UbuntuServerDotfiles, UbuntuServerZsh,
        ZshModule,
    },
};
use myconfig_utils::{LinuxSession, PackageSystem};
use xshell::Shell;

fn main() -> Result<(), Box<dyn Error>> {
    if std::env::consts::OS != "linux" {
        return Err("the Ubuntu Server installer requires Linux".into());
    }

    let sh = Shell::new()?;
    let home = PathBuf::from(std::env::var_os("HOME").ok_or("HOME is unset")?);
    let _session = LinuxSession::prepare(&sh, PackageSystem::Apt)?;

    let context = ModuleContext {
        profile: Profile::UbuntuServer,
        package_system: PackageSystem::Apt,
        shell: &sh,
        home: &home,
    };
    UbuntuServerBase.install(&context)?;
    UbuntuServerZsh.install(&context)?;
    UbuntuServerDotfiles.install(&context)?;
    println!("Ubuntu Server profile completed successfully");
    Ok(())
}
