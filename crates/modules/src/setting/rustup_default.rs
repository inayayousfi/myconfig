//! The default Rust toolchain of rustup.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct RustupDefault;

impl Kind for RustupDefault {
    fn describe(&self) -> String {
        "default Rust toolchain".to_owned()
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        let (configured, output) = ctx.read_unchecked(cmd!(ctx.shell, "rustup default"))?;
        Ok(output
            .split_whitespace()
            .next()
            .filter(|_| configured && !output.contains("no default"))
            .map(str::to_owned))
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        ctx.run(cmd!(ctx.shell, "rustup default {value}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        ctx.run(cmd!(ctx.shell, "rustup default none"))
    }
}
