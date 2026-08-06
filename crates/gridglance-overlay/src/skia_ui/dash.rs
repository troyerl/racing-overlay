//! Skia dash — visual parity with egui `widgets/dash.rs`.

use super::anim::DashAnim;
use super::canvas::{Canvas, TextAlign};
use super::chrome::{anim_dt, draw_dark_cell, draw_panel_rect, ease, section_color, still_easing};
use super::types::{FontSpec, Rect, Rgba};
use crate::config::OverlayConfig;
use crate::telemetry::TelemetryFrame;

const SECTION: &str = "dash";

/// Paint dash; returns true when the panel should keep live present cadence.
pub fn paint(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    f: &TelemetryFrame,
    anim: &mut DashAnim,
    mono_secs: f64,
) -> bool {
    let bounds = Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32);
    c.clear_transparent();

    let mut animating = f.connected;
    let w = bounds.width();
    let h = bounds.height();
    let text_scale = cfg.text_scale(SECTION);

    let m = h * 0.028;
    let gp = h * 0.014;
    let hg = w * 0.006;
    let show_pos = cfg.bool_key(SECTION, "show_position", true);
    let mut panels_top = bounds.top() + m;
    let panels_bottom = bounds.top() + h * 0.80;
    let left_left = bounds.left() + m;
    let right_edge = bounds.right() - m;
    let bar_w = right_edge - left_left;

    let mut flag_rect: Option<Rect> = None;
    if cfg.bool_key(SECTION, "show_flags", true) {
        let has_ctx = f.flag.is_some() && f.flag_context.as_ref().is_some_and(|s| !s.is_empty());
        let flag_bar_h =
            (if has_ctx { h * 0.165 } else { h * 0.105 }).max(if has_ctx { 8.0 } else { 6.0 });
        flag_rect = Some(Rect::from_xywh(left_left, panels_top, bar_w, flag_bar_h));
        panels_top += flag_bar_h + h * 0.018;
    }

    let mut delta_bar: Option<Rect> = None;
    if cfg.bool_key(SECTION, "show_delta_bar", false) {
        let db_h = h * 0.05;
        delta_bar = Some(Rect::from_xywh(left_left, panels_top, bar_w, db_h * 0.7));
        panels_top += db_h;
    }

    let total = panels_bottom - panels_top;
    let top_h = (total - gp) * 0.42;
    let bot_h = (total - gp) * 0.58;

    let mut top_right = right_edge;
    let mut p9 = Rect::from_xywh(0.0, 0.0, 0.0, 0.0);
    if show_pos {
        let p9_w = top_h * 1.30;
        p9 = Rect::from_xywh(right_edge - p9_w, panels_top, p9_w, top_h);
        top_right = p9.left() - hg;
    }

    let top_rect = Rect::from_xywh(
        left_left,
        panels_top,
        (top_right - left_left).max(1.0),
        top_h,
    );
    let bot_rect = Rect::from_xywh(
        left_left,
        panels_top + top_h + gp,
        (right_edge - left_left).max(1.0),
        bot_h,
    );

    draw_panel_rect(c, cfg, SECTION, top_rect);
    draw_panel_rect(c, cfg, SECTION, bot_rect);
    if show_pos {
        draw_position(c, cfg, p9, f, text_scale);
    }

    let ring_cx = (left_left + right_edge) * 0.5;
    let ring_cy = panels_top + total * 0.5;
    let ring_d = total * 0.80;
    // Match draw_ring's outer fill (half + 6%), then add clearance so metrics
    // sit outside the rim instead of kissing it.
    let ring_outer = ring_d * 0.56;
    let ring_clear = (h * 0.055).max(ring_d * 0.10);
    let gap_l = ring_cx - ring_outer - ring_clear;
    let gap_r = ring_cx + ring_outer + ring_clear;

    let vpad = bot_rect.height() * 0.08;
    // Horizontal inset keeps end metrics (laps / fuel) inside the rounded panel.
    let hpad = (bot_rect.height() * 0.16)
        .max(bot_rect.width() * 0.022)
        .max(12.0);

    let ipad = top_rect.height() * 0.12;
    if cfg.bool_key(SECTION, "show_shift_bar", true) {
        let shift_rect = Rect::from_ltrb(
            top_rect.left() + ipad,
            top_rect.center().1 - top_rect.height() * 0.20,
            gap_l.max(top_rect.left() + ipad + 40.0),
            top_rect.center().1 + top_rect.height() * 0.20,
        );
        if draw_shift(c, cfg, shift_rect, f, anim, mono_secs) {
            animating = true;
        }
    }

    let top_right_key = slot(cfg, "top_right", "incidents");
    if top_right_key != "none" {
        let status = Rect::from_ltrb(
            gap_r,
            top_rect.top(),
            top_rect.right() - ipad,
            top_rect.bottom(),
        );
        draw_status(c, cfg, status, &top_right_key, f, text_scale);
    }

    let primary_l = slot(cfg, "primary_left", "lap_count");
    let primary_r = slot(cfg, "primary_right", "speed");
    if primary_l != "none" || primary_r != "none" {
        let primary = Rect::from_ltrb(
            bot_rect.left() + hpad,
            bot_rect.top() + vpad,
            gap_l.max(bot_rect.left() + hpad + 10.0),
            bot_rect.bottom() - vpad,
        );
        draw_primary(c, cfg, primary, &primary_l, &primary_r, f, text_scale);
    }

    let stat_l = slot(cfg, "stat_left", "tires");
    let stat_r = slot(cfg, "stat_right", "fuel_stack");
    if stat_l != "none" || stat_r != "none" {
        let stats = Rect::from_ltrb(
            gap_r,
            bot_rect.top() + vpad,
            bot_rect.right() - hpad,
            bot_rect.bottom() - vpad,
        );
        draw_stats(c, cfg, stats, &stat_l, &stat_r, f, text_scale);
    }

    let strip_keys = [
        slot(cfg, "strip_left", "air_temp"),
        slot(cfg, "strip_center", "track_temp"),
        slot(cfg, "strip_right", "last_lap"),
    ];
    if strip_keys.iter().any(|k| k != "none") {
        let pill_w = (right_edge - left_left) * 0.78;
        let pill_h = h * 0.18;
        let pill = Rect::from_xywh(
            ring_cx - pill_w * 0.5,
            panels_bottom - pill_h * 0.22 + h * 0.01,
            pill_w,
            pill_h,
        );
        draw_strip(c, cfg, pill, &strip_keys, f, text_scale);
    }

    if cfg.bool_key(SECTION, "show_ring", true) {
        let thr = f.throttle.clamp(0.0, 1.0);
        let brk = f.brake.clamp(0.0, 1.0);
        let clt = f.clutch.clamp(0.0, 1.0);
        if cfg.str_key(SECTION, "center_mode", "ring") == "pedals" {
            draw_pedals(
                c, cfg, ring_cx, ring_cy, ring_d, f, text_scale, thr, brk, clt,
            );
        } else {
            draw_ring(
                c, cfg, ring_cx, ring_cy, ring_d, f, text_scale, thr, brk, clt,
            );
        }
    }

    if let Some(fr) = flag_rect {
        draw_flag(c, cfg, fr, f, ring_cx, text_scale);
    }
    if let Some(db) = delta_bar {
        draw_delta_bar(c, cfg, db, f);
    }

    animating
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

