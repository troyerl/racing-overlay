//! ERS / hybrid energy gauge widget.
//! Ports `overlay/widgets/ers_hybrid.py`.

use super::WidgetCtx;
use crate::chrome::{
    draw_dark_cell, full_rect, label, panel_card, panel_content_pad, panel_title,
};
use egui::{Align2, CornerRadius, Pos2, Rect, Ui, Vec2};

const SECTION: &str = "ers_hybrid";

fn cell_radius(row_h: f32) -> f32 {
    (row_h * 0.22).clamp(4.0, 8.0)
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
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_content_pad(ctx.cfg, SECTION, card.height());
    let h = card.height();
    let f = ctx.frame;
    let edit = ctx.edit_mode;
    let elegant = crate::chrome::is_elegant(ctx.cfg, SECTION);

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

    let data_bold = !elegant;
    let mut y = card.top() + pad;
    if elegant {
        if ctx.cfg.bool_key(SECTION, "show_title", true) {
            let muted = crate::chrome::color_with_alpha(
                ctx.cfg.color(SECTION, "muted", "#8b93a1"),
                200,
            );
            label(
                ui,
                Pos2::new(card.left() + pad, y + 6.0),
                Align2::LEFT_CENTER,
                &ctx.cfg.str_key(SECTION, "title", "HYBRID"),
                10.0,
                muted,
                false,
            );
            y += 14.0;
        }
    } else {
        y = panel_title(ui, ctx.cfg, SECTION, card, radius, y, pad, "HYBRID");
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
        let bar_h = if elegant {
            ((card.bottom() - pad - y) * 0.45).clamp(22.0, 36.0)
        } else {
            (h * 0.28).max(28.0)
        };
        let bar = Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            Vec2::new(card.width() - 2.0 * pad, bar_h),
        );
        if elegant {
            ui.painter().rect_filled(
                bar,
                CornerRadius::same(12),
                crate::chrome::color_with_alpha(ctx.cfg.color(SECTION, "text", "#f4f6f8"), 16),
            );
        } else {
            draw_dark_cell(ui, ctx.cfg, SECTION, bar, cell_radius(bar_h));
        }
        let inner = if elegant { bar.shrink(8.0) } else { bar.shrink(6.0) };
        ui.painter().rect_filled(
            inner,
            CornerRadius::same(if elegant { 8 } else { 4 }),
            ctx.cfg.color(SECTION, "gauge_bg", "#ffffff18"),
        );
        if let Some(p) = pct {
            let target = (p as f32 / 100.0).clamp(0.0, 1.0);
            let id = egui::Id::new("ers_battery_anim");
            let mut st = ui
                .ctx()
                .data_mut(|d| d.get_temp::<(f32, f64)>(id).unwrap_or((target, 0.0)));
            let dt = crate::chrome::anim_dt(ctx.mono_secs, &mut st.1);
            st.0 = crate::chrome::ease(st.0, target, dt, 0.14);
            if crate::chrome::still_easing(st.0, target, 0.005) {
                *ctx.panel_animating = true;
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(1));
            }
            let fill_t = st.0;
            ui.ctx().data_mut(|d| d.insert_temp(id, st));
            let fw = inner.width() * fill_t;
            ui.painter().rect_filled(
                Rect::from_min_size(inner.min, Vec2::new(fw, inner.height())),
                CornerRadius::same(if elegant { 8 } else { 4 }),
                ctx.cfg.color(SECTION, "gauge_fill", "#70df7a"),
            );
        }
        if elegant {
            let lbl = pct
                .map(|p| format!("{p:.0}%"))
                .unwrap_or_else(|| "—".into());
            let bat_lab = ctx.cfg.str_key(SECTION, "label_battery", "ERS");
            label(
                ui,
                Pos2::new(bar.left() + 8.0, bar.center().y),
                Align2::LEFT_CENTER,
                &bat_lab,
                11.0,
                crate::chrome::color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 220),
                false,
            );
            label(
                ui,
                Pos2::new(bar.right() - 8.0, bar.center().y),
                Align2::RIGHT_CENTER,
                &lbl,
                (bar_h * 0.48).clamp(13.0, 18.0),
                ctx.cfg.color(SECTION, "text", "#f4f6f8"),
                true,
            );
        } else {
            let lbl = if let Some(p) = pct {
                format!("{p:.0}%")
            } else if f.have_hybrid {
                "—".into()
            } else {
                "--".into()
            };
            let metric = Rect::from_min_max(
                Pos2::new(bar.left() + 8.0, bar.top()),
                Pos2::new(bar.right() - 8.0, bar.bottom()),
            );
            let bat_lab = ctx.cfg.str_key(SECTION, "label_battery", "ERS");
            draw_metric_row(ui, ctx, metric, &bat_lab, &lbl, data_bold);
        }
        y += bar_h + if elegant { 6.0 } else { pad * 0.4 };
    }

    let chip_h = if elegant {
        20.0
    } else {
        (h * 0.12).max(18.0)
    };
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
