use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "tire_panel";
const LABELS: [&str; 4] = ["LF", "RF", "LR", "RR"];

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let mut y = card.top() + pad;
    let hh = (card.height() * 0.12).max(22.0);
    draw_section_header(
        ui,
        ctx.cfg,
        SECTION,
        egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, hh),
        ),
        "TIRES",
        radius,
    );
    y += hh + pad * 0.4;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let cell_w = (card.width() - 2.0 * pad) / 2.0;
    let cell_h = (card.bottom() - pad - y) / 2.0;
    for (i, lab) in LABELS.iter().enumerate() {
        let col = i % 2;
        let row = i / 2;
        let cell = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad + col as f32 * cell_w, y + row as f32 * cell_h),
            egui::vec2(cell_w - 4.0, cell_h - 4.0),
        );
        ui.painter().rect_filled(
            cell,
            egui::CornerRadius::same(6),
            ctx.cfg.color(SECTION, "cell_dark", "#0b0e12"),
        );
        label(
            ui,
            Pos2::new(cell.left() + 8.0, cell.top() + 10.0),
            Align2::LEFT_TOP,
            lab,
            12.0,
            muted,
            true,
        );
        label(
            ui,
            Pos2::new(cell.center().x, cell.center().y),
            Align2::CENTER_CENTER,
            &format!("{:.0}°", ctx.frame.tire_temps[i]),
            18.0,
            text,
            true,
        );
        label(
            ui,
            Pos2::new(cell.center().x, cell.bottom() - 10.0),
            Align2::CENTER_BOTTOM,
            &format!("{:.1} psi", ctx.frame.tire_pressures[i]),
            11.0,
            muted,
            false,
        );
    }
}
