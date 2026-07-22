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

/// Downsample BGRA 2×2 → 1 (box average). Returns `(w/2, h/2, pixels)`.
/// Used for cheap hot-path map ULW (rev 19).
pub fn downsample_bgra_half(src: &[u8], w: i32, h: i32) -> Option<(i32, i32, Vec<u8>)> {
    if w < 2 || h < 2 {
        return None;
    }
    let sw = w as usize;
    let sh = h as usize;
    let need = sw.saturating_mul(sh).saturating_mul(4);
    if src.len() < need {
        return None;
    }
    let dw = w / 2;
    let dh = h / 2;
    let mut out = vec![0u8; (dw as usize) * (dh as usize) * 4];
    for y in 0..dh as usize {
        for x in 0..dw as usize {
            let x0 = x * 2;
            let y0 = y * 2;
            let mut b = 0u32;
            let mut g = 0u32;
            let mut r = 0u32;
            let mut a = 0u32;
            for dy in 0..2usize {
                for dx in 0..2usize {
                    let i = ((y0 + dy) * sw + (x0 + dx)) * 4;
                    b += src[i] as u32;
                    g += src[i + 1] as u32;
                    r += src[i + 2] as u32;
                    a += src[i + 3] as u32;
                }
            }
            let o = (y * dw as usize + x) * 4;
            out[o] = (b / 4) as u8;
            out[o + 1] = (g / 4) as u8;
            out[o + 2] = (r / 4) as u8;
            out[o + 3] = (a / 4) as u8;
        }
    }
    Some((dw, dh, out))
}

impl ReadbackScratch {
    pub fn bgra(&self) -> &[u8] {
        &self.bgra
    }

    pub fn bgra_mut(&mut self) -> &mut Vec<u8> {
        &mut self.bgra
    }
}

/// One car marker for CPU composite onto a cached full-res map background.
#[derive(Clone, Debug)]
pub struct MapCarSprite {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub b: u8,
    pub g: u8,
    pub r_ch: u8,
    pub a: u8,
    pub player: bool,
    pub label: String,
}

/// Copy `bg` into `out`, draw car dots. Labels optional (hot path skips fonts).
pub fn composite_map_cars(
    bg: &[u8],
    w: i32,
    h: i32,
    cars: &[MapCarSprite],
    out: &mut Vec<u8>,
    fonts: Option<&egui::text::Fonts>,
) {
    let need = (w as usize).saturating_mul(h as usize).saturating_mul(4);
    if bg.len() < need || w <= 0 || h <= 0 {
        out.clear();
        return;
    }
    if out.len() != need {
        out.resize(need, 0);
    }
    out.copy_from_slice(&bg[..need]);
    for car in cars {
        if car.player {
            fill_circle_fast(
                out,
                w,
                h,
                car.x,
                car.y,
                car.r + 4.0,
                car.b,
                car.g,
                car.r_ch,
                (car.a as u16 * 50 / 255) as u8,
            );
        }
        // Dark ring then fill (no AA stroke — was expensive every present).
        fill_circle_fast(out, w, h, car.x, car.y, car.r + 1.2, 0, 0, 0, car.a);
        fill_circle_fast(
            out, w, h, car.x, car.y, car.r, car.b, car.g, car.r_ch, car.a,
        );
    }
    if let Some(fonts) = fonts {
        draw_car_labels_fonts(out, w, h, cars, fonts);
    }
}

fn blend_premul_bgra(dst: &mut [u8], src_b: u8, src_g: u8, src_r: u8, src_a: u8) {
    if src_a == 0 {
        return;
    }
    let sb = (src_b as u16 * src_a as u16 / 255) as u8;
    let sg = (src_g as u16 * src_a as u16 / 255) as u8;
    let sr = (src_r as u16 * src_a as u16 / 255) as u8;
    if src_a == 255 {
        dst[0] = sb;
        dst[1] = sg;
        dst[2] = sr;
        dst[3] = 255;
        return;
    }
    let ia = 255u16 - src_a as u16;
    dst[0] = (sb as u16 + dst[0] as u16 * ia / 255) as u8;
    dst[1] = (sg as u16 + dst[1] as u16 * ia / 255) as u8;
    dst[2] = (sr as u16 + dst[2] as u16 * ia / 255) as u8;
    dst[3] = (src_a as u16 + dst[3] as u16 * ia / 255) as u8;
}

