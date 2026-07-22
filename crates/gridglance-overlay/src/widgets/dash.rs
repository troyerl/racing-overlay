//! Dash — one-to-one port of Python `overlay/widgets/dash.py` paint path.

use super::WidgetCtx;
use crate::chrome::{color_with_alpha, draw_dark_cell, draw_panel_rect, full_rect, label};
use crate::config::OverlayConfig;
use crate::icons;
use crate::telemetry::TelemetryFrame;
use egui::{Align2, Color32, FontId, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

const SECTION: &str = "dash";

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

/// Single-line metric string (Python `_m_str`).
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
            } else if let Some(rem) = f.session_laps_remain.filter(|v| v.is_finite() && *v < 32_000.0)
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

/// Stacked rows for stats cells: (sub_label, value).
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

fn text_w(ui: &Ui, font: &FontId, text: &str) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    })
}

fn icon_paint(ui: &mut Ui, pos: Pos2, size: f32, name: &str, color: Color32) {
    if let Some(g) = icons::glyph(name) {
        ui.painter()
            .text(pos, Align2::LEFT_CENTER, g, icons::font_id(size), color);
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    // Python dash has no outer card — only section panel rects (see-through between them).

    let cfg = ctx.cfg;
    let f = ctx.frame;
    // Gear / RPM / pedals must track live SDK ticks — not the idle ~10 Hz present path.
    if f.connected {
        *ctx.panel_animating = true;
    }
    let w = rect.width();
    let h = rect.height();
    let text_scale = cfg.text_scale(SECTION);

    let m = h * 0.045;
    let gp = h * 0.022;
    let hg = w * 0.007;
    let show_pos = cfg.bool_key(SECTION, "show_position", true);
    let mut panels_top = rect.top() + m;
    let panels_bottom = rect.top() + h * 0.80;
    let left_left = rect.left() + m;
    let right_edge = rect.right() - m;
    let bar_w = right_edge - left_left;

    let mut flag_rect: Option<Rect> = None;
    if cfg.bool_key(SECTION, "show_flags", true) {
        let has_ctx = f.flag.is_some() && f.flag_context.as_ref().is_some_and(|s| !s.is_empty());
        let flag_bar_h =
            (if has_ctx { h * 0.165 } else { h * 0.105 }).max(if has_ctx { 8.0 } else { 6.0 });
        flag_rect = Some(Rect::from_min_size(
            Pos2::new(left_left, panels_top),
            Vec2::new(bar_w, flag_bar_h),
        ));
        panels_top += flag_bar_h + h * 0.03;
    }

    let mut delta_bar: Option<Rect> = None;
    if cfg.bool_key(SECTION, "show_delta_bar", false) {
        let db_h = h * 0.05;
        delta_bar = Some(Rect::from_min_size(
            Pos2::new(left_left, panels_top),
            Vec2::new(bar_w, db_h * 0.7),
        ));
        panels_top += db_h;
    }

    let total = panels_bottom - panels_top;
    let top_h = (total - gp) * 0.42;
    let bot_h = (total - gp) * 0.58;

    let mut top_right = right_edge;
    let mut p9 = Rect::NOTHING;
    if show_pos {
        let p9_w = top_h * 1.30;
        p9 = Rect::from_min_size(
            Pos2::new(right_edge - p9_w, panels_top),
            Vec2::new(p9_w, top_h),
        );
        top_right = p9.left() - hg;
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
        draw_position(ui, cfg, p9, f, text_scale);
    }

    let ring_cx = (left_left + right_edge) * 0.5;
    let ring_cy = panels_top + total * 0.5;
    let ring_d = total * 0.80;
    let ring_half = ring_d * 0.5;
    let bpad = bot_rect.height() * 0.14;
    // Clearance between the center ring and left/right metric columns.
    let ring_gap = (h * 0.09).max(ring_d * 0.10);
    let gap_l = ring_cx - ring_half - ring_gap;
    let gap_r = ring_cx + ring_half + ring_gap;

    let ipad = top_rect.height() * 0.22;
    if cfg.bool_key(SECTION, "show_shift_bar", true) {
        draw_shift(
            ui,
            cfg,
            Rect::from_min_max(
                Pos2::new(
                    top_rect.left() + ipad,
                    top_rect.center().y - top_rect.height() * 0.20,
                ),
                Pos2::new(
                    gap_l.max(top_rect.left() + ipad + 40.0),
                    top_rect.center().y + top_rect.height() * 0.20,
                ),
            ),
            f,
            ctx.mono_secs,
            ctx.panel_animating,
        );
    }

    let top_right_key = slot(cfg, "top_right", "incidents");
    if top_right_key != "none" {
        let status = Rect::from_min_max(
            Pos2::new(gap_r, top_rect.top()),
            Pos2::new(top_rect.right() - ipad, top_rect.bottom()),
        );
        draw_status(ui, cfg, status, &top_right_key, f, text_scale);
    }

    let primary_l = slot(cfg, "primary_left", "lap_count");
    let primary_r = slot(cfg, "primary_right", "speed");
    if primary_l != "none" || primary_r != "none" {
        let primary = Rect::from_min_max(
            Pos2::new(bot_rect.left() + bpad, bot_rect.top() + bpad),
            Pos2::new(
                gap_l.max(bot_rect.left() + bpad + 10.0),
                bot_rect.bottom() - bpad,
            ),
        );
        draw_primary(ui, cfg, primary, &primary_l, &primary_r, f, text_scale);
    }

    let stat_l = slot(cfg, "stat_left", "tires");
    let stat_r = slot(cfg, "stat_right", "fuel_stack");
    if stat_l != "none" || stat_r != "none" {
        let stats = Rect::from_min_max(
            Pos2::new(gap_r, bot_rect.top() + bpad),
            Pos2::new(bot_rect.right() - bpad, bot_rect.bottom() - bpad),
        );
        draw_stats(ui, cfg, stats, &stat_l, &stat_r, f, text_scale);
    }

    let strip_keys = [
        slot(cfg, "strip_left", "air_temp"),
        slot(cfg, "strip_center", "track_temp"),
        slot(cfg, "strip_right", "last_lap"),
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
        draw_strip(ui, cfg, pill, &strip_keys, f, text_scale);
    }

    if cfg.bool_key(SECTION, "show_ring", true) {
        // Raw pedals — easing lagged gear/RPM via sparse presents and glitched bars.
        let thr = f.throttle.clamp(0.0, 1.0);
        let brk = f.brake.clamp(0.0, 1.0);
        let clt = f.clutch.clamp(0.0, 1.0);
        if cfg.str_key(SECTION, "center_mode", "ring") == "pedals" {
            draw_pedals(
                ui,
                cfg,
                ring_cx,
                ring_cy,
                ring_d,
                f,
                text_scale,
                thr,
                brk,
                clt,
            );
        } else {
            draw_ring(
                ui,
                cfg,
                ring_cx,
                ring_cy,
                ring_d,
                f,
                text_scale,
                thr,
                brk,
                clt,
            );
        }
    }

    if let Some(fr) = flag_rect {
        draw_flag(ui, cfg, fr, f, ring_cx, text_scale);
    }
    if let Some(db) = delta_bar {
        draw_delta_bar(ui, cfg, db, f);
    }
}

fn draw_position(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    box_r: Rect,
    f: &TelemetryFrame,
    text_scale: f32,
) {
    draw_panel_rect(ui, cfg, SECTION, box_r);
    let orange = cfg.color(SECTION, "orange", "#ff9416");
    let radius = (box_r.width().min(box_r.height())
        * cfg.f64_key(SECTION, "corner_radius_frac", 0.08) as f32)
        .max(4.0);
    ui.painter().rect_stroke(
        box_r,
        egui::CornerRadius::same(radius as u8),
        Stroke::new((box_r.height() * 0.022).max(1.6), orange),
        StrokeKind::Inside,
    );
    let text = if f.position > 0 {
        format!("P{}", f.position)
    } else {
        "--".into()
    };
    let mut fs = box_r.height() * 0.40 * text_scale;
    let font = FontId::proportional(fs);
    let tw = text_w(ui, &font, &text);
    let max_w = box_r.width() * 0.74;
    if tw > max_w && tw > 0.0 {
        fs *= max_w / tw;
    }
    label(
        ui,
        box_r.center(),
        Align2::CENTER_CENTER,
        &text,
        fs,
        orange,
        true,
    );
}

#[derive(Clone, Default)]
struct ShiftBlinkState {
    since_s: Option<f64>,
    suppressed: bool,
}

fn shift_should_blink(cfg: &OverlayConfig, f: &TelemetryFrame) -> bool {
    if !cfg.bool_key(SECTION, "shift_blink", true) {
        return false;
    }
    if f.rpm <= 0.0 {
        return false;
    }
    // No top_gear on frame — still blink in reverse/neutral skip only when gear <= 0 optional.
    // Match Python: skip blink in top gear when known; without top_gear, always eligible by rpm.
    let pct = cfg.f64_key(SECTION, "shift_blink_pct", 0.99) as f32;
    let redline = f.redline.max(1.0);
    f.rpm >= redline * pct.clamp(0.5, 1.0)
}

fn draw_shift(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    f: &TelemetryFrame,
    mono_secs: f64,
    panel_animating: &mut bool,
) {
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
    let green = cfg.color(SECTION, "shift_green", "#46df7a");
    let yel = cfg.color(SECTION, "shift_yellow", "#ffd23a");
    let red = cfg.color(SECTION, "shift_red", "#e23b3b");
    let off = cfg.color(SECTION, "shift_off", "#333a42");

    let lit_id = egui::Id::new("dash_shift_lit");
    let mut lit_st = ui.ctx().data_mut(|d| {
        d.get_temp::<(f32, f64)>(lit_id)
            .unwrap_or((lit_target, 0.0))
    });
    let dt = crate::chrome::anim_dt(mono_secs, &mut lit_st.1);
    lit_st.0 = crate::chrome::ease(lit_st.0, lit_target, dt, 0.08);
    let lit = lit_st.0;
    let lit_animating = crate::chrome::still_easing(lit, lit_target, 0.05);
    ui.ctx().data_mut(|d| d.insert_temp(lit_id, lit_st));

    // Blink: dark half forces all segments to off ticks (Python `_draw_shift`).
    let id = egui::Id::new("dash_shift_blink");
    let now = f.session_time;
    let eligible = shift_should_blink(cfg, f);
    let max_sec = cfg.f64_key(SECTION, "shift_blink_max_sec", 3.0);
    let hz = cfg.f64_key(SECTION, "shift_blink_hz", 7.0).max(0.1);
    let mut need_repaint = lit_animating;
    let blink_dark = ui.ctx().data_mut(|d| {
        let st = d.get_temp_mut_or_default::<ShiftBlinkState>(id);
        if eligible {
            if st.since_s.is_none() {
                st.since_s = Some(now);
                st.suppressed = false;
            }
            if !st.suppressed && max_sec > 0.0 {
                if let Some(since) = st.since_s {
                    if now - since >= max_sec {
                        st.suppressed = true;
                    }
                }
            }
            if !st.suppressed {
                need_repaint = true;
                (now * hz) % 1.0 >= 0.5
            } else {
                false
            }
        } else {
            st.since_s = None;
            st.suppressed = false;
            false
        }
    });
    *panel_animating = need_repaint;
    if need_repaint {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1));
    }

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
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(bw.max(1.0), bh)),
            egui::CornerRadius::same(r as u8),
            cc,
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
) {
    if key == "irating" {
        draw_irating_pair(ui, cfg, rect, f, rect.height() * 0.34 * text_scale);
        return;
    }
    let val = metric_str(cfg, f, key);
    let h = rect.height();
    let mut ic_px = h * 0.46 * text_scale;
    let mut val_px = h * 0.46 * text_scale;
    let glyph = icons::glyph(key);
    let ic_font = icons::font_id(ic_px);
    let val_font = FontId::proportional(val_px);
    let mut iw = glyph
        .as_ref()
        .map(|g| text_w(ui, &ic_font, g))
        .unwrap_or(0.0);
    let mut gap = h * 0.18;
    let mut vw = text_w(ui, &val_font, &val);
    let mut total = iw + if glyph.is_some() { gap } else { 0.0 } + vw;
    if total > rect.width() && total > 0.0 {
        let s = rect.width() / total;
        ic_px *= s;
        val_px *= s;
        gap *= s;
        let ic_font = icons::font_id(ic_px);
        let val_font = FontId::proportional(val_px);
        iw = glyph
            .as_ref()
            .map(|g| text_w(ui, &ic_font, g))
            .unwrap_or(0.0);
        vw = text_w(ui, &val_font, &val);
        total = iw + if glyph.is_some() { gap } else { 0.0 } + vw;
    }
    let mut x = rect.left() + (rect.width() - total).max(0.0) * 0.5;
    let ic_col = if key == "incidents" {
        cfg.color(SECTION, "warn", "#e0a93a")
    } else {
        cfg.color(SECTION, "label", "#8b93a1")
    };
    if let Some(g) = glyph {
        icon_paint(ui, Pos2::new(x, rect.center().y), ic_px, key, ic_col);
        // re-measure for advance
        x += text_w(ui, &icons::font_id(ic_px), &g) + gap;
        let _ = g;
    }
    label(
        ui,
        Pos2::new(x, rect.center().y),
        Align2::LEFT_CENTER,
        &val,
        val_px,
        cfg.color(SECTION, "value", "#f4f6f8"),
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
) {
    let h = rect.height();
    let show_l = left_key != "none";
    let show_r = right_key != "none";
    if !show_l && !show_r {
        return;
    }
    let both = show_l && show_r;
    let cols = if both { 2 } else { 1 };
    // Keep lap / speed (etc.) from colliding when both columns are short.
    let col_gap = if both {
        (h * 0.45).clamp(16.0, 36.0)
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
        let cell = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * (cell_w + col_gap), rect.top()),
            Vec2::new(cell_w, h),
        );
        let val = metric_str(cfg, f, key);
        let mut ic_px = h * 0.30 * text_scale;
        let mut val_px = h * 0.58 * text_scale;
        let mut gap = h * 0.18;
        let g = icons::glyph(key);
        let mut iw = g
            .as_ref()
            .map(|gg| text_w(ui, &icons::font_id(ic_px), gg))
            .unwrap_or(0.0);
        let mut vw = text_w(ui, &FontId::proportional(val_px), &val);
        let mut total = iw + if g.is_some() { gap } else { 0.0 } + vw;
        if total > cell.width() && total > 0.0 {
            let s = cell.width() / total;
            ic_px *= s;
            val_px *= s;
            gap *= s;
            iw = g
                .as_ref()
                .map(|gg| text_w(ui, &icons::font_id(ic_px), gg))
                .unwrap_or(0.0);
            vw = text_w(ui, &FontId::proportional(val_px), &val);
            total = iw + if g.is_some() { gap } else { 0.0 } + vw;
        }
        // Two columns: left metric left-aligned, right metric right-aligned
        // (toward the ring) so a shared gap stays between them.
        let mut x = if both {
            if i == 0 {
                cell.left()
            } else {
                cell.right() - total
            }
        } else {
            cell.left() + (cell.width() - total).max(0.0) * 0.5
        };
        if g.is_some() {
            icon_paint(
                ui,
                Pos2::new(x, cell.center().y),
                ic_px,
                key,
                cfg.color(SECTION, "label", "#8b93a1"),
            );
            x += iw + gap;
        }
        label(
            ui,
            Pos2::new(x, cell.center().y),
            Align2::LEFT_CENTER,
            &val,
            val_px,
            cfg.color(SECTION, "value", "#f4f6f8"),
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
) {
    let show_l = left_key != "none";
    let show_r = right_key != "none";
    if !show_l && !show_r {
        return;
    }
    let both = show_l && show_r;
    let cols = if both { 2 } else { 1 };
    let col_gap = if both {
        (rect.height() * 0.45).clamp(16.0, 36.0)
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
        let cell = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * (cell_w + col_gap), rect.top()),
            Vec2::new(cell_w, rect.height()),
        );
        if key == "irating" {
            let pair_h = cell.height() * 0.24 * text_scale;
            let pair_w = irating_pair_width(ui, cfg, f, pair_h, cell.height());
            let pair_rect = if both && i == 1 {
                Rect::from_min_size(
                    Pos2::new(cell.right() - pair_w, cell.top()),
                    Vec2::new(pair_w.min(cell.width()), cell.height()),
                )
            } else {
                cell
            };
            draw_irating_pair(ui, cfg, pair_rect, f, pair_h);
            continue;
        }
        let lines = metric_lines(cfg, f, key);
        let h = cell.height();
        let mut ic_px = h * 0.40 * text_scale;
        let mut lbl_px = h * 0.20 * text_scale;
        let mut val_px = h * 0.24 * text_scale;
        let mut icon_gap = h * 0.18;
        let mut lbl_gap = h * 0.12;
        let g = icons::glyph(key);
        let mut iw = g
            .as_ref()
            .map(|gg| text_w(ui, &icons::font_id(ic_px), gg))
            .unwrap_or(0.0);
        let mut widest = 0.0_f32;
        for (lbl, val) in &lines {
            let lw = if lbl.is_empty() {
                0.0
            } else {
                text_w(ui, &FontId::proportional(lbl_px), lbl) + lbl_gap
            };
            widest = widest.max(lw + text_w(ui, &FontId::proportional(val_px), val));
        }
        let mut total = iw + if g.is_some() { icon_gap } else { 0.0 } + widest;
        if total > cell.width() && total > 0.0 {
            let s = cell.width() / total;
            ic_px *= s;
            lbl_px *= s;
            val_px *= s;
            icon_gap *= s;
            lbl_gap *= s;
            iw = g
                .as_ref()
                .map(|gg| text_w(ui, &icons::font_id(ic_px), gg))
                .unwrap_or(0.0);
            widest = 0.0;
            for (lbl, val) in &lines {
                let lw = if lbl.is_empty() {
                    0.0
                } else {
                    text_w(ui, &FontId::proportional(lbl_px), lbl) + lbl_gap
                };
                widest = widest.max(lw + text_w(ui, &FontId::proportional(val_px), val));
            }
            total = iw + if g.is_some() { icon_gap } else { 0.0 } + widest;
        }
        // Mirror primary: left column left-aligned, right column toward the outer edge.
        let mut x = if both && i == 1 {
            cell.right() - total
        } else {
            cell.left()
        };
        if let Some(ref gg) = g {
            icon_paint(
                ui,
                Pos2::new(x, cell.center().y),
                ic_px,
                key,
                cfg.color(SECTION, "label", "#8b93a1"),
            );
            x += text_w(ui, &icons::font_id(ic_px), gg) + icon_gap;
        }
        let n = lines.len().max(1) as f32;
        for (li, (sub, val)) in lines.iter().enumerate() {
            let y = cell.top() + cell.height() * ((li as f32 + 0.5) / n);
            if li > 0 {
                let y0 = cell.top() + cell.height() * (li as f32 / n);
                ui.painter().line_segment(
                    [Pos2::new(x, y0), Pos2::new(cell.right() - 4.0, y0)],
                    Stroke::new(1.0_f32, cfg.color(SECTION, "cell_border", "#ffffff20")),
                );
            }
            let mut tx = x;
            if !sub.is_empty() {
                label(
                    ui,
                    Pos2::new(tx, y),
                    Align2::LEFT_CENTER,
                    sub,
                    lbl_px,
                    cfg.color(SECTION, "label", "#8b93a1"),
                    false,
                );
                tx += text_w(ui, &FontId::proportional(lbl_px), sub) + lbl_gap;
            }
            label(
                ui,
                Pos2::new(tx, y),
                Align2::LEFT_CENTER,
                val,
                val_px,
                cfg.color(SECTION, "value", "#f4f6f8"),
                true,
            );
        }
    }
}

