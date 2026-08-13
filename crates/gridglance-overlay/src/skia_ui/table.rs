//! Skia standings / relative table painter — visual parity with egui `widgets/table.rs`.

use super::anim::{tick_table_anim, TableAnim};
use super::canvas::{Canvas, TextAlign};
use super::chrome::{draw_card, draw_edge_band, draw_row_tint, parse_rgba, section_color};
use super::tokens::DesignTokens;
use super::types::{FontSpec, Rect, Rgba};
use crate::config::OverlayConfig;
use crate::telemetry::{format_car_number, TableRow, TableSlots};

const ROW_SNAP: f32 = 6.0;
const REL_SNAP: f32 = 8.0;
const DENSE_ROW_COUNT: usize = 14;

pub fn paint_table(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
    slots: &TableSlots,
    anim: &mut TableAnim,
    mono_secs: f64,
    _frame: &crate::telemetry::TelemetryFrame,
) -> bool {
    let bounds = Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32);
    let tokens = DesignTokens::for_section(cfg, section, bounds.height());
    c.clear_transparent();
    draw_card(c, &tokens, bounds);

    let show_footer = cfg.bool_key(section, "show_footer", true);
    let scale = cfg.text_scale(section);
    let rh = tokens.row_height;
    let pad = tokens.pad;
    let header_h = tokens.header_h;
    let footer_h = if show_footer { tokens.footer_h } else { 0.0 };

    let inner_w = bounds.width() - 2.0 * pad;
    let left = bounds.left() + pad;

    // Header band hugs its content height; only horizontal pad insets the slots.
    let hdr_band = Rect::from_xywh(bounds.x, bounds.y, bounds.w, header_h);
    let hdr_content = Rect::from_xywh(left, bounds.y, inner_w, header_h);
    draw_edge_band(
        c,
        cfg,
        section,
        &tokens,
        hdr_band,
        hdr_content,
        true,
        &slots.header_left,
        &slots.header_center,
        &slots.header_right,
    );

    let body_top = bounds.y + header_h;
    let body_bottom = if show_footer {
        bounds.bottom() - footer_h
    } else {
        bounds.bottom() - pad * 0.5
    };
    let inner = Rect::from_ltrb(left, body_top, left + inner_w, body_bottom);

    let order: Vec<String> = rows
        .iter()
        .filter(|r| !r.empty)
        .map(|r| r.key.clone())
        .collect();
    let snap = if section == "relative" {
        REL_SNAP
    } else {
        ROW_SNAP
    };
    let animating = tick_table_anim(anim, &order, mono_secs, tokens.row_slide_s, snap);

    let columns = column_order(cfg, section);
    let signed_gaps = section == "relative";
    let fs = (rh * tokens.font_scale * scale).clamp(9.0, 22.0);
    let gutter = rh * 0.12;
    let alt = cfg.bool_key(section, "alt_row_shading", true);
    let text = tokens.text;
    let muted = tokens.muted;
    let dividers = cfg.bool_key(section, "row_dividers", true);
    let dense = rows.len() >= DENSE_ROW_COUNT;

    let visible_slots = (inner.height() / rh).floor().max(1.0) as usize;
    let scroll = standings_scroll(cfg, section, rows, visible_slots);
    // Relative anim targets are already packed (non-empty only). Center that
    // live block in the body so spare panel height splits above and below.
    let live_count = rows.iter().filter(|r| !r.empty).count();
    let body_y0 = if section == "relative" {
        let content_h = live_count as f32 * rh;
        if content_h > 0.0 && content_h < inner.height() - 0.5 {
            inner.top() + (inner.height() - content_h) * 0.5
        } else {
            inner.top()
        }
    } else {
        inner.top()
    };

    let mut draw_order: Vec<usize> = (0..rows.len()).collect();
    draw_order.sort_by(|&a, &b| {
        let ia = slot_idx(anim, &rows[a], a);
        let ib = slot_idx(anim, &rows[b], b);
        ia.partial_cmp(&ib).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut prev_draw_idx: Option<f32> = None;
    c.clip_rect(inner, |c| {
        for &i in &draw_order {
            let row = &rows[i];
            // Empty relative pads only reserved slot indices in the data model;
            // they must not consume vertical space (keeps the live block centered).
            if section == "relative" && row.empty {
                continue;
            }
            let slot = if row.empty {
                i as f32
            } else {
                slot_idx(anim, row, i)
            } - scroll;
            let ry = body_y0 + slot * rh;
            let row_rect = Rect::from_xywh(inner.left(), ry, inner.width(), rh);
            if row_rect.bottom() < inner.top() - rh || row_rect.top() > inner.bottom() + rh {
                continue;
            }

            let sliding = !row.empty && dense && (slot + scroll - i as f32).abs() > 0.02;
            if dividers {
                if let Some(prev_idx) = prev_draw_idx {
                    if (slot - prev_idx).abs() <= 1.05 && !sliding {
                        let line = tokens.border;
                        let a = ((line.a as f32) * 0.20).max(10.0) as u8;
                        c.line(
                            row_rect.left(),
                            ry,
                            row_rect.right(),
                            ry,
                            line.with_alpha(a),
                            0.35,
                        );
                    }
                }
                prev_draw_idx = Some(slot);
            }

            let chrome_i = slot.round().max(0.0) as usize;
            paint_row_chrome(c, cfg, section, row, row_rect, chrome_i, alt);

            if row.empty {
                continue;
            }

            paint_cols(
                c,
                cfg,
                section,
                row,
                row_rect,
                &columns,
                gutter,
                rh,
                fs,
                tokens.gap_font_scale,
                signed_gaps,
                text,
                muted,
            );

            let fourth_is_next = rows
                .get(i + 1)
                .map(|next| !next.empty && next.position == 4)
                .unwrap_or(false);
            if section == "standings"
                && cfg.bool_key("standings", "pin_podium", false)
                && row.position == 3
                && !fourth_is_next
            {
                let y = row_rect.bottom() - 1.0;
                c.line(
                    row_rect.left(),
                    y,
                    row_rect.right(),
                    y,
                    section_color(cfg, "standings", "podium_separator", "#22c55e"),
                    2.0,
                );
            }
        }
    });

    if show_footer {
        let band_top = bounds.bottom() - footer_h;
        let ftr_band = Rect::from_ltrb(bounds.left(), band_top, bounds.right(), bounds.bottom());
        let ftr_content = Rect::from_xywh(left, band_top, inner_w, footer_h);
        draw_edge_band(
            c,
            cfg,
            section,
            &tokens,
            ftr_band,
            ftr_content,
            false,
            &slots.footer_left,
            &slots.footer_center,
            &slots.footer_right,
        );
    }

    animating
}

fn slot_idx(anim: &TableAnim, row: &TableRow, i: usize) -> f32 {
    anim.slots.get(&row.key).map(|s| s.idx).unwrap_or(i as f32)
}

fn standings_scroll(
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
    visible_slots: usize,
) -> f32 {
    if section != "standings" || !cfg.bool_key(section, "center_on_player", true) {
        return 0.0;
    }
    // Pin-podium already puts P1–P3 first; any scroll would clip them off-screen.
    if cfg.bool_key("standings", "pin_podium", false) {
        return 0.0;
    }
    if rows.len() <= visible_slots {
        return 0.0;
    }
    let Some(fi) = rows.iter().position(|r| r.is_player && !r.empty) else {
        return 0.0;
    };
    let ahead_cfg = cfg.f64_key(section, "rows_ahead", 4.0).max(0.0) as usize;
    let behind_cfg = cfg.f64_key(section, "rows_behind", 5.0).max(0.0) as usize;
    let behind_in_list = rows.len().saturating_sub(fi + 1);
    let ideal_ahead = ahead_cfg.min(fi);
    let min_behind = if behind_in_list > 0 && behind_cfg > 0 {
        behind_cfg
            .min(behind_in_list)
            .min((visible_slots / 2).max(1))
    } else {
        0
    };
    let max_ahead = visible_slots.saturating_sub(1 + min_behind);
    let want_ahead = ideal_ahead.min(max_ahead);
    fi.saturating_sub(want_ahead) as f32
}

fn column_order(cfg: &OverlayConfig, section: &str) -> Vec<String> {
    if let Some(arr) = cfg
        .section(section)
        .get("column_order")
        .and_then(|v| v.as_array())
    {
        let cols: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    vec![
        "badge".into(),
        "position".into(),
        "name".into(),
        "license".into(),
        "irating".into(),
        "gap".into(),
    ]
}

fn width_mult(cfg: &OverlayConfig, section: &str, key: &str, default: f32) -> f32 {
    cfg.section(section)
        .get("widths")
        .and_then(|w| w.get(key))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn default_width(col: &str) -> f32 {
    match col {
        "badge" => 0.95,
        "position" => 1.25,
        "car_number" => 1.60,
        "gap" | "gap_ahead" | "gap_leader" => 1.70,
        "irating" => 1.20,
        "license" => 1.35,
        "pit" => 2.10,
        "last_lap" | "best_lap" => 2.90,
        "country" => 1.3,
        "class_pos" | "status" | "car_flag" | "laps" => 1.35,
        "closing" => 1.80,
        "team" | "nickname" => 2.20,
        "gutter" => 0.12,
        _ => 1.2,
    }
}

fn paint_row_chrome(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    rect: Rect,
    i: usize,
    alt: bool,
) {
    let tint_key = if row.empty {
        None
    } else if row.is_player {
        Some("player_row")
    } else if row.lapping {
        // iRacing Relative: red = car lapping you; blue = traffic you're lapping.
        Some(if row.lap_ahead { "threat" } else { "lapped" })
    } else if row.in_pit || row.on_pit {
        Some("pit_row")
    } else if row.inactive {
        // Not in car / garage: greyed text only — no row wash.
        None
    } else if row.is_speaking {
        Some("speaking_row")
    } else if section == "relative" {
        match row.strat_tag.as_deref() {
            Some("undercut") => Some("undercut_row"),
            Some("cover") => Some("cover_row"),
            _ => None,
        }
    } else {
        None
    };

    if let Some(key) = tint_key {
        let fallback = match key {
            "player_row" => "#ff941670",
            "threat" => "#ff505060",
            "lapped" => "#2563eb60",
            "pit_row" => "#6b728040",
            "speaking_row" => "#22c55e50",
            "undercut_row" => "#3aa0ff44",
            "cover_row" => "#ff941644",
            _ => "#ffffff08",
        };
        let mut accent = section_color(cfg, section, key, fallback);
        if key == "pit_row" {
            let warm = accent.r > accent.b.saturating_add(20);
            if accent.a < 0x30 || warm {
                accent = parse_rgba(fallback);
            }
        }
        if key == "pit_row" {
            c.fill_rect(rect, accent, 0.0);
        } else {
            draw_row_tint(c, rect, accent);
        }
    } else if alt && i % 2 == 1 {
        let col = section_color(cfg, section, "row_alt", "#ffffff01");
        let a = ((col.a as f32) * 0.25).max(1.0) as u8;
        c.fill_rect(rect, col.with_alpha(a), 0.0);
    }

    if !row.empty && row.is_speaking {
        let accent = section_color(cfg, section, "badge_speaking_bg", "#22c55e");
        let h = rect.height();
        let stripe_w = (h * 0.09).max(3.5);
        c.fill_rect(
            Rect::from_xywh(rect.left(), rect.top() + h * 0.10, stripe_w, h * 0.80),
            accent.with_alpha(255),
            2.0,
        );
        c.fill_rect(rect, accent.with_alpha(38), 0.0);
    }
}

/// iRacing Relative ink: red when they lap you, blue when you lap them.
fn lap_traffic_ink(cfg: &OverlayConfig, section: &str, row: &TableRow) -> Option<Rgba> {
    if !row.lapping || row.is_player || row.empty {
        return None;
    }
    let (key, fallback) = if row.lap_ahead {
        ("threat", "#ff5050")
    } else {
        ("lapped", "#2563eb")
    };
    Some(section_color(cfg, section, key, fallback).with_alpha(255))
}

fn paint_cols(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    rect: Rect,
    columns: &[String],
    gutter: f32,
    rh: f32,
    fs: f32,
    gap_font_scale: f32,
    signed_gaps: bool,
    text: Rgba,
    _muted: Rgba,
) {
    let dim = row.in_pit || row.on_pit || row.inactive || row.empty;
    let dim_text = section_color(cfg, section, "row_dim_text", "#5a616c");
    let lap_ink = lap_traffic_ink(cfg, section, row);
    let name_col = columns.iter().any(|c| c == "name");
    let fixed: f32 = columns
        .iter()
        .filter(|c| c.as_str() != "name")
        .map(|c| rh * width_mult(cfg, section, c, default_width(c)))
        .sum();
    let n_gut = columns.len().saturating_sub(1) as f32;
    let name_w = if name_col {
        (rect.width() - fixed - n_gut * gutter).max(10.0)
    } else {
        0.0
    };

    let mut cx = rect.left();
    let cy = rect.center().1 + fs * 0.35;
    let stripe = cfg
        .section(section)
        .get("columns")
        .and_then(|c| c.get("stripe"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    for col in columns {
        let cw = if col == "name" {
            name_w
        } else {
            rh * width_mult(cfg, section, col, default_width(col))
        };
        match col.as_str() {
            "badge" => paint_badge(c, cfg, section, row, cx, rect.top(), cw, rh, dim, dim_text),
            "position" => {
                if stripe && !row.class_color.is_empty() {
                    let sc = parse_rgba(&row.class_color);
                    let is_fallback = sc.r == 255 && sc.g == 0 && sc.b == 255;
                    if !is_fallback {
                        let stripe_col = if dim {
                            sc.soften(dim_text, 0.55).with_alpha(160)
                        } else {
                            sc
                        };
                        c.fill_rect(
                            Rect::from_xywh(cx, rect.top() + rh * 0.18, rh * 0.12, rh * 0.64),
                            stripe_col,
                            2.0,
                        );
                    }
                }
                // Centered in the POS column (stripe stays on the left).
                // Lap traffic ink wins over pit dim so red/blue stay readable.
                let pos_col = lap_ink.unwrap_or(if dim { dim_text } else { text });
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    &format!("{}", row.position.max(0)),
                    fs,
                    pos_col,
                    true,
                    TextAlign::Center,
                );
            }
            "name" => {
                let colc = lap_ink.unwrap_or(if dim { dim_text } else { text });
                let bold = cfg.bool_key(section, "name_font_bold", true);
                let mut text_x = cx + 4.0;
                if !row.is_pro && !row.group_icon.is_empty() {
                    let ic_px = (rh * 0.42).clamp(10.0, fs * 1.15);
                    let gap = (rh * 0.08).max(3.0);
                    let icon_col = if row.group_color.is_empty() {
                        parse_rgba("#5bb8ff")
                    } else {
                        parse_rgba(&row.group_color)
                    };
                    let col = if dim {
                        icon_col.with_alpha(140)
                    } else {
                        icon_col
                    };
                    let gw = super::icons::paint(
                        c,
                        &row.group_icon,
                        text_x,
                        rect.center().1,
                        ic_px,
                        col,
                        TextAlign::Left,
                    );
                    if gw > 0.0 {
                        text_x += gw + gap;
                    } else {
                        // Unknown group key: small tinted disc.
                        c.circle(
                            text_x + ic_px * 0.45,
                            rect.center().1,
                            ic_px * 0.38,
                            col,
                            true,
                        );
                        text_x += ic_px + gap;
                    }
                }
                let name_right = cx + cw - 2.0;
                if text_x < name_right {
                    let clip = Rect::from_ltrb(text_x, rect.top(), name_right, rect.bottom());
                    c.clip_rect(clip, |c| {
                        label(c, text_x, cy, &row.name, fs, colc, bold, TextAlign::Left);
                    });
                }
            }
            "license" => {
                let letter = row
                    .lic_class
                    .chars()
                    .next()
                    .map(|ch| ch.to_ascii_uppercase())
                    .unwrap_or(' ');
                let mut bg =
                    license_color(cfg, section, &row.lic_class).soften(parse_rgba("#1b1f26"), 0.20);
                if dim {
                    bg = bg.soften(dim_text, 0.55).with_alpha(150);
                }
                let txt = if letter != ' ' && !row.sr.is_empty() {
                    format!("{letter} {}", row.sr)
                } else if !row.sr.is_empty() {
                    row.sr.clone()
                } else if letter != ' ' {
                    letter.to_string()
                } else {
                    "—".into()
                };
                let font_sz = fs * 0.84;
                let tw = c.measure_text(&txt, FontSpec::bold(font_sz));
                let pad_x = fs * 0.28;
                let pill_h = rh * 0.54;
                let show_sr = cfg.bool_key(section, "show_sr_projection", false)
                    && row.sr_delta.is_some_and(|d| d != 0);
                let delta = row.sr_delta.unwrap_or(0);
                let dtxt = format!("{:.2}", (delta.abs() as f32) / 100.0);
                let d_sz = font_sz * 0.90;
                let icon_slot = if show_sr { fs * 0.38 } else { 0.0 };
                let d_w = if show_sr {
                    icon_slot + fs * 0.08 + c.measure_text(&dtxt, FontSpec::new(d_sz))
                } else {
                    0.0
                };
                let gap = if show_sr { fs * 0.22 } else { 0.0 };
                let pill_w = (tw + 2.0 * pad_x).min((cw - gap - d_w).max(4.0));
                let total = pill_w + gap + d_w;
                let left = cx + (cw - total).max(0.0) * 0.5;
                let pill =
                    Rect::from_xywh(left, rect.center().1 - pill_h * 0.5, pill_w.max(4.0), pill_h);
                let edge_a = ((bg.a as f32 * 0.55) as u16 + 60).min(255) as u8;
                c.fill_rect(pill, bg, 4.0);
                c.stroke_rect(pill, bg.with_alpha(edge_a), 4.0, 1.0);
                label(
                    c,
                    pill.center().0,
                    cy,
                    &txt,
                    font_sz,
                    if dim { dim_text } else { bg.contrast_text() },
                    true,
                    TextAlign::Center,
                );
                if show_sr {
                    let dcol = if delta > 0 {
                        section_color(cfg, section, "irating_delta_up", "#46df7a")
                    } else {
                        section_color(cfg, section, "irating_delta_down", "#ff5050")
                    };
                    let mut x = pill.right() + gap;
                    let arrow = if delta > 0 {
                        "irating_up"
                    } else {
                        "irating_down"
                    };
                    super::icons::paint(
                        c,
                        arrow,
                        x + icon_slot * 0.5,
                        cy,
                        fs * 0.50,
                        dcol,
                        TextAlign::Center,
                    );
                    x += icon_slot + fs * 0.06;
                    label(c, x, cy, &dtxt, d_sz, dcol, true, TextAlign::Left);
                }
            }
            "irating" => {
                paint_irating_cell(c, cfg, section, row, cx, cy, cw, rh, fs, dim, dim_text);
            }
            "gap" => {
                let (gtxt, gcol) = gap_display(row, signed_gaps, section, cfg, text);
                label(
                    c,
                    cx + cw - gutter,
                    cy,
                    &gtxt,
                    fs * gap_font_scale,
                    if dim { dim_text } else { gcol },
                    false,
                    TextAlign::Right,
                );
            }
            "car_number" => {
                let num = format_car_number(&row.car_number);
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    &num,
                    fs,
                    if dim { dim_text } else { text },
                    true,
                    TextAlign::Center,
                );
            }
            "last_lap" | "best_lap" => {
                let v = if col == "last_lap" {
                    &row.last_lap
                } else {
                    &row.best_lap
                };
                let s = if v.is_empty() { "—" } else { v.as_str() };
                let fl = !dim
                    && !v.is_empty()
                    && v != "—"
                    && v != "--"
                    && row.session_best
                    && (col == "best_lap" || row.last_lap == row.best_lap);
                let tcol = if dim {
                    dim_text
                } else if fl {
                    section_color(cfg, section, "session_best", "#c084fc")
                } else {
                    text
                };
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    s,
                    fs * 0.92,
                    tcol,
                    fl,
                    TextAlign::Center,
                );
            }
            "pit" => {
                let s = if row.in_pit || row.on_pit {
                    "PIT"
                } else if row.pit_text.is_empty() {
                    "—"
                } else {
                    row.pit_text.as_str()
                };
                let tcol = if dim {
                    dim_text
                } else if row.in_pit || row.on_pit {
                    section_color(cfg, section, "badge_pit_text", "#ffd23a")
                } else {
                    text
                };
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    s,
                    fs * 0.85,
                    tcol,
                    true,
                    TextAlign::Center,
                );
            }
            "class_pos" => {
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    &format!("{}", row.class_position.max(0)),
                    fs,
                    if dim { dim_text } else { text },
                    true,
                    TextAlign::Center,
                );
            }
            "status" => {
                let s = row.status_kind.as_deref().unwrap_or("—");
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    s,
                    fs * 0.85,
                    if dim { dim_text } else { text },
                    false,
                    TextAlign::Center,
                );
            }
            "car_flag" => {
                let s = row.car_flag.as_deref().unwrap_or("—");
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    s,
                    fs * 0.85,
                    if dim { dim_text } else { text },
                    false,
                    TextAlign::Center,
                );
            }
            "country" => {
                if let Some(code) = row.country_code.as_deref() {
                    if let Some(png) = crate::country_flags::flag_png_bytes(code) {
                        c.draw_png_fit(png, Rect::from_xywh(cx, rect.top(), cw, rh - 2.0));
                    }
                } else if !row.empty {
                    label(
                        c,
                        cx + cw * 0.5,
                        cy,
                        "—",
                        fs * 0.85,
                        if dim { dim_text } else { text },
                        false,
                        TextAlign::Center,
                    );
                }
            }
            "laps" => {
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    &format!("{}", row.laps.max(0)),
                    fs,
                    if dim { dim_text } else { text },
                    false,
                    TextAlign::Center,
                );
            }
            "closing" => {
                let s = row
                    .closing
                    .map(|cl| format!("{cl:+.2}"))
                    .unwrap_or_else(|| "—".into());
                label(
                    c,
                    cx + cw * 0.5,
                    cy,
                    &s,
                    fs * 0.9,
                    if dim { dim_text } else { text },
                    false,
                    TextAlign::Center,
                );
            }
            "gap_ahead" | "gap_leader" => {
                let s = if row.gap_text.is_empty() {
                    "—"
                } else {
                    row.gap_text.as_str()
                };
                label(
                    c,
                    cx + cw - gutter,
                    cy,
                    s,
                    fs * gap_font_scale,
                    if dim { dim_text } else { text },
                    false,
                    TextAlign::Right,
                );
            }
            "team" | "nickname" => {
                let s = if col == "team" {
                    &row.team
                } else {
                    &row.nickname
                };
                let show = if s.is_empty() { "—" } else { s.as_str() };
                label(
                    c,
                    cx + 4.0,
                    cy,
                    show,
                    fs * 0.9,
                    if dim { dim_text } else { text },
                    false,
                    TextAlign::Left,
                );
            }
            _ => {}
        }
        cx += cw + gutter;
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
    c.text(
        text,
        x,
        y,
        if bold {
            FontSpec::bold(size)
        } else {
            FontSpec::new(size)
        },
        color,
        align,
    );
}

