//! Win32 overlay helpers: click-through and HWND lookup.

pub fn panel_title(key: &str) -> String {
    format!("GridGlance — {key}")
}

#[cfg(windows)]
pub fn find_overlay_hwnd(title: &str) -> Option<isize> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;

    let wide: Vec<u16> = OsStr::new(title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let hwnd = FindWindowW(PCWSTR::null(), PCWSTR(wide.as_ptr())).ok()?;
        if hwnd.0.is_null() || hwnd == HWND::default() {
            None
        } else {
            Some(hwnd.0 as isize)
        }
    }
}

#[cfg(not(windows))]
pub fn find_overlay_hwnd(_title: &str) -> Option<isize> {
    None
}

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

/// Work area (x, y, w, h) of the monitor that contains this panel.
#[cfg(windows)]
pub fn monitor_work_area(x: i32, y: i32, w: i32, h: i32) -> Option<(i32, i32, i32, i32)> {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    };
    unsafe {
        let rc = RECT {
            left: x,
            top: y,
            right: x.saturating_add(w.max(1)),
            bottom: y.saturating_add(h.max(1)),
        };
        let mon = MonitorFromRect(&rc, MONITOR_DEFAULTTONEAREST);
        if mon.is_invalid() {
            return None;
        }
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut info).as_bool() {
            return None;
        }
        let wr = info.rcWork;
        Some((
            wr.left,
            wr.top,
            (wr.right - wr.left).max(1),
            (wr.bottom - wr.top).max(1),
        ))
    }
}

#[cfg(not(windows))]
pub fn monitor_work_area(_x: i32, _y: i32, _w: i32, _h: i32) -> Option<(i32, i32, i32, i32)> {
    None
}
