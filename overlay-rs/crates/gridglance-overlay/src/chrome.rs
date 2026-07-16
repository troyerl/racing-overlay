//! Shared panel chrome helpers (card / header / cells).

use crate::config::OverlayConfig;
use egui::{Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};

pub fn panel_pad(h: f32) -> f32 {
    (h * 0.08).max(8.0)
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        lerp_u8(a.r(), b.r(), t),
        lerp_u8(a.g(), b.g(), t),
        lerp_u8(a.b(), b.b(), t),
        lerp_u8(a.a(), b.a(), t),
    )
}

/// Mix `c` toward dark chrome (Python `soften_color`).
pub fn soften_color(c: Color32, toward: Color32, mix: f32) -> Color32 {
    let m = mix.clamp(0.0, 1.0);
    // Un-premultiply for mixing.
    let ca = c.a().max(1) as f32;
    let ta = toward.a().max(1) as f32;
    let r = (c.r() as f32 * 255.0 / ca) * (1.0 - m) + (toward.r() as f32 * 255.0 / ta) * m;
    let g = (c.g() as f32 * 255.0 / ca) * (1.0 - m) + (toward.g() as f32 * 255.0 / ta) * m;
    let b = (c.b() as f32 * 255.0 / ca) * (1.0 - m) + (toward.b() as f32 * 255.0 / ta) * m;
    Color32::from_rgba_unmultiplied(
        r.round().clamp(0.0, 255.0) as u8,
        g.round().clamp(0.0, 255.0) as u8,
        b.round().clamp(0.0, 255.0) as u8,
        c.a(),
    )
}

/// Light-on-dark or dark-on-light text for a solid BG (Python `contrast_text`).
pub fn contrast_text(bg: Color32) -> Color32 {
    let a = bg.a().max(1) as f32;
    let r = bg.r() as f32 * 255.0 / a;
    let g = bg.g() as f32 * 255.0 / a;
    let b = bg.b() as f32 * 255.0 / a;
    let lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255.0;
    if lum > 0.6 {
        Color32::from_rgb(20, 22, 26)
    } else {
        Color32::WHITE
    }
}

/// Horizontal inset so a square band stays inside the rounded corner silhouette.
fn corner_band_inset(r: f32, y: f32, top: f32, bottom: f32) -> f32 {
    if r <= 0.0 {
        return 0.0;
    }
    let dy = if y < top + r {
        r - (y - top)
    } else if y > bottom - r {
        r - (bottom - y)
    } else {
        return 0.0;
    };
    if dy <= 0.0 || dy >= r {
        return 0.0;
    }
    r - (r * r - dy * dy).sqrt()
}

/// Smooth vertical `top`→`bottom` fill with rounded corners (Python QLinearGradient parity).
fn fill_vertical_gradient(ui: &mut Ui, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    const BANDS: usize = 24;
    let r = radius
        .min(rect.height() * 0.5)
        .min(rect.width() * 0.5)
        .max(0.0);
    let ru = r.round().clamp(0.0, 255.0) as u8;
    let h = rect.height().max(1.0);

    ui.painter().rect_filled(rect, CornerRadius::same(ru), top);

    for i in 0..BANDS {
        let t0 = i as f32 / BANDS as f32;
        let t1 = (i + 1) as f32 / BANDS as f32;
        let y0 = rect.top() + h * t0;
        let y1 = (rect.top() + h * t1 + 0.5).min(rect.bottom());
        let mid_y = (y0 + y1) * 0.5;
        let inset = corner_band_inset(r, mid_y, rect.top(), rect.bottom());
        let band = Rect::from_min_max(
            Pos2::new(rect.left() + inset, y0),
            Pos2::new(rect.right() - inset, y1),
        );
        if band.width() <= 0.0 || band.height() <= 0.0 {
            continue;
        }
        let col = lerp_color(top, bottom, (t0 + t1) * 0.5);
        ui.painter().rect_filled(band, CornerRadius::ZERO, col);
    }
}

pub fn draw_card(ui: &mut Ui, cfg: &OverlayConfig, section: &str, rect: Rect) -> (Rect, f32) {
    let h = rect.height();
    // Python: max(8, h * frac) — floor stays 8 even when frac is 0.
    let radius = (h * cfg.f64_key(section, "corner_radius_frac", 0.0) as f32).max(8.0);
    let top = cfg.color(section, "bg_top", "#1b1f26f2");
    let bottom = cfg.color(section, "bg_bottom", "#0f1216f2");
    let border = cfg.color(section, "border", "#ffffff28");

    fill_vertical_gradient(ui, rect, radius, top, bottom);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8),
        Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
    (rect, radius)
}

pub fn draw_section_header(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    title: &str,
    radius_top: f32,
) {
    let bg = cfg.color(section, "header_bg", "#0b0e12bb");
    ui.painter().rect_filled(
        rect,
        CornerRadius {
            nw: radius_top.round().clamp(0.0, 255.0) as u8,
            ne: radius_top.round().clamp(0.0, 255.0) as u8,
            sw: 0,
            se: 0,
        },
        bg,
    );
    // Python draw_edge_band: top + bottom hairlines.
    let edge = color_with_alpha(cfg.color(section, "border", "#ffffff28"), 70);
    let inset = radius_top.max(0.0);
    ui.painter().line_segment(
        [
            Pos2::new(rect.left() + inset, rect.top() + 0.5),
            Pos2::new(rect.right() - inset, rect.top() + 0.5),
        ],
        Stroke::new(1.0_f32, edge),
    );
    ui.painter().line_segment(
        [
            Pos2::new(rect.left() + inset, rect.bottom() - 0.5),
            Pos2::new(rect.right() - inset, rect.bottom() - 0.5),
        ],
        Stroke::new(1.0_f32, edge),
    );
    let size = (rect.height() * 0.55).max(10.0);
    label(
        ui,
        rect.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        size,
        cfg.color(section, "title", "#f4f6f8"),
        true,
    );
}

