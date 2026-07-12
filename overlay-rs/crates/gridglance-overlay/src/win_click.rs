//! Win32 helpers: click-through, virtual desktop bounds, window hit region.

/// Virtual-desktop rect in screen pixels: (x, y, width, height).
pub fn virtual_desktop_rect() -> (i32, i32, i32, i32) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN,
        };
        unsafe {
            (
                GetSystemMetrics(SM_XVIRTUALSCREEN),
                GetSystemMetrics(SM_YVIRTUALSCREEN),
                GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
                GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
            )
        }
    }
    #[cfg(not(windows))]
    {
        (0, 0, 1920, 1080)
    }
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

/// Restrict the window's hit-test region to `rects` (client-relative, physical px).
/// Empty `rects` clears the region (whole window is hittable again).
#[cfg(windows)]
pub fn set_hit_region(hwnd: isize, rects: &[(i32, i32, i32, i32)]) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, SetRectRgn, SetWindowRgn, RGN_OR,
    };

    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        if rects.is_empty() {
            let _ = SetWindowRgn(hwnd, None, true);
            return;
        }
        let (x0, y0, w0, h0) = rects[0];
        let combined = CreateRectRgn(x0, y0, x0 + w0.max(1), y0 + h0.max(1));
        if combined.is_invalid() {
            return;
        }
        let tmp = CreateRectRgn(0, 0, 1, 1);
        if tmp.is_invalid() {
            let _ = DeleteObject(combined.into());
            return;
        }
        for &(x, y, w, h) in &rects[1..] {
            let _ = SetRectRgn(tmp, x, y, x + w.max(1), y + h.max(1));
            let _ = CombineRgn(Some(combined), Some(combined), Some(tmp), RGN_OR);
        }
        let _ = DeleteObject(tmp.into());
        // SetWindowRgn takes ownership of `combined`.
        let _ = SetWindowRgn(hwnd, Some(combined), true);
    }
}

#[cfg(not(windows))]
pub fn set_hit_region(_hwnd: isize, _rects: &[(i32, i32, i32, i32)]) {}

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
