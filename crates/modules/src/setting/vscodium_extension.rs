//! An installed extension of VSCodium or VS Code, by its identifier.
use xshell::cmd;

use super::{Kind, PRESENT};
use crate::{Context, ModuleResult};

pub(super) struct VscodiumExtension<'a> {
    pub id: &'a str,
    /// The editor's command, `codium` or `code`.
    pub editor: &'a str,
}

impl Kind for VscodiumExtension<'_> {
    fn describe(&self) -> String {
        format!("{} extension {}", self.editor, self.id)
    }

    fn presence(&self) -> bool {
        true
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        // An editor that is gone has no extension left to remove.
        let Ok(editor) = ctx.find_program(self.editor) else {
            return Ok(None);
        };
        // Extension identifiers ignore case.
        Ok(ctx
            .read(cmd!(ctx.shell, "{editor} --list-extensions"))?
            .lines()
            .any(|installed| installed.trim().eq_ignore_ascii_case(self.id))
            .then(|| PRESENT.to_owned()))
    }

    fn write(&self, ctx: &Context, _value: &str) -> ModuleResult {
        let editor = ctx.find_program(self.editor)?;
        let id = self.id;
        ctx.run(cmd!(ctx.shell, "{editor} --install-extension {id}"))
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        let editor = ctx.find_program(self.editor)?;
        let id = self.id;
        ctx.run(cmd!(ctx.shell, "{editor} --uninstall-extension {id}"))
    }
}