/// Fast filled circle (coverage via radius², no sqrt).
fn fill_circle_fast(
    buf: &mut [u8],
    w: i32,
    h: i32,
    cx: f32,
    cy: f32,
    radius: f32,
    b: u8,
    g: u8,
    r: u8,
    a: u8,
) {
    if radius <= 0.0 || a == 0 {
        return;
    }
    let r2 = (radius + 0.5) * (radius + 0.5);
    let soft = radius.max(0.5);
    let soft2 = soft * soft;
    let x0 = ((cx - radius - 1.0).floor() as i32).max(0);
    let y0 = ((cy - radius - 1.0).floor() as i32).max(0);
    let x1 = ((cx + radius + 1.0).ceil() as i32).min(w - 1);
    let y1 = ((cy + radius + 1.0).ceil() as i32).min(h - 1);
    for y in y0..=y1 {
        let dy = y as f32 + 0.5 - cy;
        let dy2 = dy * dy;
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - cx;
            let d2 = dx * dx + dy2;
            if d2 > r2 {
                continue;
            }
            let aa = if d2 <= soft2 {
                a
            } else {
                let t = (1.0 - (d2.sqrt() - soft)).clamp(0.0, 1.0);
                ((a as f32) * t).round() as u8
            };
            if aa == 0 {
                continue;
            }
            let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
            if i + 3 < buf.len() {
                blend_premul_bgra(&mut buf[i..i + 4], b, g, r, aa);
            }
        }
    }
}

#[allow(dead_code)]
fn stroke_circle_aa(
    buf: &mut [u8],
    w: i32,
    h: i32,
    cx: f32,
    cy: f32,
    radius: f32,
    b: u8,
    g: u8,
    r: u8,
    a: u8,
    thickness: f32,
) {
    if radius <= 0.0 || a == 0 || thickness <= 0.0 {
        return;
    }
    let outer = radius + thickness * 0.5;
    let inner = (radius - thickness * 0.5).max(0.0);
    let x0 = ((cx - outer - 1.0).floor() as i32).max(0);
    let y0 = ((cy - outer - 1.0).floor() as i32).max(0);
    let x1 = ((cx + outer + 1.0).ceil() as i32).min(w - 1);
    let y1 = ((cy + outer + 1.0).ceil() as i32).min(h - 1);
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let cov = (outer + 0.5 - d).min(d - (inner - 0.5)).clamp(0.0, 1.0);
            if cov <= 0.0 {
                continue;
            }
            let aa = ((a as f32) * cov).round() as u8;
            let i = ((y as usize) * (w as usize) + (x as usize)) * 4;
            if i + 3 < buf.len() {
                blend_premul_bgra(&mut buf[i..i + 4], b, g, r, aa);
            }
        }
    }
}

/// Draw car numbers from egui's font atlas (true AA outlines, no bitmap blocks).
fn draw_car_labels_fonts(
    bgra: &mut [u8],
    w: i32,
    h: i32,
    cars: &[MapCarSprite],
    fonts: &egui::text::Fonts,
) {
    let need = (w as usize) * (h as usize) * 4;
    if bgra.len() < need {
        return;
    }
    let atlas = fonts.image();
    let atlas_w = atlas.width();
    let atlas_h = atlas.height();
    if atlas_w == 0 || atlas_h == 0 || atlas.pixels.is_empty() {
        return;
    }
    let ppp = fonts.pixels_per_point().max(0.5);

    for car in cars {
        if car.label.is_empty() {
            continue;
        }
        let text: String = car
            .label
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(3)
            .collect();
        if text.is_empty() {
            continue;
        }
        let size_pts = (car.r / ppp * 0.92).clamp(8.0, 16.0);
        let font_id = egui::FontId::new(size_pts, egui::FontFamily::Proportional);
        let galley =
            fonts.layout_no_wrap(text, font_id, egui::Color32::WHITE);
        let gw = galley.size().x * ppp;
        let gh = galley.size().y * ppp;
        let origin_x = car.x - gw * 0.5;
        let origin_y = car.y - gh * 0.5;

        // Dark halo then white fill — sample atlas with bilinear coverage.
        // Cardinal halo only (8-dir was ~2× the blit cost and spiked ULW stutter).
        for pass in 0..2 {
            let (ob, og, or_, stroke) = if pass == 0 {
                (0u8, 0u8, 0u8, true)
            } else {
                (255u8, 255u8, 255u8, false)
            };
            let offsets: &[(f32, f32)] = if stroke {
                &[(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)]
            } else {
                &[(0.0, 0.0)]
            };
            for &(ox, oy) in offsets {
                for row in &galley.rows {
                    for g in &row.glyphs {
                        let uv = g.uv_rect;
                        if uv.is_nothing() {
                            continue;
                        }
                        let left_top = g.pos + uv.offset;
                        let x0 = origin_x + left_top.x * ppp + ox;
                        let y0 = origin_y + left_top.y * ppp + oy;
                        let dw = uv.size.x * ppp;
                        let dh = uv.size.y * ppp;
                        blit_atlas_glyph(
                            bgra,
                            w,
                            h,
                            &atlas.pixels,
                            atlas_w,
                            atlas_h,
                            uv.min[0] as i32,
                            uv.min[1] as i32,
                            (uv.max[0] - uv.min[0]) as i32,
                            (uv.max[1] - uv.min[1]) as i32,
                            x0,
                            y0,
                            dw,
                            dh,
                            ob,
                            og,
                            or_,
                            if stroke { 220 } else { 255 },
                        );
                    }
                }
            }
        }
    }
}

