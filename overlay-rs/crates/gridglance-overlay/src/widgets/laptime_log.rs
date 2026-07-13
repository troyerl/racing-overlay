//! Laptime log — recent laps with delta + track temp.

use super::WidgetCtx;
use crate::chrome::{draw_card, draw_dark_cell, full_rect, label, panel_pad};
use crate::icons;
use crate::telemetry::{signed_delta_1, LapLogRow};
use egui::{Align2, FontFamily, FontId, Pos2, Rect, Stroke, Ui, Vec2};

const SECTION: &str = "laptime_log";

const DEFAULT_COLS: &[&str] = &["lap", "time", "delta", "temp"];

const HEADERS: &[(&str, &str)] = &[
    ("lap", "LAP"),
    ("time", "TIME"),
    ("delta", "DELTA"),
    ("temp", "TEMP."),
    ("sectors", "SECT"),
    ("fuel", "FUEL"),
    ("tires", "TIRE"),
    ("incidents", "INC"),
    ("tag", "TAG"),
];

fn col_header(key: &str) -> &str {
    HEADERS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, h)| *h)
        .unwrap_or(key)
}

fn col_weight(key: &str) -> f32 {
    match key {
        "lap" => 0.10,
        "time" => 0.22,
        "delta" => 0.14,
        "temp" => 0.14,
        "sectors" => 0.18,
        "fuel" => 0.10,
        "tires" | "incidents" | "tag" => 0.08,
        _ => 0.12,
    }
}

fn column_order(ctx: &WidgetCtx<'_>) -> Vec<String> {
    if let Some(arr) = ctx
        .cfg
        .section(SECTION)
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
    DEFAULT_COLS.iter().map(|s| (*s).to_string()).collect()
}

fn col_layout(order: &[String]) -> Vec<(String, f32)> {
    let weights: Vec<f32> = order.iter().map(|k| col_weight(k)).collect();
    let total: f32 = weights.iter().sum::<f32>().max(1e-6);
    order
        .iter()
        .zip(weights)
        .map(|(k, w)| (k.clone(), w / total))
        .collect()
}

fn resolve_row_height(body_h: f32, row_count: usize, panel_h: f32, max_frac: f32) -> f32 {
    let n = row_count.max(1) as f32;
    let mut rh = body_h / n;
    if max_frac > 0.0 {
        rh = rh.min(panel_h * max_frac);
    }
    rh.max(18.0)
}

fn cell_value(row: &LapLogRow, key: &str) -> String {
    match key {
        "lap" => row.lap.to_string(),
        "time" => row.time.clone(),
        "delta" => row.delta.clone(),
        "temp" => {
            if row.temp.is_empty() {
                "—".into()
            } else {
                row.temp.clone()
            }
        }
        "fuel" => row.fuel.clone().unwrap_or_else(|| "—".into()),
        "tires" => row.tires.clone().unwrap_or_else(|| "—".into()),
        "incidents" => row.incidents.clone().unwrap_or_else(|| "—".into()),
        "tag" => row.tag.clone().unwrap_or_default(),
        "sectors" => "—".into(),
        _ => "—".into(),
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height()).max((card.height() * 0.03).max(8.0));

    let order = column_order(ctx);
    let cols = col_layout(&order);
    let show_header = ctx.cfg.bool_key(SECTION, "show_header", true);
    let hscale = ctx
        .cfg
        .f64_key(SECTION, "header_font_scale", 1.0)
        .max(0.3) as f32;
    let n = ctx.cfg.f64_key(SECTION, "rows", 8.0).max(1.0) as usize;
    let fixed_rh = ctx.cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    let max_frac = ctx.cfg.f64_key(SECTION, "max_row_height_frac", 0.14) as f32;
    let font_scale = ctx.cfg.f64_key(SECTION, "font_scale", 0.42) as f32;
    let text_scale = ctx.cfg.text_scale(SECTION);

    let (header_h, row_h) = if fixed_rh > 0.0 {
        let hh = if show_header {
            (fixed_rh * 1.1 * hscale).round()
        } else {
            0.0
        };
        (hh, fixed_rh)
    } else {
        let hh = if show_header {
            (card.height() * 0.12).max(22.0)
        } else {
            0.0
        };
        let body_top_est = card.top() + pad + hh;
        let est_body = (card.bottom() - pad - body_top_est).max(1.0);
        (
            hh,
            resolve_row_height(est_body, n, card.height(), max_frac),
        )
    };

    let body_top = card.top() + pad + header_h;
    let inner_w = card.width() - 2.0 * pad;
    let inner_x = card.left() + pad;

    let mut cells: Vec<(String, f32, f32)> = Vec::with_capacity(cols.len());
    let mut cx = inner_x;
    for (key, frac) in &cols {
        let cw = inner_w * frac;
        cells.push((key.clone(), cx, cw));
        cx += cw;
    }

    let header_col = ctx.cfg.color(SECTION, "header", "#ffd23a");
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let faster = ctx.cfg.color(SECTION, "faster", "#46df7a");
    let slower = ctx.cfg.color(SECTION, "slower", "#e23b3b");
    let row_alt = ctx.cfg.color(SECTION, "row_alt", "#ffffff08");
    let divider = ctx.cfg.color(SECTION, "border", "#ffffff28");

    if show_header {
        let hdr = Rect::from_min_size(
            Pos2::new(inner_x, card.top() + pad),
            Vec2::new(inner_w, header_h),
        );
        ui.painter().rect_filled(
            hdr,
            egui::CornerRadius {
                nw: radius as u8,
                ne: radius as u8,
                sw: 0,
                se: 0,
            },
            ctx.cfg.color(SECTION, "header_bg", "#0b0e12bb"),
        );
        ui.painter().line_segment(
            [hdr.left_bottom(), hdr.right_bottom()],
            Stroke::new(1.0_f32, divider),
        );
        let hs = header_h * 0.42 * hscale * text_scale;
        for (key, x, cw) in &cells {
            label(
                ui,
                Pos2::new(x + cw * 0.5, hdr.center().y),
                Align2::CENTER_CENTER,
                col_header(key),
                hs,
                header_col,
                true,
            );
        }
    }

    let shown: Vec<&LapLogRow> = ctx.frame.lap_log.iter().take(n).collect();
    let alt = ctx.cfg.bool_key(SECTION, "alt_row_shading", true);
    let dividers = ctx.cfg.bool_key(SECTION, "row_dividers", true);
    let temp_icon = ctx.cfg.bool_key(SECTION, "temp_icon", true);
    let data_size = row_h * font_scale * text_scale;

    for (i, row) in shown.iter().enumerate() {
        let y = body_top + i as f32 * row_h;
        if y + row_h > card.bottom() - pad * 0.5 {
            break;
        }
        let row_rect = Rect::from_min_size(Pos2::new(inner_x, y), Vec2::new(inner_w, row_h));
        if alt && i % 2 == 1 {
            ui.painter().rect_filled(row_rect, 0.0, row_alt);
        }
        draw_row(
            ui,
            ctx,
            row,
            &cells,
            y,
            row_h,
            data_size,
            temp_icon,
            text,
            muted,
            faster,
            slower,
        );
        if dividers && i + 1 < shown.len() {
            ui.painter().line_segment(
                [
                    Pos2::new(inner_x, y + row_h),
                    Pos2::new(inner_x + inner_w, y + row_h),
                ],
                Stroke::new(1.0_f32, divider),
            );
        }
    }
}