fn paint_irating_cell(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    cx: f32,
    cy: f32,
    cw: f32,
    rh: f32,
    fs: f32,
    dim: bool,
    dim_text: Rgba,
) {
    let cell = Rect::from_xywh(cx, cy - rh * 0.3 - fs * 0.35, cw, rh * 0.6);
    let muted = if dim {
        dim_text
    } else {
        section_color(cfg, section, "muted", "#8b93a1")
    };

    let pill = cell;
    if pill.width() < 4.0 {
        return;
    }
    c.fill_rect(
        pill,
        section_color(cfg, section, "irating_bg", "#0b0d11cc"),
        4.0,
    );
    c.stroke_rect(
        pill,
        section_color(cfg, section, "irating_border", "#ffffff20"),
        4.0,
        1.0,
    );

    let abbrev = cfg.bool_key(section, "irating_abbreviate", true);
    let ir_txt = fmt_ir(row.irating, abbrev);
    let baseline = pill.center().1 + fs * 0.28;
    if ir_txt.is_empty() {
        label(
            c,
            pill.center().0,
            baseline,
            "—",
            fs * 0.82,
            muted,
            false,
            TextAlign::Center,
        );
        return;
    }

    let ir_col = if dim {
        dim_text
    } else {
        section_color(cfg, section, "irating_text", "#f4f6f8")
    };
    let show_delta = cfg.bool_key(section, "show_irating_projection", false)
        && row.irating_delta.is_some()
        && ir_txt != "--";

    if show_delta {
        let delta = row.irating_delta.unwrap_or(0);
        let dcol = if delta > 0 {
            section_color(cfg, section, "irating_delta_up", "#46df7a")
        } else if delta < 0 {
            section_color(cfg, section, "irating_delta_down", "#ff5050")
        } else {
            muted
        };
        let ir_sz = fs * 0.82;
        let ir_w = c.measure_text(&ir_txt, FontSpec::bold(ir_sz));
        let gap = fs * 0.50;
        let dtxt = format!("{}", delta.abs());
        let icon_name = if delta > 0 {
            "irating_up"
        } else if delta < 0 {
            "irating_down"
        } else {
            ""
        };
        let icon_slot = if icon_name.is_empty() { 0.0 } else { fs * 0.42 };
        let d_w = if icon_name.is_empty() {
            c.measure_text(&format!("{delta:+}"), FontSpec::new(ir_sz))
        } else {
            icon_slot + fs * 0.10 + c.measure_text(&dtxt, FontSpec::new(fs * 0.78))
        };
        let total = ir_w + gap + d_w;
        let pad_x = fs * 0.18;
        let left = pill.left() + pad_x.max((pill.width() - total) * 0.5);
        label(
            c,
            left,
            baseline,
            &ir_txt,
            ir_sz,
            ir_col,
            true,
            TextAlign::Left,
        );
        let dx = left + ir_w + gap;
        if !icon_name.is_empty() {
            super::icons::paint(
                c,
                icon_name,
                dx + icon_slot * 0.5,
                pill.center().1,
                fs * 0.55,
                dcol,
                TextAlign::Center,
            );
            label(
                c,
                dx + icon_slot + fs * 0.10,
                baseline,
                &dtxt,
                fs * 0.78,
                dcol,
                false,
                TextAlign::Left,
            );
        } else {
            label(
                c,
                dx,
                baseline,
                &format!("{delta:+}"),
                ir_sz,
                dcol,
                false,
                TextAlign::Left,
            );
        }
    } else {
        label(
            c,
            pill.center().0,
            baseline,
            &ir_txt,
            fs * 0.82,
            ir_col,
            true,
            TextAlign::Center,
        );
    }
}

