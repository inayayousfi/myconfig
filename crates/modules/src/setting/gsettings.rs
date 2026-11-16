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

    /// Reads the value the user stored, or `None` when the key holds its default.
    /// `gsettings get` cannot tell the two apart, so this reads the dconf database.
    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let Self { schema, key } = self;
        let schemas = ctx.read(cmd!(ctx.shell, "gsettings list-schemas --print-paths"))?;
        let path = schemas
            .lines()
            .find_map(|line| {
                line.split_once(' ')
                    .filter(|(name, _)| name == schema)
                    .map(|(_, path)| path.trim().to_owned())
            })
            .ok_or_else(|| format!("GSettings schema {schema} has no fixed path"))?;
        let value = ctx.read(cmd!(ctx.shell, "dconf read {path}{key}"))?;
        Ok((!value.is_empty()).then_some(value))
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
