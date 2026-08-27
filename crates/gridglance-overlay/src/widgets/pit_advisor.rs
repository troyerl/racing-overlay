//! Pit engineer panel (Python `pit_advisor.py` parity + strategy meta).

use super::WidgetCtx;
use crate::chrome::{
    color_with_alpha, full_rect, is_elegant, label, panel_card, panel_content_pad, panel_title,
};
use crate::telemetry::{strategy_context_line, strategy_loss_line, strategy_meta_line};
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Ui, Vec2};

const SECTION: &str = "pit_advisor";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let advice = ctx.frame.pit_advice.clone().unwrap_or_default();
    let only_actionable = ctx.cfg.bool_key(SECTION, "show_only_when_actionable", true);
    if only_actionable && !advice.actionable && !ctx.edit_mode {
        let _ = full_rect(ui);
        return;
    }

    let rect = full_rect(ui);
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_content_pad(ctx.cfg, SECTION, card.height());
    let elegant = is_elegant(ctx.cfg, SECTION);
    let mut y = card.top() + pad;
    let text_w = card.width() - 2.0 * pad;

    if elegant {
        if ctx.cfg.bool_key(SECTION, "show_title", true) {
            label(
                ui,
                Pos2::new(card.left() + pad, y + 6.0),
                Align2::LEFT_CENTER,
                &ctx.cfg.str_key(SECTION, "title", "PIT ENGINEER"),
                10.0,
                color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200),
                false,
            );
            y += 14.0;
        }
    } else {
        y = panel_title(ui, ctx.cfg, SECTION, card, radius, y, pad, "PIT ENGINEER");
    }

    let label_txt = if advice.label.is_empty() && ctx.edit_mode {
        "PIT NEXT LAP".into()
    } else {
        advice.label.clone()
    };
    let rationale = if advice.rationale.is_empty() && ctx.edit_mode {
        "Pit next lap to pass #12 — 6.2s ahead, stop costs ~28s".into()
    } else {
        advice.rationale.clone()
    };
    let rec = if advice.rec.is_empty() {
        "pit_next_lap".into()
    } else {
        advice.rec.clone()
    };
    let active = !matches!(rec.as_str(), "stay_out" | "hold");

    let chip_h = if elegant {
        24.0
    } else {
        (card.height() * 0.16).max(22.0)
    };
    let chip = Rect::from_min_size(Pos2::new(card.left() + pad, y), Vec2::new(text_w, chip_h));
    let chip_bg = if active {
        ctx.cfg.color(SECTION, "chip_active", "#ff9416")
    } else {
        ctx.cfg.color(SECTION, "chip_idle", "#333a42")
    };
    ui.painter().rect_filled(
        chip,
        CornerRadius::same(if elegant { 8 } else { 6 }),
        chip_bg,
    );
    label(
        ui,
        chip.center(),
        Align2::CENTER_CENTER,
        &label_txt,
        (chip_h * 0.42).clamp(11.0, 16.0),
        Color32::WHITE,
        true,
    );
    y += chip_h + if elegant { 6.0 } else { 8.0 };

    label(
        ui,
        Pos2::new(card.left() + pad, y),
        Align2::LEFT_TOP,
        &rationale,
        if elegant { 11.0 } else { 13.0 },
        if elegant {
            color_with_alpha(ctx.cfg.color(SECTION, "text", "#f4f6f8"), 210)
        } else {
            ctx.cfg.color(SECTION, "text", "#f4f6f8")
        },
        false,
    );
    y += if elegant { 22.0 } else { 26.0 };

    let muted = color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200);
    let meta = if ctx.edit_mode && advice.stop_window.is_none() {
        Some("Stop L24–26 · Add 18.4L · Fuel+tires".into())
    } else {
        strategy_meta_line(&advice).or(advice.secondary.clone())
    };
    if let Some(line) = meta {
        label(
            ui,
            Pos2::new(card.left() + pad, y),
            Align2::LEFT_TOP,
            &line,
            10.0,
            muted,
            false,
        );
        y += 14.0;
    }

    let loss = if ctx.edit_mode && advice.pit_loss_s.is_none() {
        Some("Loss ~22s · Clear merge".into())
    } else {
        strategy_loss_line(&advice)
    };
    if let Some(line) = loss {
        label(
            ui,
            Pos2::new(card.left() + pad, y),
            Align2::LEFT_TOP,
            &line,
            10.0,
            muted,
            false,
        );
        y += 14.0;
    }

    let ctx_line = if ctx.edit_mode && advice.fcy_note.is_none() && advice.lap_down_note.is_none() {
        Some("Pit risks lap down · Pace drop 0.08s/L · FCY ~12%".into())
    } else {
        strategy_context_line(&advice)
    };
    if let Some(line) = ctx_line {
        label(
            ui,
            Pos2::new(card.left() + pad, y),
            Align2::LEFT_TOP,
            &line,
            10.0,
            muted,
            false,
        );
    }
}