fn speed_value(cfg: &OverlayConfig, ms: f32) -> String {
    let unit = if cfg.imperial_units() { "mph" } else { "kph" };
    format!("{:.0} {unit}", cfg.conv_speed(ms))
}

fn fuel_amount(cfg: &OverlayConfig, litres: f32) -> String {
    let (v, unit) = if cfg.imperial_units() {
        (litres * 0.264_172_05, "Gal")
    } else {
        (litres, "L")
    };
    format!("{v:.1} {unit}")
}

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

fn metric_str(cfg: &OverlayConfig, f: &TelemetryFrame, key: &str) -> String {
    match key {
        "speed" => speed_value(cfg, f.speed_mps),
        "rpm" => format!("{:.0}", f.rpm),
        "gear" => gear_str(f.gear),
        "position" => {
            if f.position > 0 {
                format!("P{}", f.position)
            } else {
                "--".into()
            }
        }
        "car_number" => {
            if f.car_number.is_empty() {
                "--".into()
            } else {
                crate::telemetry::format_car_number(&f.car_number)
            }
        }
        "lap_count" => {
            if let Some(total) = crate::telemetry::finite_laps_total(f.laps_total) {
                format!("{}/{}", f.lap, total)
            } else if f.lap > 0 {
                format!("{}", f.lap)
            } else {
                "--".into()
            }
        }
        "laps_left" => {
            if let Some(total) = crate::telemetry::finite_laps_total(f.laps_total) {
                let lead = if f.lead_lap > 0 { f.lead_lap } else { f.lap };
                format!("{}", (total - lead).max(0))
            } else if let Some(rem) = f
                .session_laps_remain
                .filter(|v| v.is_finite() && *v < 32_000.0)
            {
                format!("{:.0}", rem)
            } else {
                "--".into()
            }
        }
        "lap" => {
            if f.lap > 0 {
                format!("{}", f.lap)
            } else {
                "--".into()
            }
        }
        "fuel" => fuel_amount(cfg, f.fuel_l),
        "fuel_laps" => format!("{:.1} Laps", f.laps_fuel),
        "incidents" => format!("{}x", f.incidents),
        "last_lap" => fmt_lap(f.last_lap_s),
        "best_lap" => fmt_lap(f.best_lap_s),
        "cur_lap" => fmt_lap(f.cur_lap_s),
        "delta" => f
            .delta
            .map(|d| format!("{d:+.2}"))
            .unwrap_or_else(|| "--".into()),
        "irating" => {
            let ir = f.irating;
            if cfg.bool_key(SECTION, "irating_abbreviate", true) && ir >= 1000 {
                format!("{:.1}k", ir as f32 / 1000.0)
            } else {
                format!("{ir}")
            }
        }
        "air_temp" => f
            .air_temp
            .map(|t| {
                let t = if cfg.imperial_units() {
                    t * 9.0 / 5.0 + 32.0
                } else {
                    t
                };
                format!("{t:.0}°")
            })
            .unwrap_or_else(|| "--".into()),
        "track_temp" => f
            .track_temp
            .map(|t| {
                let t = if cfg.imperial_units() {
                    t * 9.0 / 5.0 + 32.0
                } else {
                    t
                };
                format!("{t:.0}°")
            })
            .unwrap_or_else(|| "--".into()),
        _ => "--".into(),
    }
}