/// Bilinear sample of font atlas → premultiplied BGRA.
fn blit_atlas_glyph(
    bgra: &mut [u8],
    dst_w: i32,
    dst_h: i32,
    atlas: &[f32],
    atlas_w: usize,
    atlas_h: usize,
    src_x: i32,
    src_y: i32,
    src_w: i32,
    src_h: i32,
    dst_x: f32,
    dst_y: f32,
    dst_w_px: f32,
    dst_h_px: f32,
    b: u8,
    g: u8,
    r: u8,
    a_scale: u8,
) {
    if src_w <= 0 || src_h <= 0 || dst_w_px < 0.5 || dst_h_px < 0.5 {
        return;
    }
    let x0 = dst_x.floor() as i32;
    let y0 = dst_y.floor() as i32;
    let x1 = (dst_x + dst_w_px).ceil() as i32;
    let y1 = (dst_y + dst_h_px).ceil() as i32;
    for py in y0..y1 {
        if py < 0 || py >= dst_h {
            continue;
        }
        let v = ((py as f32 + 0.5 - dst_y) / dst_h_px).clamp(0.0, 1.0);
        for px in x0..x1 {
            if px < 0 || px >= dst_w {
                continue;
            }
            let u = ((px as f32 + 0.5 - dst_x) / dst_w_px).clamp(0.0, 1.0);
            let sx = src_x as f32 + u * (src_w as f32 - 1e-3);
            let sy = src_y as f32 + v * (src_h as f32 - 1e-3);
            let cov = sample_atlas_bilinear(atlas, atlas_w, atlas_h, sx, sy);
            if cov < 0.02 {
                continue;
            }
            let a = ((cov * a_scale as f32).round() as u8).max(1);
            let i = ((py as usize) * (dst_w as usize) + (px as usize)) * 4;
            if i + 3 < bgra.len() {
                blend_premul_bgra(&mut bgra[i..i + 4], b, g, r, a);
            }
        }
    }
}

fn sample_atlas_bilinear(atlas: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    if w == 0 || h == 0 {
        return 0.0;
    }
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let x0 = x.floor() as usize;
    let y0 = y.floor() as usize;
    let x1 = (x0 + 1).min(w - 1);
    let y1 = (y0 + 1).min(h - 1);
    let fx = x - x0 as f32;
    let fy = y - y0 as f32;
    let i = |xx: usize, yy: usize| atlas[yy * w + xx];
    let a = i(x0, y0);
    let b = i(x1, y0);
    let c = i(x0, y1);
    let d = i(x1, y1);
    let top = a + (b - a) * fx;
    let bot = c + (d - c) * fx;
    (top + (bot - top) * fy).clamp(0.0, 1.0)
}

/// Map-only async readback pipe. The map animates every visible frame, so the
/// sync `glReadPixels(... Slice ...)` path is too expensive for it; other panels
/// stay on the simpler synchronous path because they present infrequently.
///
/// Prefers full-res PBO for infrequent static map bg capture (sharp track).
/// Hot frames composite cars onto that cache without GL readback.
#[cfg(windows)]
pub struct MapReadbackPipe {
    buffers: [Option<glow::Buffer>; 2],
    fences: [Option<glow::Fence>; 2],
    seqs: [u64; 2],
    sizes: [usize; 2],
    dims: [(i32, i32); 2],
    next_write: usize,
    next_seq: u64,
    disabled: bool,
    /// Set true if half-res blit fails (permanent fallback to full-res PBO).
    half_disabled: bool,
    /// Same-size resolve target (MSAA default FB → single-sample).
    staging_fbo: Option<glow::Framebuffer>,
    staging_tex: Option<glow::Texture>,
    staging_w: i32,
    staging_h: i32,
    half_fbo: Option<glow::Framebuffer>,
    half_tex: Option<glow::Texture>,
    half_w: i32,
    half_h: i32,
    /// Size of the last successfully taken PBO (for scaled present).
    pub last_taken_dims: Option<(i32, i32)>,
}

