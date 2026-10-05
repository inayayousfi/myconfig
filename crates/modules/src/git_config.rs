//! Global Git settings.
use std::path::PathBuf;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, Setting, support::require_executable,
};

pub struct GitConfig {
    /// Uses the Windows OpenSSH client named by `MYCONFIG_WINDOWS_SSH`, so Git in WSL
    /// reaches the keys held by the Windows SSH agent.
    pub windows_ssh: bool,
}

impl GitConfig {
    fn windows_ssh(&self, ctx: &Context) -> Option<PathBuf> {
        self.windows_ssh
            .then(|| ctx.shell.var_os("MYCONFIG_WINDOWS_SSH"))
            .flatten()
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    }

    fn settings(&self, ctx: &Context) -> ModuleResult<Vec<(Setting, String)>> {
        let mut settings = vec![(
            Setting::GitConfig {
                key: "core.symlinks".to_owned(),
            },
            "true".to_owned(),
        )];
        if let Some(ssh) = self.windows_ssh(ctx) {
            require_executable(&ssh)?;
            settings.push((
                Setting::GitConfig {
                    key: "core.sshCommand".to_owned(),
                },
                ssh.to_string_lossy().into_owned(),
            ));
        }
        Ok(settings)
    }
}

impl Module for GitConfig {
    fn name(&self) -> &'static str {
        "git-config"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Git],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        for (setting, value) in self.settings(ctx)? {
            ctx.set(setting, &value)?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        for (setting, value) in self.settings(ctx)? {
            if setting.read(ctx)?.as_deref() != Some(value.as_str()) {
                return Err(format!("{} is not {value}", setting.describe()).into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
