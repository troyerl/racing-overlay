use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "pit_board";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let player = ctx
        .frame
        .cars
        .iter()
        .find(|c| c.is_player);
    let pos = player
        .map(|c| format!("P{}", c.position))
        .unwrap_or_else(|| "P—".into());
    let gap = player
        .map(|c| c.gap.clone())
        .unwrap_or_else(|| "—".into());

    label(
        ui,
        Pos2::new(rect.left() + pad + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        &pos,
        (rect.height() * 0.5).clamp(20.0, 48.0),
        text,
        true,
    );
    label(
        ui,
        Pos2::new(rect.center().x, rect.center().y - 10.0),
        Align2::CENTER_CENTER,
        "PIT BOARD",
        12.0,
        muted,
        true,
    );
    label(
        ui,
        Pos2::new(rect.right() - pad - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        &gap,
        (rect.height() * 0.36).clamp(16.0, 36.0),
        text,
        true,
    );
}
