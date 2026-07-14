//! Flags banner — plate + hatch/checker (Python `flags.py`).

use super::WidgetCtx;
use crate::chrome::{color_with_alpha, draw_card, draw_dark_cell, full_rect, label};
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};

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

fn text_width(ui: &Ui, text: &str, size: f32, bold: bool) -> f32 {
    let font = egui::FontId::proportional(size.max(1.0));
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font, Color32::WHITE));
    let w = galley.size().x;
    // Fake-bold widens slightly.
    if bold {
        w + 0.9
    } else {
        w
    }
}

fn draw_checker(painter: &egui::Painter, rect: Rect, color: Color32) {
    let rows = 3.0_f32;
    let sq = rect.height() / rows;
    let cols = (rect.width() / sq).ceil() as i32 + 2;
    let cell = color_with_alpha(color, 190);
    for ri in 0..3 {
        for ci in 0..cols {
            if (ri + ci) % 2 == 0 {
                let x = rect.left() + ci as f32 * sq;
                let y = rect.top() + ri as f32 * sq;
                painter.rect_filled(
                    Rect::from_min_size(
                        Pos2::new(x, y),
                        Vec2::new(sq.min(rect.right() - x), sq.min(rect.bottom() - y)),
                    ),
                    CornerRadius::ZERO,
                    cell,
                );
            }
        }
    }
}

fn draw_hatch(painter: &egui::Painter, rect: Rect, fg: Color32) {
    let hatch = color_with_alpha(fg, 64);
    let step = rect.height() * 0.5;
    let pen = Stroke::new((rect.height() * 0.10).max(2.0), hatch);
    let mut x = rect.left() - rect.height();
    while x < rect.right() + rect.height() {
        painter.line_segment(
            [
                Pos2::new(x, rect.bottom()),
                Pos2::new(x + rect.height(), rect.top()),
            ],
            pen,
        );
        x += step;
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let f = ctx.frame;
    let have_secondary = f
        .secondary
        .as_deref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    if f.flag.is_none() && !ctx.edit_mode && !f.incident_warn && !have_secondary {
        let _ = full_rect(ui);
        return;
    }
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = (rect.height() * 0.12).max(6.0);
    let inner = rect.shrink(pad);

    if f.flag.is_none() && f.incident_warn {
        draw_dark_cell(
            ui,
            ctx.cfg,
            SECTION,
            inner,
            (inner.height() * 0.34).min(22.0),
        );
        let msg = f.secondary.as_deref().unwrap_or("Incident warning");
        label(
            ui,
            inner.center(),
            Align2::CENTER_CENTER,
            msg,
            inner.height() * 0.24,
            ctx.cfg.color(SECTION, "flag_furled", "#caa23a"),
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
            .rect_filled(inner, CornerRadius::same(r as u8), bg);
        ui.painter().rect_stroke(
            inner,
            CornerRadius::same(r as u8),
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 45)),
            StrokeKind::Inside,
        );

        // Texture clipped to banner.
        {
            let painter = ui.painter().with_clip_rect(inner);
            if flag == "checkered" {
                draw_checker(&painter, inner, fg);
            } else {
                draw_hatch(&painter, inner, fg);
            }
        }

        let context = f
            .flag_context
            .as_deref()
            .or(f.secondary.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("");

        if !context.is_empty() {
            let mut title_sz = inner.height() * 0.22;
            let mut sub_sz = inner.height() * 0.15;
            let avail = inner.width() * 0.82;
            let mut tw = text_width(ui, title, title_sz, true)
                .max(text_width(ui, context, sub_sz, false));
            if tw > avail && tw > 0.0 {
                let scale = avail / tw;
                title_sz *= scale;
                sub_sz *= scale;
                tw = text_width(ui, title, title_sz, true)
                    .max(text_width(ui, context, sub_sz, false));
            }
            let plate_pad = inner.height() * 0.24;
            let plate_h = (inner.height() * 0.72).min(title_sz * 2.6 + sub_sz * 1.2);
            let plate = Rect::from_center_size(
                inner.center(),
                Vec2::new(tw + plate_pad * 2.0, plate_h),
            );
            ui.painter().rect_filled(
                plate,
                CornerRadius::same((plate_h * 0.5).round().clamp(0.0, 255.0) as u8),
                bg,
            );
            label(
                ui,
                Pos2::new(inner.center().x, inner.center().y - plate_h * 0.16),
                Align2::CENTER_CENTER,
                title,
                title_sz,
                fg,
                true,
            );
            label(
                ui,
                Pos2::new(inner.center().x, inner.center().y + plate_h * 0.18),
                Align2::CENTER_CENTER,
                context,
                sub_sz,
                color_with_alpha(fg, ((fg.a() as f32) * 0.88) as u8),
                false,
            );
        } else {
            let mut font_sz = inner.height() * 0.30;
            let avail = inner.width() * 0.82;
            let mut tw = text_width(ui, title, font_sz, true);
            if tw > avail && tw > 0.0 {
                font_sz *= avail / tw;
                tw = text_width(ui, title, font_sz, true);
            }
            let plate_pad = inner.height() * 0.28;
            let plate_h = (inner.height() * 0.62).min(font_sz * 1.9);
            let plate = Rect::from_center_size(
                inner.center(),
                Vec2::new(tw + plate_pad * 2.0, plate_h),
            );
            ui.painter().rect_filled(
                plate,
                CornerRadius::same((plate_h * 0.5).round().clamp(0.0, 255.0) as u8),
                bg,
            );
            label(
                ui,
                inner.center(),
                Align2::CENTER_CENTER,
                title,
                font_sz,
                fg,
                true,
            );
        }
    } else {
        // Idle plate (Python: dark cell + idle_text).
        let r = (inner.height() * 0.34).min(22.0);
        draw_dark_cell(ui, ctx.cfg, SECTION, inner, r);
        let idle = if let Some(sec) = f.secondary.as_deref().filter(|s| !s.is_empty()) {
            sec.to_string()
        } else {
            // Config string key shares the name `idle_text` with the color key
            // under `colors.idle_text` — str_key reads the section field.
            ctx.cfg.str_key(SECTION, "idle_text", "TRACK CLEAR")
        };
        label(
            ui,
            inner.center(),
            Align2::CENTER_CENTER,
            &idle,
            inner.height() * 0.26,
            ctx.cfg.color(SECTION, "idle_text", "#9fb0a4"),
            false,
        );
    }
}
