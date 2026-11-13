//! A GSettings key.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct Gsettings<'a> {
    pub schema: &'a str,
    pub key: &'a str,
}

impl Kind for Gsettings<'_> {
    fn describe(&self) -> String {
        format!("gsettings {} {}", self.schema, self.key)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let Self { schema, key } = self;
        Ok(Some(
            ctx.read(cmd!(ctx.shell, "gsettings get {schema} {key}"))?,
        ))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let Self { schema, key } = self;
        ctx.run(cmd!(ctx.shell, "gsettings set {schema} {key} {value}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let Self { schema, key } = self;
        ctx.run(cmd!(ctx.shell, "gsettings reset {schema} {key}"))
    }
}