#[cfg(windows)]
impl Default for MapReadbackPipe {
    fn default() -> Self {
        Self {
            buffers: [None, None],
            fences: [None, None],
            seqs: [0, 0],
            sizes: [0, 0],
            dims: [(0, 0), (0, 0)],
            next_write: 0,
            next_seq: 0,
            disabled: false,
            half_disabled: true,
            staging_fbo: None,
            staging_tex: None,
            staging_w: 0,
            staging_h: 0,
            half_fbo: None,
            half_tex: None,
            half_w: 0,
            half_h: 0,
            last_taken_dims: None,
        }
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub struct MapReadbackPipe;

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

#[cfg(windows)]
fn valid_readback_dims(width: i32, height: i32) -> Option<(usize, usize, usize)> {
    if width <= 0 || height <= 0 {
        return None;
    }
    let w = width as usize;
    let h = height as usize;
    let nbytes = w.checked_mul(h)?.checked_mul(4)?;
    if nbytes == 0 || nbytes > 64 * 1024 * 1024 {
        return None;
    }
    Some((w, h, nbytes))
}

#[cfg(windows)]
fn fence_ready(gl: &glow::Context, fence: glow::Fence) -> bool {
    unsafe {
        matches!(
            gl.client_wait_sync(fence, 0, 0),
            glow::ALREADY_SIGNALED | glow::CONDITION_SATISFIED
        )
    }
}

#[cfg(windows)]
fn half_dims(full_w: i32, full_h: i32) -> (i32, i32) {
    ((full_w / 2).max(1), (full_h / 2).max(1))
}

#[cfg(windows)]
fn ensure_color_fbo(
    gl: &glow::Context,
    fbo_slot: &mut Option<glow::Framebuffer>,
    tex_slot: &mut Option<glow::Texture>,
    stored_w: &mut i32,
    stored_h: &mut i32,
    w: i32,
    h: i32,
) -> bool {
    if *stored_w == w && *stored_h == h && fbo_slot.is_some() && tex_slot.is_some() {
        return true;
    }
    unsafe {
        while gl.get_error() != glow::NO_ERROR {}
        if let Some(fbo) = fbo_slot.take() {
            gl.delete_framebuffer(fbo);
        }
        if let Some(tex) = tex_slot.take() {
            gl.delete_texture(tex);
        }
        let Ok(tex) = gl.create_texture() else {
            return false;
        };
        let Ok(fbo) = gl.create_framebuffer() else {
            gl.delete_texture(tex);
            return false;
        };
        gl.bind_texture(glow::TEXTURE_2D, Some(tex));
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MIN_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_MAG_FILTER,
            glow::LINEAR as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_S,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_parameter_i32(
            glow::TEXTURE_2D,
            glow::TEXTURE_WRAP_T,
            glow::CLAMP_TO_EDGE as i32,
        );
        gl.tex_image_2d(
            glow::TEXTURE_2D,
            0,
            glow::RGBA8 as i32,
            w,
            h,
            0,
            glow::RGBA,
            glow::UNSIGNED_BYTE,
            glow::PixelUnpackData::Slice(None),
        );
        gl.bind_framebuffer(glow::FRAMEBUFFER, Some(fbo));
        gl.framebuffer_texture_2d(
            glow::FRAMEBUFFER,
            glow::COLOR_ATTACHMENT0,
            glow::TEXTURE_2D,
            Some(tex),
            0,
        );
        let status = gl.check_framebuffer_status(glow::FRAMEBUFFER);
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        gl.bind_texture(glow::TEXTURE_2D, None);
        if status != glow::FRAMEBUFFER_COMPLETE {
            gl.delete_framebuffer(fbo);
            gl.delete_texture(tex);
            return false;
        }
        *tex_slot = Some(tex);
        *fbo_slot = Some(fbo);
        *stored_w = w;
        *stored_h = h;
        true
    }
}

#[cfg(windows)]
fn blit_fbos(
    gl: &glow::Context,
    src: Option<glow::Framebuffer>,
    dst: glow::Framebuffer,
    src_w: i32,
    src_h: i32,
    dst_w: i32,
    dst_h: i32,
    filter: u32,
) -> bool {
    unsafe {
        while gl.get_error() != glow::NO_ERROR {}
        gl.bind_framebuffer(glow::READ_FRAMEBUFFER, src);
        if src.is_none() {
            // Default window FB (egui/Glow). Prefer BACK; ignore errors on
            // single-buffered / FBO-style defaults.
            gl.read_buffer(glow::BACK);
            let _ = gl.get_error();
        }
        gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, Some(dst));
        gl.blit_framebuffer(
            0,
            0,
            src_w,
            src_h,
            0,
            0,
            dst_w,
            dst_h,
            glow::COLOR_BUFFER_BIT,
            filter,
        );
        let err = gl.get_error();
        while gl.get_error() != glow::NO_ERROR {}
        gl.bind_framebuffer(glow::FRAMEBUFFER, None);
        err == glow::NO_ERROR
    }
}

/// Resolve (and optionally scale) the default FB into the half-res FBO.
/// Two-step when scaling: MSAA/default → same-size staging → half (many
/// drivers reject resolve+scale in a single blit).
#[cfg(windows)]
fn blit_default_to_half(
    gl: &glow::Context,
    pipe: &mut MapReadbackPipe,
    full_w: i32,
    full_h: i32,
    hw: i32,
    hh: i32,
) -> bool {
    if !ensure_color_fbo(
        gl,
        &mut pipe.half_fbo,
        &mut pipe.half_tex,
        &mut pipe.half_w,
        &mut pipe.half_h,
        hw,
        hh,
    ) {
        return false;
    }
    let half_fbo = pipe.half_fbo.unwrap();

    // Fast path: direct blit (works when default FB is single-sample).
    if blit_fbos(gl, None, half_fbo, full_w, full_h, hw, hh, glow::NEAREST)
        || blit_fbos(gl, None, half_fbo, full_w, full_h, hw, hh, glow::LINEAR)
    {
        return true;
    }

    // Resolve to same-size staging, then scale to half.
    if !ensure_color_fbo(
        gl,
        &mut pipe.staging_fbo,
        &mut pipe.staging_tex,
        &mut pipe.staging_w,
        &mut pipe.staging_h,
        full_w,
        full_h,
    ) {
        return false;
    }
    let staging_fbo = pipe.staging_fbo.unwrap();
    if !blit_fbos(
        gl,
        None,
        staging_fbo,
        full_w,
        full_h,
        full_w,
        full_h,
        glow::NEAREST,
    ) {
        // Last resort: copy into staging texture from the default FB.
        let Some(staging_tex) = pipe.staging_tex else {
            return false;
        };
        unsafe {
            while gl.get_error() != glow::NO_ERROR {}
            gl.bind_framebuffer(glow::FRAMEBUFFER, None);
            gl.bind_texture(glow::TEXTURE_2D, Some(staging_tex));
            gl.copy_tex_sub_image_2d(glow::TEXTURE_2D, 0, 0, 0, 0, 0, full_w, full_h);
            let err = gl.get_error();
            while gl.get_error() != glow::NO_ERROR {}
            gl.bind_texture(glow::TEXTURE_2D, None);
            if err != glow::NO_ERROR {
                return false;
            }
        }
    }
    blit_fbos(
        gl,
        Some(staging_fbo),
        half_fbo,
        full_w,
        full_h,
        hw,
        hh,
        glow::LINEAR,
    ) || blit_fbos(
        gl,
        Some(staging_fbo),
        half_fbo,
        full_w,
        full_h,
        hw,
        hh,
        glow::NEAREST,
    )
}

/// Kick an async PBO read of the map FB (half-res when available).
/// Returns false when the pipe is disabled or the next PBO slot is still busy.
#[cfg(windows)]
pub fn map_kick_readback(
    gl: &glow::Context,
    width: i32,
    height: i32,
    pipe: &mut MapReadbackPipe,
) -> bool {
    if pipe.disabled {
        return false;
    }

    unsafe {
        let prev_draw = gl.get_parameter_framebuffer(glow::DRAW_FRAMEBUFFER_BINDING);
        let prev_read = gl.get_parameter_framebuffer(glow::READ_FRAMEBUFFER_BINDING);
        let prev_pack = gl.get_parameter_buffer(glow::PIXEL_PACK_BUFFER_BINDING);
        let restore = |gl: &glow::Context| {
            gl.bind_framebuffer(glow::DRAW_FRAMEBUFFER, prev_draw);
            gl.bind_framebuffer(glow::READ_FRAMEBUFFER, prev_read);
            gl.bind_buffer(glow::PIXEL_PACK_BUFFER, prev_pack);
        };

        let (read_w, read_h, read_fbo) = if !pipe.half_disabled {
            let (hw, hh) = half_dims(width, height);
            if blit_default_to_half(gl, pipe, width, height, hw, hh) {
                (hw, hh, pipe.half_fbo)
            } else {
                if !pipe.half_disabled {
                    eprintln!("map PBO: half downsample failed; using full-res PBO (still async)");
                    pipe.half_disabled = true;
                }
                (width, height, None)
            }
        } else {
            (width, height, None)
        };

        let Some((_, _, nbytes)) = valid_readback_dims(read_w, read_h) else {
            restore(gl);
            return false;
        };

        while gl.get_error() != glow::NO_ERROR {}

        let idx = pipe.next_write;
        if let Some(fence) = pipe.fences[idx] {
            if !fence_ready(gl, fence) {
                restore(gl);
                return false;
            }
            gl.delete_sync(fence);
            pipe.fences[idx] = None;
        }

        let buffer = match pipe.buffers[idx] {
            Some(buffer) => buffer,
            None => match gl.create_buffer() {
                Ok(buffer) => {
                    pipe.buffers[idx] = Some(buffer);
                    buffer
                }
                Err(_) => {
                    pipe.disabled = true;
                    restore(gl);
                    return false;
                }
            },
        };

        gl.bind_framebuffer(glow::FRAMEBUFFER, read_fbo);
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(buffer));
        if pipe.sizes[idx] != nbytes {
            gl.buffer_data_size(glow::PIXEL_PACK_BUFFER, nbytes as i32, glow::STREAM_READ);
            pipe.sizes[idx] = nbytes;
        }
        gl.pixel_store_i32(glow::PACK_ALIGNMENT, 1);
        gl.read_pixels(
            0,
            0,
            read_w,
            read_h,
            glow::BGRA,
            glow::UNSIGNED_BYTE,
            glow::PixelPackData::BufferOffset(0),
        );
        let err = gl.get_error();
        while gl.get_error() != glow::NO_ERROR {}
        if err != glow::NO_ERROR {
            pipe.disabled = true;
            restore(gl);
            return false;
        }

        match gl.fence_sync(glow::SYNC_GPU_COMMANDS_COMPLETE, 0) {
            Ok(fence) => {
                pipe.fences[idx] = Some(fence);
                pipe.dims[idx] = (read_w, read_h);
                pipe.next_seq = pipe.next_seq.wrapping_add(1);
                pipe.seqs[idx] = pipe.next_seq;
                pipe.next_write = 1 - idx;
                gl.flush();
                restore(gl);
                true
            }
            Err(_) => {
                pipe.disabled = true;
                restore(gl);
                false
            }
        }
    }
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn map_kick_readback(
    _gl: &eframe::glow::Context,
    _width: i32,
    _height: i32,
    _pipe: &mut MapReadbackPipe,
) -> bool {
    false
}

/// Return the newest ready map PBO as top-down premultiplied BGRA.
#[cfg(windows)]
pub fn map_take_ready_bgra<'a>(
    gl: &glow::Context,
    pipe: &mut MapReadbackPipe,
    scratch: &'a mut ReadbackScratch,
) -> Option<&'a [u8]> {
    if pipe.disabled {
        return None;
    }
    let mut best: Option<usize> = None;
    for idx in 0..2 {
        if let Some(fence) = pipe.fences[idx] {
            if fence_ready(gl, fence) && best.map(|b| pipe.seqs[idx] > pipe.seqs[b]).unwrap_or(true)
            {
                best = Some(idx);
            }
        }
    }
    let idx = best?;
    let fence = pipe.fences[idx].take()?;
    let (width, height) = pipe.dims[idx];
    let (w, h, nbytes) = valid_readback_dims(width, height)?;
    if scratch.pack.len() != nbytes {
        scratch.pack.resize(nbytes, 0);
    }
    if scratch.bgra.len() != nbytes {
        scratch.bgra.resize(nbytes, 0);
    }

    unsafe {
        gl.delete_sync(fence);
        let buffer = pipe.buffers[idx]?;
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, Some(buffer));
        gl.get_buffer_sub_data(glow::PIXEL_PACK_BUFFER, 0, &mut scratch.pack);
        gl.bind_buffer(glow::PIXEL_PACK_BUFFER, None);
        flip_rows(&scratch.pack, &mut scratch.bgra, w, h, punch_bgra_row);
    }
    pipe.last_taken_dims = Some((width, height));
    Some(scratch.bgra.as_slice())
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn map_take_ready_bgra<'a>(
    _gl: &eframe::glow::Context,
    _pipe: &mut MapReadbackPipe,
    _scratch: &'a mut ReadbackScratch,
) -> Option<&'a [u8]> {
    None
}

