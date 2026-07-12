//! Shared BaseTable-style painter for relative / standings (Python parity).

use crate::chrome::{color_with_alpha, draw_card, draw_row_tint, ease, label, panel_pad};
use crate::config::{parse_color_str, OverlayConfig};
use crate::icons;
use crate::telemetry::{slot_label, TableRow, TableSlotItem, TableSlots};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, Ui, Vec2};
use std::collections::HashMap;

const ROW_SNAP_SLOTS: f32 = 1.25;
const DENSE_ROW_COUNT: usize = 20;
const DENSE_ROW_SNAP_SLOTS: f32 = 1.0;
const DENSE_ROW_EASE_TAU: f32 = 0.12;

#[derive(Clone, Default)]
struct RowAnimState {
    idx: f32,
    opacity: f32,
}

#[derive(Clone, Default)]
struct TableAnim {
    /// key -> visual slot + fade
    slots: HashMap<String, RowAnimState>,
    last_ms: f64,
}

pub fn paint_table(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
    slots: &TableSlots,
    signed_gaps: bool,
) {
    let rect = crate::chrome::full_rect(ui);
    let (card, radius) = draw_card(ui, cfg, section, rect);
    let pad = panel_pad(card.height());
    let show_footer = cfg.bool_key(section, "show_footer", true);
    let rh = cfg.f64_key(section, "row_height_px", 36.0) as f32;
    let scale = cfg.text_scale(section);
    let font_scale = cfg.f64_key(section, "font_scale", 0.40) as f32;
    let gap_font_scale = cfg.f64_key(section, "gap_font_scale", 1.12) as f32;
    let hscale = cfg.f64_key(section, "header_font_scale", 1.0) as f32;
    let fscale = cfg.f64_key(section, "footer_font_scale", 1.0) as f32;
    let header_h = (rh * 0.72 * scale * hscale.max(0.3)).max(18.0);
    let footer_h = if show_footer {
        (rh * 0.68 * scale * fscale.max(0.3)).max(16.0)
    } else {
        0.0
    };

    let mut y = card.top() + pad;
    let inner_w = card.width() - 2.0 * pad;
    let left = card.left() + pad;

    // Header band
    let hdr = Rect::from_min_size(Pos2::new(left, y), Vec2::new(inner_w, header_h));
    draw_edge_band(
        ui,
        cfg,
        section,
        hdr,
        radius,
        true,
        &slots.header_left,
        &slots.header_center,
        &slots.header_right,
    );
    y += header_h + 2.0;

    let body_bottom = if show_footer {
        card.bottom() - pad - footer_h - 2.0
    } else {
        card.bottom() - pad
    };

    // Row motion
    let id = egui::Id::new(("table_anim", section));
    let animating = {
        let now = ui.input(|i| i.time);
        let mut anim = ui.ctx().data_mut(|d| d.get_temp::<TableAnim>(id).unwrap_or_default());
        let dt = if anim.last_ms > 0.0 {
            ((now - anim.last_ms) as f32).clamp(0.0, 0.1)
        } else {
            0.016
        };
        anim.last_ms = now;

        let mut tau = cfg.f64_key(section, "row_ease_tau", 0.16) as f32;
        let fade_tau = cfg.f64_key(section, "fade_ease_tau", 0.12) as f32;
        let dense = rows.len() >= DENSE_ROW_COUNT;
        let snap = if dense {
            DENSE_ROW_SNAP_SLOTS
        } else {
            ROW_SNAP_SLOTS
        };
        if dense {
            tau = tau.min(DENSE_ROW_EASE_TAU);
        }

        let active: std::collections::HashSet<String> =
            rows.iter().filter(|r| !r.empty).map(|r| r.key.clone()).collect();
        anim.slots.retain(|k, _| active.contains(k));

        let mut still = false;
        for (i, row) in rows.iter().enumerate() {
            if row.empty {
                continue;
            }
            let target = i as f32;
            let st = anim.slots.entry(row.key.clone()).or_insert(RowAnimState {
                idx: target,
                opacity: 0.0,
            });
            if (st.idx - target).abs() > snap {
                st.idx = target;
            } else {
                st.idx = ease(st.idx, target, dt, tau);
            }
            st.opacity = ease(st.opacity, 1.0, dt, fade_tau);
            if (st.idx - target).abs() > 0.02 || (st.opacity - 1.0).abs() > 0.01 {
                still = true;
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, anim));
        still
    };
    if animating {
        ui.ctx().request_repaint();
    }

    let anim = ui
        .ctx()
        .data(|d| d.get_temp::<TableAnim>(id))
        .unwrap_or_default();

    let columns = column_order(cfg, section);
    let gutter = rh * width_mult(cfg, section, "gutter", 0.18);
    let alt = cfg.bool_key(section, "alt_row_shading", true);
    let text = cfg.color(section, "text", "#f4f6f8");
    let muted = cfg.color(section, "muted", "#8b93a1");
    let fs = (rh * font_scale * scale).clamp(9.0, 22.0);

    let body_top = y;
    let mut draw_order: Vec<usize> = (0..rows.len()).collect();
    draw_order.sort_by(|&a, &b| {
        let ia = if rows[a].empty {
            a as f32
        } else {
            anim.slots
                .get(&rows[a].key)
                .map(|s| s.idx)
                .unwrap_or(a as f32)
        };
        let ib = if rows[b].empty {
            b as f32
        } else {
            anim.slots
                .get(&rows[b].key)
                .map(|s| s.idx)
                .unwrap_or(b as f32)
        };
        ia.partial_cmp(&ib).unwrap_or(std::cmp::Ordering::Equal)
    });

    for &i in &draw_order {
        let row = &rows[i];
        if row.empty {
            continue;
        }
        let st = anim.slots.get(&row.key);
        let slot_y = st.map(|s| s.idx).unwrap_or(i as f32);
        let opacity = st.map(|s| s.opacity).unwrap_or(1.0);
        let ry = body_top + slot_y * rh;
        if ry + rh > body_bottom {
            continue;
        }
        let row_rect = Rect::from_min_size(Pos2::new(left, ry), Vec2::new(inner_w, rh));
        if opacity < 0.99 {
            // Soft fade-in: tint with alpha via layer (simple multiply on text later).
            ui.painter().rect_filled(
                row_rect,
                0.0,
                Color32::from_black_alpha(((1.0 - opacity) * 40.0) as u8),
            );
        }
        paint_row_chrome(ui, cfg, section, row, row_rect, i, alt);
        paint_row_cols(
            ui,
            cfg,
            section,
            row,
            row_rect,
            &columns,
            gutter,
            rh,
            fs,
            gap_font_scale,
            signed_gaps,
            text,
            muted,
        );
    }

    if show_footer {
        let fy = card.bottom() - pad - footer_h;
        let frect = Rect::from_min_size(Pos2::new(left, fy), Vec2::new(inner_w, footer_h));
        draw_edge_band(
            ui,
            cfg,
            section,
            frect,
            radius,
            false,
            &slots.footer_left,
            &slots.footer_center,
            &slots.footer_right,
        );
    }
}

