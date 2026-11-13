//! A VSCodium extension, by its identifier, whose value is `installed`.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct VscodiumExtension<'a> {
    pub id: &'a str,
}

impl Kind for VscodiumExtension<'_> {
    fn describe(&self) -> String {
        format!("VSCodium extension {}", self.id)
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        // Extension identifiers ignore case.
        Ok(ctx
            .read(cmd!(ctx.shell, "codium --list-extensions"))?
            .lines()
            .any(|installed| installed.trim().eq_ignore_ascii_case(self.id))
            .then(|| "installed".to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let id = self.id;
        ctx.run(cmd!(ctx.shell, "codium --install-extension {id}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let id = self.id;
        ctx.run(cmd!(ctx.shell, "codium --uninstall-extension {id}"))
    }
}
