use super::WidgetCtx;
use crate::chrome::{
    color_with_alpha, full_rect, is_elegant, label, panel_card, panel_content_pad, panel_title,
};
use crate::icons;
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "system_panel";

/// show_* config key → (text label, FA icon name).
const ROW_SPECS: &[(&str, &str, &str)] = &[
    ("show_cpu", "CPU", "cpu"),
    ("show_mem", "MEM", "mem"),
    ("show_gpu", "GPU", "gpu"),
    ("show_fps", "FPS", "fps"),
    ("show_network", "NET", "network"),
    ("show_ffb", "FFB", "ffb"),
];

fn collect_rows(ctx: &WidgetCtx<'_>) -> Vec<(&'static str, &'static str, String, bool)> {
    let f = ctx.frame;
    let mut rows = Vec::new();
    for &(cfg_key, text_label, icon_key) in ROW_SPECS {
        if !ctx.cfg.bool_key(SECTION, cfg_key, true) {
            continue;
        }
        let (value, warn) = match cfg_key {
            "show_cpu" => (f.cpu.clone().unwrap_or_else(|| "—".into()), false),
            "show_mem" => (f.mem.clone().unwrap_or_else(|| "—".into()), false),
            "show_gpu" => (f.gpu.clone().unwrap_or_else(|| "—".into()), false),
            "show_fps" => (
                f.fps.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
                false,
            ),
            "show_network" => {
                let v = if let Some(q) = f.chan_quality {
                    if q > 0.0 {
                        format!("{:.0}%", q)
                    } else {
                        "—".into()
                    }
                } else {
                    "—".into()
                };
                (v, false)
            }
            "show_ffb" => {
                if let Some(p) = f.ffb_pct.filter(|v| v.is_finite()) {
                    (format!("{:.0}%", p), p > 100.0)
                } else {
                    ("—".into(), false)
                }
            }
            _ => ("—".into(), false),
        };
        rows.push((text_label, icon_key, value, warn));
    }
    rows
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    if is_elegant(ctx.cfg, SECTION) {
        paint_elegant(ui, ctx);
    } else {
        paint_data(ui, ctx);
    }
}

fn paint_data(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_content_pad(ctx.cfg, SECTION, card.height());
    let mut y = card.top() + pad;
    y = panel_title(ui, ctx.cfg, SECTION, card, radius, y, pad, "SYSTEM");

    let show_icons = ctx.cfg.bool_key(SECTION, "show_icons", false);
    let rows = collect_rows(ctx);
    let n = rows.len().max(1) as f32;
    let avail = card.bottom() - pad - y;
    let rh = (avail / n).min(ctx.cfg.f64_key(SECTION, "row_height_px", 36.0) as f32);
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let header = ctx.cfg.color(SECTION, "header", "#9aa3b2");
    let warn = ctx.cfg.color(SECTION, "warn", "#ff5b5b");
    for (text_label, icon_key, value, is_warn) in rows {
        let row = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, rh),
        );
        if show_icons && icons::has(icon_key) {
            if let Some(g) = icons::glyph(icon_key) {
                ui.painter().text(
                    Pos2::new(row.left() + 8.0, row.center().y),
                    Align2::LEFT_CENTER,
                    g,
                    icons::font_id(rh * 0.42),
                    header,
                );
            }
        } else {
            label(
                ui,
                Pos2::new(row.left() + 8.0, row.center().y),
                Align2::LEFT_CENTER,
                text_label,
                rh * 0.38,
                muted,
                false,
            );
        }
        label(
            ui,
            Pos2::new(row.right() - 8.0, row.center().y),
            Align2::RIGHT_CENTER,
            &value,
            rh * 0.42,
            if is_warn { warn } else { text },
            true,
        );
        y += rh;
    }
}

/// Soft metric stack: dense icon + value rows (same data, less chrome).
fn paint_elegant(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, _radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad_x = (card.width() * 0.05).clamp(8.0, 12.0);
    let pad_y = (card.height() * 0.05).clamp(6.0, 10.0);
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200);
    let accent = ctx.cfg.color(SECTION, "accent", "#9aa3b2");
    let rows = collect_rows(ctx);
    if rows.is_empty() {
        return;
    }

    let show_title = ctx.cfg.bool_key(SECTION, "show_title", true);
    let mut y = card.top() + pad_y;
    let left = card.left() + pad_x;
    let width = card.width() - 2.0 * pad_x;

    if show_title {
        let title = ctx.cfg.str_key(SECTION, "title", "SYSTEM");
        label(
            ui,
            Pos2::new(left, y + 6.0),
            Align2::LEFT_CENTER,
            &title,
            10.0,
            muted,
            false,
        );
        y += 14.0;
    }

    let avail = (card.bottom() - pad_y - y).max(rows.len() as f32 * 18.0);
    let row_h = (avail / rows.len() as f32).clamp(16.0, 22.0);
    let warn_c = ctx.cfg.color(SECTION, "warn", "#ff5b5b");

    for (text_label, icon_key, value, is_warn) in rows {
        let cy = y + row_h * 0.5;
        let icon_sz = 12.0;
        let value_c = if is_warn { warn_c } else { text };
        if let Some(g) = icons::glyph(icon_key) {
            ui.painter().text(
                Pos2::new(left, cy),
                Align2::LEFT_CENTER,
                g,
                icons::font_id(icon_sz),
                accent,
            );
            label(
                ui,
                Pos2::new(left + icon_sz + 6.0, cy),
                Align2::LEFT_CENTER,
                text_label,
                11.0,
                muted,
                false,
            );
        } else {
            label(
                ui,
                Pos2::new(left, cy),
                Align2::LEFT_CENTER,
                text_label,
                11.0,
                muted,
                false,
            );
        }
        label(
            ui,
            Pos2::new(left + width, cy),
            Align2::RIGHT_CENTER,
            &value,
            12.0,
            value_c,
            true,
        );
        y += row_h;
    }
}
