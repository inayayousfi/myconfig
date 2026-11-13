//! The KDE accent color, as `red,green,blue`.
use xshell::cmd;

use super::Kind;
use crate::{Context, ModuleResult, Setting};

/// Plasma's tool derives the colors from the accent but leaves the accent setting that
/// System Settings shows unset, so these keys record it.
const KEYS: [&str; 2] = ["AccentColor", "LastUsedCustomAccentColor"];

pub(super) struct AccentColor;

impl Kind for AccentColor {
    fn describe(&self) -> String {
        "KDE accent color".to_owned()
    }

    fn read(&self, ctx: &Context) -> ModuleResult<Option<String>> {
        Setting::kde("kdeglobals", &["General"], "AccentColor").read(ctx)
    }

    fn write(&self, ctx: &Context, value: &str) -> ModuleResult {
        let hex = value
            .split(',')
            .map(|part| part.trim().parse::<u8>().map(|byte| format!("{byte:02x}")))
            .collect::<Result<String, _>>()?;
        let hex = format!("#{hex}");
        for key in KEYS {
            Setting::kde("kdeglobals", &["General"], key).write(ctx, value)?;
        }
        ctx.run(
            cmd!(ctx.shell, "plasma-apply-colorscheme --accent-color {hex}")
                .env("QT_QPA_PLATFORM", "offscreen"),
        )
    }

    fn delete(&self, ctx: &Context) -> ModuleResult {
        for key in KEYS {
            Setting::kde("kdeglobals", &["General"], key).delete(ctx)?;
        }
        Ok(())
    }

    fn reapplies(&self) -> bool {
        true
    }
}
