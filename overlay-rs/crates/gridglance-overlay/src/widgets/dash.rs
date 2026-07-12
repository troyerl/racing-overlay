//! Dash — multi-container racing dashboard (parity with Python `dash.py`).

use super::WidgetCtx;
use crate::chrome::{draw_card, draw_panel_rect, full_rect, label};
use crate::config::OverlayConfig;
use crate::telemetry::TelemetryFrame;
use egui::{Align2, Color32, Pos2, Rect, Stroke, Ui, Vec2};

const SECTION: &str = "dash";

fn fmt_lap(secs: Option<f64>) -> String {
    match secs {
        Some(s) if s.is_finite() && s > 0.0 => {
            let m = (s as i32) / 60;
            let rem = s - (m as f64) * 60.0;
            format!("{m}:{rem:06.3}")
        }
        _ => "--".into(),
    }
}

fn fmt_signed_delta(d: Option<f64>) -> String {
    match d {
        Some(v) => format!("{v:+.2}"),
        None => "--.--".into(),
    }
}

fn gear_str(g: i32) -> String {
    if g < 0 {
        "R".into()
    } else if g == 0 {
        "N".into()
    } else {
        g.to_string()
    }
}

fn metric_value(cfg: &OverlayConfig, f: &TelemetryFrame, key: &str) -> (String, String) {
    // Returns (label, value) — stacked metrics use "\n" in value for multi-line.
    match key {
        "none" | "" => (String::new(), String::new()),
        "speed" => {
            let ms = f.speed_mps;
            let (v, unit) = if cfg.imperial_units() {
                (ms * 2.2369363, "mph")
            } else {
                (ms * 3.6, "km/h")
            };
            (unit.into(), format!("{:.0}", v))
        }
        "rpm" => ("RPM".into(), format!("{:.0}", f.rpm)),
        "gear" => ("GEAR".into(), gear_str(f.gear)),
        "position" => ("POS".into(), format!("P{}", f.position.max(0))),
        "car_number" => (
            "CAR".into(),
            if f.car_number.is_empty() {
                "--".into()
            } else {
                f.car_number.clone()
            },
        ),
        "lap_count" => (
            "LAP".into(),
            if f.laps_total > 0 {
                format!("{}/{}", f.lap, f.laps_total)
            } else if f.lap > 0 {
                format!("{}", f.lap)
            } else {
                "--".into()
            },
        ),
        "laps_left" => (
            "LEFT".into(),
            if f.laps_total > 0 {
                format!("{}", (f.laps_total - f.lap).max(0))
            } else {
                "--".into()
            },
        ),
        "lap" => ("LAP".into(), format!("{}", f.lap.max(0))),
        "fuel" => ("FUEL".into(), format!("{:.1} L", f.fuel_l)),
        "fuel_laps" => ("LAPS".into(), format!("{:.1}", f.laps_fuel)),
        "fuel_stack" => (
            "FUEL".into(),
            format!("{:.1} L\n{:.1} laps", f.fuel_l, f.laps_fuel),
        ),
        "tires" => (
            "TIRES".into(),
            format!(
                "L {:.0}%\nR {:.0}%",
                f.tire_wear_l * 100.0,
                f.tire_wear_r * 100.0
            ),
        ),
        "incidents" => ("INC".into(), format!("{}", f.incidents)),
        "last_lap" => ("LAST".into(), fmt_lap(f.last_lap_s)),
        "best_lap" => ("BEST".into(), fmt_lap(f.best_lap_s)),
        "cur_lap" => ("CUR".into(), fmt_lap(f.cur_lap_s)),
        "delta" => ("Δ".into(), fmt_signed_delta(f.delta)),
        "irating" => {
            let base = if cfg.bool_key(SECTION, "irating_abbreviate", true) {
                format!("{:.1}k", f.irating as f32 / 1000.0)
            } else {
                format!("{}", f.irating)
            };
            let v = if cfg.bool_key(SECTION, "show_irating_projection", false) {
                match f.irating_delta {
                    Some(d) => format!("{base} {d:+}"),
                    None => base,
                }
            } else {
                base
            };
            ("iR".into(), v)
        }
        "air_temp" => (
            "AIR".into(),
            f.air_temp
                .map(|t| format!("{t:.0}°"))
                .unwrap_or_else(|| "--".into()),
        ),
        "track_temp" => (
            "TRK".into(),
            f.track_temp
                .map(|t| format!("{t:.0}°"))
                .unwrap_or_else(|| "--".into()),
        ),
        other => (other.to_uppercase(), "--".into()),
    }
}