fn gap_display(
    row: &TableRow,
    signed_gaps: bool,
    section: &str,
    cfg: &OverlayConfig,
    text: Rgba,
) -> (String, Rgba) {
    if !row.gap_text.is_empty() && !(signed_gaps && row.gap_secs.is_some()) {
        return (row.gap_text.clone(), text);
    }
    if let Some(g) = row.gap_secs {
        let gtxt = if g == 0.0 {
            "0.0".into()
        } else {
            format!("{:.1}", g.abs())
        };
        let gcol = if signed_gaps && section == "relative" && !row.is_player {
            match row.strat_tag.as_deref() {
                Some("undercut") => section_color(cfg, section, "undercut_gap", "#3aa0ff"),
                Some("cover") => section_color(cfg, section, "cover_gap", "#ff9416"),
                _ if g > 0.0 => section_color(cfg, section, "irating_delta_down", "#ff5050"),
                _ if g < 0.0 => section_color(cfg, section, "irating_delta_up", "#46df7a"),
                _ => text,
            }
        } else {
            text
        };
        (gtxt, gcol)
    } else if !row.gap_text.is_empty() {
        (row.gap_text.clone(), text)
    } else {
        ("—".into(), text)
    }
}

fn paint_badge(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    dim: bool,
    dim_text: Rgba,
) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let size = (w.min(h)) * 0.62;
    let box_r = Rect::from_xywh(cx - size * 0.5, cy - size * 0.5, size, size);

    if row.is_speaking {
        paint_speaker_badge(c, cfg, section, box_r);
        return;
    }
    if row.session_best {
        paint_session_best_badge(c, cfg, section, box_r);
        return;
    }
    if row.is_pro {
        let bg = if dim {
            section_color(cfg, section, "badge_pro", "#ffd23a")
                .soften(dim_text, 0.45)
                .with_alpha(170)
        } else {
            section_color(cfg, section, "badge_pro", "#ffd23a")
        };
        c.circle(cx, cy, size * 0.5, bg, true);
        label(
            c,
            cx,
            cy + size * 0.18,
            "★",
            size * 0.55,
            if dim {
                dim_text
            } else {
                section_color(cfg, section, "badge_pro_text", "#141414")
            },
            true,
            TextAlign::Center,
        );
        return;
    }
    if row.is_player {
        let bg = if dim {
            section_color(cfg, section, "badge_player", "#ff9416")
                .soften(dim_text, 0.45)
                .with_alpha(170)
        } else {
            section_color(cfg, section, "badge_player", "#ff9416")
        };
        c.circle(cx, cy, size * 0.5, bg, true);
        return;
    }
    if row.in_pit || row.on_pit {
        let pill_w = w.min(size * 1.55);
        let pill_h = size * 0.92;
        let pill = Rect::from_xywh(cx - pill_w * 0.5, cy - pill_h * 0.5, pill_w, pill_h);
        let mut bg = section_color(cfg, section, "badge_pit_bg", "#ebeef0");
        if dim {
            bg = bg.soften(dim_text, 0.50).with_alpha(160);
        }
        c.fill_rect(pill, bg, 4.0);
        label(
            c,
            pill.center().0,
            cy + pill_h * 0.12,
            "PIT",
            pill_h * 0.46,
            if dim {
                dim_text
            } else {
                section_color(cfg, section, "badge_pit_text", "#141414")
            },
            true,
            TextAlign::Center,
        );
        return;
    }
    if let Some(tag) = row.strat_tag.as_deref() {
        let (raw_bg, letter) = match tag {
            "undercut" => (
                section_color(cfg, section, "badge_undercut", "#3aa0ff"),
                "U",
            ),
            "cover" => (section_color(cfg, section, "badge_cover", "#ff9416"), "C"),
            _ => {
                paint_empty_badge(c, cfg, section, cx, cy, size);
                return;
            }
        };
        let bg = if dim {
            raw_bg.soften(dim_text, 0.50).with_alpha(160)
        } else {
            raw_bg
        };
        c.fill_rect(box_r, bg, 3.0);
        label(
            c,
            box_r.center().0,
            cy + size * 0.18,
            letter,
            size * 0.55,
            if dim {
                dim_text
            } else {
                section_color(cfg, section, "badge_strat_text", "#ffffff")
            },
            true,
            TextAlign::Center,
        );
        return;
    }

    if row.lapping && !row.is_player && !row.empty {
        let bg = if row.lap_ahead {
            section_color(cfg, section, "threat", "#ff5050").with_alpha(255)
        } else {
            section_color(cfg, section, "lapped", "#2563eb").with_alpha(255)
        };
        c.circle(cx, cy, size * 0.42, bg, true);
        c.circle_ex(cx, cy, size * 0.42, Rgba::new(0, 0, 0, 140), false, 1.0);
        return;
    }

    paint_empty_badge(c, cfg, section, cx, cy, size);
}