fn metric_lines(cfg: &OverlayConfig, f: &TelemetryFrame, key: &str) -> Vec<(String, String)> {
    match key {
        "fuel_stack" => vec![
            ("FUEL".into(), fuel_amount(cfg, f.fuel_l)),
            (String::new(), format!("{:.1} Laps", f.laps_fuel)),
        ],
        "tires" => vec![
            ("L".into(), format!("{:.0}%", f.tire_wear_l * 100.0)),
            ("R".into(), format!("{:.0}%", f.tire_wear_r * 100.0)),
        ],
        other => vec![(String::new(), metric_str(cfg, f, other))],
    }
}

fn slot(cfg: &OverlayConfig, key: &str, default: &str) -> String {
    let s = cfg.str_key(SECTION, key, default);
    if s.is_empty() {
        "none".into()
    } else {
        s
    }
}

fn label(
    c: &mut Canvas,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: Rgba,
    bold: bool,
    align: TextAlign,
) {
    // Baseline tweak so Skia matches egui CENTER_CENTER-ish placement.
    let baseline = y + size * 0.35;
    c.text(
        text,
        x,
        baseline,
        if bold {
            FontSpec::bold(size)
        } else {
            FontSpec::new(size)
        },
        color,
        align,
    );
}

/// Gear digit in the dash circle — centre on ink bounds so "1"/"N"/"R" sit true.
fn gear_label(c: &mut Canvas, cx: f32, cy: f32, text: &str, size: f32, color: Rgba) {
    c.text_ink_centered(text, cx, cy, FontSpec::bold(size), color);
}

fn text_w(c: &Canvas, size: f32, bold: bool, text: &str) -> f32 {
    c.measure_text(
        text,
        if bold {
            FontSpec::bold(size)
        } else {
            FontSpec::new(size)
        },
    )
}

/// Draw a named Font Awesome icon; returns advance width (0 if unknown).
fn icon_paint(c: &mut Canvas, x: f32, cy: f32, size: f32, key: &str, color: Rgba) -> f32 {
    super::icons::paint(c, key, x, cy, size, color, TextAlign::Left)
}

fn draw_position(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    box_r: Rect,
    f: &TelemetryFrame,
    text_scale: f32,
) {
    let orange = section_color(cfg, SECTION, "orange", "#ff9416");
    let frac = cfg.f64_key(SECTION, "corner_radius_frac", 0.0) as f32;
    // Position badge keeps a pill-like radius even when panels are square.
    let radius = if frac > 0.0 {
        box_r.width().min(box_r.height()) * frac
    } else {
        box_r.height() * 0.22
    };
    let top = section_color(cfg, SECTION, "bg_top", "#1b1f26f2");
    let bottom = section_color(cfg, SECTION, "bg_bottom", "#0f1216f2");
    c.fill_vertical_gradient(box_r, top, bottom, radius);
    let stroke_w = (box_r.height() * 0.045).max(2.0);
    c.stroke_rect(box_r, orange, radius, stroke_w);

    let text = if f.position > 0 {
        format!("P{}", f.position)
    } else {
        "--".into()
    };
    let mut fs = box_r.height() * 0.48 * text_scale;
    let tw = text_w(c, fs, true, &text);
    let max_w = box_r.width() * 0.78;
    if tw > max_w && tw > 0.0 {
        fs *= max_w / tw;
    }
    label(
        c,
        box_r.center().0,
        box_r.center().1,
        &text,
        fs,
        orange,
        true,
        TextAlign::Center,
    );
}

fn shift_should_blink(cfg: &OverlayConfig, f: &TelemetryFrame) -> bool {
    if !cfg.bool_key(SECTION, "shift_blink", true) {
        return false;
    }
    if f.rpm <= 0.0 {
        return false;
    }
    let pct = cfg.f64_key(SECTION, "shift_blink_pct", 0.99) as f32;
    let redline = f.redline.max(1.0);
    f.rpm >= redline * pct.clamp(0.5, 1.0)
}