fn slot_key(cfg: &OverlayConfig, key: &str, default: &str) -> String {
    let s = cfg.str_key(SECTION, key, default);
    if s.is_empty() {
        "none".into()
    } else {
        s
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);

    let cfg = ctx.cfg;
    let f = ctx.frame;
    let w = rect.width();
    let h = rect.height();
    let text_scale = cfg.f64_key(SECTION, "text_scale", 1.0) as f32;
    let value_col = cfg.color(SECTION, "value", "#f4f6f8");
    let label_col = cfg.color(SECTION, "label", "#8b93a1");
    let muted = cfg.color(SECTION, "muted", "#8b93a1");

    let m = h * 0.045;
    let gp = h * 0.022;
    let hg = w * 0.007;
    let show_pos = cfg.bool_key(SECTION, "show_position", true);
    let mut panels_top = rect.top() + m;
    let panels_bottom = rect.top() + h * 0.80;

    // Flag bar reserve
    let mut flag_rect: Option<Rect> = None;
    if cfg.bool_key(SECTION, "show_flags", true) {
        let has_ctx = f.flag.is_some() && f.flag_context.is_some();
        let flag_bar_h = (if has_ctx { h * 0.165 } else { h * 0.105 }).max(if has_ctx {
            8.0
        } else {
            6.0
        });
        flag_rect = Some(Rect::from_min_size(
            Pos2::new(rect.left() + m, panels_top),
            Vec2::new(w - 2.0 * m, flag_bar_h),
        ));
        panels_top += flag_bar_h + h * 0.03;
    }

    // Optional delta bar
    let mut delta_bar: Option<Rect> = None;
    if cfg.bool_key(SECTION, "show_delta_bar", false) {
        let db_h = h * 0.05;
        delta_bar = Some(Rect::from_min_size(
            Pos2::new(rect.left() + m, panels_top),
            Vec2::new(w - 2.0 * m, db_h * 0.7),
        ));
        panels_top += db_h;
    }

    let left_left = rect.left() + m;
    let right_edge = rect.right() - m;
    let total = panels_bottom - panels_top;
    let top_h = (total - gp) * 0.42;
    let bot_h = (total - gp) * 0.58;

    let mut top_right = right_edge;
    let mut p9_rect = Rect::NOTHING;
    if show_pos {
        let p9_w = top_h * 1.30;
        p9_rect = Rect::from_min_size(
            Pos2::new(right_edge - p9_w, panels_top),
            Vec2::new(p9_w, top_h),
        );
        top_right = p9_rect.left() - hg;
    }

    let top_rect = Rect::from_min_size(
        Pos2::new(left_left, panels_top),
        Vec2::new((top_right - left_left).max(1.0), top_h),
    );
    let bot_rect = Rect::from_min_size(
        Pos2::new(left_left, panels_top + top_h + gp),
        Vec2::new((right_edge - left_left).max(1.0), bot_h),
    );

    draw_panel_rect(ui, cfg, SECTION, top_rect);
    draw_panel_rect(ui, cfg, SECTION, bot_rect);
    if show_pos {
        draw_position(ui, cfg, p9_rect, f, text_scale, value_col, label_col);
    }

    // Center medallion geometry
    let ring_cx = (left_left + right_edge) * 0.5;
    let ring_cy = panels_top + total * 0.5;
    let ring_d = total * 0.80;
    let ring_half = ring_d * 0.5;
    let bpad = bot_rect.height() * 0.14;
    let base_pad = h * 0.035;
    let base_gap_l = ring_cx - ring_half - base_pad;
    let gap_l = base_gap_l;
    let gap_r = ring_cx + ring_half + base_pad;

    // Top: shift bar | status
    let ipad = top_rect.height() * 0.22;
    if cfg.bool_key(SECTION, "show_shift_bar", true) {
        let shift_w =
            ((ring_cx - ring_half - base_pad) - (top_rect.left() + ipad)).max(40.0);
        draw_shift(
            ui,
            cfg,
            Rect::from_min_size(
                Pos2::new(
                    top_rect.left() + ipad,
                    top_rect.center().y - top_rect.height() * 0.20,
                ),
                Vec2::new(shift_w, top_rect.height() * 0.40),
            ),
            f,
        );
    }

    let top_right_key = slot_key(cfg, "top_right", "incidents");
    if top_right_key != "none" {
        let status_rect = Rect::from_min_max(
            Pos2::new(gap_r, top_rect.top()),
            Pos2::new(top_rect.right() - ipad, top_rect.bottom()),
        );
        draw_status(ui, cfg, status_rect, &top_right_key, f, text_scale, value_col, label_col);
    }

    // Bottom: primary | stats
    let primary_l = slot_key(cfg, "primary_left", "lap_count");
    let primary_r = slot_key(cfg, "primary_right", "speed");
    if primary_l != "none" || primary_r != "none" {
        let primary_rect = Rect::from_min_max(
            Pos2::new(bot_rect.left() + bpad, bot_rect.top() + bpad),
            Pos2::new(gap_l.max(bot_rect.left() + bpad + 10.0), bot_rect.bottom() - bpad),
        );
        draw_primary(
            ui,
            cfg,
            primary_rect,
            &primary_l,
            &primary_r,
            f,
            text_scale,
            value_col,
            label_col,
        );
    }

    let stat_l = slot_key(cfg, "stat_left", "tires");
    let stat_r = slot_key(cfg, "stat_right", "fuel_stack");
    if stat_l != "none" || stat_r != "none" {
        let stats_rect = Rect::from_min_max(
            Pos2::new(gap_r, bot_rect.top() + bpad),
            Pos2::new(bot_rect.right() - bpad, bot_rect.bottom() - bpad),
        );
        draw_stats(
            ui,
            cfg,
            stats_rect,
            &stat_l,
            &stat_r,
            f,
            text_scale,
            value_col,
            label_col,
        );
    }

    // Strip pill
    let strip_keys = [
        slot_key(cfg, "strip_left", "air_temp"),
        slot_key(cfg, "strip_center", "track_temp"),
        slot_key(cfg, "strip_right", "last_lap"),
    ];
    if strip_keys.iter().any(|k| k != "none") {
        let pill_w = (right_edge - left_left) * 0.66;
        let pill_h = h * 0.22;
        let pill = Rect::from_min_size(
            Pos2::new(
                ring_cx - pill_w * 0.5,
                panels_bottom - pill_h * 0.28 + h * 0.02,
            ),
            Vec2::new(pill_w, pill_h),
        );
        draw_strip(
            ui,
            cfg,
            pill,
            &strip_keys,
            f,
            text_scale,
            value_col,
            label_col,
            muted,
        );
    }

    // Center medallion
    if cfg.bool_key(SECTION, "show_ring", true) {
        if cfg.str_key(SECTION, "center_mode", "ring") == "pedals" {
            draw_pedals(ui, cfg, ring_cx, ring_cy, ring_d, f, value_col);
        } else {
            draw_ring(ui, cfg, ring_cx, ring_cy, ring_d, f, text_scale, value_col);
        }
    }

    if let Some(fr) = flag_rect {
        draw_flag(ui, cfg, fr, f, ring_cx);
    }
    if let Some(db) = delta_bar {
        draw_delta_bar(ui, cfg, db, f);
    }
}

