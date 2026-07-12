use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label};
use egui::{Align2, Pos2, Stroke, Ui};

const SECTION: &str = "radar";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    if ctx.cfg.bool_key(SECTION, "show_panel", false) {
        draw_card(ui, ctx.cfg, SECTION, rect);
    }
    let cx = rect.center().x;
    let cy = rect.center().y;
    let r = (rect.width().min(rect.height()) * 0.38).min(90.0);
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let danger = ctx.cfg.color(SECTION, "slower", "#ff5050");
    let ok = ctx.cfg.color(SECTION, "faster", "#46df7a");
    let track = ctx.cfg.color(SECTION, "track", "#ffffff18");

    ui.painter()
        .circle_stroke(Pos2::new(cx, cy), r, Stroke::new(2.0_f32, track));
    ui.painter()
        .circle_stroke(Pos2::new(cx, cy), r * 0.55, Stroke::new(1.0_f32, track));
    // Player
    ui.painter()
        .circle_filled(Pos2::new(cx, cy), 6.0, ctx.cfg.color(SECTION, "text", "#f4f6f8"));

    let left_col = if ctx.frame.radar_left { danger } else { ok };
    let right_col = if ctx.frame.radar_right { danger } else { ok };
    ui.painter()
        .circle_filled(Pos2::new(cx - r * 0.7, cy), 8.0, left_col);
    ui.painter()
        .circle_filled(Pos2::new(cx + r * 0.7, cy), 8.0, right_col);

    // Nearby cars as dots on ring by lap_dist relative to player
    let player_pct = ctx.frame.player_lap_dist_pct;
    for car in &ctx.frame.cars {
        if car.is_player {
            continue;
        }
        let mut d = car.lap_dist_pct - player_pct;
        if d > 0.5 {
            d -= 1.0;
        }
        if d < -0.5 {
            d += 1.0;
        }
        if d.abs() > 0.12 {
            continue;
        }
        let angle = d * std::f32::consts::TAU;
        let px = cx + angle.sin() * r * 0.85;
        let py = cy - angle.cos() * r * 0.85;
        ui.painter().circle_filled(
            Pos2::new(px, py),
            5.0,
            if car.on_pit { muted } else { danger },
        );
    }

    label(
        ui,
        Pos2::new(cx, rect.bottom() - 10.0),
        Align2::CENTER_BOTTOM,
        "RADAR",
        11.0,
        muted,
        true,
    );
}
