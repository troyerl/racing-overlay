//! Shared panel chrome helpers (card / header / cells).

use crate::config::OverlayConfig;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Ui, Vec2};

pub fn panel_pad(h: f32) -> f32 {
    (h * 0.06).clamp(6.0, 14.0)
}

pub fn draw_card(ui: &mut Ui, cfg: &OverlayConfig, section: &str, rect: Rect) -> (Rect, f32) {
    let h = rect.height();
    let radius = (h * cfg.f64_key(section, "corner_radius_frac", 0.08) as f32).max(8.0);
    let top = cfg.color(section, "bg_top", "#1b1f26f2");
    let bottom = cfg.color(section, "bg_bottom", "#0f1216f2");
    let border = cfg.color(section, "border", "#ffffff28");

    // Vertical gradient via two rects (simple approximation).
    let mid = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.center().y));
    let low = Rect::from_min_max(Pos2::new(rect.min.x, rect.center().y), rect.max);
    ui.painter().rect_filled(mid, CornerRadius::same(radius as u8), top);
    ui.painter().rect_filled(low, CornerRadius {
        nw: 0,
        ne: 0,
        sw: radius as u8,
        se: radius as u8,
    }, bottom);
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(radius as u8),
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
            nw: radius_top as u8,
            ne: radius_top as u8,
            sw: 0,
            se: 0,
        },
        bg,
    );
    ui.painter().text(
        rect.left_center() + Vec2::new(10.0, 0.0),
        egui::Align2::LEFT_CENTER,
        title,
        egui::FontId::proportional((rect.height() * 0.55).clamp(11.0, 18.0)),
        cfg.color(section, "muted", "#8b93a1"),
    );
}

pub fn draw_dark_cell(ui: &mut Ui, cfg: &OverlayConfig, section: &str, rect: Rect, radius: f32) {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(radius as u8),
        cfg.color(section, "cell_dark", "#0b0e12"),
    );
}

/// Nested panel (top/bottom containers). Defaults match Python alpha chrome.
pub fn draw_panel_rect(ui: &mut Ui, cfg: &OverlayConfig, section: &str, rect: Rect) -> f32 {
    let frac = cfg.f64_key(section, "corner_radius_frac", 0.08) as f32;
    let radius = (rect.width().min(rect.height()) * frac).max(6.0);
    let top = cfg.color(section, "bg_top", "#1b1f26f2");
    let bottom = cfg.color(section, "bg_bottom", "#0f1216f2");
    let mid = Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.center().y));
    let low = Rect::from_min_max(Pos2::new(rect.min.x, rect.center().y), rect.max);
    ui.painter().rect_filled(mid, CornerRadius::same(radius as u8), top);
    ui.painter().rect_filled(
        low,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius as u8,
            se: radius as u8,
        },
        bottom,
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(radius as u8),
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
    let font = if bold {
        egui::FontId::new(size, egui::FontFamily::Proportional)
    } else {
        egui::FontId::proportional(size)
    };
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
    // Approximate a horizontal gradient with non-overlapping strips that fade to 0.
    let stops = [(0.0, 0.42), (0.35, 0.22), (0.72, 0.08), (1.0, 0.0)];
    for w in stops.windows(2) {
        let (f0, s0) = w[0];
        let (f1, s1) = w[1];
        let scale = (s0 + s1) * 0.5;
        if scale <= 0.001 {
            continue;
        }
        let x0 = rect.left() + rect.width() * f0;
        let x1 = rect.left() + rect.width() * f1;
        ui.painter().rect_filled(
            Rect::from_min_max(Pos2::new(x0, rect.top()), Pos2::new(x1, rect.bottom())),
            CornerRadius::ZERO,
            color_with_alpha(accent, (accent.a() as f32 * scale) as u8),
        );
    }
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
