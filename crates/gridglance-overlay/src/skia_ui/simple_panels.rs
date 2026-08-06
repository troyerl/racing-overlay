//! Phase A "simple" panel painters — parity with the egui
//! `widgets/{delta_bar,flags,radio_tower,pit_advisor,pace_caution,system_panel}.rs`
//! layouts, redrawn with Skia `Canvas` primitives.

use super::canvas::{Canvas, TextAlign};
use super::chrome::{
    anim_dt, draw_dark_cell, ease, is_elegant, panel_card, panel_content_pad, panel_pad,
    panel_title, section_color, still_easing, text_at,
};
use super::types::{FontSpec, Rect, Rgba};
use crate::config::OverlayConfig;
use crate::state::MapAuthoring;
use crate::telemetry::{RadioSpeaker, TelemetryFrame};

fn bounds(c: &mut Canvas) -> Rect {
    Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32)
}

fn text_w(c: &Canvas, size: f32, bold: bool, text: &str) -> f32 {
    c.measure_text(
        text,
        if bold {
            FontSpec::bold(size)
        } else {
            FontSpec::new(size)
        },
    )
}

// ============================== Delta bar ==============================

const DELTA_BAR_SECTION: &str = "delta_bar";
const DELTA_EASE_TAU: f32 = 0.10;

#[derive(Clone, Debug, Default)]
pub struct DeltaBarAnim {
    pub fill: f32,
    pub last_secs: f64,
}

fn signed_delta(d: Option<f64>) -> String {
    match d {
        Some(v) => format!("{v:+.2}"),
        None => "--.--".into(),
    }
}

/// Paint the delta bar; returns true while the fill is still easing.
pub fn paint_delta_bar(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    anim: &mut DeltaBarAnim,
    mono_secs: f64,
    _edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let section = DELTA_BAR_SECTION;
    let rect = bounds(c);

    let delta = frame.delta;
    let have = delta.is_some();
    let rng = cfg.f64_key(section, "range", 1.0).max(0.001);
    let target = delta.map(|d| (d / rng).clamp(-1.0, 1.0)).unwrap_or(0.0) as f32;
    let dt = anim_dt(mono_secs, &mut anim.last_secs);
    anim.fill = ease(anim.fill, target, dt, DELTA_EASE_TAU);
    let animating = still_easing(anim.fill, target, 0.002);
    let eased = anim.fill;

    if is_elegant(cfg, section) {
        paint_delta_elegant(c, cfg, section, rect, eased, delta, have);
    } else {
        paint_delta_data(c, cfg, section, rect, eased, delta, have);
    }
    animating
}

fn paint_delta_elegant(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    eased: f32,
    delta: Option<f64>,
    have: bool,
) {
    let pad = (rect.height() * 0.12).clamp(3.0, 8.0);
    let bar = Rect::from_xywh(
        rect.left() + pad,
        rect.top() + pad,
        rect.width() - 2.0 * pad,
        (rect.height() - 2.0 * pad).max(10.0),
    );
    let r = (bar.height() * 0.35).clamp(4.0, 12.0);
    c.fill_rect(
        bar,
        section_color(cfg, section, "track", "#262b34").with_alpha(160),
        r,
    );
    draw_delta_fill(c, cfg, section, bar, eased, r);
    c.line(
        bar.center().0,
        bar.top() + 2.0,
        bar.center().0,
        bar.bottom() - 2.0,
        section_color(cfg, section, "center", "#8b93a1"),
        1.5,
    );
    if cfg.bool_key(section, "show_value", true) {
        let tcol = if !have || delta.unwrap().abs() < 0.005 {
            section_color(cfg, section, "muted", "#8b93a1")
        } else {
            Rgba::rgb(244, 246, 248)
        };
        text_at(
            c,
            bar.center().0,
            bar.center().1,
            &signed_delta(delta),
            (bar.height() * 0.55).clamp(11.0, 22.0),
            tcol,
            true,
            TextAlign::Center,
        );
    }
}

