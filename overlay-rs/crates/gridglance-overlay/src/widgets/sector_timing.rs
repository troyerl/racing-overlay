//! Sector / lap timing widget — current, last, best + sector cells.

use super::WidgetCtx;
use crate::chrome::{draw_card, draw_dark_cell, full_rect, label, panel_pad};
use crate::telemetry::SectorCell;
use egui::{Align2, CornerRadius, Pos2, Rect, Stroke, Ui};

const SECTION: &str = "sector_timing";

/// Format seconds as `M:SS.mmm` (Python `clock`).
fn fmt_clock(sec: Option<f64>) -> String {
    match sec {
        Some(s) if s > 0.0 => {
            let m = (s / 60.0).floor() as i32;
            let rem = s - m as f64 * 60.0;
            format!("{m}:{rem:06.3}")
        }
        _ => "--:--.---".into(),
    }
}

/// Short sector split display (Python `sec`).
fn fmt_sec(sec: Option<f64>) -> String {
    match sec {
        Some(s) if s > 0.0 => format!("{s:.1}"),
        _ => "--.-".into(),
    }
}

fn signed_delta(d: f64, places: usize) -> String {
    if places == 2 {
        format!("{d:+.2}")
    } else {
        let sign = if d < 0.0 { '-' } else { '+' };
        format!("{sign}{:.prec$}", d.abs(), prec = places)
    }
}

fn cell_radius(h: f32) -> f32 {
    (h * 0.18).clamp(4.0, 10.0)
}

fn draw_edge_band(ui: &mut Ui, cfg: &crate::config::OverlayConfig, rect: Rect) {
    ui.painter().rect_filled(
        rect,
        CornerRadius::ZERO,
        cfg.color(SECTION, "header_bg", "#0b0e12bb"),
    );
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, cfg.color(SECTION, "border", "#ffffff28")),
    );
}