fn paint_row_chrome(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    rect: Rect,
    i: usize,
    alt: bool,
) {
    let tint_key = if row.is_player {
        Some("player_row")
    } else if row.lapping {
        Some(if row.lap_ahead { "threat" } else { "lapped" })
    } else if row.in_pit || row.on_pit {
        Some("pit_row")
    } else if row.inactive && section == "standings" {
        Some("inactive_row")
    } else if row.is_speaking {
        Some("speaking_row")
    } else {
        None
    };

    if let Some(key) = tint_key {
        let fallback = match key {
            "player_row" => "#ff941658",
            "threat" => "#ff505060",
            "lapped" => "#4a8cff60",
            "pit_row" => "#8b93a118",
            "inactive_row" => "#8b93a128",
            "speaking_row" => "#22c55e50",
            _ => "#ffffff14",
        };
        let accent = cfg.color(section, key, fallback);
        draw_row_tint(ui, rect, accent);
    } else if alt && i % 2 == 1 {
        ui.painter().rect_filled(
            rect,
            CornerRadius::ZERO,
            cfg.color(section, "row_alt", "#ffffff14"),
        );
    }

    if cfg.bool_key(section, "row_dividers", true) {
        let line = cfg.color(section, "border", "#ffffff28");
        let y = rect.bottom() - 0.5;
        let a = ((line.a() as f32) * 0.55).max(30.0) as u8;
        ui.painter().line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0_f32, color_with_alpha(line, a)),
        );
    }
}