fn draw_row(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    row: &LapLogRow,
    cells: &[(String, f32, f32)],
    y: f32,
    row_h: f32,
    data_size: f32,
    temp_icon: bool,
    text: egui::Color32,
    muted: egui::Color32,
    faster: egui::Color32,
    slower: egui::Color32,
) {
    for (key, x, cw) in cells {
        let rect = Rect::from_min_size(Pos2::new(*x, y), Vec2::new(*cw, row_h));
        match key.as_str() {
            "delta" => match row.delta_seconds() {
                None => {
                    label(
                        ui,
                        rect.center(),
                        Align2::CENTER_CENTER,
                        "—",
                        data_size,
                        muted,
                        false,
                    );
                }
                Some(d) => {
                    let col = if d < 0.0 { faster } else { slower };
                    let txt = if row.delta.is_empty() || row.delta == "—" {
                        signed_delta_1(d)
                    } else {
                        row.delta.clone()
                    };
                    label(
                        ui,
                        rect.center(),
                        Align2::CENTER_CENTER,
                        &txt,
                        data_size,
                        col,
                        false,
                    );
                }
            },
            "temp" if temp_icon => {
                let val = cell_value(row, "temp");
                let icon_w = row_h * 0.35;
                let font = FontId::new(data_size, FontFamily::Monospace);
                let tw = ui.fonts(|f| {
                    f.layout_no_wrap(val.clone(), font.clone(), text)
                        .size()
                        .x
                });
                let total = icon_w + tw + 4.0;
                let ox = *x + (*cw - total) * 0.5;
                if let Some(glyph) = icons::glyph("track_temp") {
                    ui.painter().text(
                        Pos2::new(ox + icon_w * 0.5, y + row_h * 0.5),
                        Align2::CENTER_CENTER,
                        &glyph,
                        icons::font_id(row_h * 0.36),
                        text,
                    );
                }
                label(
                    ui,
                    Pos2::new(ox + icon_w + 4.0, y + row_h * 0.5),
                    Align2::LEFT_CENTER,
                    &val,
                    data_size,
                    text,
                    false,
                );
            }
            "tag" => {
                let val = cell_value(row, "tag");
                if !val.is_empty() {
                    let chip = Rect::from_min_size(
                        Pos2::new(*x + *cw * 0.1, y + row_h * 0.22),
                        Vec2::new(*cw * 0.8, row_h * 0.56),
                    );
                    draw_dark_cell(ui, ctx.cfg, SECTION, chip, 4.0);
                    label(
                        ui,
                        chip.center(),
                        Align2::CENTER_CENTER,
                        &val,
                        row_h * 0.30,
                        text,
                        true,
                    );
                }
            }
            _ => {
                let val = cell_value(row, key);
                label(
                    ui,
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &val,
                    data_size,
                    text,
                    false,
                );
            }
        }
    }
}
