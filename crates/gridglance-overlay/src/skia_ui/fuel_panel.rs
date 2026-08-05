//! Skia painter for the fuel calculator panel (Phase C).
//!
//! Ports `widgets/fuel_calc.rs` (egui) onto the Skia `Canvas`, both the Data
//! (proportional block) and Elegant (fixed sequential) layouts.

use super::canvas::{Canvas, TextAlign};
use super::chrome::{
    draw_dark_cell, draw_section_header, is_elegant, panel_card, section_color, text_at,
};
use super::types::Rect;
use crate::config::OverlayConfig;
use crate::telemetry::{FuelCalcState, FuelScenario, TelemetryFrame};

const SECTION: &str = "fuel_calc";
const STAT_COLS: &[&str] = &["usage", "laps", "pits", "refuel"];
const STAT_ROWS: &[&str] = &["avg", "max", "min"];

fn full_bounds(c: &Canvas) -> Rect {
    Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32)
}

/// Paint the fuel calculator. Always a static layout (no host-owned anim
/// state is threaded through here), so this returns `false`.
pub fn paint_fuel(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    let _ = edit_mode;
    c.clear_transparent();
    if is_elegant(cfg, SECTION) {
        paint_elegant(c, cfg, frame);
    } else {
        paint_data(c, cfg, frame);
    }
    false
}

fn paint_data(c: &mut Canvas, cfg: &OverlayConfig, frame: &TelemetryFrame) {
    let bounds = full_bounds(c);
    let radius = panel_card(c, cfg, SECTION, bounds);
    let w = bounds.width();
    let h = bounds.height();
    let m = w * 0.04;
    let inner = w - 2.0 * m;
    let d = &frame.fuel;

    // Accent top bar.
    let bar_h = (h * 0.018).max(3.0);
    c.fill_rect(
        Rect::from_xywh(bounds.left() + m, bounds.top() + h * 0.012, inner, bar_h),
        section_color(cfg, SECTION, "accent", "#e23b3b"),
        2.0,
    );

    let show_pill = cfg.bool_key(SECTION, "show_pill", true);
    let show_add = cfg.bool_key(SECTION, "show_add", true);
    let show_gauge = cfg.bool_key(SECTION, "show_gauge", true);
    let top_on = show_pill || show_add || show_gauge;
    let show_time = cfg.bool_key(SECTION, "show_time", true);
    let show_laps = cfg.bool_key(SECTION, "show_laps", true);

    let mut blocks: Vec<(&str, f32)> = Vec::new();
    if cfg.bool_key(SECTION, "show_title", true) {
        blocks.push(("title", 0.55));
    }
    if top_on {
        blocks.push(("top", 1.15));
    }
    if cfg.bool_key(SECTION, "show_stats", true) {
        blocks.push(("stats", 2.6));
    }
    if cfg.bool_key(SECTION, "show_strip", true) {
        blocks.push(("strip", 0.6));
    }
    if show_time {
        blocks.push(("time", 0.95));
    }
    if show_laps {
        blocks.push(("laps", 0.95));
    }
    if blocks.is_empty() {
        return;
    }

    let content_top = bounds.top() + h * 0.012 + bar_h + h * 0.015;
    let content_bottom = bounds.top() + h * 0.985;
    let gap = (h * 0.02).max(4.0);
    let sumw: f32 = blocks.iter().map(|(_, wt)| wt).sum();
    let avail = (content_bottom - content_top) - gap * (blocks.len() as f32 - 1.0);
    let mut heights: Vec<f32> = blocks.iter().map(|(_, wt)| avail * wt / sumw).collect();

    // Shrink-wrap stats so the PIT strip sits just under the table (Python parity).
    if let Some(i) = blocks.iter().position(|(k, _)| *k == "stats") {
        let needed = stats_content_height(cfg, heights[i]);
        if needed < heights[i] {
            heights[i] = needed;
        }
        heights[i] = heights[i].max(needed.min(heights[i]));
    }

    let mut cy = content_top;
    for (i, (key, _)) in blocks.iter().enumerate() {
        let bh = heights[i];
        let x = bounds.left() + m;
        match *key {
            "title" => {
                let title = cfg.str_key(SECTION, "title", "FUEL CALCULATOR");
                let band = Rect::from_xywh(x, cy, inner, bh);
                draw_section_header(c, cfg, SECTION, band, &title, radius);
            }
            "top" => draw_top(c, cfg, d, x, cy, inner, bh, show_pill, show_add, show_gauge),
            "stats" => draw_stats(c, cfg, d, x, cy, inner, bh),
            "strip" => draw_strip(c, cfg, d, x, cy, inner, bh),
            "time" => draw_box(
                c,
                cfg,
                "TIME UNTIL EMPTY",
                &fmt_hms(d.time_empty),
                &fmt_signed_hms(d.time_margin),
                d.time_margin,
                x,
                cy,
                inner,
                bh,
            ),
            "laps" => draw_box(
                c,
                cfg,
                "LAPS UNTIL EMPTY",
                &fmt1(d.laps_empty),
                &signed1(d.laps_margin),
                d.laps_margin,
                x,
                cy,
                inner,
                bh,
            ),
            _ => {}
        }
        let extra = if *key == "stats" { gap.max(6.0) } else { 0.0 };
        cy += bh + gap + extra;
    }
}

