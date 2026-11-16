//! The Docker context that the `docker` command uses.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct DockerCurrentContext;

impl Kind for DockerCurrentContext {
    fn describe(&self) -> String {
        "current Docker context".to_owned()
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        Ok(Some(ctx.read(cmd!(ctx.shell, "docker context show"))?))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        ctx.run(cmd!(ctx.shell, "docker context use {value}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        self.write(ctx, "default")
    }
}
