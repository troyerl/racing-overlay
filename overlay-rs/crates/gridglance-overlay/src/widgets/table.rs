//! Shared BaseTable-style painter for relative / standings (Python parity).

use crate::chrome::{
    color_with_alpha, contrast_text, draw_card, draw_row_tint, ease, label, soften_color,
};
use crate::config::{parse_color_str, OverlayConfig};
use crate::icons;
use crate::telemetry::{slot_label, TableRow, TableSlotItem, TableSlots};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};
use std::collections::HashMap;

const ROW_SNAP_SLOTS: f32 = 6.0;
const DENSE_ROW_COUNT: usize = 20;
const DENSE_ROW_SNAP_SLOTS: f32 = 5.0;
const DENSE_ROW_EASE_TAU: f32 = 0.10;

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
    let show_footer = cfg.bool_key(section, "show_footer", true);
    let rh = cfg.f64_key(section, "row_height_px", 36.0) as f32;
    // Python table pad: fixed row height → 8px; else max(8, h*0.025).
    // Not shared chrome `panel_pad` (h*0.08), which oversizes relative/standings.
    let pad = if rh > 0.0 {
        8.0
    } else {
        (card.height() * 0.025).max(8.0)
    };
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

    let inner_w = card.width() - 2.0 * pad;
    let left = card.left() + pad;

    // Header band: full card width; slots inset by pad (Python draw_header).
    let hdr_band = Rect::from_min_size(
        Pos2::new(card.left(), card.top()),
        Vec2::new(card.width(), pad + header_h),
    );
    let hdr_content = Rect::from_min_size(
        Pos2::new(left, card.top() + pad),
        Vec2::new(inner_w, header_h),
    );
    draw_edge_band(
        ui,
        cfg,
        section,
        hdr_band,
        hdr_content,
        radius,
        true,
        &slots.header_left,
        &slots.header_center,
        &slots.header_right,
    );

    let body_top = card.top() + pad + header_h;
    let body_bottom = if show_footer {
        card.bottom() - pad * 0.5 - footer_h
    } else {
        card.bottom() - pad
    };

    // Row motion
    let id = egui::Id::new(("table_anim", section));
    let animating = {
        let now = ui.input(|i| i.time);
        let mut anim = ui
            .ctx()
            .data_mut(|d| d.get_temp::<TableAnim>(id).unwrap_or_default());
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

        let active: std::collections::HashSet<String> = rows
            .iter()
            .filter(|r| !r.empty)
            .map(|r| r.key.clone())
            .collect();
        anim.slots.retain(|k, _| active.contains(k));

        // Fixed tau (no distance/multi scaling) — smoother multi-car reshuffles.
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
            let delta = (st.idx - target).abs();
            if delta > snap {
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

    let dividers = cfg.bool_key(section, "row_dividers", true);
    let dense = rows.len() >= DENSE_ROW_COUNT;
    let body_clip = Rect::from_min_max(
        Pos2::new(left, body_top),
        Pos2::new(left + inner_w, body_bottom),
    )
    .intersect(ui.clip_rect());
    let prev_clip = ui.clip_rect();
    ui.set_clip_rect(body_clip);
    let mut prev_draw_idx: Option<f32> = None;
    for &i in &draw_order {
        let row = &rows[i];
        if row.empty {
            continue;
        }
        let st = anim.slots.get(&row.key);
        let slot_idx = st.map(|s| s.idx).unwrap_or(i as f32);
        let opacity = st.map(|s| s.opacity).unwrap_or(1.0);
        let ry = body_top + slot_idx * rh;
        let row_rect = Rect::from_min_size(Pos2::new(left, ry), Vec2::new(inner_w, rh));
        let sliding = dense && (slot_idx - i as f32).abs() > 0.02;
        if dividers {
            if let Some(prev_idx) = prev_draw_idx {
                // Python: divider only between nearly-adjacent settled animated slots.
                if (slot_idx - prev_idx).abs() <= 1.05 && !sliding {
                    let line = cfg.color(section, "border", "#ffffff28");
                    let a = ((line.a() as f32) * 0.20).max(10.0) as u8;
                    ui.painter().line_segment(
                        [
                            Pos2::new(row_rect.left(), ry),
                            Pos2::new(row_rect.right(), ry),
                        ],
                        Stroke::new(0.35_f32, color_with_alpha(line, a)),
                    );
                }
            }
            prev_draw_idx = Some(slot_idx);
        }
        if opacity < 0.99 {
            // Soft fade-in: tint with alpha via layer (simple multiply on text later).
            ui.painter().rect_filled(
                row_rect,
                0.0,
                Color32::from_black_alpha(((1.0 - opacity) * 40.0) as u8),
            );
        }
        paint_row_chrome(
            ui,
            cfg,
            section,
            row,
            row_rect,
            slot_idx.round().max(0.0) as usize,
            alt,
        );
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
    ui.set_clip_rect(prev_clip);

    if show_footer {
        // Full-bleed footer band; slots inset (Python draw_footer).
        let band_top = card.bottom() - pad * 0.5 - footer_h;
        let ftr_band = Rect::from_min_max(
            Pos2::new(card.left(), band_top),
            Pos2::new(card.right(), card.bottom()),
        );
        let ftr_content =
            Rect::from_min_size(Pos2::new(left, band_top), Vec2::new(inner_w, footer_h));
        draw_edge_band(
            ui,
            cfg,
            section,
            ftr_band,
            ftr_content,
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
    } else if row.in_pit {
        Some("pit_row")
    } else if row.inactive && section == "standings" {
        Some("inactive_row")
    } else if row.is_speaking {
        Some("speaking_row")
    } else if section == "relative" {
        match row.strat_tag.as_deref() {
            Some("undercut") => Some("undercut_row"),
            Some("cover") => Some("cover_row"),
            _ => None,
        }
    } else {
        None
    };

    if let Some(key) = tint_key {
        let fallback = match key {
            "player_row" => "#ff941670",
            "threat" => "#ff505060",
            "lapped" => "#2563eb60",
            "pit_row" => "#8b93a118",
            "inactive_row" => "#8b93a128",
            "speaking_row" => "#22c55e50",
            "undercut_row" => "#3aa0ff44",
            "cover_row" => "#ff941644",
            _ => "#ffffff08",
        };
        let accent = cfg.color(section, key, fallback);
        draw_row_tint(ui, rect, accent);
    } else if alt && i % 2 == 1 {
        let c = cfg.color(section, "row_alt", "#ffffff01");
        let a = (c.a() as f32 * 0.25).max(1.0) as u8;
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, color_with_alpha(c, a));
    }

    // Python `_draw_speaking_accent`: bright stripe + wash on top of any status tint.
    if row.is_speaking {
        let accent = cfg.color(section, "badge_speaking_bg", "#22c55e");
        let h = rect.height();
        let stripe_w = (h * 0.09).max(3.5);
        ui.painter().rect_filled(
            Rect::from_min_size(
                Pos2::new(rect.left(), rect.top() + h * 0.10),
                Vec2::new(stripe_w, h * 0.80),
            ),
            CornerRadius::same(2),
            color_with_alpha(accent, 255),
        );
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, color_with_alpha(accent, 38));
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
    _muted: Color32,
) {
    let dim = row.in_pit || (row.inactive && section == "standings") || row.empty;
    let dim_text = cfg.color(section, "muted", "#8b93a1");
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
                                Vec2::new(rh * 0.12, rh * 0.64),
                            ),
                            CornerRadius::same(2),
                            sc,
                        );
                    }
                }
                // Python: left-aligned after class stripe inset.
                label(
                    ui,
                    Pos2::new(cx + rh * 0.2, cy),
                    Align2::LEFT_CENTER,
                    &format!("{}", row.position.max(0)),
                    fs,
                    if dim { dim_text } else { text },
                    true,
                );
            }
            "name" => {
                let colc = if dim { dim_text } else { text };
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
                let letter = row
                    .lic_class
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .unwrap_or(' ');
                let bg = soften_color(
                    license_color(cfg, section, &row.lic_class),
                    parse_color_str("#1b1f26"),
                    0.20,
                );
                // Python: `"R 3.34"` — letter + space + full SR string.
                let txt = if letter != ' ' && !row.sr.is_empty() {
                    format!("{letter} {}", row.sr)
                } else if !row.sr.is_empty() {
                    row.sr.clone()
                } else if letter != ' ' {
                    letter.to_string()
                } else {
                    "—".into()
                };
                let font_sz = fs * 0.84;
                let tw = text_advance(ui, &txt, font_sz);
                let pad_x = fs * 0.28;
                let pill_h = rh * 0.54;
                let pill_w = (tw + 2.0 * pad_x).min(cw);
                let pill = Rect::from_min_size(
                    Pos2::new(cx, cy - pill_h * 0.5),
                    Vec2::new(pill_w.max(4.0), pill_h),
                );
                let edge_a = ((bg.a() as f32 * 0.55) as u16 + 60).min(255) as u8;
                let edge = color_with_alpha(bg, edge_a);
                ui.painter().rect_filled(pill, CornerRadius::same(4), bg);
                ui.painter().rect_stroke(
                    pill,
                    CornerRadius::same(4),
                    Stroke::new(1.0_f32, edge),
                    StrokeKind::Inside,
                );
                label(
                    ui,
                    pill.center(),
                    Align2::CENTER_CENTER,
                    &txt,
                    font_sz,
                    contrast_text(bg),
                    true,
                );
            }
            "irating" => {
                paint_irating_cell(ui, cfg, section, row, cx, cy, cw, rh, fs, dim, dim_text);
            }
            "gap" => {
                let (gtxt, gcol) = gap_display(row, signed_gaps, section, cfg, text);
                label(
                    ui,
                    Pos2::new(cx + cw - gutter, cy),
                    Align2::RIGHT_CENTER,
                    &gtxt,
                    fs * gap_font_scale,
                    if dim { dim_text } else { gcol },
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
                    if dim { dim_text } else { text },
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
                    if dim { dim_text } else { text },
                    false,
                );
            }
            "pit" => {
                // Python `_draw_pit`: "PIT" while in pits, else pit_mode history / em dash.
                let s = if row.in_pit || row.on_pit {
                    "PIT"
                } else if row.pit_text.is_empty() {
                    "—"
                } else {
                    row.pit_text.as_str()
                };
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    s,
                    fs * 0.85,
                    if row.in_pit || row.on_pit {
                        cfg.color(section, "badge_pit_text", "#ffd23a")
                    } else if dim {
                        dim_text
                    } else {
                        text
                    },
                    true,
                );
            }
            "class_pos" => {
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    &format!("{}", row.class_position.max(0)),
                    fs,
                    if dim { dim_text } else { text },
                    true,
                );
            }
            "status" => {
                let s = row.status_kind.as_deref().unwrap_or("—");
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    s,
                    fs * 0.85,
                    if dim { dim_text } else { text },
                    false,
                );
            }
            "laps" => {
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    &format!("{}", row.laps.max(0)),
                    fs,
                    if dim { dim_text } else { text },
                    false,
                );
            }
            "closing" => {
                let s = row
                    .closing
                    .map(|c| format!("{c:+.2}"))
                    .unwrap_or_else(|| "—".into());
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    &s,
                    fs * 0.9,
                    if dim { dim_text } else { text },
                    false,
                );
            }
            "gap_ahead" | "gap_leader" => {
                let s = if row.gap_text.is_empty() {
                    "—"
                } else {
                    row.gap_text.as_str()
                };
                label(
                    ui,
                    Pos2::new(cx + cw - gutter, cy),
                    Align2::RIGHT_CENTER,
                    s,
                    fs * gap_font_scale,
                    if dim { dim_text } else { text },
                    false,
                );
            }
            "team" | "nickname" => {
                let s = if col == "team" {
                    &row.team
                } else {
                    &row.nickname
                };
                let show = if s.is_empty() { "—" } else { s.as_str() };
                label(
                    ui,
                    Pos2::new(cx + 4.0, cy),
                    Align2::LEFT_CENTER,
                    show,
                    fs * 0.9,
                    if dim { dim_text } else { text },
                    false,
                );
            }
            "car_flag" | "qual_pos" | "qual_best" | "gap_pole" => {
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    "—",
                    fs * 0.9,
                    dim_text,
                    false,
                );
            }
            _ => {}
        }
        cx += cw + gutter;
    }
}