#[cfg(windows)]
pub fn map_pipe_disabled(pipe: &MapReadbackPipe) -> bool {
    pipe.disabled
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn map_pipe_disabled(_pipe: &MapReadbackPipe) -> bool {
    true
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
            flip_rows(
                &scratch.pack,
                &mut scratch.bgra,
                w,
                h,
                punch_and_swap_rgba_row,
            );
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
    /// Source-sized DIB for StretchBlt upscale (half-res map present).
    #[cfg(windows)]
    scale_src: Option<PresentSurface>,
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
    unsafe fn create_surface(
        width: i32,
        height: i32,
        screen_dc: windows::Win32::Graphics::Gdi::HDC,
    ) -> Option<PresentSurface> {
        use windows::Win32::Graphics::Gdi::{
            CreateCompatibleDC, CreateDIBSection, SelectObject, BITMAPINFO, BITMAPINFOHEADER,
            BI_RGB, DIB_RGB_COLORS,
        };

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
                biCompression: BI_RGB.0,
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
        Some(PresentSurface {
            width,
            height,
            mem_dc,
            dib,
            old_obj,
            bits: bits as *mut u8,
        })
    }

    #[cfg(windows)]
    unsafe fn ensure_surface(
        &mut self,
        hwnd_key: isize,
        width: i32,
        height: i32,
        screen_dc: windows::Win32::Graphics::Gdi::HDC,
    ) -> Option<&PresentSurface> {
        let needs_new = match self.entries.get(&hwnd_key) {
            Some(s) => s.width != width || s.height != height || s.bits.is_null(),
            None => true,
        };
        if needs_new {
            self.entries.remove(&hwnd_key);
            let surf = Self::create_surface(width, height, screen_dc)?;
            self.entries.insert(hwnd_key, surf);
        }
        self.entries.get(&hwnd_key)
    }

    #[cfg(windows)]
    unsafe fn ensure_scale_src(
        &mut self,
        width: i32,
        height: i32,
        screen_dc: windows::Win32::Graphics::Gdi::HDC,
    ) -> Option<&PresentSurface> {
        let needs_new = match &self.scale_src {
            Some(s) => s.width != width || s.height != height || s.bits.is_null(),
            None => true,
        };
        if needs_new {
            self.scale_src = None;
            self.scale_src = Self::create_surface(width, height, screen_dc);
        }
        self.scale_src.as_ref()
    }
}

/// Push BGRA with UpdateLayeredWindow. When `dirty` has rects, only those
/// rectangles are copied into the cached DIB and each is passed as `prcDirty`
/// via `UpdateLayeredWindowIndirect` (rev 22 — per-car; union was often full-frame).
#[cfg(windows)]
pub fn present_bgra_dirty(
    cache: &mut PresentCache,
    hwnd: isize,
    width: i32,
    height: i32,
    bgra: &[u8],
    dirty: &[(i32, i32, i32, i32)],
) {
    if width <= 0 || height <= 0 || hwnd == 0 {
        return;
    }
    let need = (width as usize) * (height as usize) * 4;
    if bgra.len() < need {
        return;
    }
    ensure_layered(hwnd);

    use windows::Win32::Foundation::{COLORREF, HWND, POINT, RECT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        UpdateLayeredWindow, UpdateLayeredWindowIndirect, ULW_ALPHA, UPDATELAYEREDWINDOWINFO,
    };

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

        let mut used_dirty = false;
        if !dirty.is_empty() {
            let pitch = (width as usize) * 4;
            let mut area: i64 = 0;
            for &(dx, dy, dw, dh) in dirty {
                let x0 = dx.max(0).min(width);
                let y0 = dy.max(0).min(height);
                let x1 = (dx + dw).max(0).min(width);
                let y1 = (dy + dh).max(0).min(height);
                let rw = x1 - x0;
                let rh = y1 - y0;
                if rw <= 0 || rh <= 0 {
                    continue;
                }
                area += (rw as i64) * (rh as i64);
            }
            let full = (width as i64) * (height as i64);
            // Prefer many small Indirects over one full ULW when total ink is modest.
            if area > 0 && area < full * 3 / 5 {
                for &(dx, dy, dw, dh) in dirty {
                    let x0 = dx.max(0).min(width);
                    let y0 = dy.max(0).min(height);
                    let x1 = (dx + dw).max(0).min(width);
                    let y1 = (dy + dh).max(0).min(height);
                    let rw = x1 - x0;
                    let rh = y1 - y0;
                    if rw <= 0 || rh <= 0 {
                        continue;
                    }
                    for row in 0..rh as usize {
                        let off = ((y0 as usize + row) * pitch) + (x0 as usize) * 4;
                        let n = (rw as usize) * 4;
                        std::ptr::copy_nonoverlapping(
                            bgra.as_ptr().add(off),
                            surf.bits.add(off),
                            n,
                        );
                    }
                    let dirty_rc = RECT {
                        left: x0,
                        top: y0,
                        right: x1,
                        bottom: y1,
                    };
                    let info = UPDATELAYEREDWINDOWINFO {
                        cbSize: std::mem::size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
                        hdcDst: screen_dc,
                        pptDst: std::ptr::null(),
                        psize: &size,
                        hdcSrc: surf.mem_dc,
                        pptSrc: &src_pt,
                        crKey: COLORREF(0),
                        pblend: &blend,
                        dwFlags: ULW_ALPHA,
                        prcDirty: &dirty_rc,
                    };
                    let _ = UpdateLayeredWindowIndirect(hwnd_win, &info);
                }
                used_dirty = true;
            }
        }

        if !used_dirty {
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), surf.bits, need);
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
        }
        let _ = ReleaseDC(None, screen_dc);
    }
}

