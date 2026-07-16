//! Pit board — requested pit services and fast-repair status.
//! Ports `overlay/widgets/pit_board.py`.

use super::WidgetCtx;
use crate::chrome::{draw_card, draw_dark_cell, draw_section_header, full_rect, label, panel_pad};
use crate::telemetry::PitService;
use egui::{Align2, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};

const SECTION: &str = "pit_board";

fn preview_services() -> Vec<PitService> {
    vec![
        PitService {
            key: "lf_tire".into(),
            label: "LF tire".into(),
            checked: true,
        },
        PitService {
            key: "fuel".into(),
            label: "Fuel".into(),
            checked: true,
        },
        PitService {
            key: "rf_tire".into(),
            label: "RF tire".into(),
            checked: false,
        },
    ]
}

fn cell_radius(row_h: f32) -> f32 {
    (row_h * 0.22).clamp(4.0, 8.0)
}

fn resolve_row_height(body_h: f32, row_count: usize, panel_h: f32, max_frac: f32) -> f32 {
    let n = row_count.max(1) as f32;
    let mut row_h = body_h / n;
    if max_frac > 0.0 {
        row_h = row_h.min(panel_h * max_frac);
    }
    row_h
}

fn draw_status_chip(ui: &mut Ui, ctx: &WidgetCtx<'_>, rect: Rect, text: &str, active: bool) {
    let r = (rect.height() * 0.35).min(10.0);
    let bg = if active {
        ctx.cfg.color(SECTION, "active_bg", "#3d8bfd")
    } else {
        ctx.cfg.color(SECTION, "cell_dark", "#0b0e12")
    };
    let fg = if active {
        ctx.cfg.color(SECTION, "active_text", "#ffffff")
    } else {
        ctx.cfg.color(SECTION, "muted", "#8b93a1")
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(r as u8), bg);
    label(
        ui,
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        (rect.height() * 0.48).clamp(10.0, 22.0),
        fg,
        true,
    );
}

fn draw_row_divider(ui: &mut Ui, ctx: &WidgetCtx<'_>, x: f32, y: f32, w: f32) {
    let mut edge = ctx.cfg.color(SECTION, "border", "#ffffff28");
    let a = ((edge.a() as f32) * 0.55).max(30.0) as u8;
    edge = crate::chrome::color_with_alpha(edge, a);
    ui.painter().line_segment(
        [Pos2::new(x, y), Pos2::new(x + w, y)],
        Stroke::new(1.0_f32, edge),
    );
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let f = ctx.frame;
    let mut services = f.pit_services.clone();
    if services.is_empty() && ctx.edit_mode {
        services = preview_services();
    }
    if services.is_empty() && !ctx.edit_mode && !f.pit_active {
        let _ = full_rect(ui);
        return;
    }

    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let h = card.height();
    let mut y = card.top() + pad;
    let data_bold = true;

    if f.pit_active && ctx.cfg.bool_key(SECTION, "show_pit_banner", true) {
        let banner_h = (h * 0.12).max(22.0);
        let banner = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, banner_h),
        );
        let text = ctx
            .cfg
            .str_key(SECTION, "pit_banner_text", "PIT STOP ACTIVE");
        draw_status_chip(ui, ctx, banner, &text, true);
        y += banner_h + pad * 0.4;
    }

    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        let hh = (h * 0.10).max(20.0);
        let hdr = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, hh),
        );
        let title = ctx.cfg.str_key(SECTION, "title", "PIT SERVICES");
        let radius_top = if f.pit_active { 0.0 } else { radius };
        draw_section_header(ui, ctx.cfg, SECTION, hdr, &title, radius_top);
        y += hh + pad * 0.25;
    }

    let n = services.len().max(1);
    let extras_h = h * 0.14;
    let body_h = (card.bottom() - pad - y - extras_h).max(n as f32 * 18.0);
    let fixed_rh = ctx.cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    let mut row_h = if fixed_rh > 0.0 {
        fixed_rh
    } else {
        let max_frac = ctx.cfg.f64_key(SECTION, "max_row_height_frac", 0.0) as f32;
        resolve_row_height(body_h, n, h, max_frac)
    };
    row_h = row_h.max(18.0);
    let rad = cell_radius(row_h);
    let mark_w = (row_h * 0.45).max(18.0);
    let checked_c = ctx.cfg.color(SECTION, "checked", "#70df7a");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let text_c = ctx.cfg.color(SECTION, "text", "#f4f6f8");

    for (i, svc) in services.iter().enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, row_h - 2.0),
        );
        draw_dark_cell(ui, ctx.cfg, SECTION, row, rad);
        let mark = if svc.checked { "✓" } else { "–" };
        label(
            ui,
            Pos2::new(row.left() + 8.0 + mark_w * 0.5, row.center().y),
            Align2::CENTER_CENTER,
            mark,
            (row_h * 0.42).clamp(11.0, 20.0),
            if svc.checked { checked_c } else { muted },
            data_bold,
        );
        label(
            ui,
            Pos2::new(row.left() + mark_w + 4.0, row.center().y),
            Align2::LEFT_CENTER,
            &svc.label,
            (row_h * 0.42).clamp(11.0, 18.0),
            if svc.checked { text_c } else { muted },
            false,
        );
        y += row_h;
        if ctx.cfg.bool_key(SECTION, "row_dividers", true) && i + 1 < services.len() {
            draw_row_divider(
                ui,
                ctx,
                card.left() + pad,
                y - 2.0,
                card.width() - 2.0 * pad,
            );
        }
    }

    let mut extras: Vec<String> = Vec::new();
    if ctx.cfg.bool_key(SECTION, "show_compound", true) {
        if let Some(c) = f.pit_compound {
            extras.push(format!("Set {c}"));
        }
    }
    if let Some(fuel_l) = f.pit_fuel_add_l.or(f.pit_fuel_to_add) {
        let v = ctx.cfg.conv_fuel(fuel_l);
        extras.push(format!("+{v:.1} {}", ctx.cfg.fuel_unit()));
    }
    if ctx.cfg.bool_key(SECTION, "show_fast_repairs", true) {
        if let Some(r) = f.pit_repairs {
            extras.push(format!("Repairs {r}"));
        }
    }
    if !extras.is_empty() {
        label(
            ui,
            Pos2::new(card.left() + pad, y + extras_h * 0.5),
            Align2::LEFT_CENTER,
            &extras.join("  •  "),
            (row_h * 0.38).clamp(10.0, 14.0),
            muted,
            false,
        );
    }
}
