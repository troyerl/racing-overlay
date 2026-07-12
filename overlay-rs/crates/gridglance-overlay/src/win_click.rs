//! Win32 click-through for overlay HWNDs.

#[cfg(windows)]
pub fn set_click_through(hwnd: isize, enabled: bool) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WINDOW_EX_STYLE, WS_EX_LAYERED,
        WS_EX_TRANSPARENT,
    };
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let current = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let layered = WINDOW_EX_STYLE(current as u32) | WS_EX_LAYERED;
        let new = if enabled {
            layered | WS_EX_TRANSPARENT
        } else {
            WINDOW_EX_STYLE(layered.0 & !WS_EX_TRANSPARENT.0)
        };
        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, new.0 as i32);
    }
}

#[cfg(not(windows))]
pub fn set_click_through(_hwnd: isize, _enabled: bool) {}
