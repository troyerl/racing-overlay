use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "delta_bar";

fn signed_delta(d: Option<f64>) -> String {
    match d {
        Some(v) => format!("{v:+.2}"),
        None => "--.--".into(),
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let delta = ctx.frame.delta;
    let have = delta.is_some();
    let rng = ctx.cfg.f64_key(SECTION, "range", 1.0).max(0.001);
    let target = delta.map(|d| (d / rng).clamp(-1.0, 1.0)).unwrap_or(0.0) as f32;

    if ctx.cfg.bool_key(SECTION, "show_value", true) {
        let tcol = if !have || delta.unwrap().abs() < 0.005 {
            ctx.cfg.color(SECTION, "muted", "#8b93a1")
        } else if delta.unwrap() < 0.0 {
            ctx.cfg.color(SECTION, "faster", "#46df7a")
        } else {
            ctx.cfg.color(SECTION, "slower", "#ff5050")
        };
        label(
            ui,
            Pos2::new(rect.center().x, rect.top() + pad + rect.height() * 0.22),
            Align2::CENTER_CENTER,
            &signed_delta(delta),
            (rect.height() * 0.42).clamp(18.0, 48.0),
            tcol,
            true,
        );
    }

    let bar = egui::Rect::from_min_size(
        Pos2::new(rect.left() + pad, rect.top() + rect.height() * 0.62),
        egui::vec2(rect.width() - 2.0 * pad, rect.height() * 0.24),
    );
    let r = bar.height() / 2.0;
    ui.painter().rect_filled(
        bar,
        egui::CornerRadius::same(r as u8),
        ctx.cfg.color(SECTION, "track", "#ffffff18"),
    );
    if have && target.abs() > 0.001 {
        let cx = bar.center().x;
        let fill_w = (bar.width() * 0.5) * target.abs();
        let fill = if target < 0.0 {
            egui::Rect::from_min_max(
                Pos2::new(cx - fill_w, bar.top()),
                Pos2::new(cx, bar.bottom()),
            )
        } else {
            egui::Rect::from_min_max(
                Pos2::new(cx, bar.top()),
                Pos2::new(cx + fill_w, bar.bottom()),
            )
        };
        let col = if target < 0.0 {
            ctx.cfg.color(SECTION, "faster", "#46df7a")
        } else {
            ctx.cfg.color(SECTION, "slower", "#ff5050")
        };
        ui.painter()
            .rect_filled(fill, egui::CornerRadius::same(r as u8), col);
    }
    // Center tick
    ui.painter().line_segment(
        [
            Pos2::new(bar.center().x, bar.top()),
            Pos2::new(bar.center().x, bar.bottom()),
        ],
        egui::Stroke::new(2.0, ctx.cfg.color(SECTION, "text", "#f4f6f8")),
    );
}