fn draw_position(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    f: &TelemetryFrame,
    text_scale: f32,
    value: Color32,
    label_c: Color32,
) {
    draw_panel_rect(ui, cfg, SECTION, rect);
    label(
        ui,
        Pos2::new(rect.center().x, rect.top() + rect.height() * 0.22),
        Align2::CENTER_CENTER,
        "POS",
        (rect.height() * 0.16 * text_scale).clamp(9.0, 16.0),
        label_c,
        true,
    );
    label(
        ui,
        rect.center(),
        Align2::CENTER_CENTER,
        &format!("{}", f.position.max(0)),
        (rect.height() * 0.48 * text_scale).clamp(18.0, 48.0),
        value,
        true,
    );
    if !f.car_number.is_empty() {
        label(
            ui,
            Pos2::new(rect.center().x, rect.bottom() - rect.height() * 0.18),
            Align2::CENTER_CENTER,
            &format!("#{}", f.car_number),
            (rect.height() * 0.14 * text_scale).clamp(9.0, 14.0),
            label_c,
            false,
        );
    }
}

fn draw_shift(ui: &mut Ui, cfg: &OverlayConfig, rect: Rect, f: &TelemetryFrame) {
    let segs = cfg.f64_key(SECTION, "shift_segments", 20.0).max(4.0) as i32;
    let red_frac = cfg.f64_key(SECTION, "shift_red_frac", 0.16) as f32;
    let yel_frac = cfg.f64_key(SECTION, "shift_yellow_frac", 0.24) as f32;
    let redline = f.redline.max(1.0);
    let frac = (f.rpm / redline).clamp(0.0, 1.0);
    let gap = rect.width() * 0.02;
    let seg_w = ((rect.width() - gap * (segs as f32 - 1.0)) / segs as f32).max(1.0);
    let lit = (frac * segs as f32).ceil() as i32;
    for i in 0..segs {
        let x = rect.left() + i as f32 * (seg_w + gap);
        let t = (i as f32 + 1.0) / segs as f32;
        let col = if t > 1.0 - red_frac {
            cfg.color(SECTION, "shift_red", "#ff5050")
        } else if t > 1.0 - red_frac - yel_frac {
            cfg.color(SECTION, "shift_yellow", "#ffd23a")
        } else {
            cfg.color(SECTION, "shift_green", "#46df7a")
        };
        let fill = if i < lit {
            col
        } else {
            cfg.color(SECTION, "shift_idle", "#ffffff18")
        };
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(x, rect.top()), Vec2::new(seg_w, rect.height())),
            egui::CornerRadius::same(2),
            fill,
        );
    }
}