fn paint_row_cols(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    rect: Rect,
    columns: &[String],
    gutter: f32,
    rh: f32,
    fs: f32,
    gap_font_scale: f32,
    signed_gaps: bool,
    text: Color32,
    muted: Color32,
) {
    let dim = row.inactive || (row.empty);
    let name_col = columns.iter().any(|c| c == "name");
    let fixed: f32 = columns
        .iter()
        .filter(|c| c.as_str() != "name")
        .map(|c| rh * width_mult(cfg, section, c, default_width(c)))
        .sum();
    let n_gut = columns.len().saturating_sub(1) as f32;
    let name_w = if name_col {
        (rect.width() - fixed - n_gut * gutter).max(10.0)
    } else {
        0.0
    };

    let mut cx = rect.left();
    let cy = rect.center().y;
    let stripe = cfg
        .section(section)
        .get("columns")
        .and_then(|c| c.get("stripe"))
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    for col in columns {
        let cw = if col == "name" {
            name_w
        } else {
            rh * width_mult(cfg, section, col, default_width(col))
        };
        match col.as_str() {
            "badge" => paint_badge(ui, cfg, section, row, cx, rect.top(), cw, rh),
            "position" => {
                if stripe && !row.class_color.is_empty() {
                    let sc = parse_color_str(&row.class_color);
                    // Skip parse fallback magenta (#ff00ff) from bad class colors.
                    let is_fallback = sc.r() == 255 && sc.g() == 0 && sc.b() == 255;
                    if !is_fallback {
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(cx, rect.top() + rh * 0.18),
                                Vec2::new((rh * 0.08).max(2.0), rh * 0.64),
                            ),
                            CornerRadius::same(1),
                            sc,
                        );
                    }
                }
                label(
                    ui,
                    Pos2::new(cx + cw * 0.55, cy),
                    Align2::CENTER_CENTER,
                    &format!("{}", row.position.max(0)),
                    fs,
                    if dim { muted } else { text },
                    true,
                );
            }
            "name" => {
                let colc = if row.is_speaking {
                    cfg.color(section, "badge_speaking_bg", "#22c55e")
                } else if dim {
                    muted
                } else {
                    text
                };
                let bold = cfg.bool_key(section, "name_font_bold", true);
                label(
                    ui,
                    Pos2::new(cx + 4.0, cy),
                    Align2::LEFT_CENTER,
                    &row.name,
                    fs,
                    colc,
                    bold,
                );
            }
            "license" => {
                let letter = row.lic_class.chars().next().unwrap_or(' ');
                let bg = license_color(cfg, section, &row.lic_class);
                let pill = Rect::from_center_size(
                    Pos2::new(cx + cw * 0.5, cy),
                    Vec2::new(cw * 0.9, rh * 0.55),
                );
                ui.painter()
                    .rect_filled(pill, CornerRadius::same(3), bg);
                let txt = if row.sr.is_empty() {
                    letter.to_string()
                } else {
                    format!("{letter}{}", truncate_sr(&row.sr))
                };
                label(
                    ui,
                    pill.center(),
                    Align2::CENTER_CENTER,
                    &txt,
                    fs * 0.72,
                    Color32::WHITE,
                    true,
                );
            }
            "irating" => {
                let abbrev = cfg.bool_key(section, "irating_abbreviate", true);
                let s = fmt_ir(row.irating, abbrev);
                label(
                    ui,
                    Pos2::new(cx + cw - gutter, cy),
                    Align2::RIGHT_CENTER,
                    &s,
                    fs * 0.9,
                    if dim { muted } else { text },
                    false,
                );
            }
            "gap" => {
                let (gtxt, gcol) = gap_display(row, signed_gaps, section, cfg, text);
                label(
                    ui,
                    Pos2::new(cx + cw - gutter, cy),
                    Align2::RIGHT_CENTER,
                    &gtxt,
                    fs * gap_font_scale,
                    if dim { muted } else { gcol },
                    false,
                );
            }
            "car_number" => {
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    &row.car_number,
                    fs,
                    if dim { muted } else { text },
                    true,
                );
            }
            "last_lap" | "best_lap" => {
                let v = if col == "last_lap" {
                    &row.last_lap
                } else {
                    &row.best_lap
                };
                let s = if v.is_empty() { "—" } else { v.as_str() };
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    s,
                    fs * 0.92,
                    if dim { muted } else { text },
                    false,
                );
            }
            _ => {}
        }
        cx += cw + gutter;
    }
}