fn draw_strip(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    pill: Rect,
    keys: &[String; 3],
    f: &TelemetryFrame,
    text_scale: f32,
) {
    let sh = pill.height();
    draw_dark_cell(ui, cfg, SECTION, pill, sh * 0.5);
    let items: Vec<&str> = keys
        .iter()
        .map(|s| s.as_str())
        .filter(|k| *k != "none")
        .collect();
    if items.is_empty() {
        return;
    }
    let pad = sh * 0.55;
    let cx0 = pill.left() + pad;
    let content_w = pill.width() - 2.0 * pad;
    let cell = content_w / items.len() as f32;
    let gap = sh * 0.18;
    for (i, key) in items.into_iter().enumerate() {
        if key == "irating" {
            let val_px = sh * 0.34 * text_scale;
            let pair_w = irating_pair_width(ui, cfg, f, val_px, sh);
            let tx = cx0 + i as f32 * cell + (cell - pair_w) * 0.5;
            draw_irating_pair(
                ui,
                cfg,
                Rect::from_min_size(Pos2::new(tx, pill.top()), Vec2::new(pair_w, sh)),
                f,
                val_px,
            );
            continue;
        }
        let val = metric_str(cfg, f, key);
        let g = icons::glyph(key);
        let ic_px = sh * 0.42 * text_scale;
        let val_px = sh * 0.40 * text_scale;
        let iw = g
            .as_ref()
            .map(|gg| text_w(ui, &icons::font_id(ic_px), gg))
            .unwrap_or(0.0);
        let vw = text_w(ui, &FontId::proportional(val_px), &val);
        let total = iw + if g.is_some() { gap } else { 0.0 } + vw;
        let mut tx = cx0 + i as f32 * cell + (cell - total) * 0.5;
        if g.is_some() {
            icon_paint(
                ui,
                Pos2::new(tx, pill.center().y),
                ic_px,
                key,
                cfg.color(SECTION, "label", "#8b93a1"),
            );
            tx += iw + gap;
        }
        label(
            ui,
            Pos2::new(tx, pill.center().y),
            Align2::LEFT_CENTER,
            &val,
            val_px,
            cfg.color(SECTION, "value", "#f4f6f8"),
            true,
        );
    }
}

