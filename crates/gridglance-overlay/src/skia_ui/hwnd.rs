//! Native Win32 layered HWNDs for Skia overlay panels.

use crate::win_click;

#[cfg(windows)]
use std::sync::Once;

#[derive(Clone, Debug)]
pub struct PanelHwnd {
    pub hwnd: isize,
}

#[cfg(windows)]
static REGISTER: Once = Once::new();

#[cfg(windows)]
const CLASS_NAME: &str = "GridGlanceSkiaPanel";

#[cfg(windows)]
unsafe extern "system" fn wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::{DefWindowProcW, HTTRANSPARENT, WM_NCHITTEST};
    // Default: never activate; click-through is controlled via WS_EX_TRANSPARENT.
    if msg == WM_NCHITTEST {
        // When not transparent, allow client hits for edit drag (host polls cursor).
        return windows::Win32::Foundation::LRESULT(1); // HTCLIENT
    }
    let _ = (wparam, HTTRANSPARENT);
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

#[cfg(windows)]
fn ensure_class() {
    REGISTER.call_once(|| {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows::core::PCWSTR;
        use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            LoadCursorW, RegisterClassExW, CS_HREDRAW, CS_VREDRAW, IDC_ARROW, WNDCLASSEXW,
        };

        unsafe {
            let hinstance = GetModuleHandleW(None).unwrap_or_default();
            let class: Vec<u16> = OsStr::new(CLASS_NAME)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinstance.into(),
                hIcon: Default::default(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                lpszMenuName: PCWSTR::null(),
                lpszClassName: PCWSTR(class.as_ptr()),
                hIconSm: Default::default(),
            };
            let _ = RegisterClassExW(&wc);
        }
    });
}

#[cfg(windows)]
pub fn create_panel(key: &str, x: i32, y: i32, w: i32, h: i32) -> Option<PanelHwnd> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, SetWindowPos, ShowWindow, HWND_TOPMOST, SWP_NOACTIVATE, SWP_SHOWWINDOW,
        SW_SHOWNOACTIVATE, WINDOW_EX_STYLE, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
        WS_EX_TOPMOST, WS_POPUP,
    };

    ensure_class();
    let title = win_click::panel_title(key);
    let class: Vec<u16> = OsStr::new(CLASS_NAME)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let title_w: Vec<u16> = OsStr::new(&title)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let hinstance = GetModuleHandleW(None).ok()?;
        let ex = WINDOW_EX_STYLE(
            WS_EX_LAYERED.0 | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0,
        );
        let hwnd = CreateWindowExW(
            ex,
            PCWSTR(class.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            WS_POPUP,
            x,
            y,
            w.max(1),
            h.max(1),
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .ok()?;
        if hwnd == HWND::default() || hwnd.0.is_null() {
            return None;
        }
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            x,
            y,
            w.max(1),
            h.max(1),
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        crate::layered::ensure_layered(hwnd.0 as isize);
        Some(PanelHwnd {
            hwnd: hwnd.0 as isize,
        })
    }
}

#[cfg(not(windows))]
pub fn create_panel(_key: &str, _x: i32, _y: i32, _w: i32, _h: i32) -> Option<PanelHwnd> {
    Some(PanelHwnd { hwnd: 0 })
}

#[cfg(windows)]
pub fn set_bounds(hwnd: isize, x: i32, y: i32, w: i32, h: i32) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOCOPYBITS,
    };
    unsafe {
        let _ = SetWindowPos(
            HWND(hwnd as *mut _),
            Some(HWND_TOPMOST),
            x,
            y,
            w.max(1),
            h.max(1),
            SWP_NOACTIVATE | SWP_NOCOPYBITS,
        );
    }
}

#[cfg(not(windows))]
pub fn set_bounds(_hwnd: isize, _x: i32, _y: i32, _w: i32, _h: i32) {}

#[cfg(windows)]
pub fn destroy(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::DestroyWindow;
    unsafe {
        let _ = DestroyWindow(HWND(hwnd as *mut _));
    }
}

#[cfg(not(windows))]
pub fn destroy(_hwnd: isize) {}

pub fn set_click_through(hwnd: isize, enabled: bool) {
    win_click::set_click_through(hwnd, enabled);
}

#[cfg(windows)]
pub fn cursor_pos() -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
    unsafe {
        let mut pt = POINT::default();
        GetCursorPos(&mut pt).ok()?;
        Some((pt.x, pt.y))
    }
}

#[cfg(not(windows))]
pub fn cursor_pos() -> Option<(i32, i32)> {
    None
}

#[cfg(windows)]
pub fn left_button_down() -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000 != 0 }
}

#[cfg(not(windows))]
pub fn left_button_down() -> bool {
    false
}