fn paint_empty_badge(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    cx: f32,
    cy: f32,
    size: f32,
) {
    c.circle(
        cx,
        cy,
        size * 0.5,
        section_color(cfg, section, "badge_empty_fill", "#00000078"),
        true,
    );
    c.circle(
        cx,
        cy,
        size * 0.5,
        section_color(cfg, section, "badge_empty_border", "#ffffff28"),
        false,
    );
}

fn paint_session_best_badge(c: &mut Canvas, cfg: &OverlayConfig, section: &str, box_r: Rect) {
    c.fill_rect(
        box_r,
        section_color(cfg, section, "badge_session_best", "#7638c4"),
        3.0,
    );
    paint_clock(c, box_r);
}

fn paint_clock(c: &mut Canvas, box_r: Rect) {
    let stroke_w = (box_r.width() * 0.08).max(1.0);
    let white = Rgba::WHITE;
    let inset_x = box_r.width() * 0.22;
    let inset_y = box_r.height() * 0.22;
    let inner = box_r.inset(inset_x, inset_y);
    let (icx, icy) = inner.center();
    c.circle(icx, icy, inner.width() * 0.5, white, false);
    // Thicker stroke via a second ring approximation isn't needed; draw hands.
    c.line(icx, icy, icx, icy - inner.height() * 0.32, white, stroke_w);
    c.line(icx, icy, icx + inner.width() * 0.26, icy, white, stroke_w);
}

