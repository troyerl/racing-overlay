use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "laptime_log";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let mut y = card.top() + pad;
    let hh = (card.height() * 0.1).max(22.0);
    draw_section_header(
        ui,
        ctx.cfg,
        SECTION,
        egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, hh),
        ),
        "LAP LOG",
        radius,
    );
    y += hh + 4.0;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let rh = 28.0_f32;
    label(
        ui,
        Pos2::new(card.left() + pad + 6.0, y + 10.0),
        Align2::LEFT_CENTER,
        "LAP",
        11.0,
        muted,
        true,
    );
    label(
        ui,
        Pos2::new(card.left() + pad + 60.0, y + 10.0),
        Align2::LEFT_CENTER,
        "TIME",
        11.0,
        muted,
        true,
    );
    label(
        ui,
        Pos2::new(card.right() - pad - 6.0, y + 10.0),
        Align2::RIGHT_CENTER,
        "Δ",
        11.0,
        muted,
        true,
    );
    y += 22.0;
    for row in &ctx.frame.lap_log {
        if y + rh > card.bottom() - pad {
            break;
        }
        label(
            ui,
            Pos2::new(card.left() + pad + 6.0, y + rh * 0.5),
            Align2::LEFT_CENTER,
            &format!("{}", row.lap),
            13.0,
            muted,
            false,
        );
        label(
            ui,
            Pos2::new(card.left() + pad + 60.0, y + rh * 0.5),
            Align2::LEFT_CENTER,
            &row.time,
            14.0,
            text,
            true,
        );
        label(
            ui,
            Pos2::new(card.right() - pad - 6.0, y + rh * 0.5),
            Align2::RIGHT_CENTER,
            &row.delta,
            13.0,
            text,
            false,
        );
        y += rh;
    }
}