fn text_advance(ui: &Ui, text: &str, size: f32) -> f32 {
    let font = FontId::proportional(size.max(1.0));
    ui.fonts(|f| f.layout_no_wrap(text.to_owned(), font, Color32::WHITE))
        .size()
        .x
}

fn paint_irating_cell(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    row: &TableRow,
    cx: f32,
    cy: f32,
    cw: f32,
    rh: f32,
    fs: f32,
    dim: bool,
    dim_text: Color32,
) {
    let cell = Rect::from_min_size(Pos2::new(cx, cy - rh * 0.3), Vec2::new(cw, rh * 0.6));
    let show_icon = cfg.bool_key(section, "irating_show_icon", true);
    let muted = if dim {
        dim_text
    } else {
        cfg.color(section, "muted", "#8b93a1")
    };
    let mut pill_left = cell.left();
    if show_icon {
        if let Some(g) = icons::glyph("irating") {
            let ic_px = cell.height() * 0.48;
            let font = icons::font_id(ic_px);
            let ic_w = ui
                .fonts(|f| f.layout_no_wrap(g.clone(), font.clone(), Color32::WHITE))
                .size()
                .x;
            ui.painter().text(
                Pos2::new(cell.left(), cell.center().y),
                Align2::LEFT_CENTER,
                g,
                font,
                muted,
            );
            pill_left = cell.left() + ic_w + fs * 0.10;
        }
    }

    let pill = Rect::from_min_max(
        Pos2::new(pill_left, cell.top()),
        Pos2::new(cell.right(), cell.bottom()),
    );
    if pill.width() < 4.0 {
        return;
    }
    ui.painter().rect_filled(
        pill,
        CornerRadius::same(4),
        cfg.color(section, "irating_bg", "#0b0d11cc"),
    );
    ui.painter().rect_stroke(
        pill,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, cfg.color(section, "irating_border", "#ffffff20")),
        StrokeKind::Inside,
    );

    let abbrev = cfg.bool_key(section, "irating_abbreviate", true);
    let ir_txt = fmt_ir(row.irating, abbrev);
    if ir_txt.is_empty() {
        label(
            ui,
            pill.center(),
            Align2::CENTER_CENTER,
            "—",
            fs * 0.82,
            muted,
            false,
        );
        return;
    }

    let ir_col = if dim {
        dim_text
    } else {
        cfg.color(section, "irating_text", "#f4f6f8")
    };
    let show_delta = cfg.bool_key(section, "show_irating_projection", false)
        && row.irating_delta.is_some()
        && ir_txt != "--";

    if show_delta {
        let delta = row.irating_delta.unwrap_or(0);
        let dcol = if delta > 0 {
            cfg.color(section, "irating_delta_up", "#46df7a")
        } else if delta < 0 {
            cfg.color(section, "irating_delta_down", "#ff5050")
        } else {
            muted
        };
        let ir_sz = fs * 0.82;
        let ir_w = text_advance(ui, &ir_txt, ir_sz);
        let gap = fs * 0.50;
        let use_icons = delta != 0
            && icons::glyph("irating_up").is_some()
            && icons::glyph("irating_down").is_some();
        let (d_w, dtxt): (f32, String) = if use_icons {
            let dtxt = format!("{}", delta.abs());
            let n_sz = fs * 0.78;
            let n_w = text_advance(ui, &dtxt, n_sz);
            let icon_slot = fs * 0.42;
            (icon_slot + fs * 0.10 + n_w, dtxt)
        } else {
            let dtxt = if delta == 0 {
                "0".into()
            } else {
                format!("{delta:+}")
            };
            (text_advance(ui, &dtxt, ir_sz), dtxt)
        };
        let total = ir_w + gap + d_w;
        let pad_x = fs * 0.18;
        let left = pill.left() + pad_x.max((pill.width() - total) * 0.5);
        label(
            ui,
            Pos2::new(left, pill.center().y),
            Align2::LEFT_CENTER,
            &ir_txt,
            ir_sz,
            ir_col,
            true,
        );
        let dx = left + ir_w + gap;
        if use_icons {
            let gname = if delta > 0 {
                "irating_up"
            } else {
                "irating_down"
            };
            if let Some(g) = icons::glyph(gname) {
                let icon_slot = fs * 0.42;
                let ifont = icons::font_id(fs * 0.55);
                ui.painter().text(
                    Pos2::new(dx + icon_slot * 0.5, pill.center().y),
                    Align2::CENTER_CENTER,
                    g,
                    ifont,
                    dcol,
                );
                label(
                    ui,
                    Pos2::new(dx + icon_slot + fs * 0.10, pill.center().y),
                    Align2::LEFT_CENTER,
                    &dtxt,
                    fs * 0.78,
                    dcol,
                    false,
                );
            }
        } else {
            label(
                ui,
                Pos2::new(dx, pill.center().y),
                Align2::LEFT_CENTER,
                &dtxt,
                ir_sz,
                dcol,
                false,
            );
        }
    } else {
        label(
            ui,
            pill.center(),
            Align2::CENTER_CENTER,
            &ir_txt,
            fs * 0.82,
            ir_col,
            true,
        );
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
            match row.strat_tag.as_deref() {
                Some("undercut") => cfg.color(section, "undercut_gap", "#3aa0ff"),
                Some("cover") => cfg.color(section, "cover_gap", "#ff9416"),
                _ if g > 0.0 => cfg.color(section, "irating_delta_down", "#ff5050"),
                _ if g < 0.0 => cfg.color(section, "irating_delta_up", "#46df7a"),
                _ => text,
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
    let size = (w.min(h)) * 0.62;
    let box_r = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(size, size));

    if row.is_speaking {
        paint_speaker_badge(ui, cfg, section, box_r);
        return;
    }
    if row.is_pro {
        ui.painter().circle_filled(
            Pos2::new(cx, cy),
            size * 0.5,
            cfg.color(section, "badge_pro", "#ffd23a"),
        );
        label(
            ui,
            Pos2::new(cx, cy),
            Align2::CENTER_CENTER,
            "★",
            size * 0.55,
            cfg.color(section, "badge_pro_text", "#141414"),
            true,
        );
        return;
    }
    if !row.group_icon.is_empty() {
        let bg = crate::config::parse_color_str(if row.group_color.is_empty() {
            "#5bb8ff"
        } else {
            &row.group_color
        });
        ui.painter()
            .circle_filled(Pos2::new(cx, cy), size * 0.5, bg);
        if let Some(g) = crate::icons::glyph(&row.group_icon) {
            label(
                ui,
                Pos2::new(cx, cy),
                Align2::CENTER_CENTER,
                &g,
                size * 0.45,
                Color32::WHITE,
                true,
            );
        }
        return;
    }
    if row.is_player {
        ui.painter().circle_filled(
            Pos2::new(cx, cy),
            size * 0.5,
            cfg.color(section, "badge_player", "#ff9416"),
        );
        return;
    }
    if row.in_pit {
        let pill_w = w.min(size * 1.55);
        let pill_h = size * 0.92;
        let pill = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(pill_w, pill_h));
        ui.painter().rect_filled(
            pill,
            CornerRadius::same(4),
            cfg.color(section, "badge_pit_bg", "#ebeef0"),
        );
        label(
            ui,
            pill.center(),
            Align2::CENTER_CENTER,
            "PIT",
            pill_h * 0.46,
            cfg.color(section, "badge_pit_text", "#141414"),
            true,
        );
        return;
    }
    if row.lapping {
        ui.painter().rect_filled(
            box_r,
            CornerRadius::same(3),
            cfg.color(section, "badge_lap", "#7638c4"),
        );
        paint_clock(ui, box_r);
        return;
    }
    if let Some(tag) = row.strat_tag.as_deref() {
        let (bg, letter) = match tag {
            "undercut" => (cfg.color(section, "badge_undercut", "#3aa0ff"), "U"),
            "cover" => (cfg.color(section, "badge_cover", "#ff9416"), "C"),
            _ => (cfg.color(section, "badge_empty_fill", "#00000078"), "?"),
        };
        ui.painter().rect_filled(box_r, CornerRadius::same(3), bg);
        label(
            ui,
            box_r.center(),
            Align2::CENTER_CENTER,
            letter,
            size * 0.55,
            cfg.color(section, "badge_strat_text", "#ffffff"),
            true,
        );
        return;
    }

    ui.painter().circle_filled(
        Pos2::new(cx, cy),
        size * 0.5,
        cfg.color(section, "badge_empty_fill", "#00000078"),
    );
    ui.painter().circle_stroke(
        Pos2::new(cx, cy),
        size * 0.5,
        Stroke::new(
            1.0_f32,
            cfg.color(section, "badge_empty_border", "#ffffff28"),
        ),
    );
}

