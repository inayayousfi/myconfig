//! Docker with its service, for a user outside the docker group.
use xshell::cmd;

use crate::{
    Context, Footprint, Module, ModuleResult, Package, ServiceScope,
    support::{current_user, verify_packages},
};

pub struct Docker;

const PACKAGES: &[Package] = &[
    Package::Docker,
    Package::DockerBuildx,
    Package::DockerCompose,
];

impl Module for Docker {
    fn name(&self) -> &'static str {
        "docker"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint {
            packages: PACKAGES.to_vec(),
            settings: Vec::new(),
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
        ctx.enable_service("docker.service", ServiceScope::System)?;
        ctx.run(cmd!(sh, "sudo systemctl start docker.service"))
    }

    fn verify(&self, ctx: &Context) -> ModuleResult {
        verify_packages(ctx, PACKAGES)?;
        let status = ctx.read(cmd!(ctx.shell, "systemctl is-active docker.service"))?;
        if status != "active" {
            return Err(format!("docker.service is not active: {status}").into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
