//! Win32 overlay window helpers: click-through, DWM glass, shaped regions.

/// Reserved clear / chroma color for no-panel widgets (RGB 1,0,1).
pub const CHROMA_RGB: (u8, u8, u8) = (1, 0, 1);

pub fn panel_title(key: &str) -> String {
    format!("GridGlance — {key}")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelShape {
    /// Rounded rectangle (card chrome).
    RoundRect { w: i32, h: i32, radius: i32 },
    /// Ellipse (radar / floating no-panel content).
    Ellipse { w: i32, h: i32 },
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

/// Apply layered style, DWM glass, shaped region, and optional chroma-key.
#[cfg(windows)]
pub fn apply_panel_transparency(hwnd: isize, shape: PanelShape, chroma_key: bool) {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::Graphics::Dwm::DwmExtendFrameIntoClientArea;
    use windows::Win32::Graphics::Gdi::{CreateEllipticRgn, CreateRoundRectRgn, SetWindowRgn};
    use windows::Win32::UI::Controls::MARGINS;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetLayeredWindowAttributes, SetWindowLongW, GWL_EXSTYLE, LWA_COLORKEY,
        WINDOW_EX_STYLE, WS_EX_LAYERED,
    };

    unsafe {
        let hwnd = HWND(hwnd as *mut _);

        let current = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let layered = WINDOW_EX_STYLE(current as u32) | WS_EX_LAYERED;
        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, layered.0 as i32);

        let margins = MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        };
        let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

        let rgn = match shape {
            PanelShape::RoundRect { w, h, radius } => {
                let r = radius.max(1) * 2; // CreateRoundRectRgn wants ellipse width/height of corner
                CreateRoundRectRgn(0, 0, w.max(1), h.max(1), r, r)
            }
            PanelShape::Ellipse { w, h } => CreateEllipticRgn(0, 0, w.max(1), h.max(1)),
        };
        if !rgn.is_invalid() {
            let _ = SetWindowRgn(hwnd, Some(rgn), true);
        }

        if chroma_key {
            let (r, g, b) = CHROMA_RGB;
            let key = COLORREF(u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16));
            let _ = SetLayeredWindowAttributes(hwnd, key, 0, LWA_COLORKEY);
        }
    }
}

#[cfg(not(windows))]
pub fn apply_panel_transparency(_hwnd: isize, _shape: PanelShape, _chroma_key: bool) {}

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
