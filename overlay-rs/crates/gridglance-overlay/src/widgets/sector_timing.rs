use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "sector_timing";

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
        "SECTORS",
        radius,
    );
    y += hh + pad * 0.5;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let sectors = &ctx.frame.sector_times;
    let n = sectors.len().max(3);
    let rh = (card.bottom() - pad - y) / n as f32;
    for i in 0..n {
        let val = sectors
            .get(i)
            .and_then(|s| *s)
            .map(|t| format!("{t:.3}"))
            .unwrap_or_else(|| "—".into());
        label(
            ui,
            Pos2::new(card.left() + pad + 8.0, y + rh * 0.5),
            Align2::LEFT_CENTER,
            &format!("S{}", i + 1),
            rh * 0.35,
            muted,
            true,
        );
        label(
            ui,
            Pos2::new(card.right() - pad - 8.0, y + rh * 0.5),
            Align2::RIGHT_CENTER,
            &val,
            rh * 0.4,
            text,
            true,
        );
        y += rh;
    }
}
