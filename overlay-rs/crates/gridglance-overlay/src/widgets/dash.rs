use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "dash";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let f = ctx.frame;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let accent = ctx.cfg.color(SECTION, "accent", "#70df7a");

    let speed_kmh = f.speed_mps * 3.6;
    label(
        ui,
        Pos2::new(rect.left() + pad + 8.0, rect.center().y - 8.0),
        Align2::LEFT_CENTER,
        &format!("{:.0}", speed_kmh),
        (rect.height() * 0.48).clamp(22.0, 56.0),
        text,
        true,
    );
    label(
        ui,
        Pos2::new(rect.left() + pad + 8.0, rect.bottom() - pad - 4.0),
        Align2::LEFT_BOTTOM,
        "km/h",
        12.0,
        muted,
        false,
    );

    label(
        ui,
        rect.center(),
        Align2::CENTER_CENTER,
        &format!("{}", f.gear),
        (rect.height() * 0.55).clamp(28.0, 64.0),
        accent,
        true,
    );

    label(
        ui,
        Pos2::new(rect.right() - pad - 8.0, rect.center().y - 8.0),
        Align2::RIGHT_CENTER,
        &format!("{:.0}", f.rpm),
        (rect.height() * 0.36).clamp(18.0, 40.0),
        text,
        true,
    );
    label(
        ui,
        Pos2::new(rect.right() - pad - 8.0, rect.bottom() - pad - 4.0),
        Align2::RIGHT_BOTTOM,
        "RPM",
        12.0,
        muted,
        false,
    );

    // RPM bar
    let bar = egui::Rect::from_min_size(
        Pos2::new(rect.left() + pad, rect.top() + pad * 0.6),
        egui::vec2(rect.width() - 2.0 * pad, 6.0),
    );
    ui.painter().rect_filled(
        bar,
        egui::CornerRadius::same(3),
        ctx.cfg.color(SECTION, "track", "#ffffff18"),
    );
    let frac = (f.rpm / 8000.0).clamp(0.0, 1.0);
    ui.painter().rect_filled(
        egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * frac, bar.height())),
        egui::CornerRadius::same(3),
        accent,
    );
}
