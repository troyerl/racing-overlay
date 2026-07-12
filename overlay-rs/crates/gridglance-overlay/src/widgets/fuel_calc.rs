use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "fuel_calc";

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
        &ctx.cfg.str_key(SECTION, "title", "FUEL"),
        radius,
    );
    y += hh + pad * 0.4;
    let f = ctx.frame;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let rows = [
        ("Fuel", format!("{:.1} L", f.fuel_l)),
        ("Laps", format!("{:.1}", f.laps_fuel)),
        ("Level", format!("{:.0}%", f.fuel_pct * 100.0)),
    ];
    let rh = (card.bottom() - pad - y) / rows.len() as f32;
    for (k, v) in rows {
        label(
            ui,
            Pos2::new(card.left() + pad + 8.0, y + rh * 0.5),
            Align2::LEFT_CENTER,
            k,
            rh * 0.35,
            muted,
            false,
        );
        label(
            ui,
            Pos2::new(card.right() - pad - 8.0, y + rh * 0.5),
            Align2::RIGHT_CENTER,
            &v,
            rh * 0.4,
            text,
            true,
        );
        y += rh;
    }
}
