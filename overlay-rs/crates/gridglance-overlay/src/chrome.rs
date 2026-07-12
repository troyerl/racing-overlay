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

/// Allocate the full available rect and return it (for custom painting).
pub fn full_rect(ui: &mut Ui) -> Rect {
    let size = ui.available_size();
    let (rect, _resp) = ui.allocate_exact_size(size, Sense::hover());
    rect
}