fn irating_pair_width(
    ui: &Ui,
    cfg: &OverlayConfig,
    f: &TelemetryFrame,
    val_px: f32,
    sh: f32,
) -> f32 {
    let base = metric_str(cfg, f, "irating");
    let mut w = text_w(ui, &FontId::proportional(val_px), &base);
    w += text_w(
        ui,
        &icons::font_id(sh * 0.42),
        &icons::glyph("irating").unwrap_or_default(),
    ) + sh * 0.18;
    if cfg.bool_key(SECTION, "show_irating_projection", false) {
        if let Some(d) = f.irating_delta {
            if d != 0 {
                w += text_w(
                    ui,
                    &FontId::proportional(val_px * 0.75),
                    &format!("{}", d.abs()),
                ) + val_px * 0.7;
            }
        }
    }
    w
}

fn draw_irating_pair(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    f: &TelemetryFrame,
    val_px: f32,
) {
    let sh = rect.height();
    let ic_px = sh * 0.42;
    let mut x = rect.left();
    if icons::glyph("irating").is_some() {
        icon_paint(
            ui,
            Pos2::new(x, rect.center().y),
            ic_px,
            "irating",
            cfg.color(SECTION, "label", "#8b93a1"),
        );
        x += text_w(
            ui,
            &icons::font_id(ic_px),
            &icons::glyph("irating").unwrap(),
        ) + sh * 0.18;
    }
    let base = metric_str(cfg, f, "irating");
    label(
        ui,
        Pos2::new(x, rect.center().y),
        Align2::LEFT_CENTER,
        &base,
        val_px,
        cfg.color(SECTION, "value", "#f4f6f8"),
        true,
    );
    x += text_w(ui, &FontId::proportional(val_px), &base) + val_px * 0.15;
    if cfg.bool_key(SECTION, "show_irating_projection", false) {
        if let Some(d) = f.irating_delta {
            if d != 0 {
                let up = d > 0;
                let col = if up {
                    cfg.color(SECTION, "irating_delta_up", "#46df7a")
                } else {
                    cfg.color(SECTION, "irating_delta_down", "#ff5050")
                };
                let gname = if up { "irating_up" } else { "irating_down" };
                icon_paint(ui, Pos2::new(x, rect.center().y), val_px * 0.55, gname, col);
                x += val_px * 0.55;
                label(
                    ui,
                    Pos2::new(x, rect.center().y),
                    Align2::LEFT_CENTER,
                    &format!("{}", d.abs()),
                    val_px * 0.75,
                    col,
                    true,
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

fn draw_ring(
    ui: &mut Ui,
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
    let mut border = cfg.color(SECTION, "cell_border", "#ffffff20");
    border = color_with_alpha(border, 150);
    ui.painter().circle_filled(
        Pos2::new(cx, cy),
        mr,
        cfg.color(SECTION, "bg_bottom", "#0f1216"),
    );
    ui.painter().circle_stroke(
        Pos2::new(cx, cy),
        mr,
        Stroke::new((ring_d * 0.022).max(1.5), border),
    );

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
            let on = if *abs_on {
                cfg.color(SECTION, "abs", "#ffd23a")
            } else {
                cfg.color(SECTION, colkey, "#46df7a")
            };
            draw_ring_arc(ui, cfg, cx, cy, r, pen_w, *val, on);
        }
    }

    label(
        ui,
        Pos2::new(cx, cy),
        Align2::CENTER_CENTER,
        &gear_str(f.gear),
        gear_px,
        cfg.color(SECTION, "gear", "#ffffff"),
        true,
    );
}

fn draw_ring_arc(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    cx: f32,
    cy: f32,
    r: f32,
    pen_w: f32,
    mut frac: f32,
    on_color: Color32,
) {
    let n = cfg.f64_key(SECTION, "ring_segments", 16.0).max(1.0) as i32;
    let seg = std::f32::consts::TAU / n as f32;
    let span = seg * 0.72;
    frac = frac.clamp(0.0, 1.0);
    if frac < 0.02 {
        frac = 0.0;
    }
    let lit = frac * n as f32;
    let off = cfg.color(SECTION, "ring_track", "#333a42");

    // Solid segments only (no glow pass — keeps edges crisp).
    for i in 0..n {
        let on = (i as f32) < lit;
        let col = if on { on_color } else { off };
        // Python: ang = 90 + (i+0.5)*seg_deg, sweep -span
        let mid = std::f32::consts::FRAC_PI_2 + (i as f32 + 0.5) * seg;
        let a0 = mid + span * 0.5;
        let a1 = mid - span * 0.5;
        let steps = 8;
        let mut prev = Pos2::new(cx + r * a0.cos(), cy - r * a0.sin());
        for s in 1..=steps {
            let t = s as f32 / steps as f32;
            let a = a0 + (a1 - a0) * t;
            // Convert math angle (0=east, CCW) with y-down: y = cy - r*sin
            let p = Pos2::new(cx + r * a.cos(), cy - r * a.sin());
            ui.painter()
                .line_segment([prev, p], Stroke::new(pen_w, col));
            prev = p;
        }
    }
}

fn draw_pedals(
    ui: &mut Ui,
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
    let mut border = cfg.color(SECTION, "cell_border", "#ffffff20");
    border = color_with_alpha(border, 150);
    ui.painter().circle_filled(
        Pos2::new(cx, cy),
        mr,
        cfg.color(SECTION, "bg_bottom", "#0f1216"),
    );
    ui.painter().circle_stroke(
        Pos2::new(cx, cy),
        mr,
        Stroke::new((ring_d * 0.022).max(1.5), border),
    );

    let bars = selected_inputs(cfg, f, thr, brk, clt);
    if bars.is_empty() {
        label(
            ui,
            Pos2::new(cx, cy),
            Align2::CENTER_CENTER,
            &gear_str(f.gear),
            ring_d * 0.50 * text_scale,
            cfg.color(SECTION, "gear", "#ffffff"),
            true,
        );
        return;
    }

    label(
        ui,
        Pos2::new(cx, cy - ring_d * 0.28),
        Align2::CENTER_CENTER,
        &gear_str(f.gear),
        ring_d * 0.26 * text_scale,
        cfg.color(SECTION, "gear", "#ffffff"),
        true,
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
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(x, top), Vec2::new(bar_w, area_h)),
            egui::CornerRadius::same(rad as u8),
            cfg.color(SECTION, "pedal_track", "#333a42"),
        );
        let fh = area_h * val.clamp(0.0, 1.0);
        if fh > 0.5 {
            let col = if *abs_on {
                cfg.color(SECTION, "abs", "#ffd23a")
            } else {
                cfg.color(SECTION, ckey, "#46df7a")
            };
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(x, bottom - fh), Vec2::new(bar_w, fh)),
                egui::CornerRadius::same(rad as u8),
                col,
            );
        }
    }
}

