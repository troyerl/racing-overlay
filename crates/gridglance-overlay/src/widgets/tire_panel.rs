//! Tire panel — 4-corner wear, temp, and optional cold pressure.
//! Ports `overlay/widgets/tire_panel.py`.

use super::WidgetCtx;
use crate::chrome::{
    color_with_alpha, draw_dark_cell, full_rect, label, panel_card, panel_content_pad, panel_title,
};
use crate::telemetry::TireCorner;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Ui, Vec2};

const SECTION: &str = "tire_panel";
/// Display label + TelemetryFrame index (FL/FR/RL/RR ↔ lf/rf/lr/rr).
const CORNERS: [(&str, usize); 4] = [("FL", 0), ("FR", 1), ("RL", 2), ("RR", 3)];
/// Nominal cool→hot band in °C for temp wash (converted via cfg units for display only).
const TEMP_COLD_C: f32 = 60.0;
const TEMP_HOT_C: f32 = 105.0;

fn cell_radius(row_h: f32) -> f32 {
    (row_h * 0.22).clamp(4.0, 8.0)
}

fn has_corner_data(corners: &[TireCorner; 4]) -> bool {
    corners
        .iter()
        .any(|c| c.wear.is_some() || c.temp.is_some() || c.pressure.is_some())
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    Color32::from_rgba_unmultiplied(
        (a.r() as f32 * inv + b.r() as f32 * t).round() as u8,
        (a.g() as f32 * inv + b.g() as f32 * t).round() as u8,
        (a.b() as f32 * inv + b.b() as f32 * t).round() as u8,
        (a.a() as f32 * inv + b.a() as f32 * t).round() as u8,
    )
}

fn temp_wash(cfg: &crate::config::OverlayConfig, temp_c: f32) -> Color32 {
    let cold = cfg.color(SECTION, "temp_cold", "#5aa9ff");
    let mid = cfg.color(SECTION, "wear", "#46df7a");
    let hot = cfg.color(SECTION, "temp_hot", "#ff9416");
    let t = ((temp_c - TEMP_COLD_C) / (TEMP_HOT_C - TEMP_COLD_C)).clamp(0.0, 1.0);
    if t < 0.5 {
        lerp_color(cold, mid, t * 2.0)
    } else {
        lerp_color(mid, hot, (t - 0.5) * 2.0)
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let corners = &ctx.frame.tire_corners;
    if !has_corner_data(corners) && !ctx.edit_mode {
        let _ = full_rect(ui);
        return;
    }
    if crate::chrome::is_elegant(ctx.cfg, SECTION) {
        paint_elegant(ui, ctx, corners);
    } else {
        paint_data(ui, ctx, corners);
    }
}

fn paint_data(ui: &mut Ui, ctx: &mut WidgetCtx<'_>, corners: &[TireCorner; 4]) {
    let rect = full_rect(ui);
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_content_pad(ctx.cfg, SECTION, card.height());
    let mut y = card.top() + pad;
    y = panel_title(ui, ctx.cfg, SECTION, card, radius, y, pad, "TIRES");

    let grid_top = y;
    let iw = card.width() - 2.0 * pad;
    let ih = card.bottom() - pad - y;
    let gap = (iw * 0.04).max(4.0);
    let cw = (iw - gap) / 2.0;
    let ch = (ih - gap) / 2.0;
    paint_tire_grid(
        ui,
        ctx,
        corners,
        card.left() + pad,
        grid_top,
        cw,
        ch,
        gap,
        false,
    );
}

/// Compact 2×2: label+temp on one line, thin wear bar — same data, less bulk.
fn paint_elegant(ui: &mut Ui, ctx: &mut WidgetCtx<'_>, corners: &[TireCorner; 4]) {
    let rect = full_rect(ui);
    let (card, _radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = (card.width().min(card.height()) * 0.05).clamp(6.0, 10.0);
    let mut y = card.top() + pad;
    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        let muted = color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200);
        label(
            ui,
            Pos2::new(card.left() + pad, y + 6.0),
            Align2::LEFT_CENTER,
            &ctx.cfg.str_key(SECTION, "title", "TIRES"),
            10.0,
            muted,
            false,
        );
        y += 14.0;
    }
    let iw = card.width() - 2.0 * pad;
    let ih = card.bottom() - pad - y;
    let gap = (iw * 0.03).max(4.0);
    let cw = (iw - gap) / 2.0;
    let ch = (ih - gap) / 2.0;
    paint_tire_grid(ui, ctx, corners, card.left() + pad, y, cw, ch, gap, true);
}

