//! Tire panel — 4-corner wear, temp, and optional cold pressure.
//! Ports `overlay/widgets/tire_panel.py`.

use super::WidgetCtx;
use crate::chrome::{draw_card, draw_dark_cell, draw_section_header, full_rect, label, panel_pad};
use crate::telemetry::TireCorner;
use egui::{Align2, CornerRadius, Pos2, Rect, Ui, Vec2};

const SECTION: &str = "tire_panel";
/// Display label + TelemetryFrame index (FL/FR/RL/RR ↔ lf/rf/lr/rr).
const CORNERS: [(&str, usize); 4] = [("FL", 0), ("FR", 1), ("RL", 2), ("RR", 3)];

fn cell_radius(row_h: f32) -> f32 {
    (row_h * 0.22).clamp(4.0, 8.0)
}

fn has_corner_data(corners: &[TireCorner; 4]) -> bool {
    corners
        .iter()
        .any(|c| c.wear.is_some() || c.temp.is_some() || c.pressure.is_some())
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let corners = &ctx.frame.tire_corners;
    if !has_corner_data(corners) && !ctx.edit_mode {
        let _ = full_rect(ui);
        return;
    }

    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let h = card.height();
    let mut y = card.top() + pad;

    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        let hh = (h * 0.10).max(20.0);
        let hdr = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, hh),
        );
        let title = ctx.cfg.str_key(SECTION, "title", "TIRES");
        draw_section_header(ui, ctx.cfg, SECTION, hdr, &title, radius);
        y += hh + pad * 0.25;
    }

    let grid_top = y;
    let iw = card.width() - 2.0 * pad;
    let ih = card.bottom() - pad - y;
    let gap = (iw * 0.04).max(4.0);
    let cw = (iw - gap) / 2.0;
    let ch = (ih - gap) / 2.0;
    let warn = ctx.cfg.f64_key(SECTION, "warn_wear_pct", 30.0) as f32;
    let rad = cell_radius(cw.min(ch) * 0.4);
    let header_c = ctx.cfg.color(SECTION, "header", "#c5ccd6");
    let text_c = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let bar_bg = ctx.cfg.color(SECTION, "bar_bg", "#ffffff18");
    let wear_c = ctx.cfg.color(SECTION, "wear", "#70df7a");
    let warn_c = ctx.cfg.color(SECTION, "warn", "#e23b3b");
    let show_wear = ctx.cfg.bool_key(SECTION, "show_wear", true);
    let show_temp = ctx.cfg.bool_key(SECTION, "show_temp", true);
    let show_pressure = ctx.cfg.bool_key(SECTION, "show_pressure", false);
    let edit = ctx.edit_mode;

    for (i, (lbl, idx)) in CORNERS.iter().enumerate() {
        let col_i = i % 2;
        let row_i = i / 2;
        let x = card.left() + pad + col_i as f32 * (cw + gap);
        let cy = grid_top + row_i as f32 * (ch + gap);
        let cell = Rect::from_min_size(Pos2::new(x, cy), Vec2::new(cw, ch));
        draw_dark_cell(ui, ctx.cfg, SECTION, cell, rad);

        label(
            ui,
            Pos2::new(x + 6.0, cy + 4.0 + ch * 0.11),
            Align2::LEFT_CENTER,
            lbl,
            (ch * 0.22).clamp(10.0, 16.0),
            header_c,
            true,
        );

        let cdata = &corners[*idx];
        let mut ty = cy + ch * 0.26;

        if show_wear {
            let bar = Rect::from_min_size(
                Pos2::new(x + 8.0, cy + ch - ch * 0.22),
                Vec2::new(cw - 16.0, ch * 0.14),
            );
            ui.painter().rect_filled(bar, CornerRadius::same(3), bar_bg);
            let val = if let Some(wear) = cdata.wear {
                let pct = (wear * 100.0).clamp(0.0, 100.0);
                let fill_w = bar.width() * pct / 100.0;
                let fcol = if pct <= warn { warn_c } else { wear_c };
                ui.painter().rect_filled(
                    Rect::from_min_size(bar.min, Vec2::new(fill_w, bar.height())),
                    CornerRadius::same(3),
                    fcol,
                );
                format!("{pct:.0}%")
            } else if edit {
                "—".into()
            } else {
                "--".into()
            };
            label(
                ui,
                Pos2::new(x + 6.0, ty + ch * 0.14),
                Align2::LEFT_CENTER,
                &val,
                (ch * 0.24).clamp(11.0, 18.0),
                text_c,
                true,
            );
            ty += ch * 0.30;
        }

        if show_temp {
            let ts = if let Some(temp) = cdata.temp {
                let t = ctx.cfg.conv_temp(temp);
                format!("{t:.0}°")
            } else if edit {
                "—".into()
            } else {
                "--".into()
            };
            label(
                ui,
                Pos2::new(x + 6.0, ty + ch * 0.11),
                Align2::LEFT_CENTER,
                &ts,
                (ch * 0.22).clamp(10.0, 15.0),
                muted,
                false,
            );
            ty += ch * 0.24;
        }

        if show_pressure {
            let ps = if let Some(pr) = cdata.pressure {
                format!("{pr:.0} kPa")
            } else if edit {
                "—".into()
            } else {
                "--".into()
            };
            label(
                ui,
                Pos2::new(x + 6.0, ty + ch * 0.10),
                Align2::LEFT_CENTER,
                &ps,
                (ch * 0.20).clamp(9.0, 13.0),
                muted,
                false,
            );
        }
    }
}