fn paint_delta_data(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    eased: f32,
    delta: Option<f64>,
    have: bool,
) {
    panel_card(c, cfg, section, rect);
    let pad = panel_pad(rect.height());

    let show_val = cfg.bool_key(section, "show_value", true);
    if show_val {
        let tcol = if !have || delta.unwrap().abs() < 0.005 {
            section_color(cfg, section, "muted", "#8b93a1")
        } else if delta.unwrap() < 0.0 {
            section_color(cfg, section, "faster", "#46df7a")
        } else {
            section_color(cfg, section, "slower", "#e23b3b")
        };
        text_at(
            c,
            rect.center().0,
            rect.top() + pad + rect.height() * 0.22,
            &signed_delta(delta),
            rect.height() * 0.46,
            tcol,
            true,
            TextAlign::Center,
        );
    }

    let bar = if show_val {
        Rect::from_xywh(
            rect.left() + pad,
            rect.top() + rect.height() * 0.62,
            rect.width() - 2.0 * pad,
            rect.height() * 0.24,
        )
    } else {
        Rect::from_xywh(
            rect.left() + pad,
            rect.top() + rect.height() * 0.40,
            rect.width() - 2.0 * pad,
            rect.height() * 0.20,
        )
    };
    let r = bar.height() * 0.5;
    c.fill_rect(bar, section_color(cfg, section, "track", "#262b34"), r);
    draw_delta_fill(c, cfg, section, bar, eased, r);
    let tick_w = (rect.height() * 0.02).max(1.5);
    c.line(
        bar.center().0,
        bar.top(),
        bar.center().0,
        bar.bottom(),
        section_color(cfg, section, "center", "#8b93a1"),
        tick_w,
    );
}

fn draw_delta_fill(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    bar: Rect,
    eased: f32,
    r: f32,
) {
    if eased.abs() <= 0.001 {
        return;
    }
    let cx = bar.center().0;
    let fill_w = bar.width() * 0.5 * eased.abs();
    let fill = if eased < 0.0 {
        Rect::from_ltrb(cx - fill_w, bar.top(), cx, bar.bottom())
    } else {
        Rect::from_ltrb(cx, bar.top(), cx + fill_w, bar.bottom())
    };
    let col = if eased < 0.0 {
        section_color(cfg, section, "faster", "#46df7a")
    } else {
        section_color(cfg, section, "slower", "#e23b3b")
    };
    c.fill_rect(fill, col, r);
}

// ================================ Flags =================================

const FLAGS_SECTION: &str = "flags";

const FLAG_SPEC: &[(&str, &str, &str, &str)] = &[
    ("yellow", "CAUTION", "flag_yellow", "flag_yellow_text"),
    ("black", "BLACK FLAG", "flag_black", "flag_black_text"),
    (
        "meatball",
        "MEATBALL",
        "flag_meatball",
        "flag_meatball_text",
    ),
    ("furled", "WARNING", "flag_furled", "flag_furled_text"),
    ("dq", "DISQUALIFIED", "flag_dq", "flag_dq_text"),
    ("green", "GREEN", "flag_green", "flag_green_text"),
    ("white", "FINAL NEXT", "flag_white_bg", "flag_white_text"),
    ("red", "RED FLAG", "flag_red", "flag_red_text"),
    ("blue", "LET BY", "flag_blue", "flag_blue_text"),
    ("debris", "DEBRIS", "flag_debris", "flag_debris_text"),
    ("crossed", "HALFWAY", "flag_crossed", "flag_crossed_text"),
    (
        "checkered",
        "FINISH",
        "flag_checker_bg",
        "flag_checker_text",
    ),
];

#[derive(Clone, Debug, Default)]
pub struct FlagsAnim {
    pub opacity: f32,
    pub last_secs: f64,
}

fn draw_checker(c: &mut Canvas, rect: Rect, color: Rgba) {
    let rows = 3.0_f32;
    let sq = rect.height() / rows;
    let cols = (rect.width() / sq).ceil() as i32 + 2;
    let cell = color.with_alpha(190);
    for ri in 0..3 {
        for ci in 0..cols {
            if (ri + ci) % 2 == 0 {
                let x = rect.left() + ci as f32 * sq;
                let y = rect.top() + ri as f32 * sq;
                c.fill_rect(
                    Rect::from_xywh(x, y, sq.min(rect.right() - x), sq.min(rect.bottom() - y)),
                    cell,
                    0.0,
                );
            }
        }
    }
}

fn draw_hatch(c: &mut Canvas, rect: Rect, fg: Rgba) {
    let hatch = fg.with_alpha(64);
    let step = rect.height() * 0.5;
    let pen = (rect.height() * 0.10).max(2.0);
    let mut x = rect.left() - rect.height();
    while x < rect.right() + rect.height() {
        c.line(x, rect.bottom(), x + rect.height(), rect.top(), hatch, pen);
        x += step;
    }
}

