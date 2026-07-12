use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "system_panel";

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
    let mut rows: Vec<(&str, String)> = Vec::new();
    if ctx.cfg.bool_key(SECTION, "show_cpu", true) {
        rows.push(("CPU", f.cpu.clone().unwrap_or_else(|| "—".into())));
    }
    if ctx.cfg.bool_key(SECTION, "show_mem", true) {
        rows.push(("MEM", f.mem.clone().unwrap_or_else(|| "—".into())));
    }
    if ctx.cfg.bool_key(SECTION, "show_gpu", true) {
        rows.push(("GPU", f.gpu.clone().unwrap_or_else(|| "—".into())));
    }
    if ctx.cfg.bool_key(SECTION, "show_fps", true) {
        rows.push((
            "FPS",
            f.fps.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
        ));
    }
    if ctx.cfg.bool_key(SECTION, "show_network", true) {
        let mut parts = Vec::new();
        if let Some(q) = f.chan_quality {
            if q > 0.0 {
                parts.push(format!("{:.0}%", q));
            }
        }
        if let Some(l) = f.chan_latency {
            if l > 0.0 {
                parts.push(format!("{:.0} ms", l));
            }
        }
        rows.push((
            "NET",
            if parts.is_empty() {
                "—".into()
            } else {
                parts.join(" · ")
            },
        ));
    }

    let n = rows.len().max(1) as f32;
    let avail = card.bottom() - pad - y;
    let rh = (avail / n).min(ctx.cfg.f64_key(SECTION, "row_height_px", 36.0) as f32);
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    for (label_s, value) in rows {
        let row = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, rh),
        );
        label(
            ui,
            Pos2::new(row.left() + 8.0, row.center().y),
            Align2::LEFT_CENTER,
            label_s,
            rh * 0.38,
            muted,
            false,
        );
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