fn draw_status(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    key: &str,
    f: &TelemetryFrame,
    text_scale: f32,
    value: Color32,
    label_c: Color32,
) {
    let (lab, val) = metric_value(cfg, f, key);
    label(
        ui,
        Pos2::new(rect.right(), rect.top() + rect.height() * 0.28),
        Align2::RIGHT_CENTER,
        &lab,
        (rect.height() * 0.22 * text_scale).clamp(9.0, 14.0),
        label_c,
        true,
    );
    label(
        ui,
        Pos2::new(rect.right(), rect.top() + rect.height() * 0.62),
        Align2::RIGHT_CENTER,
        &val.lines().next().unwrap_or("--").to_string(),
        (rect.height() * 0.36 * text_scale).clamp(12.0, 28.0),
        value,
        true,
    );
}

fn draw_primary(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    left_key: &str,
    right_key: &str,
    f: &TelemetryFrame,
    text_scale: f32,
    value: Color32,
    label_c: Color32,
) {
    let mid = rect.left() + rect.width() * 0.42;
    if left_key != "none" {
        let (lab, val) = metric_value(cfg, f, left_key);
        label(
            ui,
            Pos2::new(rect.left(), rect.top() + rect.height() * 0.22),
            Align2::LEFT_CENTER,
            &lab,
            (rect.height() * 0.18 * text_scale).clamp(9.0, 13.0),
            label_c,
            true,
        );
        label(
            ui,
            Pos2::new(rect.left(), rect.top() + rect.height() * 0.58),
            Align2::LEFT_CENTER,
            &val.lines().next().unwrap_or("--").to_string(),
            (rect.height() * 0.32 * text_scale).clamp(12.0, 24.0),
            value,
            true,
        );
    }
    if right_key != "none" {
        let (lab, val) = metric_value(cfg, f, right_key);
        label(
            ui,
            Pos2::new(mid, rect.top() + rect.height() * 0.18),
            Align2::LEFT_CENTER,
            &lab,
            (rect.height() * 0.16 * text_scale).clamp(9.0, 12.0),
            label_c,
            true,
        );
        label(
            ui,
            Pos2::new(mid, rect.top() + rect.height() * 0.58),
            Align2::LEFT_CENTER,
            &val.lines().next().unwrap_or("--").to_string(),
            (rect.height() * 0.48 * text_scale).clamp(18.0, 42.0),
            value,
            true,
        );
    }
}