/// Paint the flags banner; returns true while the fade in/out is still easing.
pub fn paint_flags(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    anim: &mut FlagsAnim,
    mono_secs: f64,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let section = FLAGS_SECTION;
    let f = frame;
    let have_secondary = f
        .secondary
        .as_deref()
        .map(|s| !s.is_empty())
        .unwrap_or(false);
    let visible = f.flag.is_some() || edit_mode || f.incident_warn || have_secondary;
    let dt = anim_dt(mono_secs, &mut anim.last_secs);
    let target = if visible { 1.0 } else { 0.0 };
    anim.opacity = ease(anim.opacity, target, dt, 0.12);
    let opacity = anim.opacity;
    let animating = still_easing(opacity, target, 0.02);
    if opacity < 0.01 {
        return animating;
    }

    let rect = bounds(c);
    let elegant = is_elegant(cfg, section);
    let op = |col: Rgba| col.mul_alpha(opacity);

    if !elegant {
        panel_card(c, cfg, section, rect);
    }
    let pad = if elegant {
        (rect.height() * 0.05).max(3.0)
    } else {
        (rect.height() * 0.12).max(6.0)
    };
    let inner = rect.inset(pad, pad);

    if f.flag.is_none() && f.incident_warn {
        draw_dark_cell(c, cfg, section, inner, (inner.height() * 0.34).min(22.0));
        let msg = f.secondary.as_deref().unwrap_or("Incident warning");
        text_at(
            c,
            inner.center().0,
            inner.center().1,
            msg,
            inner.height() * 0.24,
            op(section_color(cfg, section, "flag_furled", "#caa23a")),
            true,
            TextAlign::Center,
        );
        return animating;
    }

    let flag = f.flag.as_deref().unwrap_or("");
    if let Some((_, title, bgk, fgk)) = FLAG_SPEC.iter().find(|(k, ..)| *k == flag) {
        let bg = op(section_color(cfg, section, bgk, "#46df7a"));
        let fg = op(section_color(cfg, section, fgk, "#141414"));
        let r = (inner.height() * 0.34).min(22.0);
        c.fill_rect(inner, bg, r);
        c.stroke_rect(
            inner,
            Rgba::new(255, 255, 255, 45).mul_alpha(opacity),
            r,
            1.0,
        );

        c.clip_rect(inner, |c| {
            if flag == "checkered" {
                draw_checker(c, inner, fg);
            } else {
                draw_hatch(c, inner, fg);
            }
        });

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
            let mut tw = text_w(c, title_sz, true, title).max(text_w(c, sub_sz, false, context));
            if tw > avail && tw > 0.0 {
                let scale = avail / tw;
                title_sz *= scale;
                sub_sz *= scale;
                tw = text_w(c, title_sz, true, title).max(text_w(c, sub_sz, false, context));
            }
            let _ = tw;
            let plate_pad = inner.height() * 0.24;
            let plate_h = (inner.height() * 0.72).min(title_sz * 2.6 + sub_sz * 1.2);
            let plate = Rect::from_xywh(
                inner.center().0 - (tw + plate_pad * 2.0) * 0.5,
                inner.center().1 - plate_h * 0.5,
                tw + plate_pad * 2.0,
                plate_h,
            );
            c.fill_rect(plate, bg, plate_h * 0.5);
            text_at(
                c,
                inner.center().0,
                inner.center().1 - plate_h * 0.16,
                title,
                title_sz,
                fg,
                true,
                TextAlign::Center,
            );
            text_at(
                c,
                inner.center().0,
                inner.center().1 + plate_h * 0.18,
                context,
                sub_sz,
                fg.mul_alpha(0.88),
                false,
                TextAlign::Center,
            );
        } else {
            let mut font_sz = inner.height() * 0.30;
            let avail = inner.width() * 0.82;
            let mut tw = text_w(c, font_sz, true, title);
            if tw > avail && tw > 0.0 {
                font_sz *= avail / tw;
                tw = text_w(c, font_sz, true, title);
            }
            let plate_pad = inner.height() * 0.28;
            let plate_h = (inner.height() * 0.62).min(font_sz * 1.9);
            let plate = Rect::from_xywh(
                inner.center().0 - (tw + plate_pad * 2.0) * 0.5,
                inner.center().1 - plate_h * 0.5,
                tw + plate_pad * 2.0,
                plate_h,
            );
            c.fill_rect(plate, bg, plate_h * 0.5);
            text_at(
                c,
                inner.center().0,
                inner.center().1,
                title,
                font_sz,
                fg,
                true,
                TextAlign::Center,
            );
        }
    } else {
        let r = (inner.height() * 0.34).min(22.0);
        draw_dark_cell(c, cfg, section, inner, r);
        let idle = if let Some(sec) = f.secondary.as_deref().filter(|s| !s.is_empty()) {
            sec.to_string()
        } else {
            cfg.str_key(section, "idle_text", "TRACK CLEAR")
        };
        text_at(
            c,
            inner.center().0,
            inner.center().1,
            &idle,
            inner.height() * 0.26,
            op(section_color(cfg, section, "idle_text", "#9fb0a4")),
            false,
            TextAlign::Center,
        );
    }

    animating
}