fn paint_speaker_badge(c: &mut Canvas, cfg: &OverlayConfig, section: &str, box_r: Rect) {
    let pad = box_r.width() * 0.06;
    let (cx, cy) = box_r.center();
    let r = box_r.width() * 0.5 + pad;
    let border = section_color(cfg, section, "badge_speaking_border", "#ffffffcc");
    let bg = section_color(cfg, section, "badge_speaking_bg", "#22c55e");
    let fg = section_color(cfg, section, "badge_speaking_text", "#ffffff");
    c.circle(cx, cy, r, bg, true);
    c.circle(cx, cy, r, border, false);
    let ic = box_r.height() * 0.58;
    if super::icons::paint(c, "speaking", cx, cy, ic, fg, TextAlign::Center) <= 0.0 {
        // Fallback geometry if FA face missing.
        let s = box_r.width() * 0.22;
        c.fill_rect(
            Rect::from_xywh(cx - s * 0.9, cy - s * 0.55, s * 0.7, s * 1.1),
            fg,
            1.0,
        );
        c.polyline(
            &[
                (cx - s * 0.2, cy - s * 0.75),
                (cx + s * 0.85, cy),
                (cx - s * 0.2, cy + s * 0.75),
            ],
            fg,
            s * 0.35,
            true,
        );
    }
}

