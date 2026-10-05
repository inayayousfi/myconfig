//! Hides the Windows taskbar until the pointer reaches it.
use crate::{Context, Footprint, Module, ModuleResult, Setting};

pub struct TaskbarAutoHide;

const AUTO_HIDE: usize = 0x1;
const ALWAYS_ON_TOP: usize = 0x2;

#[cfg(windows)]
fn app_bar_state(new_state: Option<usize>) -> ModuleResult<usize> {
    #[repr(C)]
    #[derive(Default)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct AppBarData {
        cb_size: u32,
        window: *mut std::ffi::c_void,
        callback_message: u32,
        edge: u32,
        rect: Rect,
        state: isize,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn FindWindowW(class_name: *const u16, window_name: *const u16) -> *mut std::ffi::c_void;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHAppBarMessage(message: u32, data: *mut AppBarData) -> usize;
    }
    let class: Vec<u16> = "Shell_TrayWnd"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // The pointers and struct layout are the Win32 APPBARDATA interface.
    let window = unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) };
    if window.is_null() {
        return Err("Windows taskbar was not found".into());
    }
    let mut data = AppBarData {
        cb_size: std::mem::size_of::<AppBarData>() as u32,
        window,
        callback_message: 0,
        edge: 0,
        rect: Rect::default(),
        state: new_state.unwrap_or(0) as isize,
    };
    if new_state.is_some() {
        unsafe {
            SHAppBarMessage(0xA, &mut data);
        }
    }
    Ok(unsafe { SHAppBarMessage(0x4, &mut data) })
}

#[cfg(not(windows))]
fn app_bar_state(_new_state: Option<usize>) -> ModuleResult<usize> {
    Err("the taskbar is available only on Windows".into())
}

pub(crate) fn enabled() -> ModuleResult<bool> {
    Ok(app_bar_state(None)? & AUTO_HIDE != 0)
}

pub(crate) fn set(auto_hide: bool) -> ModuleResult {
    let wanted = if auto_hide {
        AUTO_HIDE | ALWAYS_ON_TOP
    } else {
        ALWAYS_ON_TOP
    };
    if app_bar_state(Some(wanted))? & AUTO_HIDE != wanted & AUTO_HIDE {
        return Err("Windows taskbar auto-hide could not be verified".into());
    }
    Ok(())
}

impl Module for TaskbarAutoHide {
    fn name(&self) -> &'static str {
        "taskbar-auto-hide"
    }

    fn footprint(&self, _ctx: &Context) -> Footprint {
        Footprint::default()
    }

    fn install(&self, ctx: &Context) -> ModuleResult {
        ctx.set(Setting::TaskbarAutoHide, "on")
    }

    fn verify(&self, _ctx: &Context) -> ModuleResult {
        if !enabled()? {
            return Err("the taskbar does not hide automatically".into());
        }
        Ok(())
    }

    fn remove(&self, _ctx: &Context) -> ModuleResult {
        Ok(())
    }
}
