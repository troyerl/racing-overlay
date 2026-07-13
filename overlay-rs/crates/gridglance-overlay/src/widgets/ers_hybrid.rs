//! ERS / hybrid energy gauge widget.
//! Ports `overlay/widgets/ers_hybrid.py`.

use super::WidgetCtx;
use crate::chrome::{
    draw_card, draw_dark_cell, draw_section_header, full_rect, label, panel_pad,
};
use egui::{Align2, CornerRadius, Pos2, Rect, Ui, Vec2};

const SECTION: &str = "ers_hybrid";

fn cell_radius(row_h: f32) -> f32 {
    (row_h * 0.22).clamp(4.0, 8.0)
}

fn fmt_kj(joules: Option<f32>) -> String {
    let Some(j) = joules else {
        return "—".into();
    };
    let kj = j / 1000.0;
    if kj.abs() >= 1000.0 {
        format!("{:.1} MJ", kj / 1000.0)
    } else {
        format!("{kj:.0} kJ")
    }
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
        (rect.height() * 0.48).clamp(10.0, 18.0),
        fg,
        true,
    );
}

fn draw_metric_row(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    rect: Rect,
    lab: &str,
    value: &str,
    data_bold: bool,
) {
    let lh = rect.height();
    let lw = rect.width();
    label(
        ui,
        Pos2::new(rect.left(), rect.center().y),
        Align2::LEFT_CENTER,
        lab,
        (lh * 0.38).clamp(10.0, 16.0),
        ctx.cfg.color(SECTION, "header", "#c5ccd6"),
        true,
    );
    label(
        ui,
        Pos2::new(rect.left() + lw * 0.22, rect.center().y),
        Align2::LEFT_CENTER,
        value,
        (lh * 0.42).clamp(11.0, 18.0),
        ctx.cfg.color(SECTION, "text", "#f4f6f8"),
        data_bold,
    );
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let h = card.height();
    let f = ctx.frame;
    let edit = ctx.edit_mode;

    if !f.have_hybrid && !edit {
        let empty = ctx.cfg.str_key(SECTION, "empty_text", "No hybrid data");
        label(
            ui,
            rect.center(),
            Align2::CENTER_CENTER,
            &empty,
            (h * 0.22).clamp(12.0, 20.0),
            ctx.cfg.color(SECTION, "muted", "#8b93a1"),
            false,
        );
        return;
    }

    let data_bold = true;
    let mut y = card.top() + pad;

    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        let hh = (h * 0.10).max(20.0);
        let hdr = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, hh),
        );
        let title = ctx.cfg.str_key(SECTION, "title", "HYBRID");
        draw_section_header(ui, ctx.cfg, SECTION, hdr, &title, radius);
        y += hh + pad * 0.25;
    }

    if ctx.cfg.bool_key(SECTION, "show_battery", true) {
        let mut pct = if f.have_hybrid {
            f.ers_battery_pct.or(f.ers_pct)
        } else {
            None
        };
        if edit && pct.is_none() {
            pct = Some(62.0);
        }
        let bar_h = (h * 0.28).max(28.0);
        let bar = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, bar_h),
        );
        draw_dark_cell(ui, ctx.cfg, SECTION, bar, cell_radius(bar_h));
        let inner = bar.shrink(6.0);
        ui.painter().rect_filled(
            inner,
            CornerRadius::same(4),
            ctx.cfg.color(SECTION, "gauge_bg", "#ffffff18"),
        );
        if let Some(p) = pct {
            let fw = inner.width() * (p / 100.0).clamp(0.0, 1.0);
            ui.painter().rect_filled(
                Rect::from_min_size(inner.min, Vec2::new(fw, inner.height())),
                CornerRadius::same(4),
                ctx.cfg.color(SECTION, "gauge_fill", "#70df7a"),
            );
        }
        let mut lbl = if f.have_hybrid {
            // No dedicated battery_j field yet — mirror missing joules as em dash.
            fmt_kj(None)
        } else {
            "-- kJ".into()
        };
        if let Some(p) = pct {
            lbl = format!("{p:.0}%  {lbl}");
        }
        let metric = Rect::from_min_max(
            Pos2::new(bar.left() + 8.0, bar.top()),
            Pos2::new(bar.right() - 8.0, bar.bottom()),
        );
        let bat_lab = ctx.cfg.str_key(SECTION, "label_battery", "ERS");
        draw_metric_row(ui, ctx, metric, &bat_lab, &lbl, data_bold);
        y += bar_h + pad * 0.4;
    }

    if ctx.cfg.bool_key(SECTION, "show_lap_energy", true) {
        let line: String = if edit {
            "-- / -- lap".into()
        } else {
            "—".into()
        };
        let row_h = (h * 0.14).max(20.0);
        let row = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, row_h),
        );
        draw_dark_cell(ui, ctx.cfg, SECTION, row, cell_radius(row_h));
        let metric = Rect::from_min_max(
            Pos2::new(row.left() + 8.0, row.top()),
            Pos2::new(row.right() - 8.0, row.bottom()),
        );
        let lap_lab = ctx.cfg.str_key(SECTION, "label_lap", "LAP");
        draw_metric_row(ui, ctx, metric, &lap_lab, &line, false);
        y += row_h + pad * 0.35;
    }

    let chip_h = (h * 0.12).max(18.0);
    let chip_w = (card.width() - 2.0 * pad - pad * 0.5) / 2.0;
    let mut x = card.left() + pad;
    if ctx.cfg.bool_key(SECTION, "show_boost", true) {
        let lab = ctx.cfg.str_key(SECTION, "label_boost", "BOOST");
        draw_status_chip(
            ui,
            ctx,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(chip_w, chip_h)),
            &lab,
            f.ers_boost_active,
        );
        x += chip_w + pad * 0.5;
    }
    if ctx.cfg.bool_key(SECTION, "show_p2p", true) {
        let lab = ctx.cfg.str_key(SECTION, "label_p2p", "P2P");
        draw_status_chip(
            ui,
            ctx,
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(chip_w, chip_h)),
            &lab,
            f.ers_p2p_active,
        );
    }
}