// ============================== Radio tower ==============================

const RADIO_SECTION: &str = "radio_tower";
const RADIO_HOLD_SECS: f64 = 0.75;

#[derive(Clone, Debug, Default)]
pub struct RadioAnim {
    pub hold_row: Option<RadioSpeaker>,
    pub hold_until: f64,
}

fn preview_radio_row() -> RadioSpeaker {
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

fn radio_driver_part(row: &RadioSpeaker, show_name: bool, show_car_number: bool) -> String {
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

fn radio_row_text(
    row: &RadioSpeaker,
    show_position: bool,
    show_name: bool,
    show_num: bool,
) -> String {
    let driver = radio_driver_part(row, show_name, show_num);
    let has_pos = show_position && row.position > 0;
    if has_pos && !driver.is_empty() {
        format!("{} - {driver}", row.position)
    } else if has_pos {
        row.position.to_string()
    } else {
        driver
    }
}

fn radio_badge_glyph(
    cfg: &OverlayConfig,
    section: &str,
    row: &RadioSpeaker,
) -> Option<(String, Rgba)> {
    if row.is_pro {
        return crate::icons::glyph("pro_driver")
            .map(|g| (g, section_color(cfg, section, "pro_badge", "#f5c542")));
    }
    if !row.group_icon.is_empty() {
        let col = if row.group_color.is_empty() {
            Rgba::parse("#5bb8ff")
        } else {
            Rgba::parse(&row.group_color)
        };
        if let Some(g) = crate::icons::glyph(&row.group_icon) {
            return Some((g, col));
        }
    }
    if row.active {
        return crate::icons::glyph("speaking").map(|g| {
            (
                g,
                section_color(cfg, section, "badge_speaking_bg", "#22c55e"),
            )
        });
    }
    None
}

fn draw_speaking_accent(c: &mut Canvas, cfg: &OverlayConfig, section: &str, rect: Rect) {
    let accent = section_color(cfg, section, "badge_speaking_bg", "#22c55e");
    let h = rect.height();
    let stripe_w = (h * 0.09).max(3.5);
    c.fill_rect(
        Rect::from_xywh(rect.left(), rect.top() + h * 0.10, stripe_w, h * 0.80),
        accent,
        2.0,
    );
    c.fill_rect(rect, accent.with_alpha(38), 0.0);
}

/// Paint the radio tower row; returns true whenever a row is being shown
/// (matches egui's `*ctx.panel_animating = true` while a speaker is latched).
pub fn paint_radio(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    anim: &mut RadioAnim,
    mono_secs: f64,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let section = RADIO_SECTION;
    let now = mono_secs;

    let row = if let Some(r) = &frame.radio {
        anim.hold_row = Some(r.clone());
        anim.hold_until = now + RADIO_HOLD_SECS;
        Some(r.clone())
    } else if edit_mode {
        Some(preview_radio_row())
    } else if now <= anim.hold_until {
        anim.hold_row.clone()
    } else {
        anim.hold_row = None;
        None
    };

    let Some(row) = row else {
        return false;
    };

    let rect = bounds(c);
    let radius = panel_card(c, cfg, section, rect);
    let pad = panel_content_pad(cfg, section, rect.height());
    let mut y = rect.top() + pad;
    y = panel_title(c, cfg, section, rect, radius, y, pad, "RADIO");

    let show_pos = cfg.bool_key(section, "show_position", true);
    let show_num = cfg.bool_key(section, "show_car_number", true);
    let show_name = cfg.bool_key(section, "show_name", true);
    let highlight = cfg.bool_key(section, "highlight_player", true);

    let body_h = (rect.bottom() - pad - y).max(18.0);
    let fixed_rh = cfg.f64_key(section, "row_height_px", 0.0) as f32;
    let elegant = is_elegant(cfg, section);
    let row_h = if elegant {
        body_h.clamp(20.0, 28.0)
    } else if fixed_rh > 0.0 {
        fixed_rh
    } else {
        body_h
    }
    .max(18.0);

    let content_w = rect.width() - 2.0 * pad;
    let text_size = row_h * (if elegant { 0.42 } else { 0.46 }) * cfg.text_scale(section);
    let x0 = rect.left() + pad;
    let row_rect = Rect::from_xywh(x0, y, content_w, row_h - 2.0);

    if row.active {
        draw_speaking_accent(c, cfg, section, row_rect);
    } else if row.is_player && highlight {
        c.fill_rect(
            row_rect,
            section_color(cfg, section, "player_row", "#ffffff14"),
            if elegant { 8.0 } else { 0.0 },
        );
    }

    let text = radio_row_text(&row, show_pos, show_name, show_num);
    if text.is_empty() {
        return true;
    }

    let stripe_w = ((row_h - 2.0) * 0.09).max(3.5);
    let text_inset = stripe_w + (row_h * 0.12).max(6.0);
    let mut text_x = x0 + text_inset;
    let mut text_width_avail = (content_w - text_inset).max(0.0);

    if let Some((glyph, badge_col)) = radio_badge_glyph(cfg, section, &row) {
        let ic_px = (row_h - 2.0) * 0.32;
        let gap = ((row_h - 2.0) * 0.08).max(2.0);
        let gw = c
            .icon(
                &glyph,
                text_x,
                row_rect.center().1,
                ic_px,
                badge_col,
                TextAlign::Left,
            )
            .max(ic_px * 0.6);
        text_x += gw + gap;
        text_width_avail = (text_width_avail - (gw + gap)).max(0.0);
    }

    let text_col = if row.is_pro && !row.active {
        section_color(cfg, section, "pro_name", "#f5c542")
    } else {
        section_color(cfg, section, "text", "#d8d8d8")
    };
    let text_rect = Rect::from_xywh(text_x, y, text_width_avail, row_h - 2.0);
    c.clip_rect(text_rect, |c| {
        text_at(
            c,
            text_x,
            text_rect.center().1,
            &text,
            text_size,
            text_col,
            false,
            TextAlign::Left,
        );
    });

    true
}

// ============================== Pit advisor ==============================

const PIT_ADVISOR_SECTION: &str = "pit_advisor";

/// Paint the pit engineer card. Static content — always returns false.
pub fn paint_pit_advisor(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let section = PIT_ADVISOR_SECTION;
    let advice = frame.pit_advice.clone().unwrap_or_default();
    let only_actionable = cfg.bool_key(section, "show_only_when_actionable", true);
    if only_actionable && !advice.actionable && !edit_mode {
        return false;
    }

    let rect = bounds(c);
    let radius = panel_card(c, cfg, section, rect);
    let pad = panel_content_pad(cfg, section, rect.height());
    let elegant = is_elegant(cfg, section);
    let mut y = rect.top() + pad;
    let content_w = rect.width() - 2.0 * pad;

    if elegant {
        if cfg.bool_key(section, "show_title", true) {
            text_at(
                c,
                rect.left() + pad,
                y + 6.0,
                &cfg.str_key(section, "title", "PIT ENGINEER"),
                10.0,
                section_color(cfg, section, "muted", "#8b93a1").with_alpha(200),
                false,
                TextAlign::Left,
            );
            y += 14.0;
        }
    } else {
        y = panel_title(c, cfg, section, rect, radius, y, pad, "PIT ENGINEER");
    }

    let label_txt = if advice.label.is_empty() && edit_mode {
        "PIT NEXT LAP".to_string()
    } else {
        advice.label.clone()
    };
    let rationale = if advice.rationale.is_empty() && edit_mode {
        "Pit next lap to pass #12 — 6.2s ahead, stop costs ~28s".to_string()
    } else {
        advice.rationale.clone()
    };
    let rec = if advice.rec.is_empty() {
        "pit_next_lap".to_string()
    } else {
        advice.rec.clone()
    };
    let active = !matches!(rec.as_str(), "stay_out" | "hold");

    let chip_h = if elegant {
        24.0
    } else {
        (rect.height() * 0.18).max(22.0)
    };
    let chip = Rect::from_xywh(rect.left() + pad, y, content_w, chip_h);
    let chip_bg = if active {
        section_color(cfg, section, "chip_active", "#ff9416")
    } else {
        section_color(cfg, section, "chip_idle", "#333a42")
    };
    c.fill_rect(chip, chip_bg, if elegant { 8.0 } else { 6.0 });
    text_at(
        c,
        chip.center().0,
        chip.center().1,
        &label_txt,
        (chip_h * 0.42).clamp(11.0, 16.0),
        Rgba::WHITE,
        true,
        TextAlign::Center,
    );
    y += chip_h + if elegant { 6.0 } else { 8.0 };

    let rationale_sz = if elegant { 11.0 } else { 13.0 };
    let rationale_col = if elegant {
        section_color(cfg, section, "text", "#f4f6f8").with_alpha(210)
    } else {
        section_color(cfg, section, "text", "#f4f6f8")
    };
    text_at(
        c,
        rect.left() + pad,
        y + rationale_sz * 0.5,
        &rationale,
        rationale_sz,
        rationale_col,
        false,
        TextAlign::Left,
    );
    y += 28.0;

    if let Some(sec) = advice
        .secondary
        .clone()
        .or_else(|| edit_mode.then(|| "Best stop: laps 24–26".to_string()))
    {
        text_at(
            c,
            rect.left() + pad,
            y + 5.0,
            &sec,
            10.0,
            section_color(cfg, section, "muted", "#8b93a1").with_alpha(200),
            false,
            TextAlign::Left,
        );
    }

    false
}

// ============================== Pace / caution ============================

const PACE_CAUTION_SECTION: &str = "pace_caution";

/// Yellow / full-course caution from session flags (matches
/// `widgets/pace_caution::under_caution`).
fn pace_under_caution(f: &TelemetryFrame) -> bool {
    matches!(
        f.flag.as_deref(),
        Some("yellow") | Some("caution") | Some("yellow_waving") | Some("caution_waving")
    )
}

fn pace_should_display(f: &TelemetryFrame, edit_mode: bool) -> bool {
    edit_mode || pace_under_caution(f)
}

fn pace_speed_value(cfg: &OverlayConfig, ms: f32) -> f32 {
    if cfg.imperial_units() {
        ms * 2.236_936_3
    } else {
        ms * 3.6
    }
}

fn pace_speed_unit(cfg: &OverlayConfig) -> &'static str {
    if cfg.imperial_units() {
        "MPH"
    } else {
        "KPH"
    }
}

fn format_pace_speed(cfg: &OverlayConfig, ms: f32) -> String {
    format!("{:.0} {}", pace_speed_value(cfg, ms), pace_speed_unit(cfg))
}

fn format_pace_delta(cfg: &OverlayConfig, you_mps: f32, ref_mps: f32) -> String {
    let d = pace_speed_value(cfg, you_mps) - pace_speed_value(cfg, ref_mps);
    format!("{d:+.0} {}", pace_speed_unit(cfg))
}

fn pace_car_speed_mps(f: &TelemetryFrame) -> Option<f32> {
    f.pace_car_speed_mps
        .filter(|v| v.is_finite() && *v > 0.5)
        .or_else(|| {
            f.cars
                .iter()
                .find(|c| c.is_pace_car)
                .map(|c| c.speed_mps)
                .filter(|v| v.is_finite() && *v > 0.5)
        })
}

fn pace_pit_limit_mps(f: &TelemetryFrame, map: &MapAuthoring) -> Option<f32> {
    f.pit_speed_limit_mps
        .filter(|v| v.is_finite() && *v > 0.5)
        .or_else(|| {
            map.cached_pit
                .speed_ms
                .filter(|v| v.is_finite() && *v > 0.5)
        })
        .or_else(|| {
            let v = map.pit_speed_ms as f32;
            if v.is_finite() && v > 0.5 {
                Some(v)
            } else {
                None
            }
        })
}

/// Paint the pace/caution helper. Static content — always returns false.
pub fn paint_pace_caution(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    map: &MapAuthoring,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let section = PACE_CAUTION_SECTION;
    if !pace_should_display(frame, edit_mode) {
        return false;
    }

    let rect = bounds(c);
    let radius = panel_card(c, cfg, section, rect);
    let pad = panel_content_pad(cfg, section, rect.height());
    let mut y = rect.top() + pad;
    if cfg.bool_key(section, "show_title", true) {
        y = panel_title(
            c,
            cfg,
            section,
            rect,
            radius,
            y,
            pad,
            &cfg.str_key(section, "title", "PACE"),
        );
    }

    let mut pace_mps = pace_car_speed_mps(frame);
    let mut you_mps = if frame.speed_mps.is_finite() {
        Some(frame.speed_mps.max(0.0))
    } else {
        None
    };
    let mut pit_mps = pace_pit_limit_mps(frame, map);
    if edit_mode && !pace_under_caution(frame) {
        pace_mps = pace_mps.or(Some(24.6));
        you_mps = you_mps.filter(|v| *v > 0.05).or(Some(24.6));
        pit_mps = pit_mps.or(Some(22.0));
    }

    let show_delta = cfg.bool_key(section, "show_delta", true);
    let unit = pace_speed_unit(cfg);
    let moving = you_mps.map(|v| v > 0.5).unwrap_or(false);

    let mut cols: Vec<(&str, String, bool)> = Vec::new();
    cols.push((
        "Pace",
        pace_mps
            .map(|ms| format_pace_speed(cfg, ms))
            .unwrap_or_else(|| format!("— {unit}")),
        false,
    ));
    cols.push((
        "You",
        you_mps
            .map(|ms| format_pace_speed(cfg, ms))
            .unwrap_or_else(|| format!("— {unit}")),
        false,
    ));
    if show_delta {
        let delta_s = match (pace_mps, you_mps) {
            (Some(p), Some(y)) if moving => format_pace_delta(cfg, y, p),
            _ => format!("— {unit}"),
        };
        cols.push(("ΔP", delta_s, true));
    }
    cols.push((
        "Pit",
        pit_mps
            .map(|ms| format_pace_speed(cfg, ms))
            .unwrap_or_else(|| format!("— {unit}")),
        false,
    ));
    if show_delta {
        let delta_s = match (pit_mps, you_mps) {
            (Some(p), Some(y)) if moving => format_pace_delta(cfg, y, p),
            _ => format!("— {unit}"),
        };
        cols.push(("ΔL", delta_s, true));
    }

    let n = cols.len().max(1) as f32;
    let body = Rect::from_ltrb(
        rect.left() + pad,
        y,
        rect.right() - pad,
        rect.bottom() - pad,
    );
    let col_w = body.width() / n;
    let text = section_color(cfg, section, "text", "#f4f6f8");
    let muted = section_color(cfg, section, "muted", "#8b93a1");
    let warn = section_color(cfg, section, "warn", "#ffd23a");
    let label_sz = (body.height() * 0.28).clamp(9.0, 12.0);
    let value_sz = (body.height() * 0.42).clamp(12.0, 18.0);

    for (i, (lab, value, is_delta)) in cols.into_iter().enumerate() {
        let col = Rect::from_xywh(
            body.left() + i as f32 * col_w,
            body.top(),
            col_w,
            body.height(),
        );
        let cx = col.center().0;
        text_at(
            c,
            cx,
            col.top() + body.height() * 0.28,
            lab,
            label_sz,
            muted,
            false,
            TextAlign::Center,
        );
        text_at(
            c,
            cx,
            col.top() + body.height() * 0.68,
            &value,
            value_sz,
            if is_delta { warn } else { text },
            true,
            TextAlign::Center,
        );
    }

    false
}

// =============================== System panel =============================

const SYSTEM_SECTION: &str = "system_panel";

/// show_* config key → (text label, FA icon key).
const SYSTEM_ROWS: &[(&str, &str, &str)] = &[
    ("show_cpu", "CPU", "cpu"),
    ("show_mem", "MEM", "mem"),
    ("show_gpu", "GPU", "gpu"),
    ("show_fps", "FPS", "fps"),
    ("show_network", "NET", "network"),
    ("show_ffb", "FFB", "ffb"),
];

fn collect_system_rows(
    cfg: &OverlayConfig,
    f: &TelemetryFrame,
) -> Vec<(&'static str, &'static str, String, bool)> {
    let mut rows = Vec::new();
    for &(cfg_key, text_label, icon_key) in SYSTEM_ROWS {
        if !cfg.bool_key(SYSTEM_SECTION, cfg_key, true) {
            continue;
        }
        let (value, warn) = match cfg_key {
            "show_cpu" => (f.cpu.clone().unwrap_or_else(|| "--".into()), false),
            "show_mem" => (f.mem.clone().unwrap_or_else(|| "--".into()), false),
            "show_gpu" => (f.gpu.clone().unwrap_or_else(|| "--".into()), false),
            "show_fps" => (
                f.fps.map(|v| v.to_string()).unwrap_or_else(|| "--".into()),
                false,
            ),
            "show_network" => {
                let v = if let Some(q) = f.chan_quality {
                    if q > 0.0 {
                        format!("{:.0}%", q)
                    } else {
                        "--".into()
                    }
                } else {
                    "--".into()
                };
                (v, false)
            }
            "show_ffb" => {
                if let Some(p) = f.ffb_pct.filter(|v| v.is_finite()) {
                    (format!("{:.0}%", p), p > 100.0)
                } else {
                    ("--".into(), false)
                }
            }
            _ => ("--".into(), false),
        };
        rows.push((text_label, icon_key, value, warn));
    }
    rows
}

/// Paint the system panel. Static content — always returns false.
pub fn paint_system(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    _edit_mode: bool,
) -> bool {
    c.clear_transparent();
    if is_elegant(cfg, SYSTEM_SECTION) {
        paint_system_elegant(c, cfg, frame);
    } else {
        paint_system_data(c, cfg, frame);
    }
    false
}

fn paint_system_data(c: &mut Canvas, cfg: &OverlayConfig, frame: &TelemetryFrame) {
    let rect = bounds(c);
    if rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    let section = SYSTEM_SECTION;
    let radius = panel_card(c, cfg, section, rect);
    let pad = panel_content_pad(cfg, section, rect.height());
    let mut y = rect.top() + pad;
    y = panel_title(c, cfg, section, rect, radius, y, pad, "SYSTEM");

    let show_icons = cfg.bool_key(section, "show_icons", false);
    let rows = collect_system_rows(cfg, frame);
    let n = rows.len().max(1) as f32;
    let avail = (rect.bottom() - pad - y).max(0.0);
    let rh = (avail / n)
        .min(cfg.f64_key(section, "row_height_px", 36.0) as f32)
        .clamp(12.0, 48.0);
    let text = section_color(cfg, section, "text", "#f4f6f8");
    let muted = section_color(cfg, section, "muted", "#8b93a1");
    let header = section_color(cfg, section, "header", "#9aa3b2");
    let warn = section_color(cfg, section, "warn", "#ff5b5b");
    for (text_label, icon_key, value, is_warn) in rows {
        let row = Rect::from_xywh(
            rect.left() + pad,
            y,
            (rect.width() - 2.0 * pad).max(1.0),
            rh,
        );
        if show_icons {
            let ic = (rh * 0.42).clamp(8.0, 22.0);
            super::icons::paint(
                c,
                icon_key,
                row.left() + 8.0,
                row.center().1,
                ic,
                header,
                TextAlign::Left,
            );
        } else {
            text_at(
                c,
                row.left() + 8.0,
                row.center().1,
                text_label,
                (rh * 0.38).clamp(8.0, 20.0),
                muted,
                false,
                TextAlign::Left,
            );
        }
        text_at(
            c,
            row.right() - 8.0,
            row.center().1,
            &value,
            (rh * 0.42).clamp(8.0, 22.0),
            if is_warn { warn } else { text },
            true,
            TextAlign::Right,
        );
        y += rh;
    }
}

fn paint_system_elegant(c: &mut Canvas, cfg: &OverlayConfig, frame: &TelemetryFrame) {
    let rect = bounds(c);
    let section = SYSTEM_SECTION;
    panel_card(c, cfg, section, rect);
    let pad_x = (rect.width() * 0.05).clamp(8.0, 12.0);
    let pad_y = (rect.height() * 0.05).clamp(6.0, 10.0);
    let text = section_color(cfg, section, "text", "#f4f6f8");
    let muted = section_color(cfg, section, "muted", "#8b93a1").with_alpha(200);
    let accent = section_color(cfg, section, "accent", "#9aa3b2");
    let rows = collect_system_rows(cfg, frame);
    if rows.is_empty() {
        return;
    }

    let show_title = cfg.bool_key(section, "show_title", true);
    let mut y = rect.top() + pad_y;
    let left = rect.left() + pad_x;
    let width = rect.width() - 2.0 * pad_x;

    if show_title {
        let title = cfg.str_key(section, "title", "SYSTEM");
        text_at(
            c,
            left,
            y + 6.0,
            &title,
            10.0,
            muted,
            false,
            TextAlign::Left,
        );
        y += 14.0;
    }

    let avail = (rect.bottom() - pad_y - y).max(rows.len() as f32 * 18.0);
    let row_h = (avail / rows.len() as f32).clamp(16.0, 22.0);
    let warn_c = section_color(cfg, section, "warn", "#ff5b5b");

    for (text_label, icon_key, value, is_warn) in rows {
        let cy = y + row_h * 0.5;
        let icon_sz = 12.0;
        let value_c = if is_warn { warn_c } else { text };
        let iw = super::icons::paint(c, icon_key, left, cy, icon_sz, accent, TextAlign::Left);
        text_at(
            c,
            left + iw.max(icon_sz) + 6.0,
            cy,
            text_label,
            11.0,
            muted,
            false,
            TextAlign::Left,
        );
        text_at(
            c,
            left + width,
            cy,
            &value,
            12.0,
            value_c,
            true,
            TextAlign::Right,
        );
        y += row_h;
    }
}
