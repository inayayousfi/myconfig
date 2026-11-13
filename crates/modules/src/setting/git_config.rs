//! An option in the global Git configuration.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct GitConfig<'a> {
    pub key: &'a str,
}

impl Kind for GitConfig<'_> {
    fn describe(&self) -> String {
        format!("git config --global {}", self.key)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let key = self.key;
        let (found, value) =
            ctx.read_unchecked(cmd!(ctx.shell, "git config --global --get {key}"))?;
        Ok(found.then_some(value))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let key = self.key;
        ctx.run(cmd!(ctx.shell, "git config --global {key} {value}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let key = self.key;
        ctx.run(cmd!(ctx.shell, "git config --global --unset {key}"))
    }
}
