//! Whether the Windows taskbar hides itself, as `on` or `off`.
use super::Kind;
use crate::{Context, ModuleResult};

pub(super) struct TaskbarAutoHide;

impl Kind for TaskbarAutoHide {
    fn describe(&self) -> String {
        "taskbar auto-hide".to_owned()
    }

    fn read(&self, _ctx: &Context) -> ModuleResult<Option<String>> {
        Ok(Some(
            if crate::taskbar_auto_hide::enabled()? {
                "on"
            } else {
                "off"
            }
            .to_owned(),
        ))
    }

    fn write(&self, _ctx: &Context, value: &str) -> ModuleResult {
        crate::taskbar_auto_hide::set(value == "on")
    }

    fn delete(&self, _ctx: &Context) -> ModuleResult {
        crate::taskbar_auto_hide::set(false)
    }
}
