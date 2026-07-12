use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "pit_advisor";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let mut y = card.top() + pad;
    let hh = (card.height() * 0.14).max(22.0);
    draw_section_header(
        ui,
        ctx.cfg,
        SECTION,
        egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, hh),
        ),
        "PIT ADVISOR",
        radius,
    );
    y += hh + pad;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let laps = ctx
        .frame
        .pit_laps_to_go
        .map(|n| format!("{n}"))
        .unwrap_or_else(|| "—".into());
    let fuel = ctx
        .frame
        .pit_fuel_to_add
        .map(|n| format!("{n:.1} L"))
        .unwrap_or_else(|| "—".into());
    label(
        ui,
        Pos2::new(card.left() + pad + 8.0, y),
        Align2::LEFT_TOP,
        "Laps to pit",
        13.0,
        muted,
        false,
    );
    label(
        ui,
        Pos2::new(card.right() - pad - 8.0, y),
        Align2::RIGHT_TOP,
        &laps,
        22.0,
        text,
        true,
    );
    y += 36.0;
    label(
        ui,
        Pos2::new(card.left() + pad + 8.0, y),
        Align2::LEFT_TOP,
        "Fuel to add",
        13.0,
        muted,
        false,
    );
    label(
        ui,
        Pos2::new(card.right() - pad - 8.0, y),
        Align2::RIGHT_TOP,
        &fuel,
        22.0,
        text,
        true,
    );
}
