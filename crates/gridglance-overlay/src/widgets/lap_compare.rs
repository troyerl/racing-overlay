//! Lap compare — live delta, sparkline, turn losses.

use super::WidgetCtx;
use crate::chrome::{draw_dark_cell, full_rect, label, panel_card, panel_pad};
use egui::{Align2, Pos2, Rect, Shape, Stroke, Ui};

const SECTION: &str = "lap_compare";

fn signed_delta(d: f64) -> String {
    format!("{d:+.2}")
}

fn delta_color(ctx: &WidgetCtx<'_>, d: Option<f64>) -> egui::Color32 {
    match d {
        Some(v) if v < -0.005 => ctx.cfg.color(SECTION, "faster", "#46df7a"),
        Some(v) if v > 0.005 => ctx.cfg.color(SECTION, "slower", "#e23b3b"),
        _ => ctx.cfg.color(SECTION, "muted", "#8b93a1"),
    }
}

fn draw_spark(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    rect: Rect,
    spark: &[f32],
    markers: &[crate::telemetry::CompareMarker],
) {
    use crate::telemetry::MarkerKind;
    draw_dark_cell(ui, ctx.cfg, SECTION, rect, 5.0);
    let mid = rect.center().y;
    ui.painter().line_segment(
        [Pos2::new(rect.left(), mid), Pos2::new(rect.right(), mid)],
        Stroke::new(1.0_f32, ctx.cfg.color(SECTION, "grid", "#ffffff1f")),
    );
    if spark.is_empty() {
        return;
    }
    let peak = spark
        .iter()
        .map(|v| v.abs())
        .fold(0.15_f32, f32::max)
        .max(0.15);
    let n = spark.len().max(1) as f32;
    let mut pts = Vec::with_capacity(spark.len());
    for (i, &v) in spark.iter().enumerate() {
        let x = rect.left() + (i as f32 / (n - 1.0).max(1.0)) * rect.width();
        let y = mid - (v / peak) * (rect.height() * 0.5 - 2.0);
        pts.push(Pos2::new(x, y));
    }
    ui.painter().add(Shape::line(
        pts,
        Stroke::new(1.8_f32, ctx.cfg.color(SECTION, "graph_line", "#ffd23a")),
    ));
    let show_brake = ctx.cfg.bool_key(SECTION, "show_brake_markers", true);
    let show_lift = ctx.cfg.bool_key(SECTION, "show_lift_markers", true);
    let brake_col = ctx.cfg.color(SECTION, "marker_brake", "#ff5050");
    let lift_col = ctx.cfg.color(SECTION, "marker_lift", "#3aa0ff");
    for m in markers {
        let col = match m.kind {
            MarkerKind::Brake if show_brake => brake_col,
            MarkerKind::Lift if show_lift => lift_col,
            _ => continue,
        };
        let x = rect.left() + m.pct.clamp(0.0, 1.0) * rect.width();
        ui.painter().line_segment(
            [
                Pos2::new(x, rect.top() + 2.0),
                Pos2::new(x, rect.bottom() - 2.0),
            ],
            Stroke::new(1.2_f32, col),
        );
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let elegant = crate::chrome::is_elegant(ctx.cfg, SECTION);
    let pad = if elegant {
        (card.height() * 0.05).max(6.0)
    } else {
        panel_pad(card.height()).max(7.0)
    };
    let h = card.height();
    let w = card.width();
    let iw = w - 2.0 * pad;
    let scale = ctx.cfg.text_scale(SECTION);
    let view = &ctx.frame.lap_compare;
    let mut y = card.top() + pad * 0.35;

    // Elegant: ref label + delta on one compact band (no stacked hero).
    let ref_label = if view.ref_label.is_empty() {
        "VS BEST"
    } else {
        view.ref_label.as_str()
    };
    let delta = view.delta;
    if elegant {
        let hh = (h * 0.14).clamp(22.0, 32.0);
        label(
            ui,
            Pos2::new(card.left() + pad, y + hh * 0.5),
            Align2::LEFT_CENTER,
            ref_label,
            10.0,
            crate::chrome::color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200),
            false,
        );
        label(
            ui,
            Pos2::new(card.right() - pad, y + hh * 0.5),
            Align2::RIGHT_CENTER,
            &delta.map(signed_delta).unwrap_or_else(|| "--.--".into()),
            (hh * 0.55 * scale).clamp(14.0, 22.0),
            delta_color(ctx, delta),
            true,
        );
        y += hh + 2.0;
    } else {
        let hh = h * 0.12;
        let band = Rect::from_min_size(Pos2::new(card.left(), y), egui::vec2(card.width(), hh));
        ui.painter().rect_filled(
            band,
            egui::CornerRadius {
                nw: radius as u8,
                ne: radius as u8,
                sw: 0,
                se: 0,
            },
            ctx.cfg.color(SECTION, "header_bg", "#0b0e12bb"),
        );
        label(
            ui,
            Pos2::new(card.left() + pad, y + hh * 0.5),
            Align2::LEFT_CENTER,
            ref_label,
            (hh * 0.55 * scale).clamp(11.0, 18.0),
            ctx.cfg.color(SECTION, "accent", "#e23b3b"),
            true,
        );
        y += hh;

        let bh = h * 0.20;
        label(
            ui,
            Pos2::new(card.center().x, y + bh * 0.45),
            Align2::CENTER_CENTER,
            &delta.map(signed_delta).unwrap_or_else(|| "--.--".into()),
            (bh * 0.72 * scale).clamp(18.0, 40.0),
            delta_color(ctx, delta),
            true,
        );
        y += bh;
    }

    // Sparkline
    if ctx.cfg.bool_key(SECTION, "show_graph", true) && !view.spark.is_empty() {
        let gh = h * (if elegant { 0.10 } else { 0.16 });
        let graph = Rect::from_min_size(Pos2::new(card.left() + pad, y), egui::vec2(iw, gh));
        draw_spark(ui, ctx, graph, &view.spark, &view.markers);
        y += gh + if elegant { 2.0 } else { pad * 0.4 };
    }

    // Turn losses
    let turns = &view.turns;
    if turns.is_empty() {
        label(
            ui,
            Pos2::new(card.center().x, (y + card.bottom() - pad) * 0.5),
            Align2::CENTER_CENTER,
            "Drive a clean lap to set your benchmark",
            (h * 0.06 * scale).clamp(11.0, 16.0),
            ctx.cfg.color(SECTION, "muted", "#8b93a1"),
            false,
        );
        return;
    }

    let body_h = (card.bottom() - pad - y).max(18.0);
    let max_turns = ctx.cfg.f64_key(SECTION, "max_turns", 6.0).max(1.0) as usize;
    let shown: Vec<_> = turns.iter().take(max_turns).collect();
    let rh = if elegant {
        (body_h / shown.len().max(1) as f32).clamp(16.0, 22.0)
    } else {
        let max_rh = (body_h * 0.35).max(18.0);
        (body_h / shown.len().max(1) as f32).clamp(18.0, max_rh)
    };
    let mut row_y = y;
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    for (i, (name, loss)) in shown.iter().enumerate() {
        let row = Rect::from_min_size(Pos2::new(card.left() + pad, row_y), egui::vec2(iw, rh));
        if !elegant && ctx.cfg.bool_key(SECTION, "alt_row_shading", true) && i % 2 == 1 {
            ui.painter().rect_filled(
                row,
                egui::CornerRadius::ZERO,
                ctx.cfg.color(SECTION, "row_alt", "#ffffff0a"),
            );
        }
        let chip_w = row.width() * (if elegant { 0.16 } else { 0.18 });
        let chip = Rect::from_min_size(
            Pos2::new(row.left(), row.top() + rh * 0.12),
            egui::vec2(chip_w, rh * 0.76),
        );
        if elegant {
            ui.painter().rect_filled(
                chip,
                egui::CornerRadius::same(8),
                crate::chrome::color_with_alpha(ctx.cfg.color(SECTION, "cell_dark", "#0b0e12"), 80),
            );
        } else {
            draw_dark_cell(ui, ctx.cfg, SECTION, chip, 5.0);
        }
        label(
            ui,
            chip.center(),
            Align2::CENTER_CENTER,
            name,
            (rh * 0.32 * scale).clamp(10.0, 16.0),
            text,
            true,
        );
        let d = *loss as f64;
        label(
            ui,
            Pos2::new(chip.right() + row.width() * 0.04, row.center().y),
            Align2::LEFT_CENTER,
            &signed_delta(d),
            (rh * 0.34 * scale).clamp(11.0, 18.0),
            delta_color(ctx, Some(d)),
            true,
        );
        let tip = if d.abs() < 0.02 {
            "on pace"
        } else if d > 0.0 {
            "time lost"
        } else {
            "time gained"
        };
        label(
            ui,
            Pos2::new(row.right() - 4.0, row.center().y),
            Align2::RIGHT_CENTER,
            tip,
            (rh * 0.26 * scale).clamp(9.0, 14.0),
            if elegant {
                crate::chrome::color_with_alpha(muted, 180)
            } else {
                muted
            },
            false,
        );
        row_y += rh;
    }
}