fn paint_speaker_badge(ui: &mut Ui, cfg: &OverlayConfig, section: &str, box_r: Rect) {
    let Some(g) = icons::glyph("speaking") else {
        return;
    };
    let pad = box_r.width() * 0.06;
    let pill = box_r.expand(pad);
    let border = cfg.color(section, "badge_speaking_border", "#ffffffcc");
    let bg = cfg.color(section, "badge_speaking_bg", "#22c55e");
    let fg = cfg.color(section, "badge_speaking_text", "#ffffff");
    ui.painter()
        .circle_filled(pill.center(), pill.width() * 0.5, bg);
    ui.painter().circle_stroke(
        pill.center(),
        pill.width() * 0.5,
        Stroke::new((box_r.width() * 0.08).max(1.2), border),
    );
    ui.painter().text(
        pill.center(),
        Align2::CENTER_CENTER,
        g,
        icons::font_id(pill.height() * 0.58),
        fg,
    );
}

fn paint_clock(ui: &mut Ui, box_r: Rect) {
    let stroke_w = (box_r.width() * 0.08).max(1.0);
    let white = Color32::from_rgb(255, 255, 255);
    let inner = box_r.shrink2(Vec2::new(box_r.width() * 0.22, box_r.height() * 0.22));
    ui.painter().circle_stroke(
        inner.center(),
        inner.width() * 0.5,
        Stroke::new(stroke_w, white),
    );
    let c = inner.center();
    ui.painter().line_segment(
        [c, Pos2::new(c.x, c.y - inner.height() * 0.32)],
        Stroke::new(stroke_w, white),
    );
    ui.painter().line_segment(
        [c, Pos2::new(c.x + inner.width() * 0.26, c.y)],
        Stroke::new(stroke_w, white),
    );
}