fn paint_tire_grid(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    corners: &[TireCorner; 4],
    origin_x: f32,
    grid_top: f32,
    cw: f32,
    ch: f32,
    gap: f32,
    elegant: bool,
) {
    let warn = ctx.cfg.f64_key(SECTION, "warn_wear_pct", 30.0) as f32;
    let rad = if elegant {
        (cw.min(ch) * 0.18).clamp(8.0, 14.0)
    } else {
        cell_radius(cw.min(ch) * 0.4)
    };
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
        let x = origin_x + col_i as f32 * (cw + gap);
        let cy = grid_top + row_i as f32 * (ch + gap);
        let cell = Rect::from_min_size(Pos2::new(x, cy), Vec2::new(cw, ch));
        if elegant {
            ui.painter().rect_filled(
                cell,
                CornerRadius::same(rad.round().clamp(0.0, 255.0) as u8),
                color_with_alpha(text_c, 14),
            );
        } else {
            draw_dark_cell(ui, ctx.cfg, SECTION, cell, rad);
        }

        if let Some(temp) = corners[*idx].temp {
            let wash = temp_wash(ctx.cfg, temp);
            let hotness = ((temp - TEMP_COLD_C) / (TEMP_HOT_C - TEMP_COLD_C)).clamp(0.0, 1.0);
            if hotness > 0.55 {
                let r = rad.round().clamp(0.0, 255.0) as u8;
                ui.painter().rect_filled(
                    Rect::from_min_size(Pos2::new(x, cy), Vec2::new(3.0, ch)),
                    CornerRadius {
                        nw: r,
                        sw: r,
                        ne: 0,
                        se: 0,
                    },
                    color_with_alpha(wash, 200),
                );
            }
        }

        if !elegant {
            label(
                ui,
                Pos2::new(x + 8.0, cy + 4.0 + ch * 0.11),
                Align2::LEFT_CENTER,
                lbl,
                (ch * 0.22).clamp(10.0, 16.0),
                header_c,
                true,
            );
        }

        let cdata = &corners[*idx];
        if elegant {
            // Dense: FL + temp on one line, wear as thin bottom meter.
            let top_y = cy + ch * 0.28;
            label(
                ui,
                Pos2::new(x + 6.0, top_y),
                Align2::LEFT_CENTER,
                lbl,
                10.0,
                color_with_alpha(header_c, 180),
                false,
            );
            if show_temp {
                let (ts, tcol) = if let Some(temp) = cdata.temp {
                    let t = ctx.cfg.conv_temp(temp);
                    (format!("{t:.0}°"), temp_wash(ctx.cfg, temp))
                } else if edit {
                    ("—".into(), muted)
                } else {
                    ("--".into(), muted)
                };
                label(
                    ui,
                    Pos2::new(x + cw - 6.0, top_y),
                    Align2::RIGHT_CENTER,
                    &ts,
                    (ch * 0.36).clamp(12.0, 18.0),
                    tcol,
                    true,
                );
            }
            if show_pressure {
                let ps = cdata
                    .pressure
                    .map(|pr| format!("{pr:.0}"))
                    .unwrap_or_else(|| if edit { "—" } else { "--" }.into());
                label(
                    ui,
                    Pos2::new(cell.center().x, cy + ch * 0.55),
                    Align2::CENTER_CENTER,
                    &ps,
                    10.0,
                    muted,
                    false,
                );
            }
            if show_wear {
                let bar_h = (ch * 0.14).clamp(3.0, 6.0);
                let bar = Rect::from_min_size(
                    Pos2::new(x + 6.0, cy + ch - bar_h - 5.0),
                    Vec2::new(cw - 12.0, bar_h),
                );
                ui.painter().rect_filled(
                    bar,
                    CornerRadius::same((bar.height() * 0.5) as u8),
                    color_with_alpha(text_c, 20),
                );
                if let Some(wear) = cdata.wear {
                    let pct = (wear * 100.0).clamp(0.0, 100.0);
                    let fill_w = bar.width() * pct / 100.0;
                    let fcol = if pct <= warn { warn_c } else { wear_c };
                    ui.painter().rect_filled(
                        Rect::from_min_size(bar.min, Vec2::new(fill_w, bar.height())),
                        CornerRadius::same((bar.height() * 0.5) as u8),
                        fcol,
                    );
                    label(
                        ui,
                        Pos2::new(bar.right(), bar.center().y - bar_h - 2.0),
                        Align2::RIGHT_CENTER,
                        &format!("{pct:.0}%"),
                        9.0,
                        color_with_alpha(muted, 200),
                        false,
                    );
                }
            }
            continue;
        }

        let mut ty = cy + ch * 0.28;
        if show_temp {
            let (ts, tcol) = if let Some(temp) = cdata.temp {
                let t = ctx.cfg.conv_temp(temp);
                (format!("{t:.0}°"), temp_wash(ctx.cfg, temp))
            } else if edit {
                ("—".into(), muted)
            } else {
                ("--".into(), muted)
            };
            label(
                ui,
                Pos2::new(x + 6.0, ty + ch * 0.10),
                Align2::LEFT_CENTER,
                &ts,
                (ch * 0.26).clamp(11.0, 17.0),
                tcol,
                true,
            );
            ty += ch * 0.28;
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
                Pos2::new(x + 6.0, ty + ch * 0.08),
                Align2::LEFT_CENTER,
                &ps,
                (ch * 0.18).clamp(9.0, 13.0),
                muted,
                false,
            );
        }
        if show_wear {
            let bar = Rect::from_min_size(
                Pos2::new(x + 8.0, cy + ch - ch * 0.26),
                Vec2::new(cw - 16.0, ch * 0.16),
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
                bar.center(),
                Align2::CENTER_CENTER,
                &val,
                (bar.height() * 0.85).clamp(9.0, 14.0),
                text_c,
                true,
            );
        }
    }
}
