use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "lap_compare";

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
        "LAP COMPARE",
        radius,
    );
    y += hh + pad;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let delta = ctx
        .frame
        .delta
        .map(|d| format!("{d:+.3}"))
        .unwrap_or_else(|| "—".into());
    let col = match ctx.frame.delta {
        Some(d) if d < -0.005 => ctx.cfg.color(SECTION, "faster", "#46df7a"),
        Some(d) if d > 0.005 => ctx.cfg.color(SECTION, "slower", "#ff5050"),
        _ => muted,
    };
    label(
        ui,
        Pos2::new(card.center().x, y + 8.0),
        Align2::CENTER_TOP,
        "vs reference",
        12.0,
        muted,
        false,
    );
    label(
        ui,
        Pos2::new(card.center().x, card.center().y + 10.0),
        Align2::CENTER_CENTER,
        &delta,
        (card.height() * 0.28).clamp(22.0, 42.0),
        col,
        true,
    );
    let _ = text;
}
