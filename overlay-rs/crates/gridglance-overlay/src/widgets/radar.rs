//! Directional proximity radar HUD (Python `radar.py` parity).

use super::WidgetCtx;
use crate::chrome::{color_with_alpha, draw_card, ease, full_rect, label};
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};

const SECTION: &str = "radar";

#[derive(Clone, Default)]
struct RadarAnim {
    left: f32,
    right: f32,
    ahead: f32,
    behind: f32,
    left_pos: f32,
    right_pos: f32,
    last_ms: f64,
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    if ctx.cfg.bool_key(SECTION, "show_panel", false) {
        draw_card(ui, ctx.cfg, SECTION, rect);
    }

    let w = rect.width();
    let h = rect.height();
    let cx = rect.center().x;
    let cy = rect.center().y;

    let car_w = (w * size_frac(ctx, "car_w", 0.13)).max(12.0);
    let car_h = (h * size_frac(ctx, "car_h", 0.20)).max(24.0);
    let bar_h = car_h * size_frac(ctx, "bar_h", 0.78);
    let inner = car_w * 0.75;
    let nose_len = h * size_frac(ctx, "nose_len", 0.16);
    let glow_w = w * size_frac(ctx, "glow_w", 0.17);

    let d = &ctx.frame.radar;
    let show_front = ctx.cfg.bool_key(SECTION, "show_front", true);
    let show_rear = ctx.cfg.bool_key(SECTION, "show_rear", true);
    let side_tau = ctx.cfg.f64_key(SECTION, "ease_side_tau", 0.10) as f32;
    let glow_tau = ctx.cfg.f64_key(SECTION, "ease_glow_tau", 0.13) as f32;
    let prox = ctx.cfg.bool_key(SECTION, "side_proximity_color", false);

    let id = egui::Id::new("radar_anim");
    let now = ui.input(|i| i.time);
    let mut a = ui
        .ctx()
        .data_mut(|data| data.get_temp::<RadarAnim>(id).unwrap_or_default());
    let dt = if a.last_ms > 0.0 {
        ((now - a.last_ms) as f32).clamp(0.0, 0.1)
    } else {
        0.016
    };
    a.last_ms = now;

    let t_left = if d.left { 1.0 } else { 0.0 };
    let t_right = if d.right { 1.0 } else { 0.0 };
    let t_ahead = if show_front {
        d.ahead.unwrap_or(0.0)
    } else {
        0.0
    };
    let t_behind = if show_rear {
        d.behind.unwrap_or(0.0)
    } else {
        0.0
    };
    a.left = ease(a.left, t_left, dt, side_tau);
    a.right = ease(a.right, t_right, dt, side_tau);
    a.ahead = ease(a.ahead, t_ahead, dt, glow_tau);
    a.behind = ease(a.behind, t_behind, dt, glow_tau);
    a.left_pos = ease(a.left_pos, d.left_pos, dt, side_tau);
    a.right_pos = ease(a.right_pos, d.right_pos, dt, side_tau);
    ui.ctx().data_mut(|data| data.insert_temp(id, a.clone()));

    if show_front && a.ahead > 0.01 {
        v_glow(
            ui,
            ctx,
            cx,
            cy - car_h * 0.45,
            rect.top() + h * 0.06,
            a.ahead,
            glow_w,
            true,
        );
    }
    if show_rear && a.behind > 0.01 {
        v_glow(
            ui,
            ctx,
            cx,
            cy + car_h * 0.45,
            rect.top() + h * 0.94,
            a.behind,
            glow_w,
            false,
        );
    }

    let marker_h = bar_h.max(18.0);
    let travel = (h * 0.5 - marker_h * 0.5 - h * 0.06).max(0.0);

    if a.left > 0.01 {
        let yc = cy - a.left_pos * travel;
        side_marker(
            ui,
            ctx,
            rect.left() + w * 0.07,
            cx - inner,
            yc,
            marker_h,
            d.left2,
            true,
            a.left,
            if prox {
                Some(1.0 - a.left_pos.abs())
            } else {
                None
            },
            &d.left_label,
        );
    }
    if a.right > 0.01 {
        let yc = cy - a.right_pos * travel;
        side_marker(
            ui,
            ctx,
            cx + inner,
            rect.left() + w * 0.93,
            yc,
            marker_h,
            d.right2,
            false,
            a.right,
            if prox {
                Some(1.0 - a.right_pos.abs())
            } else {
                None
            },
            &d.right_label,
        );
    }

    if ctx.cfg.bool_key(SECTION, "show_clear_timer", false) {
        if let Some(secs) = d.clear_secs {
            if secs >= 0.0 {
                let txt = format!("Clear {secs:.0}s");
                label(
                    ui,
                    Pos2::new(cx, rect.bottom() - h * 0.08),
                    Align2::CENTER_CENTER,
                    &txt,
                    10.0,
                    ctx.cfg.color(SECTION, "nose", "#f4f6f8"),
                    true,
                );
            }
        }
    }