fn draw_flag(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    f: &TelemetryFrame,
    center_x: f32,
    text_scale: f32,
) {
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
        "furled" => ("WARNING", "flag_furled", "flag_furled_text"),
        "dq" => ("DISQUALIFIED", "flag_dq", "flag_dq_text"),
        "debris" => ("DEBRIS", "flag_debris", "flag_debris_text"),
        "crossed" => ("HALFWAY", "flag_crossed", "flag_crossed_text"),
        _ => return,
    };
    let bg = cfg.color(SECTION, bgk, "#ebeef0");
    let fg = cfg.color(SECTION, fgk, "#141414");
    let r = rect.height() * 0.5;
    ui.painter()
        .rect_filled(rect, egui::CornerRadius::same(r as u8), bg);
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(r as u8),
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 45)),
        StrokeKind::Inside,
    );

    // Diagonal hatch (or checker for finish), clipped to the bar.
    {
        let painter = ui.painter().with_clip_rect(rect);
        let hatch = color_with_alpha(fg, 70);
        if flag == "checkered" {
            let sq = rect.height() * 0.5;
            let mut row = 0;
            let mut y = rect.top();
            while y < rect.bottom() - 0.5 {
                let mut col = row % 2;
                let mut x = rect.left();
                while x < rect.right() - 0.5 {
                    if col % 2 == 0 {
                        painter.rect_filled(
                            Rect::from_min_size(
                                Pos2::new(x, y),
                                Vec2::new(sq.min(rect.right() - x), sq.min(rect.bottom() - y)),
                            ),
                            egui::CornerRadius::ZERO,
                            color_with_alpha(fg, 90),
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
            let pen = Stroke::new((rect.height() * 0.16).max(2.0), hatch);
            let mut x = rect.left() - rect.height();
            while x < rect.right() + rect.height() {
                painter.line_segment(
                    [
                        Pos2::new(x, rect.bottom()),
                        Pos2::new(x + rect.height(), rect.top()),
                    ],
                    pen,
                );
                x += step;
            }
        }
    }

    let context = f.flag_context.as_deref().unwrap_or("").trim();
    if !context.is_empty() {
        let title_px = rect.height() * 0.36 * text_scale;
        let sub_px = rect.height() * 0.24 * text_scale;
        let tw = text_w(ui, &FontId::proportional(title_px), title).max(text_w(
            ui,
            &FontId::proportional(sub_px),
            context,
        ));
        let pad = rect.height() * 0.28;
        let gap = Rect::from_center_size(
            Pos2::new(center_x, rect.center().y),
            Vec2::new(tw + pad * 2.0, rect.height()),
        );
        ui.painter().rect_filled(
            gap,
            egui::CornerRadius::same((gap.height() * 0.5) as u8),
            bg,
        );
        label(
            ui,
            Pos2::new(center_x, rect.center().y - rect.height() * 0.26),
            Align2::CENTER_CENTER,
            title,
            title_px,
            fg,
            true,
        );
        let sub_fg = color_with_alpha(fg, ((fg.a() as f32) * 0.88) as u8);
        label(
            ui,
            Pos2::new(center_x, rect.center().y + rect.height() * 0.28),
            Align2::CENTER_CENTER,
            context,
            sub_px,
            sub_fg,
            false,
        );
    } else {
        let title_px = rect.height() * 0.52 * text_scale;
        let tw = text_w(ui, &FontId::proportional(title_px), title);
        let pad = rect.height() * 0.32;
        let gap = Rect::from_center_size(
            Pos2::new(center_x, rect.center().y),
            Vec2::new(tw + pad * 2.0, rect.height()),
        );
        ui.painter().rect_filled(
            gap,
            egui::CornerRadius::same((gap.height() * 0.5) as u8),
            bg,
        );
        label(
            ui,
            rect.center(),
            Align2::CENTER_CENTER,
            title,
            title_px,
            fg,
            true,
        );
    }
}

fn draw_delta_bar(ui: &mut Ui, cfg: &OverlayConfig, rect: Rect, f: &TelemetryFrame) {
    let r = rect.height() * 0.5;
    ui.painter().rect_filled(
        rect,
        egui::CornerRadius::same(r as u8),
        cfg.color(SECTION, "track", "#262b34"),
    );
    let rng = cfg.f64_key(SECTION, "delta_bar_range", 1.0).max(0.001) as f32;
    let delta = f.delta.unwrap_or(0.0) as f32;
    let t = (delta / rng).clamp(-1.0, 1.0);
    let cx = rect.center().x;
    if t.abs() > 0.001 {
        let fill_w = rect.width() * 0.5 * t.abs();
        // Python: faster (neg) fills to the right of center; slower to the left
        let fill = if t < 0.0 {
            Rect::from_min_max(
                Pos2::new(cx, rect.top()),
                Pos2::new(cx + fill_w, rect.bottom()),
            )
        } else {
            Rect::from_min_max(
                Pos2::new(cx - fill_w, rect.top()),
                Pos2::new(cx, rect.bottom()),
            )
        };
        let col = if t < 0.0 {
            cfg.color(SECTION, "faster", "#46df7a")
        } else {
            cfg.color(SECTION, "slower", "#e23b3b")
        };
        ui.painter()
            .rect_filled(fill, egui::CornerRadius::same(r as u8), col);
    }
    ui.painter().line_segment(
        [
            Pos2::new(cx, rect.top() + 1.0),
            Pos2::new(cx, rect.bottom() - 1.0),
        ],
        Stroke::new(1.0_f32, cfg.color(SECTION, "border", "#ffffff28")),
    );
}