fn draw_shift(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    f: &TelemetryFrame,
    anim: &mut DashAnim,
    mono_secs: f64,
) -> bool {
    let n = cfg.f64_key(SECTION, "shift_segments", 20.0).max(1.0) as i32;
    let gap = rect.width() / n as f32 * 0.30;
    let bw = rect.width() / n as f32 - gap;
    let redline = f.redline.max(1.0);
    let lit_target = (f.rpm / redline).clamp(0.0, 1.0) * n as f32;
    let red_f = cfg.f64_key(SECTION, "shift_red_frac", 0.16).clamp(0.0, 1.0) as f32;
    let yel_f = cfg
        .f64_key(SECTION, "shift_yellow_frac", 0.24)
        .clamp(0.0, (1.0 - red_f as f64).max(0.0)) as f32;
    let red0 = n as f32 * (1.0 - red_f);
    let yel0 = n as f32 * (1.0 - red_f - yel_f);
    let green = section_color(cfg, SECTION, "shift_green", "#46df7a");
    let yel = section_color(cfg, SECTION, "shift_yellow", "#ffd23a");
    let red = section_color(cfg, SECTION, "shift_red", "#e23b3b");
    let off = section_color(cfg, SECTION, "shift_off", "#333a42");

    let dt = anim_dt(mono_secs, &mut anim.lit_last_secs);
    anim.lit = ease(anim.lit, lit_target, dt, 0.08);
    let lit = anim.lit;
    let lit_animating = still_easing(lit, lit_target, 0.05);

    let now = f.session_time;
    let eligible = shift_should_blink(cfg, f);
    let max_sec = cfg.f64_key(SECTION, "shift_blink_max_sec", 3.0);
    let hz = cfg.f64_key(SECTION, "shift_blink_hz", 7.0).max(0.1);
    let mut need_repaint = lit_animating;
    let blink_dark = if eligible {
        if anim.blink_since_s.is_none() {
            anim.blink_since_s = Some(now);
            anim.blink_suppressed = false;
        }
        if !anim.blink_suppressed && max_sec > 0.0 {
            if let Some(since) = anim.blink_since_s {
                if now - since >= max_sec {
                    anim.blink_suppressed = true;
                }
            }
        }
        if !anim.blink_suppressed {
            need_repaint = true;
            (now * hz) % 1.0 >= 0.5
        } else {
            false
        }
    } else {
        anim.blink_since_s = None;
        anim.blink_suppressed = false;
        false
    };

    let full_h = rect.height();
    let tick_h = rect.height() * 0.5;
    for i in 0..n {
        let x = rect.left() + i as f32 * (bw + gap);
        let (cc, y, bh) = if blink_dark {
            (off, rect.top() + (full_h - tick_h) * 0.5, tick_h)
        } else if (i as f32) < lit {
            let cc = if (i as f32) >= red0 {
                red
            } else if (i as f32) >= yel0 {
                yel
            } else {
                green
            };
            (cc, rect.top(), full_h)
        } else {
            (off, rect.top() + (full_h - tick_h) * 0.5, tick_h)
        };
        let r = (bw * 0.4).min(bh * 0.5);
        c.fill_rect(Rect::from_xywh(x, y, bw.max(1.0), bh), cc, r);
    }
    need_repaint
}

fn draw_status(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    key: &str,
    f: &TelemetryFrame,
    text_scale: f32,
) {
    if key == "irating" {
        draw_irating_pair(c, cfg, rect, f, rect.height() * 0.34 * text_scale);
        return;
    }
    let val = metric_str(cfg, f, key);
    let h = rect.height();
    let mut ic_px = h * 0.46 * text_scale;
    let mut val_px = h * 0.46 * text_scale;
    let mut gap = h * 0.18;
    let mut iw = ic_px * 0.7;
    let mut vw = text_w(c, val_px, true, &val);
    let mut total = iw + gap + vw;
    if total > rect.width() && total > 0.0 {
        let s = rect.width() / total;
        ic_px *= s;
        val_px *= s;
        gap *= s;
        iw = ic_px * 0.7;
        vw = text_w(c, val_px, true, &val);
        total = iw + gap + vw;
    }
    let mut x = rect.left() + (rect.width() - total).max(0.0) * 0.5;
    let ic_col = if key == "incidents" {
        section_color(cfg, SECTION, "warn", "#e0a93a")
    } else {
        section_color(cfg, SECTION, "label", "#8b93a1")
    };
    iw = icon_paint(c, x, rect.center().1, ic_px, key, ic_col);
    x += iw + gap;
    label(
        c,
        x,
        rect.center().1,
        &val,
        val_px,
        section_color(cfg, SECTION, "value", "#f4f6f8"),
        true,
        TextAlign::Left,
    );
}

fn draw_primary(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    left_key: &str,
    right_key: &str,
    f: &TelemetryFrame,
    text_scale: f32,
) {
    let h = rect.height();
    let show_l = left_key != "none";
    let show_r = right_key != "none";
    if !show_l && !show_r {
        return;
    }
    let both = show_l && show_r;
    let cols = if both { 2 } else { 1 };
    let col_gap = if both {
        (h * 0.28).clamp(8.0, 22.0)
    } else {
        0.0
    };
    let usable = (rect.width() - col_gap).max(1.0);
    let cell_w = usable / cols as f32;
    let keys: Vec<&str> = match (show_l, show_r) {
        (true, true) => vec![left_key, right_key],
        (true, false) => vec![left_key],
        (false, true) => vec![right_key],
        _ => return,
    };
    for (i, key) in keys.into_iter().enumerate() {
        let cell = Rect::from_xywh(
            rect.left() + i as f32 * (cell_w + col_gap),
            rect.top(),
            cell_w,
            h,
        );
        let val = metric_str(cfg, f, key);
        let mut ic_px = h * 0.30 * text_scale;
        let mut val_px = h * 0.58 * text_scale;
        let mut gap = h * 0.12;
        let mut iw = ic_px * 0.7;
        let mut vw = text_w(c, val_px, true, &val);
        let mut total = iw + gap + vw;
        let fit_w = cell.width() * 0.96;
        if total > fit_w && total > 0.0 {
            let s = fit_w / total;
            ic_px *= s;
            val_px *= s;
            gap *= s;
            iw = ic_px * 0.7;
            vw = text_w(c, val_px, true, &val);
            total = iw + gap + vw;
        }
        let mut x = if both {
            if i == 0 {
                cell.left()
            } else {
                cell.right() - total
            }
        } else {
            cell.left() + (cell.width() - total).max(0.0) * 0.5
        };
        iw = icon_paint(
            c,
            x,
            cell.center().1,
            ic_px,
            key,
            section_color(cfg, SECTION, "label", "#8b93a1"),
        );
        x += iw + gap;
        label(
            c,
            x,
            cell.center().1,
            &val,
            val_px,
            section_color(cfg, SECTION, "value", "#f4f6f8"),
            true,
            TextAlign::Left,
        );
    }
}

