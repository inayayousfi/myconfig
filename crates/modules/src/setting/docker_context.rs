//! A Docker context, whose value is its Docker endpoint.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct DockerContext<'a> {
    pub name: &'a str,
}

impl Kind for DockerContext<'_> {
    fn describe(&self) -> String {
        format!("Docker context {}", self.name)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let name = self.name;
        let format = "{{.Endpoints.docker.Host}}";
        let (found, host) = ctx.read_unchecked(cmd!(
            ctx.shell,
            "docker context inspect {name} --format {format}"
        ))?;
        Ok(found.then_some(host))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        if self.read(ctx)?.is_some() {
            self.delete(ctx)?;
        }
        let name = self.name;
        let endpoint = format!("host={value}");
        ctx.run(cmd!(
            ctx.shell,
            "docker context create {name} --docker {endpoint}"
        ))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let name = self.name;
        ctx.run(cmd!(ctx.shell, "docker context rm --force {name}"))
    }
}
