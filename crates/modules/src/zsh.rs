//! Zsh with Oh My Zsh, its plugins, and the shared Zsh config.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, Setting,
    deploy::verify_package,
    support::{current_user, verify_packages},
};

pub struct Zsh {
    /// Arch WSL leaves the login shell to the Windows side, which sets it when it
    /// creates the distribution.
    pub set_login_shell: bool,
}

const PLUGINS: [(&str, &str); 2] = [
    (
        "zsh-autosuggestions",
        "https://github.com/zsh-users/zsh-autosuggestions",
    ),
    (
        "zsh-syntax-highlighting",
        "https://github.com/zsh-users/zsh-syntax-highlighting.git",
    ),
];

impl Zsh {
    pub fn plugin_directory(&self, ctx: &Context) -> std::path::PathBuf {
        ctx.shell
            .var_os("ZSH_CUSTOM")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| ctx.home.join(".oh-my-zsh/custom"))
            .join("plugins")
    }
}

impl Module for Zsh {
    fn name(&self) -> &'static str {
        "zsh"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Zsh, Package::Git, Package::Curl],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        ctx.install_packages(&[Package::Zsh])?;

        let oh_my_zsh = ctx.home.join(".oh-my-zsh");
        if !oh_my_zsh.is_dir() {
            let url = "https://raw.githubusercontent.com/ohmyzsh/ohmyzsh/master/tools/install.sh";
            let script = ctx.read_bytes(cmd!(sh, "curl -fsSL {url}"))?;
            ctx.created(&oh_my_zsh, false)?;
            ctx.run_with_input(
                cmd!(sh, "sh -s")
                    .env("HOME", ctx.home)
                    .env("RUNZSH", "no")
                    .env("CHSH", "no")
                    .env("KEEP_ZSHRC", "yes"),
                &script,
            )?;
        }

        let plugins = self.plugin_directory(ctx);
        for (name, url) in PLUGINS {
            let destination = plugins.join(name);
            if !destination.is_dir() {
                ctx.created(&destination, false)?;
                ctx.run(cmd!(sh, "git clone {url} {destination}"))?;
            }
        }

        if self.set_login_shell {
            let zsh = ctx.find_program("zsh")?;
            ctx.set(
                Setting::LoginShell {
                    user: current_user(ctx)?,
                },
                &zsh.to_string_lossy(),
            )?;
        }
        ctx.deploy_config("zsh")
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &[Package::Zsh])?;
        if !ctx.home.join(".oh-my-zsh").is_dir() {
            return Err("Oh My Zsh is not installed".into());
        }
        let plugins = self.plugin_directory(ctx);
        for (name, _) in PLUGINS {
            if !plugins.join(name).is_dir() {
                return Err(format!("Zsh plugin {name} is not installed").into());
            }
        }
        if self.set_login_shell {
            let zsh = ctx.find_program("zsh")?;
            let shell = Setting::LoginShell {
                user: current_user(ctx)?,
            }
            .read(ctx)?;
            if shell.as_deref() != Some(&*zsh.to_string_lossy()) {
                return Err("zsh is not the login shell".into());
            }
        }
        verify_package(ctx, "zsh")
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