fn draw_stats(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    left_key: &str,
    right_key: &str,
    f: &TelemetryFrame,
    text_scale: f32,
) {
    let show_l = left_key != "none";
    let show_r = right_key != "none";
    if !show_l && !show_r {
        return;
    }
    let both = show_l && show_r;
    let cols = if both { 2 } else { 1 };
    let col_gap = if both {
        (rect.height() * 0.28).clamp(8.0, 22.0)
    } else {
        0.0
    };
    let usable = (rect.width() - col_gap).max(1.0);
    let cell_w = usable / cols as f32;
    let keys: Vec<(usize, &str)> = match (show_l, show_r) {
        (true, true) => vec![(0, left_key), (1, right_key)],
        (true, false) => vec![(0, left_key)],
        (false, true) => vec![(0, right_key)],
        _ => return,
    };
    for (i, key) in keys {
        let cell = Rect::from_xywh(
            rect.left() + i as f32 * (cell_w + col_gap),
            rect.top(),
            cell_w,
            rect.height(),
        );
        if key == "irating" {
            let pair_h = cell.height() * 0.24 * text_scale;
            let pair_w = irating_pair_width(c, cfg, f, pair_h, cell.height());
            let pair_rect = if both && i == 1 {
                Rect::from_xywh(
                    cell.right() - pair_w,
                    cell.top(),
                    pair_w.min(cell.width()),
                    cell.height(),
                )
            } else {
                cell
            };
            draw_irating_pair(c, cfg, pair_rect, f, pair_h);
            continue;
        }
        let lines = metric_lines(cfg, f, key);
        let h = cell.height();
        let mut ic_px = h * 0.40 * text_scale;
        let mut lbl_px = h * 0.20 * text_scale;
        let mut val_px = h * 0.24 * text_scale;
        let mut icon_gap = h * 0.18;
        let mut lbl_gap = h * 0.12;
        let mut iw = ic_px * 0.7;
        let mut widest = 0.0_f32;
        for (lbl, val) in &lines {
            let lw = if lbl.is_empty() {
                0.0
            } else {
                text_w(c, lbl_px, false, lbl) + lbl_gap
            };
            widest = widest.max(lw + text_w(c, val_px, true, val));
        }
        let mut total = iw + icon_gap + widest;
        let fit_w = cell.width() * 0.96;
        if total > fit_w && total > 0.0 {
            let s = fit_w / total;
            ic_px *= s;
            lbl_px *= s;
            val_px *= s;
            icon_gap *= s;
            lbl_gap *= s;
            iw = ic_px * 0.7;
            widest = 0.0;
            for (lbl, val) in &lines {
                let lw = if lbl.is_empty() {
                    0.0
                } else {
                    text_w(c, lbl_px, false, lbl) + lbl_gap
                };
                widest = widest.max(lw + text_w(c, val_px, true, val));
            }
            total = iw + icon_gap + widest;
        }
        let mut x = if both && i == 1 {
            cell.right() - total
        } else {
            cell.left()
        };
        iw = icon_paint(
            c,
            x,
            cell.center().1,
            ic_px,
            key,
            section_color(cfg, SECTION, "label", "#8b93a1"),
        );
        x += iw + icon_gap;
        let n = lines.len().max(1) as f32;
        for (li, (sub, val)) in lines.iter().enumerate() {
            let y = cell.top() + cell.height() * ((li as f32 + 0.5) / n);
            if li > 0 {
                let y0 = cell.top() + cell.height() * (li as f32 / n);
                c.line(
                    x,
                    y0,
                    cell.right() - 4.0,
                    y0,
                    section_color(cfg, SECTION, "cell_border", "#ffffff20"),
                    1.0,
                );
            }
            let mut tx = x;
            if !sub.is_empty() {
                label(
                    c,
                    tx,
                    y,
                    sub,
                    lbl_px,
                    section_color(cfg, SECTION, "label", "#8b93a1"),
                    false,
                    TextAlign::Left,
                );
                tx += text_w(c, lbl_px, false, sub) + lbl_gap;
            }
            label(
                c,
                tx,
                y,
                val,
                val_px,
                section_color(cfg, SECTION, "value", "#f4f6f8"),
                true,
                TextAlign::Left,
            );
        }
    }
}