fn gap_display(
    row: &TableRow,
    signed_gaps: bool,
    section: &str,
    cfg: &OverlayConfig,
    text: Color32,
) -> (String, Color32) {
    if !row.gap_text.is_empty() && !(signed_gaps && row.gap_secs.is_some()) {
        return (row.gap_text.clone(), text);
    }
    if let Some(g) = row.gap_secs {
        let gtxt = if g == 0.0 {
            "0.0".into()
        } else {
            format!("{:.1}", g.abs())
        };
        let gcol = if signed_gaps && section == "relative" && !row.is_player {
            if g > 0.0 {
                cfg.color(section, "irating_delta_down", "#ff5050")
            } else if g < 0.0 {
                cfg.color(section, "irating_delta_up", "#46df7a")
            } else {
                text
            }
        } else {
            text
        };
        (gtxt, gcol)
    } else if !row.gap_text.is_empty() {
        (row.gap_text.clone(), text)
    } else {
        ("—".into(), text)
    }
}

fn paint_badge(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let r = (h * 0.28).min(w * 0.35);
    if row.is_player {
        ui.painter().circle_filled(
            Pos2::new(cx, cy),
            r,
            cfg.color(section, "badge_player", "#ff9416"),
        );
    } else if row.in_pit || row.on_pit {
        let pill = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(w * 0.85, h * 0.42));
        ui.painter().rect_filled(
            pill,
            CornerRadius::same(3),
            cfg.color(section, "badge_pit_bg", "#ebeef0"),
        );
        label(
            ui,
            pill.center(),
            Align2::CENTER_CENTER,
            "PIT",
            h * 0.22,
            cfg.color(section, "badge_pit_text", "#141414"),
            true,
        );
    } else if row.is_speaking {
        ui.painter().circle_filled(
            Pos2::new(cx, cy),
            r,
            cfg.color(section, "badge_speaking_bg", "#22c55e"),
        );
    } else {
        ui.painter().circle_stroke(
            Pos2::new(cx, cy),
            r,
            Stroke::new(
                1.0_f32,
                cfg.color(section, "badge_empty_border", "#ffffff28"),
            ),
        );
    }
}

fn draw_edge_band(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    radius: f32,
    is_header: bool,
    left: &TableSlotItem,
    center: &TableSlotItem,
    right: &TableSlotItem,
) {
    let bg = if is_header {
        cfg.color(section, "header_bg", "#0b0e12bb")
    } else {
        cfg.color(section, "footer_bg", "#0f1216")
    };
    let cr = if is_header {
        CornerRadius {
            nw: radius as u8,
            ne: radius as u8,
            sw: 0,
            se: 0,
        }
    } else {
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: radius as u8,
            se: radius as u8,
        }
    };
    ui.painter().rect_filled(rect, cr, bg);
    let muted = cfg.color(section, "muted", "#8b93a1");
    let text = cfg.color(section, "text", "#f4f6f8");
    let fs = (rect.height() * 0.48).clamp(9.0, 16.0) * cfg.text_scale(section);
    let icons_group = if is_header {
        "header_icons"
    } else {
        "footer_icons"
    };

    paint_band_slot(
        ui,
        cfg,
        section,
        icons_group,
        "left",
        left,
        Pos2::new(rect.left() + 8.0, rect.center().y),
        Align2::LEFT_CENTER,
        fs,
        muted,
        text,
    );
    paint_band_slot(
        ui,
        cfg,
        section,
        icons_group,
        "center",
        center,
        rect.center(),
        Align2::CENTER_CENTER,
        fs,
        muted,
        text,
    );
    paint_band_slot(
        ui,
        cfg,
        section,
        icons_group,
        "right",
        right,
        Pos2::new(rect.right() - 8.0, rect.center().y),
        Align2::RIGHT_CENTER,
        fs,
        muted,
        text,
    );
}

fn text_width(ui: &Ui, font: &FontId, s: &str) -> f32 {
    ui.fonts(|f| f.layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE).size().x)
}

