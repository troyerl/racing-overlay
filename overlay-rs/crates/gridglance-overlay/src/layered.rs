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

/// Reusable readback buffers (avoids per-frame alloc of W×H×8 bytes).
#[derive(Default)]
pub struct ReadbackScratch {
    #[allow(dead_code)] // filled on Windows readback path
    rgba: Vec<u8>,
    bgra: Vec<u8>,
}

impl ReadbackScratch {
    pub fn bgra(&self) -> &[u8] {
        &self.bgra
    }
}

/// FNV-1a over BGRA — used to skip UpdateLayeredWindow when a panel is unchanged.
pub fn hash_bgra(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    // Stride reduces cost on huge panels while still catching typical UI changes.
    let step = if data.len() > 256 * 1024 { 16 } else { 4 };
    let mut i = 0;
    while i < data.len() {
        h ^= data[i] as u64;
        h = h.wrapping_mul(0x100000001b3);
        i += step;
    }
    h ^= data.len() as u64;
    h = h.wrapping_mul(0x100000001b3);
    h
}

/// Read current GL default framebuffer → premultiplied BGRA (top-down).
/// Call while the viewport's WGL context is still current (right after paint).
#[cfg(windows)]
pub fn read_gl_to_bgra<'a>(
    gl: &glow::Context,
    width: i32,
    height: i32,
    scratch: &'a mut ReadbackScratch,
) -> Option<&'a [u8]> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let w = width as usize;
    let h = height as usize;
    // Cap absurd sizes (bad hwnd / transient resize) so we never OOM.
    let nbytes = w.saturating_mul(h).saturating_mul(4);
    if nbytes == 0 || nbytes > 64 * 1024 * 1024 {
        return None;
    }
    if scratch.rgba.len() != nbytes {
        scratch.rgba.resize(nbytes, 0);
    }
    if scratch.bgra.len() != nbytes {
        scratch.bgra.resize(nbytes, 0);
    }
    unsafe {
        // Drain any stale GL error so a bad pack doesn’t stick across viewports.
        while gl.get_error() != glow::NO_ERROR {}
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            width,
            height,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut scratch.rgba)),
        );
        let err = gl.get_error();
        while gl.get_error() != glow::NO_ERROR {}
        if err != glow::NO_ERROR {
            return None;
        }
    }

    // Convert RGBA (bottom-up) → premultiplied BGRA (top-down) for GDI.
    let row = w * 4;
    let rgba = scratch.rgba.as_slice();
    let bgra = scratch.bgra.as_mut_slice();
    for y in 0..h {
        let src_row = (h - 1 - y) * row;
        let dst_row = y * row;
        let src = &rgba[src_row..src_row + row];
        let dst = &mut bgra[dst_row..dst_row + row];
        for (s, d) in src.chunks_exact(4).zip(dst.chunks_exact_mut(4)) {
            let r = s[0];
            let g = s[1];
            let b = s[2];
            let a = s[3];
            // egui-glow writes premultiplied RGBA. Punch failed opaque-black clears.
            if r == 0 && g == 0 && b == 0 && a < 8 {
                d[0] = 0;
                d[1] = 0;
                d[2] = 0;
                d[3] = 0;
            } else {
                d[0] = b;
                d[1] = g;
                d[2] = r;
                d[3] = a;
            }
        }
    }
    Some(scratch.bgra.as_slice())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn read_gl_to_bgra<'a>(
    _gl: &eframe::glow::Context,
    _w: i32,
    _h: i32,
    _scratch: &'a mut ReadbackScratch,
) -> Option<&'a [u8]> {
    None
}

/// Push a pre-captured BGRA buffer with UpdateLayeredWindow (no GL).
/// Call after all viewport GL context switches for the frame are done.
#[cfg(windows)]
pub fn present_bgra(hwnd: isize, width: i32, height: i32, bgra: &[u8]) {
    if width <= 0 || height <= 0 || hwnd == 0 {
        return;
    }
    if bgra.len() < (width as usize) * (height as usize) * 4 {
        return;
    }
    ensure_layered(hwnd);

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

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn present_bgra(_hwnd: isize, _width: i32, _height: i32, _bgra: &[u8]) {}

/// Convenience: read + present in one call (prefer deferred present in the host).
#[cfg(windows)]
#[allow(dead_code)]
pub fn present_gl_framebuffer(gl: &glow::Context, hwnd: isize, width: i32, height: i32) {
    let mut scratch = ReadbackScratch::default();
    if let Some(bgra) = read_gl_to_bgra(gl, width, height, &mut scratch) {
        present_bgra(hwnd, width, height, bgra);
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn present_gl_framebuffer(_gl: &eframe::glow::Context, _hwnd: isize, _w: i32, _h: i32) {}

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
