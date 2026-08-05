//! Skia painter for the IMS-style leaderboard/scoring-pylon strip (Phase C).
//!
//! Ports `widgets/leaderboard_strip.rs` (egui) onto the Skia `Canvas`. The
//! LED car-number digits use a bold monospace-ish text stand-in instead of a
//! true 7-segment glyph renderer (no `scoreboard_digits` port yet on Skia).

use super::canvas::{Canvas, TextAlign};
use super::chrome::{section_color, text_at};
use super::types::{FontSpec, Rect, Rgba};
use crate::config::OverlayConfig;
use crate::telemetry::{format_car_number, CarRow, TelemetryFrame};

const SECTION: &str = "leaderboard_strip";

struct PreviewRow {
    position: i32,
    car_number: &'static str,
    is_player: bool,
}

const PREVIEW: &[PreviewRow] = &[
    PreviewRow {
        position: 1,
        car_number: "45",
        is_player: false,
    },
    PreviewRow {
        position: 2,
        car_number: "10",
        is_player: false,
    },
    PreviewRow {
        position: 3,
        car_number: "12",
        is_player: true,
    },
];

struct StripRow {
    position: i32,
    car_number: String,
    name: String,
    gap: String,
    lap: Option<i32>,
    speed_mph: Option<f32>,
    is_player: bool,
}

fn full_bounds(c: &Canvas) -> Rect {
    Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32)
}

fn collect_rows(cars: &[CarRow], cap: usize) -> Vec<StripRow> {
    let mut ranked: Vec<&CarRow> = cars
        .iter()
        .filter(|c| !c.is_pace_car && c.position > 0)
        .collect();
    ranked.sort_by_key(|c| c.position);
    if cap > 0 {
        ranked.truncate(cap);
    }
    ranked
        .into_iter()
        .map(|c| StripRow {
            position: c.position,
            car_number: c.car_number.clone(),
            name: c.name.clone(),
            gap: c.gap.clone(),
            lap: if c.lap > 0 { Some(c.lap) } else { None },
            speed_mph: speed_mps_to_mph(c.speed_mps),
            is_player: c.is_player,
        })
        .collect()
}

fn speed_mps_to_mph(mps: f32) -> Option<f32> {
    if !mps.is_finite() || mps <= 0.0 {
        return None;
    }
    Some((mps * 2.236_936_3).round())
}

fn preview_rows() -> Vec<StripRow> {
    PREVIEW
        .iter()
        .map(|p| StripRow {
            position: p.position,
            car_number: p.car_number.into(),
            name: String::new(),
            gap: "—".into(),
            lap: None,
            speed_mph: Some(if p.is_player { 148.0 } else { 151.0 }),
            is_player: p.is_player,
        })
        .collect()
}

fn position_column_width(c: &Canvas, rows: &[StripRow], pos_size: f32) -> f32 {
    let font = FontSpec::new(pos_size);
    let mut w = 0.0_f32;
    for row in rows {
        w = w.max(c.measure_text(&row.position.to_string(), font));
    }
    w
}

fn draw_dot_separator(c: &mut Canvas, x: f32, y0: f32, y1: f32) {
    if y1 <= y0 {
        return;
    }
    let dot = 2.0_f32;
    let gap = 5.0_f32;
    let span = y1 - y0;
    let n = ((span / (dot + gap)) as i32).max(3);
    let step = span / (n + 1) as f32;
    let col = Rgba::new(255, 255, 255, 90);
    for i in 1..=n {
        let cy = y0 + step * i as f32;
        c.circle(x, cy, dot * 0.5, col, true);
    }
}

fn resolve_row_height(body_h: f32, row_count: usize, panel_h: f32, cfg_max_frac: f32) -> f32 {
    let n = row_count.max(1) as f32;
    let mut rh = body_h / n;
    if cfg_max_frac > 0.0 {
        rh = rh.min(panel_h * cfg_max_frac);
    }
    rh
}