pub fn draw_dark_cell(ui: &mut Ui, cfg: &OverlayConfig, section: &str, rect: Rect, radius: f32) {
    let r = radius.round().clamp(0.0, 255.0) as u8;
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(r),
        cfg.color(section, "cell_dark", "#0b0e12"),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(r),
        Stroke::new(1.0_f32, cfg.color(section, "cell_border", "#ffffff20")),
        egui::StrokeKind::Inside,
    );
}

/// Nested panel (top/bottom containers). Defaults match Python alpha chrome.
pub fn draw_panel_rect(ui: &mut Ui, cfg: &OverlayConfig, section: &str, rect: Rect) -> f32 {
    let frac = cfg.f64_key(section, "corner_radius_frac", 0.0) as f32;
    // Python: min(w,h) * frac — frac 0 yields sharp corners.
    let radius = rect.width().min(rect.height()) * frac;
    let top = cfg.color(section, "bg_top", "#1b1f26f2");
    let bottom = cfg.color(section, "bg_bottom", "#0f1216f2");
    fill_vertical_gradient(ui, rect, radius, top, bottom);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8),
        Stroke::new(1.0_f32, cfg.color(section, "border", "#ffffff28")),
        egui::StrokeKind::Inside,
    );
    radius
}

pub fn label(
    ui: &mut Ui,
    pos: Pos2,
    align: egui::Align2,
    text: &str,
    size: f32,
    color: Color32,
    bold: bool,
) {
    // Real Bold face when requested (registered in icons::install_fonts).
    let family = if bold {
        FontFamily::Name(crate::icons::BOLD_FAMILY.into())
    } else {
        FontFamily::Proportional
    };
    let font = FontId::new(size.max(1.0), family);
    ui.painter().text(pos, align, text, font, color);
}

pub fn ease(cur: f32, target: f32, dt: f32, tau: f32) -> f32 {
    if !tau.is_finite() || tau <= 1e-6 {
        return target;
    }
    let a = (1.0 - (-dt / tau).exp()).clamp(0.0, 1.0);
    cur + (target - cur) * a
}

/// Rebuild a color with a new alpha. egui `Color32` stores premultiplied RGB;
/// feeding `.r()/.g()/.b()` into `from_rgba_unmultiplied` double-multiplies and
/// muddies translucent washes — un-premultiply first.
pub fn color_with_alpha(c: Color32, a: u8) -> Color32 {
    if a == 0 {
        return Color32::TRANSPARENT;
    }
    let ca = c.a();
    if ca == 0 {
        return Color32::TRANSPARENT;
    }
    let r = (c.r() as u16 * 255 / ca as u16).min(255) as u8;
    let g = (c.g() as u16 * 255 / ca as u16).min(255) as u8;
    let b = (c.b() as u16 * 255 / ca as u16).min(255) as u8;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Soft left→right row wash matching Python `_draw_row_tint`.
pub fn draw_row_tint(ui: &mut Ui, rect: Rect, accent: Color32) {
    let h = rect.height();
    let stripe_w = (h * 0.07).max(2.5);
    let edge_a = (accent.a() as u16 + 50).min(255) as u8;
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left(), rect.top() + h * 0.12),
            Vec2::new(stripe_w, h * 0.76),
        ),
        CornerRadius::same(2),
        color_with_alpha(accent, edge_a),
    );

    let stops = [(0.0_f32, 0.42_f32), (0.35, 0.22), (0.72, 0.08), (1.0, 0.0)];
    let mut mesh = egui::Mesh::default();
    for w in stops.windows(2) {
        let (f0, s0) = w[0];
        let (f1, s1) = w[1];
        let x0 = rect.left() + rect.width() * f0;
        let x1 = rect.left() + rect.width() * f1;
        let c0 = color_with_alpha(accent, (accent.a() as f32 * s0) as u8);
        let c1 = color_with_alpha(accent, (accent.a() as f32 * s1) as u8);
        let i = mesh.vertices.len() as u32;
        mesh.vertices.push(egui::epaint::Vertex {
            pos: Pos2::new(x0, rect.top()),
            uv: egui::epaint::WHITE_UV,
            color: c0,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: Pos2::new(x1, rect.top()),
            uv: egui::epaint::WHITE_UV,
            color: c1,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: Pos2::new(x1, rect.bottom()),
            uv: egui::epaint::WHITE_UV,
            color: c1,
        });
        mesh.vertices.push(egui::epaint::Vertex {
            pos: Pos2::new(x0, rect.bottom()),
            uv: egui::epaint::WHITE_UV,
            color: c0,
        });
        mesh.indices
            .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }
    ui.painter().add(egui::Shape::mesh(mesh));

    let rim = color_with_alpha(accent, ((accent.a() as f32) * 0.55) as u8);
    let inset = rect.shrink(0.5);
    ui.painter().line_segment(
        [inset.left_top(), inset.right_top()],
        Stroke::new(1.0_f32, rim),
    );
    ui.painter().line_segment(
        [inset.left_bottom(), inset.right_bottom()],
        Stroke::new(1.0_f32, rim),
    );
}

/// Allocate the full available rect and return it (for custom painting).
pub fn full_rect(ui: &mut Ui) -> Rect {
    let size = ui.available_size();
    let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
    rect
}
