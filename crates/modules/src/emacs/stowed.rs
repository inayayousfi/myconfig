//! Linux: native Wayland Emacs with the `emacs` config package through GNU Stow.
use super::EmacsOptions;
use crate::{
    Context, Footprint, Module, ModuleResult, Package, Setting,
    deploy::verify_package,
    support::{require_file, verify_packages},
};

pub struct EmacsStowed(pub EmacsOptions);

const PACKAGES: &[Package] = &[Package::EmacsWayland, Package::Sshfs, Package::IosevkaFont];
const PRIVATE_NETWORKS: [&str; 3] = ["10.0.0.0/8", "172.16.0.0/12", "192.168.0.0/16"];

fn firewall_rule(subnet: &str) -> Setting {
    Setting::UfwRule {
        rule: [
            "allow",
            "in",
            "proto",
            "tcp",
            "from",
            subnet,
            "to",
            "any",
            "port",
            "18080,18081",
            "comment",
            "myconfig Emacs browser terminal",
        ]
        .iter()
        .map(|word| (*word).to_owned())
        .collect(),
    }
}

impl EmacsStowed {
    fn packages(&self) -> Vec<Package> {
        let mut packages = PACKAGES.to_vec();
        if self.0.browser_terminal_firewall {
            packages.push(Package::Ufw);
        }
        packages
    }
}

impl Module for EmacsStowed {
    fn name(&self) -> &'static str {
        "emacs"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: self.packages(),
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.install_packages(&self.packages())?;
        // An old ~/.emacs.d would shadow ~/.config/emacs.
        ctx.delete(&ctx.home.join(".emacs.d"))?;
        ctx.deploy_config("emacs")?;
        if self.0.browser_terminal_firewall {
            for subnet in PRIVATE_NETWORKS {
                ctx.set(firewall_rule(subnet), "present")?;
            }
            ctx.set(Setting::UfwEnabled, "active")?;
        }
        Ok(())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, &self.packages())?;
        verify_package(ctx, "emacs")?;
        let config = ctx.home.join(".config/emacs");
        require_file(&config.join("early-init.el"))?;
        require_file(&config.join("init.el"))?;
        if !config.join("lisp").is_dir() {
            return Err("Emacs Lisp modules were not installed".into());
        }
        if self.0.browser_terminal_firewall {
            for subnet in PRIVATE_NETWORKS {
                if firewall_rule(subnet).read(ctx)?.is_none() {
                    return Err(format!(
                        "the browser terminal firewall rule for {subnet} is missing"
                    )
                    .into());
                }
            }
            if Setting::UfwEnabled.read(ctx)?.as_deref() != Some("active") {
                return Err("the firewall is not active".into());
            }
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