/// One padded dirty rect per previous/current car sprite (rev 22).
pub fn car_sprites_dirty_rects(
    prev: &[MapCarSprite],
    cur: &[MapCarSprite],
    width: i32,
    height: i32,
) -> Vec<(i32, i32, i32, i32)> {
    let mut out = Vec::with_capacity(prev.len() + cur.len());
    if width <= 0 || height <= 0 {
        return out;
    }
    for car in prev.iter().chain(cur.iter()) {
        let pad = (car.r + 10.0).ceil() as i32;
        let cx = car.x.round() as i32;
        let cy = car.y.round() as i32;
        let x0 = (cx - pad).max(0);
        let y0 = (cy - pad).max(0);
        let x1 = (cx + pad).min(width);
        let y1 = (cy + pad).min(height);
        let rw = x1 - x0;
        let rh = y1 - y0;
        if rw > 0 && rh > 0 {
            out.push((x0, y0, rw, rh));
        }
    }
    out
}

/// Bounding dirty rect covering previous + current car sprites (with pad).
/// Returns `None` when a full-frame present is cheaper / required.
#[allow(dead_code)]
pub fn car_sprites_dirty_rect(
    prev: &[MapCarSprite],
    cur: &[MapCarSprite],
    width: i32,
    height: i32,
) -> Option<(i32, i32, i32, i32)> {
    let rects = car_sprites_dirty_rects(prev, cur, width, height);
    if rects.is_empty() {
        return None;
    }
    let mut x0 = i32::MAX;
    let mut y0 = i32::MAX;
    let mut x1 = i32::MIN;
    let mut y1 = i32::MIN;
    for &(x, y, w, h) in &rects {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x + w);
        y1 = y1.max(y + h);
    }
    let rw = x1 - x0;
    let rh = y1 - y0;
    if rw <= 0 || rh <= 0 {
        return None;
    }
    if (rw as i64) * (rh as i64) >= (width as i64) * (height as i64) * 2 / 5 {
        return None;
    }
    Some((x0, y0, rw, rh))
}