/// Bold text stand-in for the LED 7-segment car-number glyph.
fn draw_car_number(c: &mut Canvas, rect: Rect, text: &str, color: Rgba) {
    if text.is_empty() {
        return;
    }
    let fs = (rect.height() * 0.66).clamp(10.0, 40.0);
    text_at(
        c,
        rect.center().0,
        rect.center().1,
        text,
        fs,
        color,
        true,
        TextAlign::Center,
    );
}

/// Paint the leaderboard strip. Static layout — always returns `false`.
pub fn paint_leaderboard(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let bounds = full_bounds(c);
    let bg = section_color(cfg, SECTION, "pylon_bg", "#000000");
    c.fill_rect(bounds, bg, 0.0);

    let cap = cfg.f64_key(SECTION, "rows", 0.0).max(0.0) as usize;
    let mut rows = collect_rows(&frame.cars, cap);
    if rows.is_empty() && edit_mode {
        rows = preview_rows();
    }
    if rows.is_empty() {
        return false;
    }

    let w = bounds.width();
    let h = bounds.height();
    let pad_x = (w * 0.08).max(6.0);
    let pad_y = (h * 0.03).max(4.0);
    let inner_w = w - 2.0 * pad_x;
    let inner_h = h - 2.0 * pad_y;

    let show_lap = cfg.bool_key(SECTION, "show_lap", false);
    let show_mph = cfg.bool_key(SECTION, "show_mph", false);
    let show_pos = cfg.bool_key(SECTION, "show_position", true);
    let show_num = cfg.bool_key(SECTION, "show_car_number", true);
    let show_name = cfg.bool_key(SECTION, "show_name", false);
    let show_gap = cfg.bool_key(SECTION, "show_gap", false);
    let highlight = cfg.bool_key(SECTION, "highlight_player", true);
    let show_header = show_lap || show_mph;
    let extra_row = show_name || show_gap;

    let lap_w = if show_lap { inner_w * 0.14 } else { 0.0 };
    let mph_w = if show_mph { inner_w * 0.16 } else { 0.0 };
    let sep_w = if show_pos && show_num {
        inner_w * 0.10
    } else {
        0.0
    };
    let core_w = inner_w - lap_w - mph_w;

    let n = rows.len().max(1);
    let header_h = if show_header {
        (inner_h * 0.11).max(14.0)
    } else {
        0.0
    };
    let fixed_rh = cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    let body_h = inner_h - header_h;
    let max_frac = cfg.f64_key(SECTION, "max_row_height_frac", 0.0) as f32;
    let mut row_h = if fixed_rh > 0.0 {
        fixed_rh
    } else {
        resolve_row_height(body_h, n, h, max_frac)
    };
    row_h = row_h.max(22.0);
    if extra_row {
        row_h = row_h.max(28.0);
    }

    let pos_size = row_h * if extra_row { 0.50 } else { 0.62 };
    let data_size = row_h * if extra_row { 0.28 } else { 0.34 };
    let lap_size = row_h * 0.34;

    let (pos_w, num_w) = if show_pos && show_num {
        let pw = position_column_width(c, &rows, pos_size);
        (pw, (core_w - pw - sep_w).max(0.0))
    } else if show_pos {
        (position_column_width(c, &rows, pos_size).min(core_w), 0.0)
    } else if show_num {
        (0.0, (core_w - sep_w).max(0.0))
    } else {
        (0.0, 0.0)
    };

    let x_lap = bounds.left() + pad_x;
    let x_pos = x_lap + lap_w;
    let x_sep = x_pos + pos_w;
    let x_num = x_sep + sep_w;
    let x_mph = x_num + num_w;

    let header_color = section_color(cfg, SECTION, "header", "#e8e8e8");
    let pos_color = section_color(cfg, SECTION, "pos", "#ffffff");
    // Prefer car_number; digit is the Rust-default alias for LED orange.
    let num_fill = {
        let has_custom = cfg
            .section(SECTION)
            .get("colors")
            .and_then(|cols| cols.get("car_number"))
            .is_some();
        if has_custom {
            section_color(cfg, SECTION, "car_number", "#ff8c00")
        } else {
            section_color(cfg, SECTION, "digit", "#ff9416")
        }
    };
    let data_color = section_color(cfg, SECTION, "text", "#d8d8d8");
    let player_bg = section_color(cfg, SECTION, "player_row", "#ffffff18");
    let muted = section_color(cfg, SECTION, "muted", "#707070");
    let slower = section_color(cfg, SECTION, "slower", "#ff6a3a");

    if show_header {
        let hdr_size = (header_h * 0.42).max(7.0);
        let y = bounds.top() + pad_y;
        if show_lap {
            c.text(
                "LAP",
                x_lap,
                y + header_h,
                FontSpec::new(hdr_size),
                header_color,
                TextAlign::Left,
            );
        }
        if show_mph {
            c.text(
                "MPH",
                x_mph + mph_w,
                y + header_h,
                FontSpec::new(hdr_size),
                header_color,
                TextAlign::Right,
            );
        }
    }

    let mut y = bounds.top() + pad_y + header_h;
    for row in &rows {
        let row_top = y;
        let row_rect = Rect::from_xywh(
            bounds.left() + pad_x,
            row_top,
            inner_w,
            (row_h - 2.0).max(1.0),
        );
        if row.is_player && highlight {
            c.fill_rect(row_rect, player_bg, 0.0);
        }

        if show_lap {
            let lap_txt = row.lap.map(|l| l.to_string()).unwrap_or_default();
            if !lap_txt.is_empty() {
                text_at(
                    c,
                    x_lap + lap_w,
                    row_top + (row_h - 2.0) * 0.5,
                    &lap_txt,
                    lap_size,
                    data_color,
                    false,
                    TextAlign::Right,
                );
            }
        }

        if show_pos {
            text_at(
                c,
                x_pos + pos_w,
                row_top + (row_h - 2.0) * 0.5,
                &row.position.to_string(),
                pos_size,
                pos_color,
                true,
                TextAlign::Right,
            );
        }

        if show_pos && show_num && sep_w > 0.0 {
            draw_dot_separator(
                c,
                x_sep + sep_w * 0.5,
                row_top + row_h * 0.18,
                row_top + row_h * 0.82,
            );
        }

        if show_num {
            let num = format_car_number(row.car_number.trim());
            if !num.is_empty() {
                let num_rect = Rect::from_xywh(x_num, row_top, num_w, row_h - 2.0);
                draw_car_number(c, num_rect, &num, num_fill);
            }
        }

        if show_mph {
            let mph_txt = row
                .speed_mph
                .map(|m| format!("{}", m as i32))
                .unwrap_or_default();
            if !mph_txt.is_empty() {
                text_at(
                    c,
                    x_mph + mph_w,
                    row_top + (row_h - 2.0) * 0.5,
                    &mph_txt,
                    lap_size,
                    num_fill,
                    false,
                    TextAlign::Right,
                );
            }
        }

        if extra_row {
            let meta_y = row_top + row_h * 0.52;
            let meta_h = row_h * 0.42;
            let meta_x = x_pos;
            let meta_w = inner_w - (x_pos - (bounds.left() + pad_x));
            if show_name && !row.name.is_empty() {
                let name_rect = Rect::from_xywh(meta_x, meta_y, meta_w * 0.62, meta_h);
                c.clip_rect(name_rect, |c| {
                    text_at(
                        c,
                        meta_x,
                        meta_y + meta_h * 0.5,
                        &row.name,
                        data_size * 0.9,
                        data_color,
                        false,
                        TextAlign::Left,
                    );
                });
            }
            if show_gap {
                let gap = if row.gap.is_empty() {
                    "—"
                } else {
                    row.gap.as_str()
                };
                let gcol = if gap.starts_with('+') { slower } else { muted };
                text_at(
                    c,
                    bounds.left() + pad_x + inner_w,
                    meta_y + meta_h * 0.5,
                    gap,
                    data_size,
                    gcol,
                    false,
                    TextAlign::Right,
                );
            }
        }

        y += row_h;
    }
    false
}
