//! Windows per-pixel alpha present via UpdateLayeredWindow.

#[cfg(windows)]
use eframe::glow::{self, HasContext};
#[cfg(windows)]
use std::collections::HashMap;

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
    /// Pack buffer (BGRA or RGBA depending on path). Windows readback only.
    #[cfg(windows)]
    pack: Vec<u8>,
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

#[cfg(windows)]
/// Punch near-black clears and swap R↔B in place on a packed row (RGBA → BGRA).
fn punch_and_swap_rgba_row(row: &mut [u8]) {
    for px in row.chunks_exact_mut(4) {
        let r = px[0];
        let g = px[1];
        let b = px[2];
        let a = px[3];
        if r == 0 && g == 0 && b == 0 && a < 8 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 0;
        } else {
            px[0] = b;
            px[1] = g;
            px[2] = r;
        }
    }
}

#[cfg(windows)]
/// Punch near-black clears on an already-BGRA row (no channel swap).
fn punch_bgra_row(row: &mut [u8]) {
    for px in row.chunks_exact_mut(4) {
        let b = px[0];
        let g = px[1];
        let r = px[2];
        let a = px[3];
        if r == 0 && g == 0 && b == 0 && a < 8 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 0;
        }
    }
}

#[cfg(windows)]
/// Flip bottom-up pack buffer into top-down `bgra`, applying `row_fn` per destination row.
fn flip_rows(pack: &[u8], bgra: &mut [u8], w: usize, h: usize, mut row_fn: impl FnMut(&mut [u8])) {
    let row = w * 4;
    for y in 0..h {
        let src_row = (h - 1 - y) * row;
        let dst_row = y * row;
        bgra[dst_row..dst_row + row].copy_from_slice(&pack[src_row..src_row + row]);
        row_fn(&mut bgra[dst_row..dst_row + row]);
    }
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
    let nbytes = w.saturating_mul(h).saturating_mul(4);
    if nbytes == 0 || nbytes > 64 * 1024 * 1024 {
        return None;
    }
    if scratch.pack.len() != nbytes {
        scratch.pack.resize(nbytes, 0);
    }
    if scratch.bgra.len() != nbytes {
        scratch.bgra.resize(nbytes, 0);
    }

    unsafe {
        while gl.get_error() != glow::NO_ERROR {}
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);

        // Prefer BGRA pack (skip R↔B swap). Fall back to RGBA on driver error.
        gl.read_pixels(
            0,
            0,
            width,
            height,
            glow::BGRA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::Slice(Some(&mut scratch.pack)),
        );
        let mut err = gl.get_error();
        while gl.get_error() != glow::NO_ERROR {}
        let used_bgra = err == glow::NO_ERROR;
        if !used_bgra {
            while gl.get_error() != glow::NO_ERROR {}
            gl.read_pixels(
                0,
                0,
                width,
                height,
                glow::RGBA,
                glow::UNSIGNED_BYTE,
                glow::PixelPackData::Slice(Some(&mut scratch.pack)),
            );
            err = gl.get_error();
            while gl.get_error() != glow::NO_ERROR {}
            if err != glow::NO_ERROR {
                return None;
            }
        }

        if used_bgra {
            flip_rows(&scratch.pack, &mut scratch.bgra, w, h, punch_bgra_row);
        } else {
            flip_rows(&scratch.pack, &mut scratch.bgra, w, h, punch_and_swap_rgba_row);
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

/// Cached GDI DIB + mem DC per HWND (avoids Create/Delete every present).
#[derive(Default)]
pub struct PresentCache {
    #[cfg(windows)]
    entries: HashMap<isize, PresentSurface>,
}

#[cfg(windows)]
struct PresentSurface {
    width: i32,
    height: i32,
    mem_dc: windows::Win32::Graphics::Gdi::HDC,
    dib: windows::Win32::Graphics::Gdi::HBITMAP,
    /// Object previously selected into `mem_dc` (restore before DeleteObject).
    old_obj: windows::Win32::Graphics::Gdi::HGDIOBJ,
    bits: *mut u8,
}

#[cfg(windows)]
impl Drop for PresentSurface {
    fn drop(&mut self) {
        unsafe {
            use windows::Win32::Graphics::Gdi::{DeleteDC, DeleteObject, SelectObject};
            let _ = SelectObject(self.mem_dc, self.old_obj);
            let _ = DeleteObject(self.dib.into());
            let _ = DeleteDC(self.mem_dc);
            self.bits = std::ptr::null_mut();
        }
    }
}

impl PresentCache {
    pub fn retain_hwnds(&mut self, live: &std::collections::HashSet<isize>) {
        #[cfg(windows)]
        self.entries.retain(|hwnd, _| live.contains(hwnd));
        #[cfg(not(windows))]
        let _ = live;
    }

    #[cfg(windows)]
    unsafe fn ensure_surface(
        &mut self,
        hwnd_key: isize,
        width: i32,
        height: i32,
        screen_dc: windows::Win32::Graphics::Gdi::HDC,
    ) -> Option<&PresentSurface> {
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
            BI_RGB, DIB_RGB_COLORS,
        };

        let needs_new = match self.entries.get(&hwnd_key) {
            Some(s) => s.width != width || s.height != height || s.bits.is_null(),
            None => true,
        };
        if needs_new {
            self.entries.remove(&hwnd_key);
            let mem_dc = CreateCompatibleDC(Some(screen_dc));
            if mem_dc.is_invalid() {
                return None;
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
            let dib = match CreateDIBSection(Some(mem_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(h) => h,
                Err(_) => {
                    let _ = windows::Win32::Graphics::Gdi::DeleteDC(mem_dc);
                    return None;
                }
            };
            if bits.is_null() {
                let _ = windows::Win32::Graphics::Gdi::DeleteObject(dib.into());
                let _ = windows::Win32::Graphics::Gdi::DeleteDC(mem_dc);
                return None;
            }
            let old_obj = SelectObject(mem_dc, dib.into());
            self.entries.insert(
                hwnd_key,
                PresentSurface {
                    width,
                    height,
                    mem_dc,
                    dib,
                    old_obj,
                    bits: bits as *mut u8,
                },
            );
        }
        self.entries.get(&hwnd_key)
    }
}

/// Push a pre-captured BGRA buffer with UpdateLayeredWindow (no GL).
/// Reuses a cached DIB/mem DC per hwnd when size is unchanged.
#[cfg(windows)]
pub fn present_bgra(
    cache: &mut PresentCache,
    hwnd: isize,
    width: i32,
    height: i32,
    bgra: &[u8],
) {
    if width <= 0 || height <= 0 || hwnd == 0 {
        return;
    }
    if bgra.len() < (width as usize) * (height as usize) * 4 {
        return;
    }
    ensure_layered(hwnd);

    use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

    unsafe {
        let hwnd_win = HWND(hwnd as *mut _);
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return;
        }
        let Some(surf) = cache.ensure_surface(hwnd, width, height, screen_dc) else {
            let _ = ReleaseDC(None, screen_dc);
            return;
        };

        let nbytes = (width as usize) * (height as usize) * 4;
        std::ptr::copy_nonoverlapping(bgra.as_ptr(), surf.bits, nbytes.min(bgra.len()));

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
            hwnd_win,
            Some(screen_dc),
            None,
            Some(&size),
            Some(surf.mem_dc),
            Some(&src_pt),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        );
        let _ = ReleaseDC(None, screen_dc);
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn present_bgra(
    _cache: &mut PresentCache,
    _hwnd: isize,
    _width: i32,
    _height: i32,
    _bgra: &[u8],
) {
}

/// Convenience: read + present in one call (prefer deferred present in the host).
#[cfg(windows)]
#[allow(dead_code)]
pub fn present_gl_framebuffer(gl: &glow::Context, hwnd: isize, width: i32, height: i32) {
    let mut scratch = ReadbackScratch::default();
    let mut cache = PresentCache::default();
    if let Some(bgra) = read_gl_to_bgra(gl, width, height, &mut scratch) {
        present_bgra(&mut cache, hwnd, width, height, bgra);
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
