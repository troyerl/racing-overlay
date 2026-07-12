use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "radio_tower";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let accent = ctx.cfg.color(SECTION, "faster", "#46df7a");

    label(
        ui,
        Pos2::new(rect.center().x, rect.top() + pad + 12.0),
        Align2::CENTER_TOP,
        "RADIO",
        12.0,
        muted,
        true,
    );

    // Simple tower glyph
    let cx = rect.center().x;
    let cy = rect.center().y + 8.0;
    let stroke = egui::Stroke::new(2.0_f32, accent);
    ui.painter()
        .line_segment([Pos2::new(cx, cy - 40.0), Pos2::new(cx, cy + 20.0)], stroke);
    ui.painter().line_segment(
        [Pos2::new(cx - 18.0, cy + 20.0), Pos2::new(cx + 18.0, cy + 20.0)],
        stroke,
    );
    ui.painter().circle_stroke(Pos2::new(cx, cy - 40.0), 6.0, stroke);
    if ctx.frame.radio_name.is_some() {
        ui.painter()
            .circle_filled(Pos2::new(cx, cy - 40.0), 4.0, accent);
    }

    let name = ctx
        .frame
        .radio_name
        .as_deref()
        .unwrap_or("Listening…");
    label(
        ui,
        Pos2::new(cx, rect.bottom() - pad - 4.0),
        Align2::CENTER_BOTTOM,
        name,
        14.0,
        if ctx.frame.radio_name.is_some() {
            text
        } else {
            muted
        },
        true,
    );
}
