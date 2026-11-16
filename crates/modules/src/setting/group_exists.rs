//! A system group.
use xshell::cmd;

use super::{Kind, PRESENT};
use crate::{Context, ModuleResult};

pub(super) struct GroupExists<'a> {
    pub group: &'a str,
}

impl Kind for GroupExists<'_> {
    fn describe(&self) -> String {
        format!("group {}", self.group)
    }

    fn presence(&self) -> bool {
        true
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let group = self.group;
        Ok(ctx
            .succeeds(cmd!(ctx.shell, "getent group {group}"))?
            .then(|| PRESENT.to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let group = self.group;
        ctx.run(cmd!(ctx.shell, "sudo groupadd --system --force {group}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let group = self.group;
        ctx.run(cmd!(ctx.shell, "sudo groupdel {group}"))
    }
}