fn paint_band_slot(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    icons_group: &str,
    pos: &str,
    item: &TableSlotItem,
    anchor: Pos2,
    align: Align2,
    fs: f32,
    muted: Color32,
    text: Color32,
) {
    if item.key.is_empty() {
        return;
    }
    let use_icon = cfg.nested_bool(section, icons_group, pos, false);

    // Special cases: title / order_pill / count are value-only (Python parity).
    if item.key == "title" {
        label(ui, anchor, align, &item.value, fs, text, true);
        return;
    }
    if item.key == "order_pill" {
        let pill_w = fs * 2.8;
        let pill_h = fs * 1.15;
        let x0 = if align == Align2::LEFT_CENTER {
            anchor.x
        } else if align == Align2::RIGHT_CENTER {
            anchor.x - pill_w
        } else {
            anchor.x - pill_w * 0.5
        };
        let pill = Rect::from_min_size(
            Pos2::new(x0, anchor.y - pill_h * 0.5),
            Vec2::new(pill_w, pill_h),
        );
        ui.painter().rect_stroke(
            pill,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, muted),
            egui::StrokeKind::Inside,
        );
        label(ui, pill.center(), Align2::CENTER_CENTER, "ORDER", fs * 0.72, muted, true);
        return;
    }
    if item.key == "count" || item.key == "track_name" {
        label(ui, anchor, align, &item.value, fs, muted, true);
        return;
    }

    let glyph = if use_icon {
        icons::glyph(&item.key)
    } else {
        None
    };
    let lead_is_icon = glyph.is_some();
    let lead: String = if let Some(g) = glyph {
        g
    } else {
        slot_label(&item.key).to_string()
    };

    let lead_font = if lead_is_icon {
        icons::font_id(fs * 0.82)
    } else {
        FontId::proportional(fs * 0.62)
    };
    let val_font = FontId::proportional(fs * 0.9);
    let lead_w = if lead.is_empty() {
        0.0
    } else {
        text_width(ui, &lead_font, &lead)
    };
    let gap = if lead.is_empty() { 0.0 } else { fs * 0.35 };
    let val_w = text_width(ui, &val_font, &item.value);
    let total_w = lead_w + gap + val_w;

    let x0 = if align == Align2::LEFT_CENTER {
        anchor.x
    } else if align == Align2::RIGHT_CENTER {
        anchor.x - total_w
    } else {
        anchor.x - total_w * 0.5
    };
    let y = anchor.y;

    if !lead.is_empty() {
        ui.painter().text(
            Pos2::new(x0, y),
            Align2::LEFT_CENTER,
            &lead,
            lead_font,
            muted,
        );
    }
    ui.painter().text(
        Pos2::new(x0 + lead_w + gap, y),
        Align2::LEFT_CENTER,
        &item.value,
        val_font,
        text,
    );
}

fn column_order(cfg: &OverlayConfig, section: &str) -> Vec<String> {
    if let Some(arr) = cfg.section(section).get("column_order").and_then(|v| v.as_array()) {
        let cols: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    vec![
        "badge".into(),
        "position".into(),
        "name".into(),
        "license".into(),
        "irating".into(),
        "gap".into(),
    ]
}

fn width_mult(cfg: &OverlayConfig, section: &str, key: &str, default: f32) -> f32 {
    cfg.section(section)
        .get("widths")
        .and_then(|w| w.get(key))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default)
}

fn default_width(col: &str) -> f32 {
    match col {
        "badge" => 0.95,
        "position" => 1.25,
        "car_number" => 1.60,
        "gap" => 1.70,
        "irating" => 1.20,
        "license" => 1.35,
        "last_lap" | "best_lap" => 2.90,
        "gutter" => 0.18,
        _ => 1.2,
    }
}

fn license_color(cfg: &OverlayConfig, section: &str, lic: &str) -> Color32 {
    let letter = lic
        .chars()
        .next()
        .unwrap_or('R')
        .to_ascii_uppercase()
        .to_string();
    let raw = cfg
        .section(section)
        .get("license_colors")
        .and_then(|c| c.get(&letter))
        .and_then(|v| v.as_str());
    match raw {
        Some(s) => parse_color_str(s),
        None => match letter.as_str() {
            "R" => parse_color_str("#d34a3c"),
            "D" => parse_color_str("#e0791a"),
            "C" => parse_color_str("#d6b400"),
            "B" => parse_color_str("#3a9b3a"),
            "A" => parse_color_str("#2f6bd8"),
            "P" => parse_color_str("#1a1a1a"),
            _ => parse_color_str("#666666"),
        },
    }
}

fn fmt_ir(ir: i32, abbrev: bool) -> String {
    if ir <= 0 {
        return String::new();
    }
    if abbrev && ir >= 1000 {
        format!("{:.1}k", ir as f32 / 1000.0)
    } else {
        ir.to_string()
    }
}

fn truncate_sr(sr: &str) -> String {
    if let Ok(v) = sr.parse::<f32>() {
        format!("{v:.1}")
    } else {
        sr.chars().take(4).collect()
    }
}