fn license_color(cfg: &OverlayConfig, section: &str, lic: &str) -> Rgba {
    let letter = lic
        .chars()
        .next()
        .unwrap_or('R')
        .to_ascii_uppercase()
        .to_string();
    let raw = cfg
        .section(section)
        .get("license_colors")
        .and_then(|c| c.get(&letter))
        .and_then(|v| v.as_str());
    match raw {
        Some(s) => parse_rgba(s),
        None => match letter.as_str() {
            "R" => parse_rgba("#d34a3c"),
            "D" => parse_rgba("#e0791a"),
            "C" => parse_rgba("#d6b400"),
            "B" => parse_rgba("#3a9b3a"),
            "A" => parse_rgba("#2f6bd8"),
            "P" => parse_rgba("#1a1a1a"),
            _ => parse_rgba("#666666"),
        },
    }
}

fn fmt_ir(ir: i32, abbrev: bool) -> String {
    if ir <= 0 {
        return String::new();
    }
    if abbrev && ir >= 1000 {
        format!("{:.1}k", ir as f32 / 1000.0)
    } else {
        ir.to_string()
    }
}

/// Content height for standings with `row_count` body rows (header + footer chrome).
pub fn standings_content_size(cfg: &OverlayConfig, row_count: usize) -> (i32, i32) {
    let tokens = DesignTokens::for_section(cfg, "standings", 400.0);
    let n = row_count.max(1) as f32;
    let show_footer = cfg.bool_key("standings", "show_footer", true);
    let bottom = if show_footer {
        tokens.footer_h
    } else {
        tokens.pad * 0.5
    };
    let h = tokens.header_h + n * tokens.row_height + bottom;
    let (_, _, w, _) = crate::config::default_geom("standings");
    (w, h.ceil().max(32.0) as i32)
}
