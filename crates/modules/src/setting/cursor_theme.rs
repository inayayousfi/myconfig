//! The applied KDE cursor theme.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult, Setting};

pub(super) struct CursorTheme;

impl Kind for CursorTheme {
    fn describe(&self) -> String {
        "KDE cursor theme".to_owned()
    }

    /// A running Plasma session drops the user's key when it equals the global theme's
    /// default in `kdedefaults`, so the theme that applies is read there when the key is
    /// missing.
    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        match Setting::kde("kcminputrc", &["Mouse"], "cursorTheme").read(ctx)? {
            Some(theme) => Ok(Some(theme)),
            None => Setting::kde("kdedefaults/kcminputrc", &["Mouse"], "cursorTheme").read(ctx),
        }
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let size = Setting::kde("kcminputrc", &["Mouse"], "cursorSize")
            .read(ctx)?
            .unwrap_or_else(|| "24".to_owned());
        ctx.run(
            cmd!(ctx.shell, "plasma-apply-cursortheme --size {size} {value}")
                .env("QT_QPA_PLATFORM", "offscreen"),
        )
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        self.write(ctx, "breeze_cursors")
    }

    fn reapplies(&self) -> bool {
        true
    }

    /// The tool also writes the cursor size there.
    fn kde_files(&self) -> &'static [&'static str] {
        &["kcminputrc"]
    }
}
