//! Pace / caution speed helper — match the pace car under yellow.

use super::WidgetCtx;
use crate::chrome::{full_rect, label, panel_card, panel_content_pad, panel_title};
use crate::config::OverlayConfig;
use crate::telemetry::TelemetryFrame;
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "pace_caution";

/// Yellow / full-course caution from session flags.
///
/// Note: do **not** use raw `PaceMode != 0`. iRacing `PaceMode` is an enum where
/// `4 = NotPacing` and `0..3` are formation/restart pacing (including green starts).
pub fn under_caution(f: &TelemetryFrame) -> bool {
    matches!(
        f.flag.as_deref(),
        Some("yellow") | Some("caution") | Some("yellow_waving") | Some("caution_waving")
    )
}

/// Host gate: show when under caution, or always in edit mode for placement.
pub fn should_display(f: &TelemetryFrame, edit_mode: bool) -> bool {
    edit_mode || under_caution(f)
}

fn speed_value(cfg: &OverlayConfig, ms: f32) -> f32 {
    if cfg.imperial_units() {
        ms * 2.236_936_3
    } else {
        ms * 3.6
    }
}

fn speed_unit(cfg: &OverlayConfig) -> &'static str {
    if cfg.imperial_units() {
        "MPH"
    } else {
        "KPH"
    }
}

fn format_speed(cfg: &OverlayConfig, ms: f32) -> String {
    format!("{:.0} {}", speed_value(cfg, ms), speed_unit(cfg))
}

fn format_delta(cfg: &OverlayConfig, you_mps: f32, ref_mps: f32) -> String {
    let d = speed_value(cfg, you_mps) - speed_value(cfg, ref_mps);
    format!("{d:+.0} {}", speed_unit(cfg))
}

fn pace_car_speed_mps(f: &TelemetryFrame) -> Option<f32> {
    f.cars
        .iter()
        .find(|c| c.is_pace_car)
        .map(|c| c.speed_mps)
        .filter(|v| v.is_finite() && *v > 0.5)
}

fn pit_limit_mps(ctx: &WidgetCtx<'_>) -> Option<f32> {
    ctx.frame
        .pit_speed_limit_mps
        .filter(|v| v.is_finite() && *v > 0.5)
        .or_else(|| {
            ctx.map
                .cached_pit
                .speed_ms
                .filter(|v| v.is_finite() && *v > 0.5)
        })
        .or_else(|| {
            let v = ctx.map.pit_speed_ms as f32;
            if v.is_finite() && v > 0.5 {
                Some(v)
            } else {
                None
            }
        })
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    if !should_display(ctx.frame, ctx.edit_mode) {
        let _ = full_rect(ui);
        return;
    }
    if under_caution(ctx.frame) {
        *ctx.panel_animating = true;
    }

    let rect = full_rect(ui);
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_content_pad(ctx.cfg, SECTION, card.height());
    let mut y = card.top() + pad;
    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        y = panel_title(
            ui,
            ctx.cfg,
            SECTION,
            card,
            radius,
            y,
            pad,
            &ctx.cfg.str_key(SECTION, "title", "PACE"),
        );
    }

    let mut pace_mps = pace_car_speed_mps(ctx.frame);
    let mut you_mps = if ctx.frame.speed_mps.is_finite() {
        Some(ctx.frame.speed_mps.max(0.0))
    } else {
        None
    };
    let mut pit_mps = pit_limit_mps(ctx);
    if ctx.edit_mode && !under_caution(ctx.frame) {
        pace_mps = pace_mps.or(Some(24.6));
        you_mps = you_mps.filter(|v| *v > 0.05).or(Some(24.6));
        pit_mps = pit_mps.or(Some(22.0));
    }

    let show_delta = ctx.cfg.bool_key(SECTION, "show_delta", true);
    let unit = speed_unit(ctx.cfg);

    let mut cols: Vec<(&str, String, bool)> = Vec::new();
    cols.push((
        "Pace",
        pace_mps
            .map(|ms| format_speed(ctx.cfg, ms))
            .unwrap_or_else(|| format!("-- {unit}")),
        false,
    ));
    cols.push((
        "You",
        you_mps
            .map(|ms| format_speed(ctx.cfg, ms))
            .unwrap_or_else(|| format!("-- {unit}")),
        false,
    ));
    if show_delta {
        let delta_s = match (pace_mps, you_mps) {
            (Some(p), Some(y)) => format_delta(ctx.cfg, y, p),
            _ => format!("-- {unit}"),
        };
        cols.push(("ΔP", delta_s, true));
    }
    cols.push((
        "Pit",
        pit_mps
            .map(|ms| format_speed(ctx.cfg, ms))
            .unwrap_or_else(|| format!("-- {unit}")),
        false,
    ));
    if show_delta {
        let delta_s = match (pit_mps, you_mps) {
            (Some(p), Some(y)) => format_delta(ctx.cfg, y, p),
            _ => format!("-- {unit}"),
        };
        cols.push(("ΔL", delta_s, true));
    }

    let n = cols.len().max(1) as f32;
    let body = egui::Rect::from_min_max(
        Pos2::new(card.left() + pad, y),
        Pos2::new(card.right() - pad, card.bottom() - pad),
    );
    let col_w = body.width() / n;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let warn = ctx.cfg.color(SECTION, "warn", "#ffd23a");
    let label_sz = (body.height() * 0.28).clamp(9.0, 12.0);
    let value_sz = (body.height() * 0.42).clamp(12.0, 18.0);

    for (i, (lab, value, is_delta)) in cols.into_iter().enumerate() {
        let col = egui::Rect::from_min_size(
            Pos2::new(body.left() + i as f32 * col_w, body.top()),
            egui::vec2(col_w, body.height()),
        );
        let cx = col.center().x;
        label(
            ui,
            Pos2::new(cx, col.top() + body.height() * 0.28),
            Align2::CENTER_CENTER,
            lab,
            label_sz,
            muted,
            false,
        );
        label(
            ui,
            Pos2::new(cx, col.top() + body.height() * 0.68),
            Align2::CENTER_CENTER,
            &value,
            value_sz,
            if is_delta { warn } else { text },
            true,
        );
    }
}