fn draw_strip(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    pill: Rect,
    keys: &[String; 3],
    f: &TelemetryFrame,
    text_scale: f32,
) {
    let sh = pill.height();
    draw_dark_cell(c, cfg, SECTION, pill, sh * 0.5);
    let items: Vec<&str> = keys
        .iter()
        .map(|s| s.as_str())
        .filter(|k| *k != "none")
        .collect();
    if items.is_empty() {
        return;
    }
    let pad = sh * 0.28;
    let cx0 = pill.left() + pad;
    let content_w = pill.width() - 2.0 * pad;
    let cell = content_w / items.len() as f32;
    let gap = sh * 0.12;
    for (i, key) in items.into_iter().enumerate() {
        if key == "irating" {
            let val_px = sh * 0.34 * text_scale;
            let pair_w = irating_pair_width(c, cfg, f, val_px, sh);
            let tx = cx0 + i as f32 * cell + (cell - pair_w) * 0.5;
            draw_irating_pair(
                c,
                cfg,
                Rect::from_xywh(tx, pill.top(), pair_w, sh),
                f,
                val_px,
            );
            continue;
        }
        let val = metric_str(cfg, f, key);
        let ic_px = sh * 0.42 * text_scale;
        let val_px = sh * 0.40 * text_scale;
        let iw = ic_px * 0.7;
        let vw = text_w(c, val_px, true, &val);
        let total = iw + gap + vw;
        let mut tx = cx0 + i as f32 * cell + (cell - total) * 0.5;
        let iw = icon_paint(
            c,
            tx,
            pill.center().1,
            ic_px,
            key,
            section_color(cfg, SECTION, "label", "#8b93a1"),
        );
        tx += iw + gap;
        label(
            c,
            tx,
            pill.center().1,
            &val,
            val_px,
            section_color(cfg, SECTION, "value", "#f4f6f8"),
            true,
            TextAlign::Left,
        );
    }
}

fn irating_pair_width(
    c: &Canvas,
    cfg: &OverlayConfig,
    f: &TelemetryFrame,
    val_px: f32,
    sh: f32,
) -> f32 {
    let base = metric_str(cfg, f, "irating");
    let mut w = text_w(c, val_px, true, &base);
    w += sh * 0.42 * 0.7 + sh * 0.18;
    if cfg.bool_key(SECTION, "show_irating_projection", false) {
        if let Some(d) = f.irating_delta {
            if d != 0 {
                w += text_w(c, val_px * 0.75, true, &format!("{}", d.abs())) + val_px * 0.7;
            }
        }
    }
    w
}

fn draw_irating_pair(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    f: &TelemetryFrame,
    val_px: f32,
) {
    let sh = rect.height();
    let ic_px = sh * 0.42;
    let mut x = rect.left();
    let iw = icon_paint(
        c,
        x,
        rect.center().1,
        ic_px,
        "irating",
        section_color(cfg, SECTION, "label", "#8b93a1"),
    );
    x += iw + sh * 0.18;
    let base = metric_str(cfg, f, "irating");
    label(
        c,
        x,
        rect.center().1,
        &base,
        val_px,
        section_color(cfg, SECTION, "value", "#f4f6f8"),
        true,
        TextAlign::Left,
    );
    x += text_w(c, val_px, true, &base) + val_px * 0.15;
    if cfg.bool_key(SECTION, "show_irating_projection", false) {
        if let Some(d) = f.irating_delta {
            if d != 0 {
                let up = d > 0;
                let col = if up {
                    section_color(cfg, SECTION, "irating_delta_up", "#46df7a")
                } else {
                    section_color(cfg, SECTION, "irating_delta_down", "#ff5050")
                };
                let arrow_key = if up { "irating_up" } else { "irating_down" };
                let aw = icon_paint(c, x, rect.center().1, val_px * 0.55, arrow_key, col);
                x += aw.max(val_px * 0.45);
                label(
                    c,
                    x,
                    rect.center().1,
                    &format!("{}", d.abs()),
                    val_px * 0.75,
                    col,
                    true,
                    TextAlign::Left,
                );
            }
        }
    }
}

fn selected_inputs(
    cfg: &OverlayConfig,
    f: &TelemetryFrame,
    thr: f32,
    brk: f32,
    clt: f32,
) -> Vec<(f32, &'static str, bool)> {
    let mut out = Vec::new();
    if cfg.bool_key(SECTION, "show_throttle", true) {
        out.push((thr, "throttle", false));
    }
    if cfg.bool_key(SECTION, "show_brake", true) {
        out.push((brk, "brake", f.abs_active));
    }
    if cfg.bool_key(SECTION, "show_clutch", false) {
        out.push((clt, "clutch", false));
    }
    out
}

fn pedal_color(cfg: &OverlayConfig, colkey: &str, abs_on: bool) -> Rgba {
    if abs_on {
        return section_color(cfg, SECTION, "abs", "#ffd23a");
    }
    let fallback = match colkey {
        "brake" => "#e23b3b",
        "clutch" => "#3aa0ff",
        _ => "#46df7a",
    };
    section_color(cfg, SECTION, colkey, fallback)
}

