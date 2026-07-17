use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
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
];

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let mut y = card.top() + pad;
    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        let hh = (card.height() * 0.12).max(22.0);
        let hdr = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, hh),
        );
        draw_section_header(
            ui,
            ctx.cfg,
            SECTION,
            hdr,
            &ctx.cfg.str_key(SECTION, "title", "SYSTEM"),
            radius,
        );
        y += hh + pad * 0.35;
    }

    let f = ctx.frame;
    let show_icons = ctx.cfg.bool_key(SECTION, "show_icons", false);
    let mut rows: Vec<(&str, &str, String)> = Vec::new();
    for &(cfg_key, text_label, icon_key) in ROW_SPECS {
        if !ctx.cfg.bool_key(SECTION, cfg_key, true) {
            continue;
        }
        let value = match cfg_key {
            "show_cpu" => f.cpu.clone().unwrap_or_else(|| "—".into()),
            "show_mem" => f.mem.clone().unwrap_or_else(|| "—".into()),
            "show_gpu" => f.gpu.clone().unwrap_or_else(|| "—".into()),
            "show_fps" => f.fps.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
            "show_network" => {
                if let Some(q) = f.chan_quality {
                    if q > 0.0 {
                        format!("{:.0}%", q)
                    } else {
                        "—".into()
                    }
                } else {
                    "—".into()
                }
            }
            _ => "—".into(),
        };
        rows.push((text_label, icon_key, value));
    }

    let n = rows.len().max(1) as f32;
    let avail = card.bottom() - pad - y;
    let rh = (avail / n).min(ctx.cfg.f64_key(SECTION, "row_height_px", 36.0) as f32);
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let header = ctx.cfg.color(SECTION, "header", "#9aa3b2");
    for (text_label, icon_key, value) in rows {
        let row = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, rh),
        );
        let icon_on = show_icons && icons::has(icon_key);
        if icon_on {
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
            text,
            true,
        );
        y += rh;
    }
}