    if ctx.cfg.bool_key(SECTION, "show_axis", true) {
        let axis = ctx.cfg.color(SECTION, "axis", "#ffffff28");
        let sw = (w * 0.006).max(1.0);
        ui.painter().line_segment(
            [
                Pos2::new(rect.left() + w * 0.08, cy),
                Pos2::new(rect.left() + w * 0.92, cy),
            ],
            Stroke::new(sw, axis),
        );
        ui.painter().line_segment(
            [
                Pos2::new(cx, rect.top() + h * 0.10),
                Pos2::new(cx, rect.top() + h * 0.90),
            ],
            Stroke::new(sw, axis),
        );
    }
    if ctx.cfg.bool_key(SECTION, "show_nose", true) {
        let nose = ctx.cfg.color(SECTION, "nose", "#f4f6f8");
        ui.painter().line_segment(
            [
                Pos2::new(cx, cy - car_h * 0.5),
                Pos2::new(cx, cy - car_h * 0.5 - nose_len),
            ],
            Stroke::new((w * 0.012).max(1.5), nose),
        );
    }

    // Center car silhouette
    let car = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(car_w, car_h));
    ui.painter().rect_filled(
        car,
        CornerRadius::same((car_w * 0.4) as u8),
        ctx.cfg.color(SECTION, "car", "#f4f6f8"),
    );
}

fn size_frac(ctx: &WidgetCtx<'_>, key: &str, default: f32) -> f32 {
    ctx.cfg
        .section(SECTION)
        .get("sizes")
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn prox_color(ctx: &WidgetCtx<'_>, closeness: f32, alpha: u8) -> Color32 {
    let c = closeness.clamp(0.0, 1.0);
    let y = ctx.cfg.color(SECTION, "yellow", "#ffd23a");
    let r = ctx.cfg.color(SECTION, "red", "#ff5050");
    Color32::from_rgba_unmultiplied(
        (y.r() as f32 + (r.r() as f32 - y.r() as f32) * c) as u8,
        (y.g() as f32 + (r.g() as f32 - y.g() as f32) * c) as u8,
        (y.b() as f32 + (r.b() as f32 - y.b() as f32) * c) as u8,
        alpha,
    )
}

fn side_marker(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    x0: f32,
    x1: f32,
    yc: f32,
    marker_h: f32,
    strong: bool,
    to_left: bool,
    opacity: f32,
    closeness: Option<f32>,
    label_txt: &str,
) {
    let left = x0.min(x1);
    let right = x0.max(x1);
    let w = (right - left).max(1.0);
    let h = marker_h.max(1.0);
    let alpha = ((if strong { 235.0 } else { 195.0 }) * opacity.clamp(0.0, 1.0)) as u8;
    let base = if let Some(c) = closeness {
        prox_color(ctx, c, 255)
    } else {
        ctx.cfg.color(SECTION, "red", "#ff5050")
    };
    // Approximate feathered gradient with strips.
    let steps = 8;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let fade = if to_left {
            // solid at right (near car), transparent at left
            1.0 - (t0 + t1) * 0.5
        } else {
            (t0 + t1) * 0.5
        };
        // Vertical feather
        let vfade = {
            let mid = ((t0 + t1) * 0.5 - 0.5).abs() * 2.0;
            1.0 - mid
        };
        let a = (alpha as f32 * fade * vfade.max(0.15)) as u8;
        let x_a = left + w * t0;
        let x_b = left + w * t1;
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(x_a, yc - h * 0.5),
                Pos2::new(x_b, yc + h * 0.5),
            ),
            CornerRadius::ZERO,
            color_with_alpha(base, a),
        );
    }
    if !label_txt.is_empty() {
        label(
            ui,
            Pos2::new((left + right) * 0.5, yc),
            Align2::CENTER_CENTER,
            label_txt,
            (w.min(h) * 0.38).max(6.0),
            Color32::WHITE,
            true,
        );
    }
}

fn v_glow(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    cx: f32,
    y_inner: f32,
    y_outer: f32,
    closeness: f32,
    half_w: f32,
    _up: bool,
) {
    let top = y_inner.min(y_outer);
    let bottom = y_inner.max(y_outer);
    let h = (bottom - top).max(1.0);
    let peak = (80.0 + 130.0 * closeness.clamp(0.0, 1.0)) as u8;
    let col = prox_color(ctx, closeness, peak);
    let steps = 10;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        // Fade from inner (t toward y_inner) to outer.
        let along = (t0 + t1) * 0.5;
        let from_inner = if y_inner < y_outer { along } else { 1.0 - along };
        let a = (peak as f32 * (1.0 - from_inner)) as u8;
        // Horizontal feather
        let hf = 1.0 - ((along - 0.5).abs() * 0.3);
        let y0 = top + h * t0;
        let y1 = top + h * t1;
        ui.painter().rect_filled(
            Rect::from_min_max(
                Pos2::new(cx - half_w, y0),
                Pos2::new(cx + half_w, y1),
            ),
            CornerRadius::ZERO,
            Color32::from_rgba_unmultiplied(
                col.r(),
                col.g(),
                col.b(),
                (a as f32 * hf) as u8,
            ),
        );
    }
}