fn draw_ring(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    cx: f32,
    cy: f32,
    ring_d: f32,
    f: &TelemetryFrame,
    text_scale: f32,
    thr: f32,
    brk: f32,
    clt: f32,
) {
    let mr = ring_d * 0.5 + ring_d * 0.06;
    let border = section_color(cfg, SECTION, "cell_border", "#ffffff20").with_alpha(150);
    c.circle(
        cx,
        cy,
        mr,
        section_color(cfg, SECTION, "bg_bottom", "#0f1216"),
        true,
    );
    c.circle_ex(cx, cy, mr, border, false, (ring_d * 0.022).max(1.5));

    let inputs = selected_inputs(cfg, f, thr, brk, clt);
    let n = inputs.len();
    let gear_px = if n <= 1 {
        ring_d * 0.50
    } else if n == 2 {
        ring_d * 0.40
    } else {
        ring_d * 0.32
    } * text_scale;

    if n > 0 {
        let pen_w = ring_d
            * (if n == 1 {
                0.11
            } else if n == 2 {
                0.075
            } else {
                0.055
            });
        let gap = pen_w * 0.55;
        let r_out = ring_d * 0.5 - pen_w * 0.5 - ring_d * 0.015;
        for (i, (val, colkey, abs_on)) in inputs.iter().enumerate() {
            let r = r_out - i as f32 * (pen_w + gap);
            let on = pedal_color(cfg, colkey, *abs_on);
            draw_ring_arc(c, cfg, cx, cy, r, pen_w, *val, on);
        }
    }

    gear_label(
        c,
        cx,
        cy,
        &gear_str(f.gear),
        gear_px,
        section_color(cfg, SECTION, "gear", "#ffffff"),
    );
}

fn draw_ring_arc(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    cx: f32,
    cy: f32,
    r: f32,
    pen_w: f32,
    mut frac: f32,
    on_color: Rgba,
) {
    frac = frac.clamp(0.0, 1.0);
    if frac < 0.02 {
        frac = 0.0;
    }
    let off = section_color(cfg, SECTION, "ring_track", "#333a42");
    // Continuous track + lit sweep (no segmented gaps).
    let steps = 64;
    c.stroke_arc(cx, cy, r, 0.0, std::f32::consts::TAU, off, pen_w, steps);
    if frac > 0.0 {
        // Start at top (π/2), sweep clockwise to match prior segment order.
        let a0 = std::f32::consts::FRAC_PI_2;
        let a1 = a0 - frac * std::f32::consts::TAU;
        let lit_steps = ((steps as f32) * frac).ceil().max(2.0) as usize;
        c.stroke_arc(cx, cy, r, a0, a1, on_color, pen_w, lit_steps);
    }
}

fn draw_pedals(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    cx: f32,
    cy: f32,
    ring_d: f32,
    f: &TelemetryFrame,
    text_scale: f32,
    thr: f32,
    brk: f32,
    clt: f32,
) {
    let mr = ring_d * 0.5 + ring_d * 0.06;
    let border = section_color(cfg, SECTION, "cell_border", "#ffffff20").with_alpha(150);
    c.circle(
        cx,
        cy,
        mr,
        section_color(cfg, SECTION, "bg_bottom", "#0f1216"),
        true,
    );
    c.circle_ex(cx, cy, mr, border, false, (ring_d * 0.022).max(1.5));

    let bars = selected_inputs(cfg, f, thr, brk, clt);
    if bars.is_empty() {
        gear_label(
            c,
            cx,
            cy,
            &gear_str(f.gear),
            ring_d * 0.50 * text_scale,
            section_color(cfg, SECTION, "gear", "#ffffff"),
        );
        return;
    }

    gear_label(
        c,
        cx,
        cy - ring_d * 0.28,
        &gear_str(f.gear),
        ring_d * 0.26 * text_scale,
        section_color(cfg, SECTION, "gear", "#ffffff"),
    );

    let n = bars.len();
    let area_w = ring_d
        * (if n == 1 {
            0.26
        } else if n == 2 {
            0.46
        } else {
            0.60
        });
    let area_h = ring_d * 0.44;
    let top = cy - ring_d * 0.14;
    let bottom = top + area_h;
    let bar_w = area_w / (n as f32 + (n as f32 - 1.0) * 0.6);
    let gap = bar_w * 0.6;
    let x0 = cx - area_w * 0.5;
    let rad = bar_w * 0.30;
    for (i, (val, ckey, abs_on)) in bars.iter().enumerate() {
        let x = x0 + i as f32 * (bar_w + gap);
        c.fill_rect(
            Rect::from_xywh(x, top, bar_w, area_h),
            section_color(cfg, SECTION, "pedal_track", "#333a42"),
            rad,
        );
        let fh = area_h * val.clamp(0.0, 1.0);
        if fh > 0.5 {
            c.fill_rect(
                Rect::from_xywh(x, bottom - fh, bar_w, fh),
                pedal_color(cfg, ckey, *abs_on),
                rad,
            );
        }
    }
}

fn flag_bar_style(cfg: &OverlayConfig, flag: &str) -> Option<(String, &'static str, &'static str)> {
    Some(match flag {
        "yellow" => ("CAUTION".into(), "flag_yellow", "flag_yellow_text"),
        "black" => ("BLACK FLAG".into(), "flag_black", "flag_black_text"),
        "green" => ("GREEN".into(), "flag_green", "flag_green_text"),
        "white" => ("FINAL NEXT".into(), "flag_white_bg", "flag_white_text"),
        "red" => ("RED FLAG".into(), "flag_red", "flag_red_text"),
        "blue" => ("LET BY".into(), "flag_blue", "flag_blue_text"),
        "checkered" => ("FINISH".into(), "flag_checker_bg", "flag_checker_text"),
        "meatball" => ("MEATBALL".into(), "flag_meatball", "flag_meatball_text"),
        "furled" => ("WARNING".into(), "flag_furled", "flag_furled_text"),
        "dq" => ("DISQUALIFIED".into(), "flag_dq", "flag_dq_text"),
        "debris" => ("DEBRIS".into(), "flag_debris", "flag_debris_text"),
        "crossed" => ("HALFWAY".into(), "flag_crossed", "flag_crossed_text"),
        "start_go" => (
            cfg.str_key(SECTION, "start_go_text", "GO"),
            "flag_green",
            "flag_green_text",
        ),
        "start_set" => (
            cfg.str_key(SECTION, "start_set_text", "SET"),
            "flag_green",
            "flag_green_text",
        ),
        "start_ready" => (
            cfg.str_key(SECTION, "start_ready_text", "READY"),
            "flag_green",
            "flag_green_text",
        ),
        _ => return None,
    })
}

