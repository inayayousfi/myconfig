//! Rootless Docker, whose daemon runs as the user instead of root.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope, Setting,
    support::{current_user, verify_packages},
};

pub struct Docker {
    /// Creates the user service with Docker's setup tool, for packages that ship the
    /// tool instead of the unit.
    pub setup_tool: bool,
}

const PACKAGES: &[Package] = &[
    Package::Docker,
    Package::DockerBuildx,
    Package::DockerCompose,
    Package::DockerRootlessExtras,
    Package::Slirp4netns,
    Package::Uidmap,
];
const CONTEXT: &str = "rootless";

fn context() -> Setting {
    Setting::DockerContext {
        name: CONTEXT.to_owned(),
    }
}

/// Docker's setup tool copies the PATH it runs with into the unit, so it runs with the
/// system PATH only.
fn create_user_unit(ctx: &Context) -> ModuleResult {
    let unit = ctx.home.join(".config/systemd/user/docker.service");
    if unit.is_file() {
        return Ok(());
    }
    let tool = ctx.find_program("dockerd-rootless-setuptool.sh")?;
    ctx.created(&unit, false)?;
    ctx.run(cmd!(ctx.shell, "{tool} install").env(
        "PATH",
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
    ))
    .map_err(|error| format!("Docker's rootless setup tool failed: {error}").into())
}

impl Module for Docker {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: vec![context(), Setting::DockerCurrentContext],
        }
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        let sh = ctx.shell;
        let user = current_user(ctx)?;
        // Membership in the docker group gives root access, so this setup refuses it.
        for groups in [
            ctx.read(cmd!(sh, "id -Gn"))?,
            ctx.read(cmd!(sh, "id -nG {user}"))?,
        ] {
            if groups.split_whitespace().any(|group| group == "docker") {
                return Err(
                    "current user is already in the docker group; refusing Docker setup".into(),
                );
            }
        }
        ctx.install_packages(PACKAGES)?;
        ctx.run(cmd!(
            sh,
            "sudo systemctl disable --now docker.service docker.socket"
        ))?;
        if self.setup_tool {
            create_user_unit(ctx)?;
        }
        ctx.run(cmd!(sh, "systemctl --user daemon-reload"))?;
        ctx.enable_service("docker.service", ServiceScope::User)?;
        ctx.run(cmd!(sh, "systemctl --user start docker.service"))?;
        let status = ctx.read(cmd!(sh, "systemctl --user is-active docker.service"))?;
        if status != "active" {
            return Err("the rootless docker.service user unit is not active".into());
        }
        let uid = ctx.read(cmd!(sh, "id -u"))?;
        ctx.set(context(), &format!("unix:///run/user/{uid}/docker.sock"))?;
        ctx.set(Setting::DockerCurrentContext, CONTEXT)?;
        self.verify(ctx)
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        let format = "{{.SecurityOptions}}";
        let options = ctx.read(cmd!(ctx.shell, "docker info --format {format}"))?;
        if !options.contains("name=rootless") {
            return Err("the selected Docker daemon is not rootless".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
