//! Windows per-pixel alpha present via UpdateLayeredWindow.

#[cfg(windows)]
use eframe::glow::{self, HasContext};

/// Ensure WS_EX_LAYERED so UpdateLayeredWindow is allowed.
#[cfg(windows)]
pub fn ensure_layered(hwnd: isize) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongW, SetWindowLongW, GWL_EXSTYLE, WINDOW_EX_STYLE, WS_EX_LAYERED,
    };
    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let current = GetWindowLongW(hwnd, GWL_EXSTYLE);
        let layered = WINDOW_EX_STYLE(current as u32) | WS_EX_LAYERED;
        let _ = SetWindowLongW(hwnd, GWL_EXSTYLE, layered.0 as i32);
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn ensure_layered(_hwnd: isize) {}

/// Read the current GL default framebuffer (RGBA, bottom-up) and present with
/// per-pixel alpha via UpdateLayeredWindow.
#[cfg(windows)]
pub fn present_gl_framebuffer(gl: &glow::Context, hwnd: isize, width: i32, height: i32) {
    if width <= 0 || height <= 0 {
        return;
    }
    ensure_layered(hwnd);

    let w = width as usize;
    let h = height as usize;
    let mut rgba = vec![0u8; w * h * 4];
    unsafe {
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            width,
            height,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut rgba)),
        );
    }

    // Convert RGBA (bottom-up) → premultiplied BGRA (top-down) for GDI.
    let mut bgra = vec![0u8; w * h * 4];
    for y in 0..h {
        let src_row = (h - 1 - y) * w * 4;
        let dst_row = y * w * 4;
        for x in 0..w {
            let si = src_row + x * 4;
            let di = dst_row + x * 4;
            let r = rgba[si];
            let g = rgba[si + 1];
            let b = rgba[si + 2];
            let a = rgba[si + 3];
            // egui-glow writes premultiplied RGBA. Punch failed opaque-black clears.
            if r == 0 && g == 0 && b == 0 && a < 8 {
                bgra[di] = 0;
                bgra[di + 1] = 0;
                bgra[di + 2] = 0;
                bgra[di + 3] = 0;
            } else {
                bgra[di] = b;
                bgra[di + 1] = g;
                bgra[di + 2] = r;
                bgra[di + 3] = a;
            }
        }
    }

    present_bgra(hwnd, width, height, &bgra);
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn present_gl_framebuffer(_gl: &eframe::glow::Context, _hwnd: isize, _w: i32, _h: i32) {}

#[cfg(windows)]
fn present_bgra(hwnd: isize, width: i32, height: i32, bgra: &[u8]) {
    use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
        SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        BLENDFUNCTION, DIB_RGB_COLORS,
    };
    use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

    unsafe {
        let hwnd = HWND(hwnd as *mut _);
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return;
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return;
        }

        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [Default::default()],
        };
        let dib = match CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            Ok(h) => h,
            Err(_) => {
                let _ = DeleteDC(mem_dc);
                let _ = ReleaseDC(None, screen_dc);
                return;
            }
        };
        if bits.is_null() {
            let _ = DeleteObject(dib.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return;
        }

        let nbytes = (width as usize) * (height as usize) * 4;
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits as *mut u8, nbytes.min(bgra.len()));

        let old = SelectObject(mem_dc, dib.into());
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let src_pt = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = UpdateLayeredWindow(
            hwnd,
            Some(screen_dc),
            None,
            Some(&size),
            Some(mem_dc),
            Some(&src_pt),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );

        let _ = SelectObject(mem_dc, old);
        let _ = DeleteObject(dib.into());
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);
    }
}

/// Client size in physical pixels.
#[cfg(windows)]
pub fn client_size(hwnd: isize) -> Option<(i32, i32)> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;
    unsafe {
        let mut rc = RECT::default();
        GetClientRect(HWND(hwnd as *mut _), &mut rc).ok()?;
        let w = rc.right - rc.left;
        let h = rc.bottom - rc.top;
        if w <= 0 || h <= 0 {
            None
        } else {
            Some((w, h))
        }
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn client_size(_hwnd: isize) -> Option<(i32, i32)> {
    None
}
