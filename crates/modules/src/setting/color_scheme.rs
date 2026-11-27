//! The applied KDE color scheme, by the name of its `.colors` file.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult, Setting};

pub(super) struct ColorScheme;

impl Kind for ColorScheme {
    fn describe(&self) -> String {
        "KDE color scheme".to_owned()
    }

    /// A running Plasma session drops the user's key when it equals the global theme's
    /// default in `kdedefaults`, so the scheme that applies is read there when the key is
    /// missing.
    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        match Setting::kde("kdeglobals", &["General"], "ColorScheme").read(ctx)? {
            Some(scheme) => Ok(Some(scheme)),
            None => Setting::kde("kdedefaults/kdeglobals", &["General"], "ColorScheme").read(ctx),
        }
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        ctx.run(
            cmd!(ctx.shell, "plasma-apply-colorscheme {value}").env("QT_QPA_PLATFORM", "offscreen"),
        )
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        self.write(ctx, "BreezeDark")
    }

    fn reapplies(&self) -> bool {
        true
    }

    /// The tool copies every color of the scheme into the same file.
    fn kde_files(&self) -> &'static [&'static str] {
        &["kdeglobals"]
    }
}