fn draw_stats(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    left_key: &str,
    right_key: &str,
    f: &TelemetryFrame,
    text_scale: f32,
    value: Color32,
    label_c: Color32,
) {
    let half = rect.width() * 0.5;
    for (i, key) in [left_key, right_key].into_iter().enumerate() {
        if key == "none" {
            continue;
        }
        let cell = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * half, rect.top()),
            Vec2::new(half - 4.0, rect.height()),
        );
        let (lab, val) = metric_value(cfg, f, key);
        label(
            ui,
            Pos2::new(cell.left() + 4.0, cell.top() + cell.height() * 0.18),
            Align2::LEFT_CENTER,
            &lab,
            (cell.height() * 0.16 * text_scale).clamp(9.0, 12.0),
            label_c,
            true,
        );
        let lines: Vec<&str> = val.lines().collect();
        for (li, line) in lines.iter().enumerate() {
            label(
                ui,
                Pos2::new(
                    cell.left() + 4.0,
                    cell.top() + cell.height() * (0.42 + li as f32 * 0.28),
                ),
                Align2::LEFT_CENTER,
                line,
                (cell.height() * 0.22 * text_scale).clamp(11.0, 18.0),
                value,
                true,
            );
        }
    }
}

fn draw_strip(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    keys: &[String; 3],
    f: &TelemetryFrame,
    text_scale: f32,
    value: Color32,
    label_c: Color32,
    muted: Color32,
) {
    draw_panel_rect(ui, cfg, SECTION, rect);
    let n = keys.iter().filter(|k| *k != "none").count().max(1) as f32;
    let cell_w = rect.width() / n;
    let mut i = 0usize;
    for key in keys {
        if key == "none" {
            continue;
        }
        let cell = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * cell_w, rect.top()),
            Vec2::new(cell_w, rect.height()),
        );
        let (lab, val) = metric_value(cfg, f, key);
        label(
            ui,
            Pos2::new(cell.center().x, cell.top() + cell.height() * 0.28),
            Align2::CENTER_CENTER,
            &lab,
            (cell.height() * 0.22 * text_scale).clamp(9.0, 12.0),
            label_c,
            true,
        );
        label(
            ui,
            Pos2::new(cell.center().x, cell.top() + cell.height() * 0.62),
            Align2::CENTER_CENTER,
            &val.lines().next().unwrap_or("--").to_string(),
            (cell.height() * 0.32 * text_scale).clamp(11.0, 18.0),
            if val == "--" { muted } else { value },
            true,
        );
        i += 1;
    }
}

fn draw_ring(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    cx: f32,
    cy: f32,
    d: f32,
    f: &TelemetryFrame,
    text_scale: f32,
    value: Color32,
) {
    let r = d * 0.5;
    let track = cfg.color(SECTION, "ring_track", "#ffffff18");
    ui.painter()
        .circle_stroke(Pos2::new(cx, cy), r * 0.92, Stroke::new(6.0_f32, track));

    let inputs = [
        (
            cfg.bool_key(SECTION, "show_throttle", true),
            f.throttle,
            cfg.color(SECTION, "throttle", "#46df7a"),
            0.92,
        ),
        (
            cfg.bool_key(SECTION, "show_brake", true),
            f.brake,
            cfg.color(SECTION, "brake", "#ff5050"),
            0.78,
        ),
        (
            cfg.bool_key(SECTION, "show_clutch", false),
            f.clutch,
            cfg.color(SECTION, "clutch", "#4a8cff"),
            0.64,
        ),
    ];
    for (show, frac, col, scale) in inputs {
        if !show || frac <= 0.01 {
            continue;
        }
        // Approximate arc with chord segments
        let rr = r * scale;
        let start = -std::f32::consts::FRAC_PI_2;
        let end = start + std::f32::consts::TAU * frac.clamp(0.0, 1.0);
        let steps = 24;
        let mut prev = Pos2::new(cx + rr * start.cos(), cy + rr * start.sin());
        for s in 1..=steps {
            let a = start + (end - start) * (s as f32 / steps as f32);
            let p = Pos2::new(cx + rr * a.cos(), cy + rr * a.sin());
            ui.painter()
                .line_segment([prev, p], Stroke::new(5.0_f32, col));
            prev = p;
        }
    }

    label(
        ui,
        Pos2::new(cx, cy),
        Align2::CENTER_CENTER,
        &gear_str(f.gear),
        (d * 0.28 * text_scale).clamp(18.0, 48.0),
        value,
        true,
    );
}

