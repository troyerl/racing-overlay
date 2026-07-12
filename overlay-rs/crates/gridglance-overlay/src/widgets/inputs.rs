use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "inputs";

fn bar(ui: &mut Ui, rect: egui::Rect, frac: f32, color: egui::Color32, track: egui::Color32) {
    let r = (rect.height() / 2.0).min(6.0);
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(r as u8), track);
    let w = rect.width() * frac.clamp(0.0, 1.0);
    if w > 0.5 {
        let fill = egui::Rect::from_min_size(rect.min, egui::vec2(w, rect.height()));
        ui.painter()
            .rect_filled(fill, egui::CornerRadius::same(r as u8), color);
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let f = ctx.frame;
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let track = ctx.cfg.color(SECTION, "track", "#ffffff18");
    let rows = [
        ("THR", f.throttle, ctx.cfg.color(SECTION, "faster", "#46df7a")),
        ("BRK", f.brake, ctx.cfg.color(SECTION, "slower", "#ff5050")),
        ("CLT", f.clutch, ctx.cfg.color(SECTION, "accent", "#70df7a")),
    ];
    let n = rows.len() as f32;
    let avail = rect.height() - 2.0 * pad;
    let rh = avail / n;
    let mut y = rect.top() + pad;
    for (name, val, col) in rows {
        let row = egui::Rect::from_min_size(
            Pos2::new(rect.left() + pad, y),
            egui::vec2(rect.width() - 2.0 * pad, rh),
        );
        label(
            ui,
            Pos2::new(row.left(), row.center().y),
            Align2::LEFT_CENTER,
            name,
            rh * 0.35,
            muted,
            true,
        );
        let bar_r = egui::Rect::from_min_size(
            Pos2::new(row.left() + rh * 1.4, row.center().y - rh * 0.18),
            egui::vec2(row.width() - rh * 2.8, rh * 0.36),
        );
        bar(ui, bar_r, val, col, track);
        label(
            ui,
            Pos2::new(row.right(), row.center().y),
            Align2::RIGHT_CENTER,
            &format!("{:.0}%", val * 100.0),
            rh * 0.32,
            text,
            false,
        );
        y += rh;
    }
}
