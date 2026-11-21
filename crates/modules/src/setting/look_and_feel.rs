//! The applied KDE Plasma global theme.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult, Setting};

/// The files where `plasma-apply-lookandfeel` writes its color scheme, icons, cursor,
/// widget style, window decoration, Plasma style and splash screen.
const FILES: [&str; 5] = [
    "kdeglobals",
    "kcminputrc",
    "kwinrc",
    "plasmarc",
    "ksplashrc",
];

pub(super) struct LookAndFeel;

impl Kind for LookAndFeel {
    fn describe(&self) -> String {
        "KDE global theme".to_owned()
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        Setting::kde("kdeglobals", &["KDE"], "LookAndFeelPackage").read(ctx)
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        ctx.run(
            cmd!(ctx.shell, "plasma-apply-lookandfeel --apply {value}")
                .env("QT_QPA_PLATFORM", "offscreen")
                .env("XDG_CONFIG_DIRS", super::kde_config_dirs(ctx)),
        )
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        self.write(ctx, "org.kde.breeze.desktop")
    }

    fn reapplies(&self) -> bool {
        true
    }

    fn kde_files(&self) -> &'static [&'static str] {
        &FILES
    }
}
