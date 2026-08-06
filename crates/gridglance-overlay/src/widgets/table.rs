//! Shared BaseTable-style painter for relative / standings (Python parity).

use crate::chrome::{
    color_with_alpha, contrast_text, draw_card, draw_row_tint, ease, label, soften_color,
};
use crate::config::{parse_color_str, OverlayConfig};
use crate::icons;
use crate::telemetry::{slot_label, TableRow, TableSlotItem, TableSlots};
use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Ui, Vec2};
use std::collections::HashMap;

/// Snap only on large jumps — one/two-slot position swaps must ease.
const ROW_SNAP_SLOTS: f32 = 6.0;
const DENSE_ROW_COUNT: usize = 20;
const DENSE_ROW_SNAP_SLOTS: f32 = 5.0;
/// Relative passes skip the player row (|Δidx|≈2); use a high threshold so
/// those slides ease instead of teleporting. Larger window jumps still snap.
const RELATIVE_ROW_SNAP_SLOTS: f32 = 8.0;
/// Default slide length (seconds). Same duration for every row so multi-row
/// reorders finish together instead of each chasing with its own exponential.
const DEFAULT_ROW_SLIDE_S: f32 = 0.24;

#[derive(Clone, Default)]
struct RowAnimState {
    /// Current visual slot (may be mid-slide).
    idx: f32,
    /// Slide start visual index.
    from: f32,
    /// Slide destination slot.
    to: f32,
    /// Host mono time when this slide started.
    t0: f64,
    opacity: f32,
}

#[derive(Clone, Default)]
struct TableAnim {
    /// key -> visual slot + fade
    slots: HashMap<String, RowAnimState>,
    last_order: Vec<String>,
    last_paint_secs: f64,
}

/// Smooth ease-in-out so batches of rows accelerate/decelerate together.
fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        let u = -2.0 * t + 2.0;
        1.0 - (u * u * u) * 0.5
    }
}

/// Vertical scroll (in row slots) so the focus car stays visible with behind rows.
fn standings_paint_scroll(
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
    visible_slots: usize,
) -> f32 {
    if section != "standings" || !cfg.bool_key(section, "center_on_player", true) {
        return 0.0;
    }
    if rows.len() <= visible_slots {
        return 0.0;
    }
    let Some(fi) = rows.iter().position(|r| r.is_player && !r.empty) else {
        return 0.0;
    };
    let ahead_cfg = cfg.f64_key(section, "rows_ahead", 4.0).max(0.0) as usize;
    let behind_cfg = cfg.f64_key(section, "rows_behind", 5.0).max(0.0) as usize;
    let behind_in_list = rows.len().saturating_sub(fi + 1);
    let ideal_ahead = ahead_cfg.min(fi);
    // Keep some behind rows on-screen when the focus is near the front.
    let min_behind = if behind_in_list > 0 && behind_cfg > 0 {
        behind_cfg
            .min(behind_in_list)
            .min((visible_slots / 2).max(1))
    } else {
        0
    };
    let max_ahead = visible_slots.saturating_sub(1 + min_behind);
    let want_ahead = ideal_ahead.min(max_ahead);
    fi.saturating_sub(want_ahead) as f32
}