fn draw_pedals(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    cx: f32,
    cy: f32,
    d: f32,
    f: &TelemetryFrame,
    _value: Color32,
) {
    let bars: Vec<(bool, f32, Color32)> = vec![
        (
            cfg.bool_key(SECTION, "show_throttle", true),
            f.throttle,
            cfg.color(SECTION, "throttle", "#46df7a"),
        ),
        (
            cfg.bool_key(SECTION, "show_brake", true),
            f.brake,
            if f.abs_active {
                cfg.color(SECTION, "abs", "#ffd23a")
            } else {
                cfg.color(SECTION, "brake", "#ff5050")
            },
        ),
        (
            cfg.bool_key(SECTION, "show_clutch", false),
            f.clutch,
            cfg.color(SECTION, "clutch", "#4a8cff"),
        ),
    ];
    let active: Vec<_> = bars.into_iter().filter(|(s, ..)| *s).collect();
    let n = active.len().max(1) as f32;
    let total_w = d * 0.55;
    let bar_w = total_w / (n * 1.4);
    let gap = bar_w * 0.4;
    let start_x = cx - (n * bar_w + (n - 1.0) * gap) * 0.5;
    let bar_h = d * 0.7;
    let top = cy - bar_h * 0.5;
    let track = cfg.color(SECTION, "ring_track", "#ffffff18");
    for (i, (_, frac, col)) in active.iter().enumerate() {
        let x = start_x + i as f32 * (bar_w + gap);
        let track_r = Rect::from_min_size(Pos2::new(x, top), Vec2::new(bar_w, bar_h));
        ui.painter()
            .rect_filled(track_r, egui::CornerRadius::same(4), track);
        let fh = bar_h * frac.clamp(0.0, 1.0);
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(x, top + bar_h - fh), Vec2::new(bar_w, fh)),
            egui::CornerRadius::same(4),
            *col,
        );
    }
}

fn draw_flag(ui: &mut Ui, cfg: &OverlayConfig, rect: Rect, f: &TelemetryFrame, center_x: f32) {
    let Some(flag) = f.flag.as_deref() else {
        return;
    };
    let (title, bgk, fgk) = match flag {
        "yellow" => ("CAUTION", "flag_yellow", "flag_yellow_text"),
        "black" => ("BLACK FLAG", "flag_black", "flag_black_text"),
        "green" => ("GREEN", "flag_green", "flag_green_text"),
        "white" => ("LAST LAP", "flag_white_bg", "flag_white_text"),
        "red" => ("RED FLAG", "flag_red", "flag_red_text"),
        "blue" => ("LET BY", "flag_blue", "flag_blue_text"),
        "checkered" => ("FINISH", "flag_checker_bg", "flag_checker_text"),
        "meatball" => ("MEATBALL", "flag_meatball", "flag_meatball_text"),
        other => (other, "flag_yellow", "flag_yellow_text"),
    };
    let bg = cfg.color(SECTION, bgk, "#ffd23a");
    let fg = cfg.color(SECTION, fgk, "#141414");
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(4), bg);
    label(
        ui,
        Pos2::new(center_x, rect.center().y),
        Align2::CENTER_CENTER,
        title,
        (rect.height() * 0.55).clamp(9.0, 16.0),
        fg,
        true,
    );
    if let Some(ctx_line) = f.flag_context.as_deref() {
        label(
            ui,
            Pos2::new(rect.right() - 8.0, rect.center().y),
            Align2::RIGHT_CENTER,
            ctx_line,
            (rect.height() * 0.4).clamp(8.0, 12.0),
            fg,
            false,
        );
    }
}

fn draw_delta_bar(ui: &mut Ui, cfg: &OverlayConfig, rect: Rect, f: &TelemetryFrame) {
    let track = cfg.color(SECTION, "track", "#ffffff18");
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(3), track);
    let rng = cfg.f64_key(SECTION, "delta_bar_range", 1.0).max(0.001) as f32;
    let delta = f.delta.unwrap_or(0.0) as f32;
    let t = (delta / rng).clamp(-1.0, 1.0);
    let cx = rect.center().x;
    if t.abs() > 0.001 {
        let fill_w = rect.width() * 0.5 * t.abs();
        let fill = if t < 0.0 {
            Rect::from_min_max(Pos2::new(cx - fill_w, rect.top()), Pos2::new(cx, rect.bottom()))
        } else {
            Rect::from_min_max(Pos2::new(cx, rect.top()), Pos2::new(cx + fill_w, rect.bottom()))
        };
        let col = if t < 0.0 {
            cfg.color(SECTION, "faster", "#46df7a")
        } else {
            cfg.color(SECTION, "slower", "#ff5050")
        };
        ui.painter()
            .rect_filled(fill, egui::CornerRadius::same(3), col);
    }
}
