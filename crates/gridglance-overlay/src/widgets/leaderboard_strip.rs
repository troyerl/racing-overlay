//! Leaderboard strip — IMS scoring-pylon style top-N tower.

use super::WidgetCtx;
use crate::chrome::{full_rect, label};
use crate::telemetry::CarRow;
use egui::{Align2, Color32, FontFamily, FontId, Pos2, Rect, Stroke, Ui, Vec2};

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

fn measure_text(ui: &Ui, text: &str, font: FontId) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(text.to_owned(), font, Color32::WHITE)
            .size()
            .x
    })
}

fn position_column_width(ui: &Ui, rows: &[StripRow], pos_size: f32) -> f32 {
    let font = FontId::new(pos_size, FontFamily::Proportional);
    let mut w = 0.0_f32;
    for row in rows {
        w = w.max(measure_text(ui, &row.position.to_string(), font.clone()));
    }
    w
}

fn draw_dot_separator(ui: &mut Ui, x: f32, y0: f32, y1: f32) {
    if y1 <= y0 {
        return;
    }
    let dot = 2.0_f32;
    let gap = 5.0_f32;
    let span = y1 - y0;
    let n = ((span / (dot + gap)) as i32).max(3);
    let step = span / (n + 1) as f32;
    let c = Color32::from_rgba_unmultiplied(255, 255, 255, 90);
    for i in 1..=n {
        let cy = y0 + step * i as f32;
        ui.painter().circle_filled(Pos2::new(x, cy), dot * 0.5, c);
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

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let bg = ctx.cfg.color(SECTION, "pylon_bg", "#000000");
    ui.painter().rect_filled(rect, 0.0, bg);

    let cap = ctx.cfg.f64_key(SECTION, "rows", 0.0).max(0.0) as usize;
    let mut rows = collect_rows(&ctx.frame.cars, cap);
    if rows.is_empty() && ctx.edit_mode {
        rows = preview_rows();
    }
    if rows.is_empty() {
        let border = ctx.cfg.color(SECTION, "panel_border", "#ffffff10");
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            0.0,
            Stroke::new(1.0_f32, border),
            egui::StrokeKind::Inside,
        );
        return;
    }

    let w = rect.width();
    let h = rect.height();
    let pad_x = (w * 0.08).max(6.0);
    let pad_y = (h * 0.03).max(4.0);
    let inner_w = w - 2.0 * pad_x;
    let inner_h = h - 2.0 * pad_y;

    let show_lap = ctx.cfg.bool_key(SECTION, "show_lap", false);
    let show_mph = ctx.cfg.bool_key(SECTION, "show_mph", false);
    let show_pos = ctx.cfg.bool_key(SECTION, "show_position", true);
    let show_num = ctx.cfg.bool_key(SECTION, "show_car_number", true);
    let show_name = ctx.cfg.bool_key(SECTION, "show_name", false);
    let show_gap = ctx.cfg.bool_key(SECTION, "show_gap", false);
    let highlight = ctx.cfg.bool_key(SECTION, "highlight_player", true);
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
    let fixed_rh = ctx.cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    let body_h = inner_h - header_h;
    let max_frac = ctx.cfg.f64_key(SECTION, "max_row_height_frac", 0.0) as f32;
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
        let pw = position_column_width(ui, &rows, pos_size);
        (pw, (core_w - pw - sep_w).max(0.0))
    } else if show_pos {
        (position_column_width(ui, &rows, pos_size).min(core_w), 0.0)
    } else if show_num {
        (0.0, (core_w - sep_w).max(0.0))
    } else {
        (0.0, 0.0)
    };

    let x_lap = rect.left() + pad_x;
    let x_pos = x_lap + lap_w;
    let x_sep = x_pos + pos_w;
    let x_num = x_sep + sep_w;
    let x_mph = x_num + num_w;

    let header_color = ctx.cfg.color(SECTION, "header", "#e8e8e8");
    let pos_color = ctx.cfg.color(SECTION, "pos", "#ffffff");
    // Prefer car_number; digit is the Rust-default alias for LED orange.
    let num_fill = {
        let c = ctx.cfg.color(SECTION, "car_number", "#ff8c00");
        if ctx
            .cfg
            .section(SECTION)
            .get("colors")
            .and_then(|cols| cols.get("car_number"))
            .is_none()
        {
            ctx.cfg.color(SECTION, "digit", "#ff9416")
        } else {
            c
        }
    };
    let data_color = ctx.cfg.color(SECTION, "text", "#d8d8d8");
    let player_bg = ctx.cfg.color(SECTION, "player_row", "#ffffff18");
    let muted = ctx.cfg.color(SECTION, "muted", "#707070");
    let slower = ctx.cfg.color(SECTION, "slower", "#ff6a3a");

    if show_header {
        let hdr_size = (header_h * 0.42).max(7.0);
        let y = rect.top() + pad_y;
        if show_lap {
            label(
                ui,
                Pos2::new(x_lap, y + header_h),
                Align2::LEFT_BOTTOM,
                "LAP",
                hdr_size,
                header_color,
                false,
            );
        }
        if show_mph {
            label(
                ui,
                Pos2::new(x_mph + mph_w, y + header_h),
                Align2::RIGHT_BOTTOM,
                "MPH",
                hdr_size,
                header_color,
                false,
            );
        }
    }

    let mut y = rect.top() + pad_y + header_h;
    for row in &rows {
        let row_top = y;
        let row_rect = Rect::from_min_size(
            Pos2::new(rect.left() + pad_x, row_top),
            Vec2::new(inner_w, (row_h - 2.0).max(1.0)),
        );
        if row.is_player && highlight {
            ui.painter().rect_filled(row_rect, 0.0, player_bg);
        }

        if show_lap {
            let lap_txt = row.lap.map(|l| l.to_string()).unwrap_or_default();
            if !lap_txt.is_empty() {
                label(
                    ui,
                    Pos2::new(x_lap + lap_w, row_top + (row_h - 2.0) * 0.5),
                    Align2::RIGHT_CENTER,
                    &lap_txt,
                    lap_size,
                    data_color,
                    false,
                );
            }
        }

        if show_pos {
            label(
                ui,
                Pos2::new(x_pos + pos_w, row_top + (row_h - 2.0) * 0.5),
                Align2::RIGHT_CENTER,
                &row.position.to_string(),
                pos_size,
                pos_color,
                true,
            );
        }

        if show_pos && show_num && sep_w > 0.0 {
            draw_dot_separator(
                ui,
                x_sep + sep_w * 0.5,
                row_top + row_h * 0.18,
                row_top + row_h * 0.82,
            );
        }

        if show_num {
            let num = crate::telemetry::format_car_number(row.car_number.trim());
            if !num.is_empty() {
                let num_rect =
                    Rect::from_min_size(Pos2::new(x_num, row_top), Vec2::new(num_w, row_h - 2.0));
                super::scoreboard_digits::draw_scoreboard_text(ui, num_rect, &num, num_fill, 2);
            }
        }

        if show_mph {
            let mph_txt = row
                .speed_mph
                .map(|m| format!("{}", m as i32))
                .unwrap_or_default();
            if !mph_txt.is_empty() {
                label(
                    ui,
                    Pos2::new(x_mph + mph_w, row_top + (row_h - 2.0) * 0.5),
                    Align2::RIGHT_CENTER,
                    &mph_txt,
                    lap_size,
                    num_fill,
                    false,
                );
            }
        }

        if extra_row {
            let meta_y = row_top + row_h * 0.52;
            let meta_h = row_h * 0.42;
            let meta_x = x_pos;
            let meta_w = inner_w - (x_pos - (rect.left() + pad_x));
            if show_name && !row.name.is_empty() {
                let name_rect = Rect::from_min_size(
                    Pos2::new(meta_x, meta_y),
                    Vec2::new(meta_w * 0.62, meta_h),
                );
                let painter = ui.painter().with_clip_rect(name_rect);
                painter.text(
                    Pos2::new(meta_x, meta_y + meta_h * 0.5),
                    Align2::LEFT_CENTER,
                    &row.name,
                    FontId::proportional(data_size * 0.9),
                    data_color,
                );
            }
            if show_gap {
                let gap = if row.gap.is_empty() {
                    "—"
                } else {
                    row.gap.as_str()
                };
                let gcol = if gap.starts_with('+') { slower } else { muted };
                label(
                    ui,
                    Pos2::new(rect.left() + pad_x + inner_w, meta_y + meta_h * 0.5),
                    Align2::RIGHT_CENTER,
                    gap,
                    data_size,
                    gcol,
                    false,
                );
            }
        }

        y += row_h;
    }

    let border = ctx.cfg.color(SECTION, "panel_border", "#ffffff10");
    ui.painter().rect_stroke(
        rect.shrink(0.5),
        0.0,
        Stroke::new(1.0_f32, border),
        egui::StrokeKind::Inside,
    );
}