pub fn paint_table(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
    slots: &TableSlots,
    signed_gaps: bool,
    mono_secs: f64,
    panel_animating: &mut bool,
) {
    let rect = crate::chrome::full_rect(ui);
    let (card, radius) = draw_card(ui, cfg, section, rect);
    let show_footer = cfg.bool_key(section, "show_footer", true);
    // Dense defaults — legacy 36px rows left large empty bands around text.
    let rh_cfg = cfg.f64_key(section, "row_height_px", 28.0) as f32;
    let rh = if (rh_cfg - 36.0).abs() < 0.01 {
        28.0
    } else {
        rh_cfg.max(16.0)
    };
    // Horizontal inset for band slots / row body.
    let pad = if rh > 0.0 {
        6.0
    } else {
        (card.height() * 0.02).max(6.0)
    };
    let scale = cfg.text_scale(section);
    let font_scale_cfg = cfg.f64_key(section, "font_scale", 0.48) as f32;
    let font_scale = if (font_scale_cfg - 0.40).abs() < 0.001 {
        0.48
    } else {
        font_scale_cfg
    };
    let gap_font_scale = cfg.f64_key(section, "gap_font_scale", 1.12) as f32;
    let hscale = cfg.f64_key(section, "header_font_scale", 1.0) as f32;
    let fscale = cfg.f64_key(section, "footer_font_scale", 1.0) as f32;
    // Compact chrome bands — height is the band itself (no stacked vertical pad).
    let header_h = (rh * 0.50 * scale * hscale.max(0.3)).max(15.0);
    let footer_h = if show_footer {
        (rh * 0.48 * scale * fscale.max(0.3)).max(14.0)
    } else {
        0.0
    };

    let inner_w = card.width() - 2.0 * pad;
    let left = card.left() + pad;

    // Header band: full card width; slots inset horizontally only.
    let hdr_band = Rect::from_min_size(
        Pos2::new(card.left(), card.top()),
        Vec2::new(card.width(), header_h),
    );
    let hdr_content = Rect::from_min_size(
        Pos2::new(left, card.top()),
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

    let body_top = card.top() + header_h;
    let body_bottom = if show_footer {
        card.bottom() - footer_h
    } else {
        card.bottom() - pad * 0.5
    };

    // Row motion: fixed-duration ease-in-out keyed to wall clock so irregular
    // paint gaps don't stutter, and every row in a multi-swap finishes together.
    let id = egui::Id::new(("table_anim", section));
    let now = mono_secs;
    let animating = {
        let mut anim = ui
            .ctx()
            .data_mut(|d| d.get_temp::<TableAnim>(id).unwrap_or_default());

        // `row_ease_tau` is the slide duration in seconds (legacy name).
        let mut slide_s = cfg.f64_key(section, "row_ease_tau", DEFAULT_ROW_SLIDE_S as f64) as f32;
        let mut fade_tau = cfg.f64_key(section, "fade_ease_tau", 0.12) as f32;
        if !(slide_s.is_finite() && slide_s > 0.05) {
            slide_s = DEFAULT_ROW_SLIDE_S;
        }
        if !(fade_tau.is_finite() && fade_tau > 1e-3) {
            fade_tau = 0.12;
        }
        // Dense fields: keep the same duration (don't rush) so big reshuffles
        // stay readable instead of snapping past each other.
        let dense = rows.len() >= DENSE_ROW_COUNT;
        let snap = if section == "relative" {
            RELATIVE_ROW_SNAP_SLOTS
        } else if dense {
            DENSE_ROW_SNAP_SLOTS
        } else {
            ROW_SNAP_SLOTS
        };

        let order: Vec<String> = rows
            .iter()
            .filter(|r| !r.empty)
            .map(|r| r.key.clone())
            .collect();
        if order != anim.last_order {
            anim.last_order = order.clone();
        }

        let active: std::collections::HashSet<String> = order.into_iter().collect();
        anim.slots.retain(|k, _| active.contains(k));

        let fade_dt = if anim.last_paint_secs > 0.0 {
            ((now - anim.last_paint_secs) as f32).clamp(0.0, 0.1)
        } else {
            1.0 / 60.0
        };
        anim.last_paint_secs = now;

        let mut still = false;
        for (i, row) in rows.iter().enumerate() {
            if row.empty {
                continue;
            }
            let target = i as f32;
            // Start visible — under race-load ULW throttling a 0→1 fade could
            // leave the whole table invisible if paints are sparse.
            let st = anim.slots.entry(row.key.clone()).or_insert(RowAnimState {
                idx: target,
                from: target,
                to: target,
                t0: now,
                opacity: 1.0,
            });

            // Retarget: restart a timed slide from the current visual position.
            // Mid-flight retargets keep moving (no snap) so multi-row cascades
            // stay smooth instead of teleporting on the next telem tick.
            if (st.to - target).abs() > 0.01 {
                let jump = (st.idx - target).abs();
                if jump > snap {
                    st.idx = target;
                    st.from = target;
                    st.to = target;
                    st.t0 = now;
                } else {
                    st.from = st.idx;
                    st.to = target;
                    st.t0 = now;
                }
            }

            let t = if slide_s <= 1e-4 {
                1.0
            } else {
                ((now - st.t0) as f32 / slide_s).clamp(0.0, 1.0)
            };
            if t >= 1.0 {
                st.idx = st.to;
            } else {
                st.idx = st.from + (st.to - st.from) * ease_in_out_cubic(t);
            }
            st.opacity = ease(st.opacity, 1.0, fade_dt, fade_tau);
            if t < 1.0 - 1e-3 || (st.opacity - 1.0).abs() > 0.01 {
                still = true;
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, anim));
        still
    };
    *panel_animating = animating;
    if animating {
        // Schedule almost immediately after the current frame. request_repaint_after
        // is a post-frame delay, so 16 ms plus rendering time only reached ~35 FPS.
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1));
    }

    let anim = ui
        .ctx()
        .data(|d| d.get_temp::<TableAnim>(id))
        .unwrap_or_default();

    let columns = column_order(cfg, section);
    let gutter = rh * width_mult(cfg, section, "gutter", 0.12);
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

    // When the panel is shorter than the row window, scroll so the focus car
    // stays on-screen with configured behind rows (fixes spectating near P1–P5
    // where the window starts at the leader and clips everyone behind).
    let visible_slots = ((body_bottom - body_top) / rh).floor().max(1.0) as usize;
    let scroll = standings_paint_scroll(cfg, section, rows, visible_slots);

    let mut prev_draw_idx: Option<f32> = None;
    for &i in &draw_order {
        let row = &rows[i];
        let st = anim.slots.get(&row.key);
        // Empty pads keep a fixed slot index so the configured window stays filled.
        let slot_idx = if row.empty {
            i as f32 - scroll
        } else {
            st.map(|s| s.idx).unwrap_or(i as f32) - scroll
        };
        let opacity = if row.empty {
            1.0
        } else {
            st.map(|s| s.opacity).unwrap_or(1.0)
        };
        let ry = body_top + slot_idx * rh;
        let row_rect = Rect::from_min_size(Pos2::new(left, ry), Vec2::new(inner_w, rh));
        // Skip rows wholly outside the body (scrolled away).
        if row_rect.bottom() < body_top - rh || row_rect.top() > body_bottom + rh {
            continue;
        }
        let sliding = !row.empty && dense && (slot_idx + scroll - i as f32).abs() > 0.02;
        if dividers {
            if let Some(prev_idx) = prev_draw_idx {
                // Divider only between nearly-adjacent settled animated slots.
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
        if row.empty {
            // Placeholder slot: alt shading only (no badges/text).
            paint_row_chrome(
                ui,
                cfg,
                section,
                row,
                row_rect,
                slot_idx.round().max(0.0) as usize,
                alt,
            );
            continue;
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
        // When the podium is pinned, separate it only when the next displayed
        // row skips P4. A continuous P1-P4 list needs no boundary.
        let fourth_is_next = rows
            .get(i + 1)
            .map(|next| !next.empty && next.position == 4)
            .unwrap_or(false);
        if section == "standings"
            && cfg.bool_key("standings", "pin_podium", false)
            && row.position == 3
            && !fourth_is_next
        {
            let y = row_rect.bottom() - 1.0;
            ui.painter().line_segment(
                [
                    Pos2::new(row_rect.left(), y),
                    Pos2::new(row_rect.right(), y),
                ],
                Stroke::new(
                    2.0_f32,
                    cfg.color("standings", "podium_separator", "#22c55e"),
                ),
            );
        }
    }
    ui.set_clip_rect(prev_clip);

    if show_footer {
        // Full-bleed footer band; slots inset horizontally only.
        let band_top = card.bottom() - footer_h;
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
        // iRacing Relative: red = car lapping you; blue = traffic you're lapping.
        Some(if row.lap_ahead { "threat" } else { "lapped" })
    } else if row.in_pit || row.on_pit {
        Some("pit_row")
    } else if row.inactive {
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
            // Neutral muted wash for pit / away (not yellow — that reads as player/warn).
            "pit_row" => "#6b728040",
            "inactive_row" => "#6b728048",
            "speaking_row" => "#22c55e50",
            "undercut_row" => "#3aa0ff44",
            "cover_row" => "#ff941644",
            _ => "#ffffff08",
        };
        let accent = cfg.color(section, key, fallback);
        // Old defaults used a near-invisible cool grey (α≈0x18) or the short-lived
        // olive tint — treat both as unset so the neutral disabled wash applies.
        let accent = if key == "pit_row" || key == "inactive_row" {
            let warm = accent.r() > accent.b().saturating_add(20);
            if accent.a() < 0x30 || warm {
                parse_color_str(fallback)
            } else {
                accent
            }
        } else {
            accent
        };
        if key == "pit_row" || key == "inactive_row" {
            // Even muted fill (not the left accent stripe used for live rows).
            ui.painter().rect_filled(rect, CornerRadius::ZERO, accent);
        } else {
            draw_row_tint(ui, rect, accent);
        }
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

/// iRacing Relative ink: red when they lap you, blue when you lap them.
fn lap_traffic_ink(cfg: &OverlayConfig, section: &str, row: &TableRow) -> Option<Color32> {
    if !row.lapping || row.is_player || row.empty {
        return None;
    }
    let (key, fallback) = if row.lap_ahead {
        ("threat", "#ff5050")
    } else {
        ("lapped", "#2563eb")
    };
    Some(color_with_alpha(cfg.color(section, key, fallback), 255))
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
    let dim = row.in_pit || row.on_pit || row.inactive || row.empty;
    let dim_text = cfg.color(section, "row_dim_text", "#5a616c");
    let lap_ink = lap_traffic_ink(cfg, section, row);
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
            "badge" => paint_badge(ui, cfg, section, row, cx, rect.top(), cw, rh, dim, dim_text),
            "position" => {
                if stripe && !row.class_color.is_empty() {
                    let sc = parse_color_str(&row.class_color);
                    // Skip parse fallback magenta (#ff00ff) from bad class colors.
                    let is_fallback = sc.r() == 255 && sc.g() == 0 && sc.b() == 255;
                    if !is_fallback {
                        let stripe_col = if dim {
                            color_with_alpha(soften_color(sc, dim_text, 0.55), 160)
                        } else {
                            sc
                        };
                        ui.painter().rect_filled(
                            Rect::from_min_size(
                                Pos2::new(cx, rect.top() + rh * 0.18),
                                Vec2::new(rh * 0.12, rh * 0.64),
                            ),
                            CornerRadius::same(2),
                            stripe_col,
                        );
                    }
                }
                // Python: left-aligned after class stripe inset.
                let pos_col = if dim {
                    dim_text
                } else {
                    lap_ink.unwrap_or(text)
                };
                label(
                    ui,
                    Pos2::new(cx + rh * 0.2, cy),
                    Align2::LEFT_CENTER,
                    &format!("{}", row.position.max(0)),
                    fs,
                    pos_col,
                    true,
                );
            }
            "name" => {
                let colc = if dim {
                    dim_text
                } else {
                    lap_ink.unwrap_or(text)
                };
                let bold = cfg.bool_key(section, "name_font_bold", true);
                let mut text_x = cx + 4.0;
                // Driver-group / league icons sit beside the name — not in the
                // status badge (trophy groups were colliding with session-best).
                if !row.is_pro && !row.group_icon.is_empty() {
                    if let Some(g) = icons::glyph(&row.group_icon) {
                        let ic_px = (rh * 0.42).clamp(10.0, fs * 1.15);
                        let gap = (rh * 0.08).max(3.0);
                        let icon_col = if row.group_color.is_empty() {
                            parse_color_str("#5bb8ff")
                        } else {
                            parse_color_str(&row.group_color)
                        };
                        let font = icons::font_id(ic_px);
                        let gw = ui
                            .fonts(|f| f.layout_no_wrap(g.clone(), font.clone(), Color32::WHITE))
                            .size()
                            .x;
                        ui.painter().text(
                            Pos2::new(text_x, cy),
                            Align2::LEFT_CENTER,
                            g,
                            font,
                            if dim {
                                color_with_alpha(icon_col, 140)
                            } else {
                                icon_col
                            },
                        );
                        text_x += gw + gap;
                    }
                }
                let name_right = cx + cw - 2.0;
                if text_x < name_right {
                    let prev_clip = ui.clip_rect();
                    ui.set_clip_rect(
                        Rect::from_min_max(
                            Pos2::new(text_x, rect.top()),
                            Pos2::new(name_right, rect.bottom()),
                        )
                        .intersect(prev_clip),
                    );
                    label(
                        ui,
                        Pos2::new(text_x, cy),
                        Align2::LEFT_CENTER,
                        &row.name,
                        fs,
                        colc,
                        bold,
                    );
                    ui.set_clip_rect(prev_clip);
                }
            }
            "license" => {
                let letter = row
                    .lic_class
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .unwrap_or(' ');
                let mut bg = soften_color(
                    license_color(cfg, section, &row.lic_class),
                    parse_color_str("#1b1f26"),
                    0.20,
                );
                if dim {
                    bg = soften_color(bg, dim_text, 0.55);
                    bg = color_with_alpha(bg, 150);
                }
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
                    if dim { dim_text } else { contrast_text(bg) },
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
                let num = crate::telemetry::format_car_number(&row.car_number);
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    &num,
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
                // Purple session-best: best column for the FL holder; last column
                // when that lap matches their best (just set purple).
                let fl = !dim
                    && !v.is_empty()
                    && v != "—"
                    && v != "--"
                    && row.session_best
                    && (col == "best_lap" || row.last_lap == row.best_lap);
                let tcol = if dim {
                    dim_text
                } else if fl {
                    cfg.color(section, "session_best", "#c084fc")
                } else {
                    text
                };
                label(
                    ui,
                    Pos2::new(cx + cw * 0.5, cy),
                    Align2::CENTER_CENTER,
                    s,
                    fs * 0.92,
                    tcol,
                    fl,
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
                    if dim {
                        dim_text
                    } else if row.in_pit || row.on_pit {
                        cfg.color(section, "badge_pit_text", "#ffd23a")
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
            "car_flag" => {
                let s = row.car_flag.as_deref().unwrap_or("—");
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
    dim: bool,
    dim_text: Color32,
) {
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    let size = (w.min(h)) * 0.62;
    let box_r = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(size, size));

    if row.is_speaking {
        paint_speaker_badge(ui, cfg, section, box_r);
        return;
    }
    if row.session_best {
        paint_session_best_badge(ui, cfg, section, box_r, size);
        return;
    }
    if row.is_pro {
        let bg = if dim {
            color_with_alpha(
                soften_color(cfg.color(section, "badge_pro", "#ffd23a"), dim_text, 0.45),
                170,
            )
        } else {
            cfg.color(section, "badge_pro", "#ffd23a")
        };
        ui.painter()
            .circle_filled(Pos2::new(cx, cy), size * 0.5, bg);
        label(
            ui,
            Pos2::new(cx, cy),
            Align2::CENTER_CENTER,
            "★",
            size * 0.55,
            if dim {
                dim_text
            } else {
                cfg.color(section, "badge_pro_text", "#141414")
            },
            true,
        );
        return;
    }
    // Driver-group icons render beside the name, not here.
    if row.is_player {
        let bg = if dim {
            color_with_alpha(
                soften_color(
                    cfg.color(section, "badge_player", "#ff9416"),
                    dim_text,
                    0.45,
                ),
                170,
            )
        } else {
            cfg.color(section, "badge_player", "#ff9416")
        };
        ui.painter()
            .circle_filled(Pos2::new(cx, cy), size * 0.5, bg);
        return;
    }
    if row.in_pit || row.on_pit {
        let pill_w = w.min(size * 1.55);
        let pill_h = size * 0.92;
        let pill = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(pill_w, pill_h));
        let mut bg = cfg.color(section, "badge_pit_bg", "#ebeef0");
        if dim {
            bg = color_with_alpha(soften_color(bg, dim_text, 0.50), 160);
        }
        ui.painter().rect_filled(pill, CornerRadius::same(4), bg);
        label(
            ui,
            pill.center(),
            Align2::CENTER_CENTER,
            "PIT",
            pill_h * 0.46,
            if dim {
                dim_text
            } else {
                cfg.color(section, "badge_pit_text", "#141414")
            },
            true,
        );
        return;
    }
    // Lapped traffic uses row tint only — no clock badge (looked like
    // multiple "fast lap" icons). Strategy U/C only when the tag is known.
    if let Some(tag) = row.strat_tag.as_deref() {
        let (raw_bg, letter) = match tag {
            "undercut" => (cfg.color(section, "badge_undercut", "#3aa0ff"), "U"),
            "cover" => (cfg.color(section, "badge_cover", "#ff9416"), "C"),
            _ => {
                // Unknown tag: fall through to the empty status dot.
                paint_empty_badge(ui, cfg, section, cx, cy, size);
                return;
            }
        };
        let bg = if dim {
            color_with_alpha(soften_color(raw_bg, dim_text, 0.50), 160)
        } else {
            raw_bg
        };
        ui.painter().rect_filled(box_r, CornerRadius::same(3), bg);
        label(
            ui,
            box_r.center(),
            Align2::CENTER_CENTER,
            letter,
            size * 0.55,
            if dim {
                dim_text
            } else {
                cfg.color(section, "badge_strat_text", "#ffffff")
            },
            true,
        );
        return;
    }

    paint_empty_badge(ui, cfg, section, cx, cy, size);
}

fn paint_empty_badge(ui: &mut Ui, cfg: &OverlayConfig, section: &str, cx: f32, cy: f32, size: f32) {
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

fn paint_session_best_badge(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    section: &str,
    box_r: Rect,
    _size: f32,
) {
    // Purple clock = session fastest lap (one per table). Lapped traffic is
    // row tint only — no badge icon.
    ui.painter().rect_filled(
        box_r,
        CornerRadius::same(3),
        cfg.color(section, "badge_session_best", "#7638c4"),
    );
    paint_clock(ui, box_r);
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
    // Font from content height — fill the compact band.
    let fs = (content.height() * 0.52).clamp(9.0, 16.0) * cfg.text_scale(section);
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
        "gap" | "gap_ahead" | "gap_leader" => 1.70,
        "irating" => 1.20,
        "license" => 1.35,
        "pit" => 2.10,
        "last_lap" | "best_lap" => 2.90,
        "class_pos" | "status" | "car_flag" | "laps" => 1.35,
        "closing" => 1.80,
        "team" | "nickname" => 2.20,
        "gutter" => 0.12,
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
