use super::WidgetCtx;
use crate::chrome::{draw_card, draw_dark_cell, full_rect, label};
use egui::{Align2, Ui};

const SECTION: &str = "flags";

const SPEC: &[(&str, &str, &str, &str)] = &[
    ("yellow", "CAUTION", "flag_yellow", "flag_yellow_text"),
    ("black", "BLACK FLAG", "flag_black", "flag_black_text"),
    ("meatball", "MEATBALL", "flag_meatball", "flag_meatball_text"),
    ("furled", "WARNING", "flag_furled", "flag_furled_text"),
    ("dq", "DISQUALIFIED", "flag_dq", "flag_dq_text"),
    ("green", "GREEN", "flag_green", "flag_green_text"),
    ("white", "LAST LAP", "flag_white_bg", "flag_white_text"),
    ("red", "RED FLAG", "flag_red", "flag_red_text"),
    ("blue", "LET BY", "flag_blue", "flag_blue_text"),
    ("debris", "DEBRIS", "flag_debris", "flag_debris_text"),
    ("crossed", "HALFWAY", "flag_crossed", "flag_crossed_text"),
    ("checkered", "FINISH", "flag_checker_bg", "flag_checker_text"),
];

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let f = ctx.frame;
    if f.flag.is_none() && !ctx.edit_mode && !f.incident_warn {
        let _ = full_rect(ui);
        return;
    }
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = (rect.height() * 0.12).max(6.0);
    let inner = rect.shrink(pad);

    if f.flag.is_none() && f.incident_warn {
        draw_dark_cell(ui, ctx.cfg, SECTION, inner, (inner.height() * 0.34).min(22.0));
        let msg = f.secondary.as_deref().unwrap_or("Incident warning");
        label(
            ui,
            inner.center(),
            Align2::CENTER_CENTER,
            msg,
            inner.height() * 0.24,
            ctx.cfg.color(SECTION, "flag_furled", "#ffd23a"),
            true,
        );
        return;
    }

    let flag = f.flag.as_deref().unwrap_or("");
    if let Some((_, title, bgk, fgk)) = SPEC.iter().find(|(k, ..)| *k == flag) {
        let bg = ctx.cfg.color(SECTION, bgk, "#46df7a");
        let fg = ctx.cfg.color(SECTION, fgk, "#141414");
        let r = (inner.height() * 0.34).min(22.0);
        ui.painter()
            .rect_filled(inner, egui::CornerRadius::same(r as u8), bg);
        label(
            ui,
            inner.center(),
            Align2::CENTER_CENTER,
            title,
            (inner.height() * 0.32).clamp(14.0, 36.0),
            fg,
            true,
        );
        if let Some(ctx_line) = f.flag_context.as_deref() {
            label(
                ui,
                egui::pos2(inner.center().x, inner.bottom() - pad * 0.8),
                Align2::CENTER_BOTTOM,
                ctx_line,
                (inner.height() * 0.16).clamp(10.0, 16.0),
                fg,
                false,
            );
        }
    } else {
        draw_dark_cell(ui, ctx.cfg, SECTION, inner, (inner.height() * 0.34).min(22.0));
        let idle = ctx.cfg.str_key(SECTION, "idle_text", "TRACK CLEAR");
        label(
            ui,
            inner.center(),
            Align2::CENTER_CENTER,
            &idle,
            inner.height() * 0.26,
            ctx.cfg.color(SECTION, "idle_text", "#8b93a1"),
            false,
        );
    }
}
