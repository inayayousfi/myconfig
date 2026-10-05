//! The OpenSSH server, listening on every address for the current user only.
use std::path::{Path, PathBuf};

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope,
    support::{current_user, verify_packages, with_temporary_directory},
};

pub struct Ssh;

const CONFIG: &str = "/etc/ssh/sshd_config.d/10-myconfig.conf";
/// Written by an earlier version of this setup.
const LEGACY: &str = "/etc/ssh/sshd_config.d/10-local-only.conf";

fn config(user: &str) -> String {
    format!("ListenAddress 0.0.0.0\nListenAddress ::\nAllowUsers {user}\n")
}

fn host_keys(ctx: &Context) -> ModuleResult<Vec<PathBuf>> {
    let listing = ctx.read(cmd!(
        ctx.shell,
        "sudo find /etc/ssh -maxdepth 1 -name ssh_host_*"
    ))?;
    Ok(listing.lines().map(PathBuf::from).collect())
}

impl Module for Ssh {
    fn name(&self) -> &'static str {
        "ssh"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Openssh],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        if !Path::new("/run/systemd/system").is_dir() {
            return Err("systemd is not running".into());
        }
        let sh = ctx.shell;
        ctx.install_packages(&[Package::Openssh])?;
        let user = current_user(ctx)?;
        let contents = config(&user);

        let keys_before = host_keys(ctx)?;
        ctx.run(cmd!(sh, "sudo ssh-keygen -A"))?;
        for key in host_keys(ctx)? {
            if !keys_before.contains(&key) {
                ctx.created(&key, true)?;
            }
        }
        with_temporary_directory("sshd", |directory| {
            let proposed = directory.join("10-myconfig.conf");
            std::fs::write(&proposed, &contents)?;
            ctx.run(cmd!(sh, "sudo sshd -t -f {proposed}"))
        })?;

        let previous_config = ctx.read_unchecked(cmd!(sh, "sudo cat {CONFIG}"))?;
        let previous_legacy = ctx.read_unchecked(cmd!(sh, "sudo cat {LEGACY}"))?;
        let applied = (|| -> ModuleResult {
            ctx.write_system_file(Path::new(CONFIG), contents.as_bytes(), "0644")?;
            ctx.delete_system_file(Path::new(LEGACY))?;
            ctx.run(cmd!(sh, "sudo sshd -t"))?;
            ctx.enable_service("sshd.service", ServiceScope::System)?;
            ctx.run(cmd!(sh, "sudo systemctl restart sshd.service"))
        })();
        let Err(failure) = applied else {
            return Ok(());
        };
        // Put the working configuration back before reporting, so SSH access survives.
        let restored = (|| -> ModuleResult {
            for (path, (existed, previous)) in
                [(CONFIG, previous_config), (LEGACY, previous_legacy)]
            {
                if existed {
                    ctx.run_with_input(
                        cmd!(sh, "sudo tee {path}").ignore_stdout(),
                        format!("{previous}\n").as_bytes(),
                    )?;
                } else {
                    ctx.run(cmd!(sh, "sudo rm -f -- {path}"))?;
                }
            }
            ctx.run(cmd!(sh, "sudo sshd -t"))?;
            ctx.run(cmd!(sh, "sudo systemctl restart sshd.service"))
        })();
        Err(match restored {
            Ok(()) => format!("OpenSSH setup failed, previous configuration restored: {failure}"),
            Err(error) => format!(
                "OpenSSH setup failed: {failure}; previous configuration could not be restored: {error}"
            ),
        }
        .into())
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        verify_packages(ctx, &[Package::Openssh])?;
        let user = current_user(ctx)?;
        let installed = ctx.read(cmd!(sh, "sudo cat {CONFIG}"))?;
        if format!("{installed}\n") != config(&user) {
            return Err(format!("{CONFIG} differs from the expected configuration").into());
        }
        ctx.run(cmd!(sh, "sudo sshd -t"))?;
        let status = ctx.read(cmd!(sh, "systemctl is-active sshd.service"))?;
        if status != "active" {
            return Err(format!("sshd.service is not active: {status}").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
