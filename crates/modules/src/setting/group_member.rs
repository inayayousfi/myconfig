//! A user's membership in a group.
use xshell::cmd;

use super::{Kind, PRESENT};
use crate::{Context, ModuleResult};

pub(super) struct GroupMember<'a> {
    pub group: &'a str,
    pub user: &'a str,
}

impl Kind for GroupMember<'_> {
    fn describe(&self) -> String {
        format!("{} in group {}", self.user, self.group)
    }

    fn presence(&self) -> bool {
        true
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let Self { group, user } = *self;
        let groups = ctx.read(cmd!(ctx.shell, "id -nG {user}"))?;
        Ok(groups
            .split_whitespace()
            .any(|name| name == group)
            .then(|| PRESENT.to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let Self { group, user } = self;
        ctx.run(cmd!(ctx.shell, "sudo usermod -aG {group} {user}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let Self { group, user } = self;
        ctx.run(cmd!(ctx.shell, "sudo gpasswd -d {user} {group}"))
    }
}