/// Push a pre-captured BGRA buffer with UpdateLayeredWindow (no GL).
/// Reuses a cached DIB/mem DC per hwnd when size is unchanged.
#[cfg(windows)]
pub fn present_bgra(cache: &mut PresentCache, hwnd: isize, width: i32, height: i32, bgra: &[u8]) {
    present_bgra_dirty(cache, hwnd, width, height, bgra, &[]);
}

/// Present half-res (or any src-sized) BGRA stretched to the HWND client size.
/// Uses GDI StretchBlt between 32bpp DIBs so the A channel is copied (COLORONCOLOR).
#[cfg(windows)]
pub fn present_bgra_scaled(
    cache: &mut PresentCache,
    hwnd: isize,
    dst_w: i32,
    dst_h: i32,
    src_w: i32,
    src_h: i32,
    bgra: &[u8],
) {
    if src_w == dst_w && src_h == dst_h {
        present_bgra(cache, hwnd, dst_w, dst_h, bgra);
        return;
    }
    if dst_w <= 0 || dst_h <= 0 || src_w <= 0 || src_h <= 0 || hwnd == 0 {
        return;
    }
    let need = (src_w as usize)
        .saturating_mul(src_h as usize)
        .saturating_mul(4);
    if bgra.len() < need {
        return;
    }
    ensure_layered(hwnd);

    use windows::Win32::Foundation::{COLORREF, HWND, POINT, SIZE};
    use windows::Win32::Graphics::Gdi::{
        GetDC, ReleaseDC, SetStretchBltMode, StretchBlt, AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION,
        COLORONCOLOR, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{UpdateLayeredWindow, ULW_ALPHA};

    unsafe {
        let hwnd_win = HWND(hwnd as *mut _);
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return;
        }

        let src_dc;
        {
            let Some(src) = cache.ensure_scale_src(src_w, src_h, screen_dc) else {
                let _ = ReleaseDC(None, screen_dc);
                return;
            };
            std::ptr::copy_nonoverlapping(bgra.as_ptr(), src.bits, need);
            src_dc = src.mem_dc;
        }

        let Some(dst) = cache.ensure_surface(hwnd, dst_w, dst_h, screen_dc) else {
            let _ = ReleaseDC(None, screen_dc);
            return;
        };
        let dst_dc = dst.mem_dc;

        // Nearest-neighbor upscale — sharper than HALFTONE for overlay dots/text.
        let _ = SetStretchBltMode(dst_dc, COLORONCOLOR);
        let stretched = StretchBlt(
            dst_dc,
            0,
            0,
            dst_w,
            dst_h,
            Some(src_dc),
            0,
            0,
            src_w,
            src_h,
            SRCCOPY,
        );
        if !stretched.as_bool() {
            let _ = ReleaseDC(None, screen_dc);
            return;
        }

        let size = SIZE {
            cx: dst_w,
            cy: dst_h,
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
            Some(dst_dc),
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
pub fn present_bgra_scaled(
    _cache: &mut PresentCache,
    _hwnd: isize,
    _dst_w: i32,
    _dst_h: i32,
    _src_w: i32,
    _src_h: i32,
    _bgra: &[u8],
) {
}

#[cfg(not(windows))]
#[allow(dead_code)]
pub fn present_bgra_dirty(
    _cache: &mut PresentCache,
    _hwnd: isize,
    _width: i32,
    _height: i32,
    _bgra: &[u8],
    _dirty: &[(i32, i32, i32, i32)],
) {
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
