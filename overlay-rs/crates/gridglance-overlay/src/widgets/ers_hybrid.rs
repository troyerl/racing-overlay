use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "ers_hybrid";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let accent = ctx.cfg.color(SECTION, "accent", "#70df7a");
    let pct = ctx.frame.ers_pct.unwrap_or(0.0) / 100.0;

    label(
        ui,
        Pos2::new(rect.left() + pad + 8.0, rect.top() + pad),
        Align2::LEFT_TOP,
        "ERS",
        12.0,
        muted,
        true,
    );
    label(
        ui,
        Pos2::new(rect.right() - pad - 8.0, rect.top() + pad),
        Align2::RIGHT_TOP,
        ctx.frame.ers_mode.as_deref().unwrap_or("—"),
        12.0,
        text,
        false,
    );

    let bar = egui::Rect::from_min_size(
        Pos2::new(rect.left() + pad, rect.center().y - 10.0),
        egui::vec2(rect.width() - 2.0 * pad, 20.0),
    );
    ui.painter().rect_filled(
        bar,
        egui::CornerRadius::same(6),
        ctx.cfg.color(SECTION, "track", "#ffffff18"),
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * pct.clamp(0.0, 1.0), bar.height())),
        egui::CornerRadius::same(6),
        accent,
    );
    label(
        ui,
        Pos2::new(rect.center().x, rect.bottom() - pad - 4.0),
        Align2::CENTER_BOTTOM,
        &format!("{:.0}%", pct * 100.0),
        16.0,
        text,
        true,
    );
}