fn draw_metric_pair(ui: &mut Ui, ctx: &WidgetCtx<'_>, rect: Rect, lab: &str, value: &str) {
    let scale = ctx.cfg.text_scale(SECTION);
    label(
        ui,
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        lab,
        (rect.height() * 0.38 * scale).clamp(9.0, 14.0),
        ctx.cfg.color(SECTION, "muted", "#8b93a1"),
        true,
    );
    label(
        ui,
        Pos2::new(rect.right() - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        (rect.height() * 0.46 * scale).clamp(11.0, 18.0),
        ctx.cfg.color(SECTION, "text", "#f4f6f8"),
        true,
    );
}

fn paint_sector_cell(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    rect: Rect,
    num: usize,
    cell: &SectorCell,
    show_delta: bool,
) {
    let scale = ctx.cfg.text_scale(SECTION);
    let inner = rect.shrink(1.0);
    let rad = cell_radius(rect.height());
    let status = cell.status.as_str();
    let bg_key = match status {
        "best" => ("sec_best", "#6b39c8"),
        "running" => ("sec_running", "#1d3a2a"),
        "done" => ("sec_done", "#22303f"),
        _ => ("sec_idle", "#161a20"),
    };
    if status == "idle" || status.is_empty() {
        draw_dark_cell(ui, ctx.cfg, SECTION, inner, rad);
    } else {
        ui.painter().rect_filled(
            inner,
            CornerRadius::same(rad as u8),
            ctx.cfg.color(SECTION, bg_key.0, bg_key.1),
        );
        ui.painter().rect_stroke(
            inner,
            CornerRadius::same(rad as u8),
            Stroke::new(1.0_f32, ctx.cfg.color(SECTION, "cell_border", "#ffffff28")),
            egui::StrokeKind::Inside,
        );
    }
    if cell.active {
        ui.painter().rect_stroke(
            inner,
            CornerRadius::same(rad as u8),
            Stroke::new(
                1.6_f32,
                ctx.cfg.color(SECTION, "sec_running_edge", "#46df7a"),
            ),
            egui::StrokeKind::Inside,
        );
    }
    let sec_text = ctx.cfg.color(SECTION, "sec_text", "#dfe3ea");
    label(
        ui,
        Pos2::new(rect.center().x, rect.top() + rect.height() * 0.28),
        Align2::CENTER_CENTER,
        &format!("S{num}"),
        (rect.height() * 0.26 * scale).clamp(9.0, 16.0),
        sec_text,
        false,
    );
    label(
        ui,
        Pos2::new(rect.center().x, rect.center().y + rect.height() * 0.12),
        Align2::CENTER_CENTER,
        &fmt_sec(cell.time),
        (rect.height() * 0.34 * scale).clamp(11.0, 20.0),
        sec_text,
        true,
    );
    if show_delta {
        if let Some(d) = cell.delta.filter(|d| d.abs() >= 0.005) {
            let dc = if d > 0.0 {
                ctx.cfg.color(SECTION, "slower", "#e23b3b")
            } else {
                ctx.cfg.color(SECTION, "faster", "#46df7a")
            };
            label(
                ui,
                Pos2::new(rect.center().x, rect.bottom() - rect.height() * 0.16),
                Align2::CENTER_CENTER,
                &signed_delta(d, 2),
                (rect.height() * 0.22 * scale).clamp(8.0, 14.0),
                dc,
                false,
            );
        }
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, _radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let h = card.height();
    let scale = ctx.cfg.text_scale(SECTION);
    let snap = &ctx.frame.sectors_ui;
    let show_pred = ctx.cfg.bool_key(SECTION, "show_predicted_lap", false);
    let show_delta = ctx.cfg.bool_key(SECTION, "show_sector_delta", false);

    let iw = card.width() - 2.0 * pad;
    let cur_h = if show_pred { h * 0.26 } else { h * 0.30 };
    label(
        ui,
        Pos2::new(card.center().x, card.top() + pad + cur_h * 0.5),
        Align2::CENTER_CENTER,
        &fmt_clock(snap.cur_lap),
        (cur_h * 0.72 * scale).clamp(18.0, 42.0),
        ctx.cfg.color(SECTION, "text", "#f4f6f8"),
        true,
    );

    if show_pred {
        if let Some(pred) = snap.predicted_lap.filter(|t| *t > 0.0) {
            label(
                ui,
                Pos2::new(card.center().x, card.top() + pad + cur_h * 0.95),
                Align2::CENTER_CENTER,
                &format!("Pred {}", fmt_clock(Some(pred))),
                (h * 0.09 * scale).clamp(10.0, 16.0),
                ctx.cfg.color(SECTION, "muted", "#8b93a1"),
                false,
            );
        }
    }

    let sub_top = card.top() + pad + if show_pred { h * 0.34 } else { h * 0.30 };
    let fixed_rh = ctx.cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    let mut sub_h = if fixed_rh > 0.0 { fixed_rh } else { h * 0.18 };
    let max_frac = ctx.cfg.f64_key(SECTION, "max_row_height_frac", 0.0) as f32;
    if max_frac > 0.0 {
        sub_h = sub_h.min(h * max_frac);
    }
    sub_h = sub_h.max(18.0);
    let sub = Rect::from_min_size(Pos2::new(card.left() + pad, sub_top), egui::vec2(iw, sub_h));
    draw_edge_band(ui, ctx.cfg, sub);
    let half = sub.width() * 0.5;
    draw_metric_pair(
        ui,
        ctx,
        Rect::from_min_size(sub.left_top(), egui::vec2(half, sub.height())),
        "LAST",
        &fmt_clock(snap.last_lap),
    );
    draw_metric_pair(
        ui,
        ctx,
        Rect::from_min_size(
            Pos2::new(sub.left() + half, sub.top()),
            egui::vec2(half, sub.height()),
        ),
        "BEST",
        &fmt_clock(snap.best_lap),
    );

    let sectors = &snap.sectors;
    if sectors.is_empty() {
        return;
    }
    let top = sub.bottom() + h * 0.04;
    let ch = (card.bottom() - pad - top).max(24.0);
    let gap = iw * 0.03;
    let n = sectors.len() as f32;
    let cw = (iw - gap * (n - 1.0).max(0.0)) / n.max(1.0);
    let mut x = card.left() + pad;
    for (i, cell) in sectors.iter().enumerate() {
        let cell_rect = Rect::from_min_size(Pos2::new(x, top), egui::vec2(cw, ch));
        paint_sector_cell(ui, ctx, cell_rect, i + 1, cell, show_delta);
        x += cw + gap;
    }
}