fn draw_flag(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    f: &TelemetryFrame,
    center_x: f32,
    text_scale: f32,
) {
    let Some(flag) = f.flag.as_deref() else {
        return;
    };
    let Some((title, bgk, fgk)) = flag_bar_style(cfg, flag) else {
        return;
    };
    let bg = section_color(cfg, SECTION, bgk, "#ebeef0");
    let fg = section_color(cfg, SECTION, fgk, "#141414");
    let r = rect.height() * 0.5;
    c.fill_rect(rect, bg, r);
    c.stroke_rect(rect, Rgba::WHITE.with_alpha(45), r, 1.0);

    c.clip_rect(rect, |c| {
        let hatch = fg.with_alpha(70);
        if flag == "checkered" {
            let sq = rect.height() * 0.5;
            let mut row = 0;
            let mut y = rect.top();
            while y < rect.bottom() - 0.5 {
                let mut col = row % 2;
                let mut x = rect.left();
                while x < rect.right() - 0.5 {
                    if col % 2 == 0 {
                        c.fill_rect(
                            Rect::from_xywh(
                                x,
                                y,
                                sq.min(rect.right() - x),
                                sq.min(rect.bottom() - y),
                            ),
                            fg.with_alpha(90),
                            0.0,
                        );
                    }
                    x += sq;
                    col += 1;
                }
                y += sq;
                row += 1;
            }
        } else {
            let step = rect.height() * 0.6;
            let pen = (rect.height() * 0.16).max(2.0);
            let mut x = rect.left() - rect.height();
            while x < rect.right() + rect.height() {
                c.line(x, rect.bottom(), x + rect.height(), rect.top(), hatch, pen);
                x += step;
            }
        }
    });

    let context = f.flag_context.as_deref().unwrap_or("").trim();
    if !context.is_empty() {
        let title_px = rect.height() * 0.36 * text_scale;
        let sub_px = rect.height() * 0.24 * text_scale;
        let tw = text_w(c, title_px, true, &title).max(text_w(c, sub_px, false, context));
        let pad = rect.height() * 0.28;
        let gap = Rect::from_xywh(
            center_x - (tw + pad * 2.0) * 0.5,
            rect.top(),
            tw + pad * 2.0,
            rect.height(),
        );
        c.fill_rect(gap, bg, gap.height() * 0.5);
        label(
            c,
            center_x,
            rect.center().1 - rect.height() * 0.26,
            &title,
            title_px,
            fg,
            true,
            TextAlign::Center,
        );
        label(
            c,
            center_x,
            rect.center().1 + rect.height() * 0.28,
            context,
            sub_px,
            fg.with_alpha(((fg.a as f32) * 0.88) as u8),
            false,
            TextAlign::Center,
        );
    } else {
        let title_px = rect.height() * 0.52 * text_scale;
        let tw = text_w(c, title_px, true, &title);
        let pad = rect.height() * 0.32;
        let gap = Rect::from_xywh(
            center_x - (tw + pad * 2.0) * 0.5,
            rect.top(),
            tw + pad * 2.0,
            rect.height(),
        );
        c.fill_rect(gap, bg, gap.height() * 0.5);
        label(
            c,
            rect.center().0,
            rect.center().1,
            &title,
            title_px,
            fg,
            true,
            TextAlign::Center,
        );
    }
}

fn draw_delta_bar(c: &mut Canvas, cfg: &OverlayConfig, rect: Rect, f: &TelemetryFrame) {
    let r = rect.height() * 0.5;
    c.fill_rect(rect, section_color(cfg, SECTION, "track", "#262b34"), r);
    let rng = cfg.f64_key(SECTION, "delta_bar_range", 1.0).max(0.001) as f32;
    let delta = f.delta.unwrap_or(0.0) as f32;
    let t = (delta / rng).clamp(-1.0, 1.0);
    let cx = rect.center().0;
    if t.abs() > 0.001 {
        let fill_w = rect.width() * 0.5 * t.abs();
        let fill = if t < 0.0 {
            Rect::from_ltrb(cx, rect.top(), cx + fill_w, rect.bottom())
        } else {
            Rect::from_ltrb(cx - fill_w, rect.top(), cx, rect.bottom())
        };
        let col = if t < 0.0 {
            section_color(cfg, SECTION, "faster", "#46df7a")
        } else {
            section_color(cfg, SECTION, "slower", "#e23b3b")
        };
        c.fill_rect(fill, col, r);
    }
    c.line(
        cx,
        rect.top() + 1.0,
        cx,
        rect.bottom() - 1.0,
        section_color(cfg, SECTION, "border", "#ffffff28"),
        1.0,
    );
}
