//! Skia painter for the directional proximity radar HUD (Phase C).
//!
//! Ports `widgets/radar.rs` (egui) onto the Skia `Canvas`. Animation state is
//! host-owned (`RadarAnim`) instead of an egui temp-data slot.

use super::canvas::{Canvas, TextAlign};
use super::chrome::{anim_dt, ease, panel_card, section_color, still_easing, text_at};
use super::types::{Rect, Rgba};
use crate::config::OverlayConfig;
use crate::telemetry::TelemetryFrame;

const SECTION: &str = "radar";

/// Host-owned eased state for the radar side markers / glows.
#[derive(Clone, Default)]
pub struct RadarAnim {
    pub left: f32,
    pub right: f32,
    /// Latest "strong" flags (not eased — mirror the raw telemetry booleans).
    pub left2: bool,
    pub right2: bool,
    pub front: f32,
    pub rear: f32,
    pub left_pos: f32,
    pub right_pos: f32,
    pub last_secs: f64,
}

fn full_bounds(c: &Canvas) -> Rect {
    Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32)
}

fn size_frac(cfg: &OverlayConfig, key: &str, default: f32) -> f32 {
    cfg.section(SECTION)
        .get("sizes")
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn prox_color(cfg: &OverlayConfig, closeness: f32, alpha: u8) -> Rgba {
    let t = closeness.clamp(0.0, 1.0);
    let yellow = section_color(cfg, SECTION, "yellow", "#ffd23a");
    let red = section_color(cfg, SECTION, "red", "#ff5050");
    Rgba::new(
        (yellow.r as f32 + (red.r as f32 - yellow.r as f32) * t) as u8,
        (yellow.g as f32 + (red.g as f32 - yellow.g as f32) * t) as u8,
        (yellow.b as f32 + (red.b as f32 - yellow.b as f32) * t) as u8,
        alpha,
    )
}

/// Tent map 0→1→0 across [0, 1] (Python `_feather_mask` edge dissolve).
fn tent(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.5 {
        (t * 2.0).clamp(0.0, 1.0)
    } else {
        ((1.0 - t) * 2.0).clamp(0.0, 1.0)
    }
}

#[allow(clippy::too_many_arguments)]
fn side_marker(
    c: &mut Canvas,
    cfg: &OverlayConfig,
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
    let peak = ((if strong { 235.0 } else { 195.0 }) * opacity.clamp(0.0, 1.0)) as u8;
    let base = if let Some(cl) = closeness {
        prox_color(cfg, cl, 255)
    } else {
        section_color(cfg, SECTION, "red", "#ff5050")
    };
    // Horizontal fade toward car + vertical tent feather (Python side pixmap).
    let nx = 16;
    let ny = 10;
    for ix in 0..nx {
        let tx0 = ix as f32 / nx as f32;
        let tx1 = (ix + 1) as f32 / nx as f32;
        let tx = (tx0 + tx1) * 0.5;
        let hfade = if to_left { 1.0 - tx } else { tx };
        for iy in 0..ny {
            let ty0 = iy as f32 / ny as f32;
            let ty1 = (iy + 1) as f32 / ny as f32;
            let ty = (ty0 + ty1) * 0.5;
            let vfade = tent(ty);
            let a = (peak as f32 * hfade * vfade) as u8;
            if a < 2 {
                continue;
            }
            c.fill_rect(
                Rect::from_xywh(
                    left + w * tx0,
                    yc - h * 0.5 + h * ty0,
                    w * (tx1 - tx0),
                    h * (ty1 - ty0),
                ),
                base.with_alpha(a),
                0.0,
            );
        }
    }
    if !label_txt.is_empty() {
        text_at(
            c,
            (left + right) * 0.5,
            yc,
            label_txt,
            (w.min(h) * 0.38).max(6.0),
            Rgba::WHITE,
            true,
            TextAlign::Center,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn v_glow(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    cx: f32,
    y_inner: f32,
    y_outer: f32,
    closeness: f32,
    half_w: f32,
) {
    let top = y_inner.min(y_outer);
    let bottom = y_inner.max(y_outer);
    let h = (bottom - top).max(1.0);
    let w = (half_w * 2.0).max(1.0);
    let peak = (80.0 + 130.0 * closeness.clamp(0.0, 1.0)) as u8;
    let base = prox_color(cfg, closeness, 255);
    // Inner→outer fade × horizontal tent feather (Python _build_glow_pixmap).
    let ny = 20;
    let nx = 12;
    for iy in 0..ny {
        let ty0 = iy as f32 / ny as f32;
        let ty1 = (iy + 1) as f32 / ny as f32;
        let ty = (ty0 + ty1) * 0.5;
        let from_inner = if y_inner <= y_outer { ty } else { 1.0 - ty };
        let a_v = 1.0 - from_inner;
        for ix in 0..nx {
            let tx0 = ix as f32 / nx as f32;
            let tx1 = (ix + 1) as f32 / nx as f32;
            let tx = (tx0 + tx1) * 0.5;
            let a_h = tent(tx);
            let a = (peak as f32 * a_v * a_h) as u8;
            if a < 2 {
                continue;
            }
            c.fill_rect(
                Rect::from_xywh(
                    cx - half_w + w * tx0,
                    top + h * ty0,
                    w * (tx1 - tx0),
                    h * (ty1 - ty0),
                ),
                base.with_alpha(a),
                0.0,
            );
        }
    }
}

/// Paint the radar HUD. Returns `true` while any eased value is still moving
/// toward its target (mirrors egui `panel_animating`).
pub fn paint_radar(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    anim: &mut RadarAnim,
    mono_secs: f64,
    edit_mode: bool,
) -> bool {
    let _ = edit_mode;
    c.clear_transparent();
    let bounds = full_bounds(c);
    if cfg.bool_key(SECTION, "show_panel", false) {
        panel_card(c, cfg, SECTION, bounds);
    }

    let w = bounds.width();
    let h = bounds.height();
    let (cx, cy) = bounds.center();

    let car_w = (w * size_frac(cfg, "car_w", 0.13)).max(12.0);
    let car_h = (h * size_frac(cfg, "car_h", 0.20)).max(24.0);
    let bar_h = car_h * size_frac(cfg, "bar_h", 0.78);
    let inner = car_w * 0.75;
    let nose_len = h * size_frac(cfg, "nose_len", 0.16);
    let glow_w = w * size_frac(cfg, "glow_w", 0.17);

    let d = &frame.radar;
    let show_front = cfg.bool_key(SECTION, "show_front", true);
    let show_rear = cfg.bool_key(SECTION, "show_rear", true);
    let side_tau = cfg.f64_key(SECTION, "ease_side_tau", 0.10) as f32;
    let glow_tau = cfg.f64_key(SECTION, "ease_glow_tau", 0.13) as f32;
    let prox = cfg.bool_key(SECTION, "side_proximity_color", false);

    let dt = anim_dt(mono_secs, &mut anim.last_secs);

    let t_left = if d.left { 1.0 } else { 0.0 };
    let t_right = if d.right { 1.0 } else { 0.0 };
    let t_front = if show_front {
        d.ahead.unwrap_or(0.0)
    } else {
        0.0
    };
    let t_rear = if show_rear {
        d.behind.unwrap_or(0.0)
    } else {
        0.0
    };

    anim.left = ease(anim.left, t_left, dt, side_tau);
    anim.right = ease(anim.right, t_right, dt, side_tau);
    anim.front = ease(anim.front, t_front, dt, glow_tau);
    anim.rear = ease(anim.rear, t_rear, dt, glow_tau);
    anim.left_pos = ease(anim.left_pos, d.left_pos, dt, side_tau);
    anim.right_pos = ease(anim.right_pos, d.right_pos, dt, side_tau);
    anim.left2 = d.left2;
    anim.right2 = d.right2;

    let still_animating = still_easing(anim.left, t_left, 0.01)
        || still_easing(anim.right, t_right, 0.01)
        || still_easing(anim.front, t_front, 0.01)
        || still_easing(anim.rear, t_rear, 0.01)
        || still_easing(anim.left_pos, d.left_pos, 0.01)
        || still_easing(anim.right_pos, d.right_pos, 0.01);

    if show_front && anim.front > 0.01 {
        v_glow(
            c,
            cfg,
            cx,
            cy - car_h * 0.45,
            bounds.top() + h * 0.06,
            anim.front,
            glow_w,
        );
    }
    if show_rear && anim.rear > 0.01 {
        v_glow(
            c,
            cfg,
            cx,
            cy + car_h * 0.45,
            bounds.top() + h * 0.94,
            anim.rear,
            glow_w,
        );
    }

    let marker_h = bar_h.max(18.0);
    let travel = (h * 0.5 - marker_h * 0.5 - h * 0.06).max(0.0);

    if anim.left > 0.01 {
        let yc = cy - anim.left_pos * travel;
        side_marker(
            c,
            cfg,
            bounds.left() + w * 0.07,
            cx - inner,
            yc,
            marker_h,
            anim.left2,
            true,
            anim.left,
            if prox {
                Some(1.0 - anim.left_pos.abs())
            } else {
                None
            },
            &d.left_label,
        );
    }
    if anim.right > 0.01 {
        let yc = cy - anim.right_pos * travel;
        side_marker(
            c,
            cfg,
            cx + inner,
            bounds.left() + w * 0.93,
            yc,
            marker_h,
            anim.right2,
            false,
            anim.right,
            if prox {
                Some(1.0 - anim.right_pos.abs())
            } else {
                None
            },
            &d.right_label,
        );
    }

    if cfg.bool_key(SECTION, "show_clear_timer", false) {
        if let Some(secs) = d.clear_secs {
            if secs >= 0.0 {
                let txt = format!("Clear {secs:.0}s");
                text_at(
                    c,
                    cx,
                    bounds.bottom() - h * 0.08,
                    &txt,
                    10.0,
                    section_color(cfg, SECTION, "nose", "#f4f6f8"),
                    true,
                    TextAlign::Center,
                );
            }
        }
    }

    if cfg.bool_key(SECTION, "show_axis", true) {
        let axis = section_color(cfg, SECTION, "axis", "#ffffff28");
        let sw = (w * 0.006).max(1.0);
        c.line(
            bounds.left() + w * 0.08,
            cy,
            bounds.left() + w * 0.92,
            cy,
            axis,
            sw,
        );
        c.line(
            cx,
            bounds.top() + h * 0.10,
            cx,
            bounds.top() + h * 0.90,
            axis,
            sw,
        );
    }
    if cfg.bool_key(SECTION, "show_nose", true) {
        let nose = section_color(cfg, SECTION, "nose", "#f4f6f8");
        c.line(
            cx,
            cy - car_h * 0.5,
            cx,
            cy - car_h * 0.5 - nose_len,
            nose,
            (w * 0.012).max(1.5),
        );
    }

    // Center car silhouette.
    let car = Rect::from_xywh(cx - car_w * 0.5, cy - car_h * 0.5, car_w, car_h);
    c.fill_rect(
        car,
        section_color(cfg, SECTION, "car", "#f4f6f8"),
        car_w * 0.4,
    );

    still_animating
}
