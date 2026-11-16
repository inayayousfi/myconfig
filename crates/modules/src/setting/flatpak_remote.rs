//! A system-wide Flatpak remote, added from its `.flatpakrepo` file.
use xshell::cmd;

use super::{Kind, PRESENT};
use crate::{Context, ModuleResult};

pub(super) struct FlatpakRemote<'a> {
    pub name: &'a str,
    pub from: &'a str,
}

impl Kind for FlatpakRemote<'_> {
    fn describe(&self) -> String {
        format!("Flatpak remote {}", self.name)
    }

    fn presence(&self) -> bool {
        true
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        Ok(ctx
            .read(cmd!(ctx.shell, "flatpak remotes --system --columns=name"))?
            .lines()
            .any(|remote| remote.trim() == self.name)
            .then(|| PRESENT.to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let Self { name, from } = self;
        ctx.run(cmd!(
            ctx.shell,
            "sudo flatpak remote-add --system --if-not-exists {name} {from}"
        ))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let name = self.name;
        ctx.run(cmd!(
            ctx.shell,
            "sudo flatpak remote-delete --system {name}"
        ))
    }
}
