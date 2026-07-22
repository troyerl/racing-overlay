//! Radio tower — current team-radio speaker (Python parity).

use super::WidgetCtx;
use crate::chrome::{color_with_alpha, full_rect, panel_card, panel_content_pad, panel_title};
use crate::config::parse_color_str;
use crate::icons;
use crate::telemetry::RadioSpeaker;
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Ui, Vec2};

const SECTION: &str = "radio_tower";

fn preview_row() -> RadioSpeaker {
    RadioSpeaker {
        position: 2,
        car_number: "10".into(),
        name: "Preview Driver".into(),
        active: true,
        is_player: false,
        is_pro: false,
        group_icon: "league".into(),
        group_color: "#5bb8ff".into(),
    }
}

fn driver_part(row: &RadioSpeaker, show_name: bool, show_car_number: bool) -> String {
    let name = row.name.trim();
    let num = row.car_number.trim();
    match (
        show_name && !name.is_empty(),
        show_car_number && !num.is_empty(),
    ) {
        (true, true) => format!("{name} #{num}"),
        (true, false) => name.to_string(),
        (false, true) => {
            if num.starts_with('#') {
                num.to_string()
            } else {
                format!("#{num}")
            }
        }
        (false, false) => String::new(),
    }
}

fn row_text(row: &RadioSpeaker, show_position: bool, show_name: bool, show_num: bool) -> String {
    let driver = driver_part(row, show_name, show_num);
    let has_pos = show_position && row.position > 0;
    if has_pos && !driver.is_empty() {
        format!("{} - {driver}", row.position)
    } else if has_pos {
        row.position.to_string()
    } else {
        driver
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    #[derive(Clone, Default)]
    struct RadioHold {
        row: RadioSpeaker,
        until: f64,
    }

    const HOLD_SECS: f64 = 0.45;
    let id = egui::Id::new("radio_tower_hold");
    let now = ctx.mono_secs;
    let mut hold = ui.ctx().data(|d| d.get_temp::<RadioHold>(id));

    let row = if let Some(r) = &ctx.frame.radio {
        hold = Some(RadioHold {
            row: r.clone(),
            until: now + HOLD_SECS,
        });
        Some(r.clone())
    } else if ctx.edit_mode {
        Some(preview_row())
    } else {
        hold.as_ref()
            .filter(|h| now <= h.until)
            .map(|h| h.row.clone())
    };

    match &hold {
        Some(h) if ctx.frame.radio.is_some() || now <= h.until => {
            ui.ctx().data_mut(|d| d.insert_temp(id, h.clone()));
        }
        _ => {
            ui.ctx().data_mut(|d| {
                d.remove_temp::<RadioHold>(id);
            });
        }
    }

    let Some(row) = row else {
        let _ = full_rect(ui);
        return; // silent: paint nothing (transparent)
    };
    *ctx.panel_animating = true;

    let rect = full_rect(ui);
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_content_pad(ctx.cfg, SECTION, card.height());
    let mut y = card.top() + pad;
    y = panel_title(ui, ctx.cfg, SECTION, card, radius, y, pad, "RADIO");

    let show_pos = ctx.cfg.bool_key(SECTION, "show_position", true);
    let show_num = ctx.cfg.bool_key(SECTION, "show_car_number", true);
    let show_name = ctx.cfg.bool_key(SECTION, "show_name", true);
    let highlight = ctx.cfg.bool_key(SECTION, "highlight_player", true);

    let body_h = (card.bottom() - pad - y).max(18.0);
    let fixed_rh = ctx.cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    let elegant = crate::chrome::is_elegant(ctx.cfg, SECTION);
    let row_h = if elegant {
        body_h.clamp(20.0, 28.0)
    } else if fixed_rh > 0.0 {
        fixed_rh
    } else {
        body_h
    }
    .max(18.0);

    let content_w = card.width() - 2.0 * pad;
    let text_size = row_h * (if elegant { 0.42 } else { 0.46 }) * ctx.cfg.text_scale(SECTION);
    let x0 = card.left() + pad;
    let row_y = y;
    let row_rect = Rect::from_min_size(Pos2::new(x0, row_y), Vec2::new(content_w, row_h - 2.0));

    if row.active {
        draw_speaking_accent(ui, ctx, row_rect);
    } else if row.is_player && highlight {
        ui.painter().rect_filled(
            row_rect,
            if elegant {
                CornerRadius::same(8)
            } else {
                CornerRadius::ZERO
            },
            ctx.cfg.color(SECTION, "player_row", "#ffffff14"),
        );
    }

    let text = row_text(&row, show_pos, show_name, show_num);
    if text.is_empty() {
        return;
    }

    let stripe_w = ((row_h - 2.0) * 0.09).max(3.5);
    let text_inset = stripe_w + (row_h * 0.12).max(6.0);
    let mut text_x = x0 + text_inset;
    let mut text_w = (content_w - text_inset).max(0.0);

    if let Some((glyph, badge_col)) = badge_glyph(ctx, &row) {
        let ic_px = (row_h - 2.0) * 0.32;
        let gap = ((row_h - 2.0) * 0.08).max(2.0);
        let gw = ic_px * 0.85; // approximate advance
        ui.painter().text(
            Pos2::new(text_x, row_rect.center().y),
            Align2::LEFT_CENTER,
            &glyph,
            icons::font_id(ic_px),
            badge_col,
        );
        text_x += gw + gap;
        text_w = (text_w - (gw + gap)).max(0.0);
    }

    let text_col = if row.is_pro && !row.active {
        ctx.cfg.color(SECTION, "pro_name", "#f5c542")
    } else {
        ctx.cfg.color(SECTION, "text", "#d8d8d8")
    };
    // Elide by clipping painter to the text slot.
    let text_rect = Rect::from_min_size(Pos2::new(text_x, y), Vec2::new(text_w, row_h - 2.0));
    let painter = ui.painter().with_clip_rect(text_rect);
    painter.text(
        Pos2::new(text_x, text_rect.center().y),
        Align2::LEFT_CENTER,
        &text,
        egui::FontId::new(text_size, egui::FontFamily::Proportional),
        text_col,
    );
}

fn draw_speaking_accent(ui: &mut Ui, ctx: &WidgetCtx<'_>, rect: Rect) {
    let accent = ctx.cfg.color(SECTION, "badge_speaking_bg", "#22c55e");
    let h = rect.height();
    let stripe_w = (h * 0.09).max(3.5);
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(rect.left(), rect.top() + h * 0.10),
            Vec2::new(stripe_w, h * 0.80),
        ),
        CornerRadius::same(2),
        accent,
    );
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, color_with_alpha(accent, 38));
}

fn badge_glyph(ctx: &WidgetCtx<'_>, row: &RadioSpeaker) -> Option<(String, Color32)> {
    if row.is_pro {
        if let Some(g) = icons::glyph("pro_driver") {
            return Some((g, ctx.cfg.color(SECTION, "pro_badge", "#f5c542")));
        }
    }
    if !row.group_icon.is_empty() {
        if let Some(g) = icons::glyph(&row.group_icon) {
            let col = if row.group_color.is_empty() {
                parse_color_str("#5bb8ff")
            } else {
                parse_color_str(&row.group_color)
            };
            return Some((g, col));
        }
    }
    None
}
