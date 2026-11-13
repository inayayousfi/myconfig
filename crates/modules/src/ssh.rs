//! The OpenSSH server configuration, listening on every address for the current user only.
use std::path::{Path, PathBuf};

use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope,
    support::{current_user, verify_packages, with_temporary_directory},
};

/// Writes the server configuration without starting or stopping the server.
pub struct Ssh;

/// Restarts a running server with the new configuration, and offers to turn on a server
/// that is off. It comes late in a profile, so its question waits until the rest is done.
pub struct SshServer;

const CONFIG: &str = "/etc/ssh/sshd_config.d/10-myconfig.conf";
const SERVICE: &str = "sshd.service";
const START_COMMAND: &str = "sudo systemctl enable --now sshd.service";

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

        let (existed, previous) = ctx.read_unchecked(cmd!(sh, "sudo cat {CONFIG}"))?;
        ctx.write_system_file(Path::new(CONFIG), contents.as_bytes(), "0644")?;
        let Err(failure) = ctx.run(cmd!(sh, "sudo sshd -t")) else {
            return Ok(());
        };
        // Put the working configuration back before reporting, so SSH access survives.
        let restored = if existed {
            ctx.run_with_input(
                cmd!(sh, "sudo tee {CONFIG}").ignore_stdout(),
                format!("{previous}\n").as_bytes(),
            )
        } else {
            ctx.run(cmd!(sh, "sudo rm -f -- {CONFIG}"))
        };
        Err(match restored {
            Ok(()) => format!(
                "OpenSSH rejected its new configuration; the previous one was restored: {failure}"
            ),
            Err(error) => format!(
                "OpenSSH rejected its new configuration: {failure}; the previous one could not be restored: {error}"
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
        ctx.run(cmd!(sh, "sudo sshd -t"))
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}

impl Module for SshServer {
    fn name(&self) -> &'static str {
        "ssh-server"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: vec![Package::Openssh],
            settings: Vec::new(),
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        // Another program on port 22, such as a second WSL distribution sharing the same
        // network, must not stop the rest of the install, so a failed start only warns.
        if ctx.succeeds(cmd!(sh, "systemctl is-active --quiet {SERVICE}"))? {
            ctx.note("Restarting the OpenSSH server with its new configuration");
            if ctx
                .run(cmd!(sh, "sudo systemctl restart {SERVICE}"))
                .is_err()
            {
                ctx.note(
                    "WARNING: the OpenSSH server did not restart. See: journalctl -u sshd.service",
                );
            }
            return Ok(());
        }
        match ctx.confirm("The OpenSSH server is off. Turn it on now?") {
            Ok(true) => {}
            Ok(false) | Err(_) => {
                ctx.note(&format!("The OpenSSH server is off. Run: {START_COMMAND}"));
                return Ok(());
            }
        }
        ctx.enable_service(SERVICE, ServiceScope::System)?;
        if ctx.run(cmd!(sh, "sudo systemctl start {SERVICE}")).is_err() {
            ctx.note("WARNING: the OpenSSH server did not start. See: journalctl -u sshd.service");
            ctx.note(&format!("Retry with: {START_COMMAND}"));
        }
        Ok(())
    }

    fn verify(&self, _ctx: &Context) -> ModuleResult {
        // The server may stay off by choice, and a failed start is only a warning.
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
