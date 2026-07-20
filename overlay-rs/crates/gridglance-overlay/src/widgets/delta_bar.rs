use super::WidgetCtx;
use crate::chrome::{anim_dt, draw_card, ease, full_rect, label, panel_pad, still_easing};
use egui::{Align2, Pos2, Stroke, Ui};

const SECTION: &str = "delta_bar";
const EASE_TAU: f32 = 0.10;

#[derive(Clone, Default)]
struct DeltaAnim {
    fill: f32,
    last_secs: f64,
}

fn signed_delta(d: Option<f64>) -> String {
    match d {
        Some(v) => format!("{v:+.2}"),
        None => "--.--".into(),
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let delta = ctx.frame.delta;
    let have = delta.is_some();
    let rng = ctx.cfg.f64_key(SECTION, "range", 1.0).max(0.001);
    let target = delta.map(|d| (d / rng).clamp(-1.0, 1.0)).unwrap_or(0.0) as f32;

    let id = egui::Id::new("delta_bar_anim");
    let mut anim = ui
        .ctx()
        .data_mut(|d| d.get_temp::<DeltaAnim>(id).unwrap_or_default());
    let dt = anim_dt(ctx.mono_secs, &mut anim.last_secs);
    anim.fill = ease(anim.fill, target, dt, EASE_TAU);
    let animating = still_easing(anim.fill, target, 0.002);
    *ctx.panel_animating = animating;
    if animating {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1));
    }
    let eased = anim.fill;
    ui.ctx().data_mut(|d| d.insert_temp(id, anim));

    let show_val = ctx.cfg.bool_key(SECTION, "show_value", true);
    if show_val {
        let tcol = if !have || delta.unwrap().abs() < 0.005 {
            ctx.cfg.color(SECTION, "muted", "#8b93a1")
        } else if delta.unwrap() < 0.0 {
            ctx.cfg.color(SECTION, "faster", "#46df7a")
        } else {
            ctx.cfg.color(SECTION, "slower", "#e23b3b")
        };
        label(
            ui,
            Pos2::new(rect.center().x, rect.top() + pad + rect.height() * 0.22),
            Align2::CENTER_CENTER,
            &signed_delta(delta),
            rect.height() * 0.46,
            tcol,
            true,
        );
    }

    let bar = if show_val {
        egui::Rect::from_min_size(
            Pos2::new(rect.left() + pad, rect.top() + rect.height() * 0.62),
            egui::vec2(rect.width() - 2.0 * pad, rect.height() * 0.24),
        )
    } else {
        egui::Rect::from_min_size(
            Pos2::new(rect.left() + pad, rect.top() + rect.height() * 0.40),
            egui::vec2(rect.width() - 2.0 * pad, rect.height() * 0.20),
        )
    };
    let r = bar.height() / 2.0;
    ui.painter().rect_filled(
        bar,
        egui::CornerRadius::same(r as u8),
        ctx.cfg.color(SECTION, "track", "#262b34"),
    );
    if eased.abs() > 0.001 {
        let cx = bar.center().x;
        let fill_w = (bar.width() * 0.5) * eased.abs();
        let fill = if eased < 0.0 {
            egui::Rect::from_min_max(
                Pos2::new(cx - fill_w, bar.top()),
                Pos2::new(cx, bar.bottom()),
            )
        } else {
            egui::Rect::from_min_max(
                Pos2::new(cx, bar.top()),
                Pos2::new(cx + fill_w, bar.bottom()),
            )
        };
        let col = if eased < 0.0 {
            ctx.cfg.color(SECTION, "faster", "#46df7a")
        } else {
            ctx.cfg.color(SECTION, "slower", "#e23b3b")
        };
        ui.painter()
            .rect_filled(fill, egui::CornerRadius::same(r as u8), col);
    }
    // Center tick — Python uses `center` color and thin stroke.
    let tick_w = (rect.height() * 0.02).max(1.5);
    ui.painter().line_segment(
        [
            Pos2::new(bar.center().x, bar.top()),
            Pos2::new(bar.center().x, bar.bottom()),
        ],
        Stroke::new(tick_w, ctx.cfg.color(SECTION, "center", "#8b93a1")),
    );
}