fn draw_edge_band(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    band: Rect,
    content: Rect,
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
    ui.painter().rect_filled(band, cr, bg);
    let muted = cfg.color(section, "muted", "#8b93a1");
    let text = cfg.color(section, "text", "#f4f6f8");
    // Font from content height (Python: h * 0.42), not the padded band.
    let fs = (content.height() * 0.42).clamp(9.0, 16.0) * cfg.text_scale(section);
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
        Pos2::new(content.left(), content.center().y),
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
        content.center(),
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
        Pos2::new(content.right(), content.center().y),
        Align2::RIGHT_CENTER,
        fs,
        muted,
        text,
    );
}

fn text_width(ui: &Ui, font: &FontId, s: &str) -> f32 {
    ui.fonts(|f| {
        f.layout_no_wrap(s.to_owned(), font.clone(), Color32::WHITE)
            .size()
            .x
    })
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
        label(
            ui,
            pill.center(),
            Align2::CENTER_CENTER,
            "ORDER",
            fs * 0.72,
            muted,
            true,
        );
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
    if let Some(arr) = cfg
        .section(section)
        .get("column_order")
        .and_then(|v| v.as_array())
    {
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
        "gap" | "gap_ahead" | "gap_leader" | "gap_pole" => 1.70,
        "irating" => 1.20,
        "license" => 1.35,
        "pit" => 2.10,
        "last_lap" | "best_lap" | "qual_best" => 2.90,
        "class_pos" | "status" | "car_flag" | "laps" | "qual_pos" => 1.35,
        "closing" => 1.80,
        "team" | "nickname" => 2.20,
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