/// Elegant: fixed sequential layout (no proportional crush).
fn paint_elegant(c: &mut Canvas, cfg: &OverlayConfig, frame: &TelemetryFrame) {
    let bounds = full_bounds(c);
    panel_card(c, cfg, SECTION, bounds);
    let pad = 10.0_f32;
    let gap = 8.0_f32;
    let x = bounds.left() + pad;
    let inner = (bounds.width() - 2.0 * pad).max(40.0);
    let d = &frame.fuel;
    let muted = section_color(cfg, SECTION, "muted", "#8b93a1").with_alpha(200);
    let mut y = bounds.top() + pad;

    if cfg.bool_key(SECTION, "show_title", true) {
        text_at(
            c,
            x,
            y + 7.0,
            &cfg.str_key(SECTION, "title", "FUEL CALCULATOR"),
            11.0,
            muted,
            false,
            TextAlign::Left,
        );
        y += 18.0;
    }

    let show_pill = cfg.bool_key(SECTION, "show_pill", true);
    let show_add = cfg.bool_key(SECTION, "show_add", true);
    let show_gauge = cfg.bool_key(SECTION, "show_gauge", true);
    if show_pill || show_add || show_gauge {
        let top_h = 56.0_f32;
        draw_top_elegant(
            c, cfg, d, x, y, inner, top_h, show_pill, show_add, show_gauge,
        );
        y += top_h + gap;
    }

    let show_time = cfg.bool_key(SECTION, "show_time", true);
    let show_laps = cfg.bool_key(SECTION, "show_laps", true);
    let box_h = 40.0_f32;
    if show_time {
        draw_box_elegant(
            c,
            cfg,
            "TIME UNTIL EMPTY",
            &fmt_hms(d.time_empty),
            &fmt_signed_hms(d.time_margin),
            d.time_margin,
            x,
            y,
            inner,
            box_h,
        );
        y += box_h + gap;
    }
    if show_laps {
        draw_box_elegant(
            c,
            cfg,
            "LAPS UNTIL EMPTY",
            &fmt1(d.laps_empty),
            &signed1(d.laps_margin),
            d.laps_margin,
            x,
            y,
            inner,
            box_h,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_top_elegant(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    show_pill: bool,
    show_add: bool,
    show_gauge: bool,
) {
    let left_on = show_pill || show_add;
    if show_gauge && !left_on {
        draw_gauge_elegant(c, cfg, d, x, y, w, h);
        return;
    }
    let left_w = if show_gauge { w * 0.42 } else { w };
    let g = 6.0;
    if show_pill && show_add {
        let half = (left_w - g) * 0.5;
        draw_pill(c, cfg, d, x, y, half, h);
        draw_add(c, cfg, d, x + half + g, y, half, h);
    } else if show_pill {
        draw_pill(c, cfg, d, x, y, left_w, h);
    } else if show_add {
        draw_add(c, cfg, d, x, y, left_w, h);
    }
    if show_gauge {
        let gx = x + left_w + g;
        draw_gauge_elegant(c, cfg, d, gx, y, (w - left_w - g).max(40.0), h);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_top(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    show_pill: bool,
    show_add: bool,
    show_gauge: bool,
) {
    let left_on = show_pill || show_add;
    if show_gauge && !left_on {
        draw_gauge(c, cfg, d, x, y, w, h);
        return;
    }
    let left_w = if show_gauge { w * 0.46 } else { w };
    if show_pill && show_add {
        let pill_w = left_w * 0.46;
        draw_pill(c, cfg, d, x, y, pill_w, h);
        let ax = x + pill_w + w * 0.015;
        draw_add(c, cfg, d, ax, y, x + left_w - ax, h);
    } else if show_pill {
        draw_pill(c, cfg, d, x, y, left_w, h);
    } else if show_add {
        draw_add(c, cfg, d, x, y, left_w, h);
    }
    if show_gauge {
        let gx = x + left_w + w * 0.04;
        draw_gauge(c, cfg, d, gx, y, w - left_w - w * 0.04, h);
    }
}

fn draw_pill(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let pill = Rect::from_xywh(x, y, w, h);
    let bg = if d.window_open {
        section_color(cfg, SECTION, "pill_open", "#46df7a")
    } else {
        section_color(cfg, SECTION, "pill_closed", "#6e747d")
    };
    c.fill_rect(pill, bg, 8.0);
    let fg = section_color(cfg, SECTION, "pill_text", "#06210f");
    let title_sz = (h * 0.28).clamp(9.0, 15.0);
    let sub_sz = (h * 0.18).clamp(8.0, 11.0);
    text_at(
        c,
        pill.center().0,
        pill.top() + h * 0.34,
        if d.window_open { "OPEN" } else { "CLOSED" },
        title_sz,
        fg,
        true,
        TextAlign::Center,
    );
    let sub = match d.window {
        Some((a, b)) => format!("L{a}-{b}"),
        None => "—".into(),
    };
    text_at(
        c,
        pill.center().0,
        pill.top() + h * 0.70,
        &sub,
        sub_sz,
        fg,
        true,
        TextAlign::Center,
    );
}

fn draw_add(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let rect = Rect::from_xywh(x, y, w, h);
    if is_elegant(cfg, SECTION) {
        c.fill_rect(
            rect,
            section_color(cfg, SECTION, "cell_dark", "#0b0e12").with_alpha(70),
            10.0,
        );
    } else {
        draw_dark_cell(c, cfg, SECTION, rect, 8.0);
    }
    let txt = match d.add {
        Some(add) => {
            let v = cfg.conv_fuel(add);
            format!("+{v:.1}{}", cfg.fuel_unit())
        }
        None => "—".into(),
    };
    text_at(
        c,
        rect.center().0,
        rect.center().1,
        &txt,
        (h * 0.40).clamp(11.0, 20.0),
        section_color(cfg, SECTION, "add_text", "#f4f6f8"),
        true,
        TextAlign::Center,
    );
}

/// Compact gauge for Elegant top row — bar + single fuel readout (no stacked hints).
fn draw_gauge_elegant(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let show_low_alert = cfg.bool_key(SECTION, "show_low_fuel_alert", true);
    let alert = d.alert && show_low_alert;
    let bar = Rect::from_xywh(x, y + 4.0, w, (h * 0.42).clamp(14.0, 22.0));
    c.fill_rect(bar, section_color(cfg, SECTION, "gauge_bg", "#0b0e12"), 4.0);
    let fill_col = if alert {
        section_color(cfg, SECTION, "box_warn", "#e23b3b")
    } else {
        section_color(cfg, SECTION, "gauge_fill", "#f4f6f8")
    };
    if let (Some(level), Some(cap)) = (d.level, d.cap) {
        if cap > 0.0 {
            let frac = (level / cap).clamp(0.0, 1.0);
            c.fill_rect(
                Rect::from_xywh(
                    bar.left() + 1.0,
                    bar.top() + 1.0,
                    (bar.width() - 2.0) * frac,
                    bar.height() - 2.0,
                ),
                fill_col,
                3.0,
            );
        }
    }
    let muted = section_color(cfg, SECTION, "muted", "#8b93a1").with_alpha(210);
    let cur = d
        .level
        .map(|l| format!("{:.1} {}", cfg.conv_fuel(l), cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    let cap = d
        .cap
        .map(|cap_v| format!("{:.0}{}", cfg.conv_fuel(cap_v), cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    let line_y = bar.bottom() + (h - (bar.bottom() - y)) * 0.45;
    text_at(
        c,
        x,
        line_y,
        &cur,
        11.0,
        if alert {
            section_color(cfg, SECTION, "box_warn", "#e23b3b")
        } else {
            section_color(cfg, SECTION, "text", "#f4f6f8")
        },
        true,
        TextAlign::Left,
    );
    text_at(c, x + w, line_y, &cap, 11.0, muted, false, TextAlign::Right);
}

fn draw_gauge(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let show_tank_pct = cfg.bool_key(SECTION, "show_tank_pct", false);
    let show_live_burn = cfg.bool_key(SECTION, "show_live_burn", false);
    let show_low_alert = cfg.bool_key(SECTION, "show_low_fuel_alert", true);
    let alert = d.alert && show_low_alert;

    let bar = Rect::from_xywh(x, y + h * 0.10, w, h * 0.40);
    c.fill_rect(bar, section_color(cfg, SECTION, "gauge_bg", "#0b0e12"), 4.0);
    c.stroke_rect(
        bar,
        section_color(cfg, SECTION, "cell_border", "#ffffff20"),
        4.0,
        1.0,
    );
    let fill_col = if alert {
        section_color(cfg, SECTION, "box_warn", "#e23b3b")
    } else {
        section_color(cfg, SECTION, "gauge_fill", "#f4f6f8")
    };
    if let (Some(level), Some(cap)) = (d.level, d.cap) {
        if cap > 0.0 {
            let frac = (level / cap).clamp(0.0, 1.0);
            let fill = Rect::from_xywh(
                bar.left() + 1.0,
                bar.top() + 1.0,
                (bar.width() - 2.0) * frac,
                bar.height() - 2.0,
            );
            c.fill_rect(fill, fill_col, 3.0);
        }
    }
    let muted = section_color(cfg, SECTION, "muted", "#8b93a1");
    let label_y = bar.bottom() + h * 0.14;
    let edge_sz = (h * 0.14).clamp(9.0, 13.0);
    text_at(c, x, label_y, "E", edge_sz, muted, true, TextAlign::Left);
    let cur = d.level.map(|l| cfg.conv_fuel(l));
    let mut cur_txt = cur
        .map(|c| format!("{c:.1} {}", cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    if show_tank_pct {
        if let Some(pct) = d.fuel_pct {
            cur_txt = format!("{cur_txt} ({pct:.0}%)");
        }
    }
    text_at(
        c,
        x + w * 0.5,
        label_y,
        &cur_txt,
        edge_sz,
        muted,
        true,
        TextAlign::Center,
    );
    let cap_txt = d
        .cap
        .map(|cap_v| format!("{:.0}{}", cfg.conv_fuel(cap_v), cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    text_at(
        c,
        x + w,
        label_y,
        &cap_txt,
        edge_sz,
        muted,
        true,
        TextAlign::Right,
    );
    let hint_y = (label_y + edge_sz * 0.9 + 4.0).min(y + h - 8.0);
    if alert {
        text_at(
            c,
            x + w * 0.5,
            hint_y,
            "LOW FUEL",
            (h * 0.12).clamp(9.0, 12.0),
            section_color(cfg, SECTION, "box_warn", "#e23b3b"),
            true,
            TextAlign::Center,
        );
    } else if let Some(hint) = &d.pit_hint {
        text_at(
            c,
            x + w * 0.5,
            hint_y,
            hint,
            (h * 0.11).clamp(8.0, 11.0),
            muted,
            false,
            TextAlign::Center,
        );
    } else if show_live_burn {
        if let Some(burn) = d.live_burn {
            let b = cfg.conv_fuel(burn);
            text_at(
                c,
                x + w * 0.5,
                hint_y,
                &format!("{b:.2}{}/lap", cfg.fuel_unit()),
                (h * 0.11).clamp(8.0, 11.0),
                muted,
                false,
                TextAlign::Center,
            );
        }
    }
}

fn stats_row_metrics(cfg: &OverlayConfig, h: f32) -> (f32, f32) {
    let n = STAT_ROWS.len() as f32;
    let fixed_rh = cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    if fixed_rh > 0.0 {
        let head = (fixed_rh * 1.1).min(h * 0.3);
        let row = ((h - head) / n).min(fixed_rh).max(1.0);
        return (head, row);
    }
    // Always fit within allocated height — never overflow into the PIT strip.
    let head_h = (h * 0.22).min(h * 0.28).max(1.0).min(h * 0.4);
    let row_h = ((h - head_h) / n).max(1.0);
    (head_h, row_h)
}

fn stats_content_height(cfg: &OverlayConfig, allocated_h: f32) -> f32 {
    let (head_h, row_h) = stats_row_metrics(cfg, allocated_h);
    let n = STAT_ROWS.len() as f32;
    let min_row = 18.0_f32;
    let min_head = 20.0_f32;
    let comfortable = min_head + n * min_row;
    if comfortable <= allocated_h {
        comfortable
    } else {
        head_h + n * row_h
    }
}

fn scenario_for<'a>(d: &'a FuelCalcState, key: &str) -> &'a FuelScenario {
    match key {
        "max" => &d.max,
        "min" => &d.min,
        _ => &d.avg,
    }
}

fn draw_stats(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let label_w = w * 0.13;
    let col_w = (w - label_w) / STAT_COLS.len() as f32;
    let (head_h, row_h) = stats_row_metrics(cfg, h);
    let hscale = cfg.f64_key(SECTION, "stats_header_font_scale", 1.0) as f32;
    let rscale = cfg.f64_key(SECTION, "stats_row_font_scale", 1.0) as f32;
    let muted = section_color(cfg, SECTION, "muted", "#8b93a1");
    let text = section_color(cfg, SECTION, "text", "#f4f6f8");
    let header = section_color(cfg, SECTION, "header", "#8b93a1");

    let band = Rect::from_xywh(x, y, w, head_h);
    c.fill_rect(
        band,
        section_color(cfg, SECTION, "header_bg", "#0b0e12bb"),
        0.0,
    );
    let headers = ["USAGE", "LAPS", "PITS", "REFUEL"];
    for (i, hdr) in headers.iter().enumerate() {
        let cx = x + label_w + i as f32 * col_w;
        text_at(
            c,
            cx + col_w * 0.5,
            y + head_h * 0.5,
            hdr,
            (head_h * 0.48 * hscale).clamp(8.0, 14.0),
            header,
            false,
            TextAlign::Center,
        );
    }

    let labels = ["AVG", "HIGH", "LOW"];
    for (r, rk) in STAT_ROWS.iter().enumerate() {
        let ry = y + head_h + r as f32 * row_h;
        if ry + row_h > y + h + 0.5 {
            break;
        }
        if r % 2 == 1 {
            c.fill_rect(
                Rect::from_xywh(x, ry, w, row_h),
                section_color(cfg, SECTION, "row_alt", "#ffffff08"),
                4.0,
            );
        }
        text_at(
            c,
            x + 4.0,
            ry + row_h * 0.5,
            labels[r],
            (row_h * 0.40 * rscale).clamp(8.0, 14.0),
            muted,
            true,
            TextAlign::Left,
        );
        let data = scenario_for(d, rk);
        for (i, ck) in STAT_COLS.iter().enumerate() {
            let cx = x + label_w + i as f32 * col_w;
            let cell = fmt_stat_cell(cfg, ck, data);
            text_at(
                c,
                cx + col_w * 0.5,
                ry + row_h * 0.5,
                &cell,
                (row_h * 0.44 * rscale).clamp(8.0, 15.0),
                text,
                false,
                TextAlign::Center,
            );
        }
        if cfg.bool_key(SECTION, "row_dividers", true) && r + 1 < STAT_ROWS.len() {
            let line = section_color(cfg, SECTION, "border", "#ffffff28");
            c.line(x, ry + row_h, x + w, ry + row_h, line, 1.0);
        }
    }
}

fn draw_strip(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let muted = section_color(cfg, SECTION, "muted", "#8b93a1");
    let lbl_w = w * 0.10;
    text_at(
        c,
        x,
        y + h * 0.5,
        "PIT",
        (h * 0.45).clamp(9.0, 14.0),
        muted,
        true,
        TextAlign::Left,
    );
    let total = d.strip.total;
    if total <= 0 {
        return;
    }
    let sx = x + lbl_w;
    let sw = w - lbl_w;
    let gap = sw / total as f32 * 0.22;
    let seg_w = sw / total as f32 - gap;
    let bar_h = (h * 0.55).min(h - 2.0);
    let by = y + (h - bar_h) * 0.5;
    let win = d.strip.window;
    let now = d.strip.now;
    for i in 0..total {
        let cx = sx + i as f32 * (seg_w + gap);
        let color = if now == Some(i) {
            section_color(cfg, SECTION, "strip_now", "#ffd23a")
        } else if win.map(|(a, b)| i >= a && i <= b).unwrap_or(false) {
            section_color(cfg, SECTION, "strip_window", "#46df7a")
        } else {
            section_color(cfg, SECTION, "strip_none", "#333a42")
        };
        c.fill_rect(Rect::from_xywh(cx, by, seg_w.max(2.0), bar_h), color, 2.0);
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_box_elegant(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    title: &str,
    value: &str,
    margin_txt: &str,
    margin_val: Option<f32>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let rect = Rect::from_xywh(x, y, w, h);
    c.fill_rect(
        rect,
        section_color(cfg, SECTION, "cell_dark", "#0b0e12").with_alpha(55),
        8.0,
    );
    let pad = 10.0;
    let warn = margin_val.map(|v| v < 0.0).unwrap_or(false);
    let mcol = if warn {
        section_color(cfg, SECTION, "box_warn", "#e23b3b")
    } else {
        section_color(cfg, SECTION, "muted", "#8b93a1").with_alpha(200)
    };
    text_at(
        c,
        x + pad,
        rect.center().1,
        title,
        11.0,
        section_color(cfg, SECTION, "muted", "#8b93a1").with_alpha(210),
        false,
        TextAlign::Left,
    );
    text_at(
        c,
        x + w * 0.62,
        rect.center().1,
        value,
        15.0,
        section_color(cfg, SECTION, "box_value", "#f4f6f8"),
        true,
        TextAlign::Right,
    );
    text_at(
        c,
        x + w - pad,
        rect.center().1,
        margin_txt,
        12.0,
        mcol,
        false,
        TextAlign::Right,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_box(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    title: &str,
    value: &str,
    margin_txt: &str,
    margin_val: Option<f32>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let rect = Rect::from_xywh(x, y, w, h);
    draw_dark_cell(c, cfg, SECTION, rect, 6.0);
    let pad = w * 0.02;
    let gap = w * 0.02;
    let label_w = w * 0.34;
    let margin_w = w * 0.22;
    let value_w = w - label_w - margin_w - pad * 2.0 - gap * 2.0;
    let text = section_color(cfg, SECTION, "text", "#f4f6f8");
    let warn = margin_val.map(|v| v < 0.0).unwrap_or(false);
    let mcol = if warn {
        section_color(cfg, SECTION, "box_warn", "#e23b3b")
    } else {
        section_color(cfg, SECTION, "muted", "#8b93a1")
    };
    text_at(
        c,
        x + pad,
        rect.center().1,
        title,
        (h * 0.30).min(18.0),
        text,
        true,
        TextAlign::Left,
    );
    text_at(
        c,
        x + label_w + gap + value_w,
        rect.center().1,
        value,
        (h * 0.52).min(28.0),
        section_color(cfg, SECTION, "box_value", "#f4f6f8"),
        true,
        TextAlign::Right,
    );
    text_at(
        c,
        x + w - pad,
        rect.center().1,
        margin_txt,
        (h * 0.32).min(20.0),
        mcol,
        false,
        TextAlign::Right,
    );
}

fn fmt1(x: Option<f32>) -> String {
    x.map(|v| format!("{v:.1}")).unwrap_or_else(|| "–".into())
}

fn signed1(x: Option<f32>) -> String {
    match x {
        Some(v) if v >= 0.0 => format!("+{:.1}", v),
        Some(v) => format!("-{:.1}", v.abs()),
        None => "–".into(),
    }
}

fn fmt_hms(sec: Option<f32>) -> String {
    let Some(sec) = sec else {
        return "--:--:--".into();
    };
    let sec = sec.max(0.0) as i32;
    let h = sec / 3600;
    let m = (sec % 3600) / 60;
    let s = sec % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn fmt_signed_hms(sec: Option<f32>) -> String {
    let Some(sec) = sec else {
        return "--:--:--".into();
    };
    let sign = if sec < 0.0 { "-" } else { "+" };
    format!("{sign}{}", fmt_hms(Some(sec.abs())))
}

fn fmt_stat_cell(cfg: &OverlayConfig, col: &str, data: &FuelScenario) -> String {
    match col {
        "usage" => data
            .usage
            .map(|u| format!("{:.1}", cfg.conv_fuel(u)))
            .unwrap_or_else(|| "–".into()),
        "refuel" => data
            .refuel
            .map(|u| format!("{:.1}", cfg.conv_fuel(u)))
            .unwrap_or_else(|| "–".into()),
        "laps" => fmt1(data.laps),
        "pits" => fmt1(data.pits),
        _ => "–".into(),
    }
}
