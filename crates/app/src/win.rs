//! Win32 glue GPUI does not offer on Windows: hide/show of the main window,
//! DWM cloaking for the first-frame reveal, and the process start time.

use gpui_kit::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows_sys::Win32::{
    Foundation::{FILETIME, HWND},
    Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CLOAK},
    System::{
        SystemInformation::GetSystemTimePreciseAsFileTime,
        Threading::{GetCurrentProcess, GetProcessTimes},
    },
    UI::WindowsAndMessaging::{ShowWindow, SW_HIDE, SW_SHOWNA},
};

pub fn hwnd(window: &Window) -> HWND {
    match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get() as HWND,
        _ => std::ptr::null_mut(),
    }
}

/// A cloaked window is "shown" to the compositor (so GPUI renders frames)
/// but not drawn on screen.
pub fn set_cloaked(hwnd: HWND, cloaked: bool) {
    let value: i32 = cloaked.into();
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_CLOAK as u32,
            (&raw const value).cast(),
            size_of::<i32>() as u32,
        );
    }
}

/// Show without activating; used together with [`set_cloaked`] before the first frame.
pub fn show_no_activate(hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_SHOWNA) };
}

pub fn hide(hwnd: HWND) {
    unsafe { ShowWindow(hwnd, SW_HIDE) };
}

/// Show (also from hidden or minimized) and bring to the front.
pub fn show(window: &Window) {
    show_no_activate(hwnd(window));
    window.activate_window();
}

fn filetime_ms(t: FILETIME) -> f64 {
    let ticks = (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    ticks as f64 / 10_000.0
}

/// Milliseconds since the OS created this process.
pub fn ms_since_process_start() -> f64 {
    let zero = FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    };
    let (mut created, mut exited, mut kernel, mut user, mut now) = (zero, zero, zero, zero, zero);
    unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        );
        GetSystemTimePreciseAsFileTime(&mut now);
    }
    filetime_ms(now) - filetime_ms(created)
}
