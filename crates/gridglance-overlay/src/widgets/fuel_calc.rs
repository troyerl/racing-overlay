//! Fuel calculator — Python `fuel_calc.py` parity.

use super::WidgetCtx;
use crate::chrome::{color_with_alpha, draw_dark_cell, full_rect, is_elegant, label, panel_card};
use crate::telemetry::{FuelCalcState, FuelScenario};
use egui::{Align2, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};

const SECTION: &str = "fuel_calc";
const STAT_COLS: &[&str] = &["usage", "laps", "pits", "refuel"];
const STAT_ROWS: &[&str] = &["avg", "max", "min"];

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    if is_elegant(ctx.cfg, SECTION) {
        paint_elegant(ui, ctx);
    } else {
        paint_data(ui, ctx);
    }
}

fn paint_data(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let w = card.width();
    let h = card.height();
    let m = w * 0.04;
    let inner = w - 2.0 * m;
    let d = &ctx.frame.fuel;

    let bar_h = (h * 0.018).max(3.0);
    ui.painter().rect_filled(
        Rect::from_min_size(
            Pos2::new(card.left() + m, card.top() + h * 0.012),
            Vec2::new(inner, bar_h),
        ),
        CornerRadius::same(2),
        ctx.cfg.color(SECTION, "accent", "#e23b3b"),
    );

    let show_pill = ctx.cfg.bool_key(SECTION, "show_pill", true);
    let show_add = ctx.cfg.bool_key(SECTION, "show_add", true);
    let show_gauge = ctx.cfg.bool_key(SECTION, "show_gauge", true);
    let top_on = show_pill || show_add || show_gauge;
    let show_time = ctx.cfg.bool_key(SECTION, "show_time", true);
    let show_laps = ctx.cfg.bool_key(SECTION, "show_laps", true);

    let mut blocks: Vec<(&str, f32)> = Vec::new();
    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        blocks.push(("title", 0.55));
    }
    if top_on {
        blocks.push(("top", 1.15));
    }
    if ctx.cfg.bool_key(SECTION, "show_stats", true) {
        blocks.push(("stats", 2.6));
    }
    if ctx.cfg.bool_key(SECTION, "show_strip", true) {
        blocks.push(("strip", 0.6));
    }
    if show_time {
        blocks.push(("time", 0.95));
    }
    if show_laps {
        blocks.push(("laps", 0.95));
    }
    if blocks.is_empty() {
        return;
    }

    let content_top = card.top() + h * 0.012 + bar_h + h * 0.015;
    let content_bottom = card.top() + h * 0.985;
    let gap = (h * 0.02).max(4.0);
    let sumw: f32 = blocks.iter().map(|(_, wt)| wt).sum();
    let avail = (content_bottom - content_top) - gap * (blocks.len() as f32 - 1.0);
    let mut heights: Vec<f32> = blocks.iter().map(|(_, wt)| avail * wt / sumw).collect();

    // Shrink-wrap stats so the PIT strip sits just under the table (Python parity).
    if let Some(i) = blocks.iter().position(|(k, _)| *k == "stats") {
        let needed = stats_content_height(ctx, heights[i]);
        if needed < heights[i] {
            heights[i] = needed;
        }
        // Never let stats draw past their slot.
        heights[i] = heights[i].max(needed.min(heights[i]));
    }

    let mut cy = content_top;
    for (i, (key, _)) in blocks.iter().enumerate() {
        let bh = heights[i];
        let x = card.left() + m;
        match *key {
            "title" => {
                let title = ctx.cfg.str_key(SECTION, "title", "FUEL CALCULATOR");
                let band = Rect::from_min_size(Pos2::new(x, cy), Vec2::new(inner, bh));
                crate::chrome::draw_section_header(ui, ctx.cfg, SECTION, band, &title, radius);
            }
            "top" => draw_top(
                ui, ctx, d, x, cy, inner, bh, show_pill, show_add, show_gauge,
            ),
            "stats" => draw_stats(ui, ctx, d, x, cy, inner, bh),
            "strip" => draw_strip(ui, ctx, d, x, cy, inner, bh),
            "time" => draw_box(
                ui,
                ctx,
                "TIME UNTIL EMPTY",
                &fmt_hms(d.time_empty),
                &fmt_signed_hms(d.time_margin),
                d.time_margin,
                x,
                cy,
                inner,
                bh,
            ),
            "laps" => draw_box(
                ui,
                ctx,
                "LAPS UNTIL EMPTY",
                &fmt1(d.laps_empty),
                &signed1(d.laps_margin),
                d.laps_margin,
                x,
                cy,
                inner,
                bh,
            ),
            _ => {}
        }
        // Extra breathing room after the stats table so the PIT strip never collides.
        let extra = if *key == "stats" { gap.max(6.0) } else { 0.0 };
        cy += bh + gap + extra;
    }
}

/// Elegant: fixed sequential layout (no proportional crush).
fn paint_elegant(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, _radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = 10.0_f32;
    let gap = 8.0_f32;
    let x = card.left() + pad;
    let inner = (card.width() - 2.0 * pad).max(40.0);
    let d = &ctx.frame.fuel;
    let muted = color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200);
    let mut y = card.top() + pad;

    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        label(
            ui,
            Pos2::new(x, y + 7.0),
            Align2::LEFT_CENTER,
            &ctx.cfg.str_key(SECTION, "title", "FUEL CALCULATOR"),
            11.0,
            muted,
            false,
        );
        y += 18.0;
    }

    let show_pill = ctx.cfg.bool_key(SECTION, "show_pill", true);
    let show_add = ctx.cfg.bool_key(SECTION, "show_add", true);
    let show_gauge = ctx.cfg.bool_key(SECTION, "show_gauge", true);
    if show_pill || show_add || show_gauge {
        let top_h = 56.0_f32;
        draw_top_elegant(
            ui, ctx, d, x, y, inner, top_h, show_pill, show_add, show_gauge,
        );
        y += top_h + gap;
    }

    let show_time = ctx.cfg.bool_key(SECTION, "show_time", true);
    let show_laps = ctx.cfg.bool_key(SECTION, "show_laps", true);
    let box_h = 40.0_f32;
    if show_time {
        draw_box_elegant(
            ui,
            ctx,
            "TIME UNTIL EMPTY",
            &fmt_hms(d.time_empty),
            &fmt_signed_hms(d.time_margin),
            d.time_margin,
            x,
            y,
            inner,
            box_h,
        );
        y += box_h + gap;
    }
    if show_laps {
        draw_box_elegant(
            ui,
            ctx,
            "LAPS UNTIL EMPTY",
            &fmt1(d.laps_empty),
            &signed1(d.laps_margin),
            d.laps_margin,
            x,
            y,
            inner,
            box_h,
        );
    }
}

fn draw_top_elegant(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    show_pill: bool,
    show_add: bool,
    show_gauge: bool,
) {
    let left_on = show_pill || show_add;
    if show_gauge && !left_on {
        draw_gauge_elegant(ui, ctx, d, x, y, w, h);
        return;
    }
    let left_w = if show_gauge { w * 0.42 } else { w };
    let g = 6.0;
    if show_pill && show_add {
        let half = (left_w - g) * 0.5;
        draw_pill(ui, ctx, d, x, y, half, h);
        draw_add(ui, ctx, d, x + half + g, y, half, h);
    } else if show_pill {
        draw_pill(ui, ctx, d, x, y, left_w, h);
    } else if show_add {
        draw_add(ui, ctx, d, x, y, left_w, h);
    }
    if show_gauge {
        let gx = x + left_w + g;
        draw_gauge_elegant(ui, ctx, d, gx, y, (w - left_w - g).max(40.0), h);
    }
}

fn draw_top(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    show_pill: bool,
    show_add: bool,
    show_gauge: bool,
) {
    let left_on = show_pill || show_add;
    if show_gauge && !left_on {
        draw_gauge(ui, ctx, d, x, y, w, h);
        return;
    }
    let left_w = if show_gauge { w * 0.46 } else { w };
    if show_pill && show_add {
        let pill_w = left_w * 0.46;
        draw_pill(ui, ctx, d, x, y, pill_w, h);
        let ax = x + pill_w + w * 0.015;
        draw_add(ui, ctx, d, ax, y, x + left_w - ax, h);
    } else if show_pill {
        draw_pill(ui, ctx, d, x, y, left_w, h);
    } else if show_add {
        draw_add(ui, ctx, d, x, y, left_w, h);
    }
    if show_gauge {
        let gx = x + left_w + w * 0.04;
        draw_gauge(ui, ctx, d, gx, y, w - left_w - w * 0.04, h);
    }
}

fn draw_pill(ui: &mut Ui, ctx: &WidgetCtx<'_>, d: &FuelCalcState, x: f32, y: f32, w: f32, h: f32) {
    let pill = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    let bg = if d.window_open {
        ctx.cfg.color(SECTION, "pill_open", "#46df7a")
    } else {
        ctx.cfg.color(SECTION, "pill_closed", "#6e747d")
    };
    ui.painter().rect_filled(pill, CornerRadius::same(8), bg);
    let fg = ctx.cfg.color(SECTION, "pill_text", "#06210f");
    let title_sz = (h * 0.28).clamp(9.0, 15.0);
    let sub_sz = (h * 0.18).clamp(8.0, 11.0);
    label(
        ui,
        Pos2::new(pill.center().x, pill.top() + h * 0.34),
        Align2::CENTER_CENTER,
        if d.window_open { "OPEN" } else { "CLOSED" },
        title_sz,
        fg,
        true,
    );
    let sub = match d.window {
        Some((a, b)) => format!("L{a}-{b}"),
        None => "—".into(),
    };
    label(
        ui,
        Pos2::new(pill.center().x, pill.top() + h * 0.70),
        Align2::CENTER_CENTER,
        &sub,
        sub_sz,
        fg,
        true,
    );
}

fn draw_add(ui: &mut Ui, ctx: &WidgetCtx<'_>, d: &FuelCalcState, x: f32, y: f32, w: f32, h: f32) {
    let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    let elegant = is_elegant(ctx.cfg, SECTION);
    if elegant {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(10),
            color_with_alpha(ctx.cfg.color(SECTION, "cell_dark", "#0b0e12"), 70),
        );
    } else {
        draw_dark_cell(ui, ctx.cfg, SECTION, rect, 8.0);
    }
    let txt = match d.add {
        Some(add) => {
            let v = ctx.cfg.conv_fuel(add);
            format!("+{v:.1}{}", ctx.cfg.fuel_unit())
        }
        None => "—".into(),
    };
    label(
        ui,
        rect.center(),
        Align2::CENTER_CENTER,
        &txt,
        (h * 0.40).clamp(11.0, 20.0),
        ctx.cfg.color(SECTION, "add_text", "#f4f6f8"),
        true,
    );
}

/// Compact gauge for Elegant top row — bar + single fuel readout (no stacked hints).
fn draw_gauge_elegant(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let show_low_alert = ctx.cfg.bool_key(SECTION, "show_low_fuel_alert", true);
    let alert = d.alert && show_low_alert;
    let bar = Rect::from_min_size(
        Pos2::new(x, y + 4.0),
        Vec2::new(w, (h * 0.42).clamp(14.0, 22.0)),
    );
    ui.painter().rect_filled(
        bar,
        CornerRadius::same(4),
        ctx.cfg.color(SECTION, "gauge_bg", "#0b0e12"),
    );
    let fill_col = if alert {
        ctx.cfg.color(SECTION, "box_warn", "#e23b3b")
    } else {
        ctx.cfg.color(SECTION, "gauge_fill", "#f4f6f8")
    };
    if let (Some(level), Some(cap)) = (d.level, d.cap) {
        if cap > 0.0 {
            let frac = (level / cap).clamp(0.0, 1.0);
            ui.painter().rect_filled(
                Rect::from_min_size(
                    Pos2::new(bar.left() + 1.0, bar.top() + 1.0),
                    Vec2::new((bar.width() - 2.0) * frac, bar.height() - 2.0),
                ),
                CornerRadius::same(3),
                fill_col,
            );
        }
    }
    let muted = color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 210);
    let cur = d
        .level
        .map(|l| format!("{:.1} {}", ctx.cfg.conv_fuel(l), ctx.cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    let cap = d
        .cap
        .map(|c| format!("{:.0}{}", ctx.cfg.conv_fuel(c), ctx.cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    let line_y = bar.bottom() + (h - (bar.bottom() - y)) * 0.45;
    label(
        ui,
        Pos2::new(x, line_y),
        Align2::LEFT_CENTER,
        &cur,
        11.0,
        if alert {
            ctx.cfg.color(SECTION, "box_warn", "#e23b3b")
        } else {
            ctx.cfg.color(SECTION, "text", "#f4f6f8")
        },
        true,
    );
    label(
        ui,
        Pos2::new(x + w, line_y),
        Align2::RIGHT_CENTER,
        &cap,
        11.0,
        muted,
        false,
    );
}

fn draw_gauge(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    d: &FuelCalcState,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let show_tank_pct = ctx.cfg.bool_key(SECTION, "show_tank_pct", false);
    let show_live_burn = ctx.cfg.bool_key(SECTION, "show_live_burn", false);
    let show_low_alert = ctx.cfg.bool_key(SECTION, "show_low_fuel_alert", true);
    let alert = d.alert && show_low_alert;

    let bar = Rect::from_min_size(Pos2::new(x, y + h * 0.10), Vec2::new(w, h * 0.40));
    ui.painter().rect_filled(
        bar,
        CornerRadius::same(4),
        ctx.cfg.color(SECTION, "gauge_bg", "#0b0e12"),
    );
    ui.painter().rect_stroke(
        bar,
        CornerRadius::same(4),
        Stroke::new(1.0_f32, ctx.cfg.color(SECTION, "cell_border", "#ffffff20")),
        egui::StrokeKind::Inside,
    );
    let fill_col = if alert {
        ctx.cfg.color(SECTION, "box_warn", "#e23b3b")
    } else {
        ctx.cfg.color(SECTION, "gauge_fill", "#f4f6f8")
    };
    if let (Some(level), Some(cap)) = (d.level, d.cap) {
        if cap > 0.0 {
            let target = (level / cap).clamp(0.0, 1.0);
            let id = egui::Id::new("fuel_gauge_anim");
            let mut st = ui
                .ctx()
                .data_mut(|d| d.get_temp::<(f32, f64)>(id).unwrap_or((target, 0.0)));
            let dt = crate::chrome::anim_dt(ctx.mono_secs, &mut st.1);
            st.0 = crate::chrome::ease(st.0, target, dt, 0.14);
            if crate::chrome::still_easing(st.0, target, 0.004) {
                *ctx.panel_animating = true;
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(1));
            }
            let frac = st.0;
            ui.ctx().data_mut(|d| d.insert_temp(id, st));
            let fill = Rect::from_min_size(
                Pos2::new(bar.left() + 1.0, bar.top() + 1.0),
                Vec2::new((bar.width() - 2.0) * frac, bar.height() - 2.0),
            );
            ui.painter()
                .rect_filled(fill, CornerRadius::same(3), fill_col);
        }
    }
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let label_y = bar.bottom() + h * 0.14;
    let edge_sz = (h * 0.14).clamp(9.0, 13.0);
    label(
        ui,
        Pos2::new(x, label_y),
        Align2::LEFT_CENTER,
        "E",
        edge_sz,
        muted,
        true,
    );
    let cur = d.level.map(|l| ctx.cfg.conv_fuel(l));
    let mut cur_txt = cur
        .map(|c| format!("{c:.1} {}", ctx.cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    if show_tank_pct {
        if let Some(pct) = d.fuel_pct {
            cur_txt = format!("{cur_txt} ({pct:.0}%)");
        }
    }
    label(
        ui,
        Pos2::new(x + w * 0.5, label_y),
        Align2::CENTER_CENTER,
        &cur_txt,
        edge_sz,
        muted,
        true,
    );
    let cap_txt = d
        .cap
        .map(|c| format!("{:.0}{}", ctx.cfg.conv_fuel(c), ctx.cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    label(
        ui,
        Pos2::new(x + w, label_y),
        Align2::RIGHT_CENTER,
        &cap_txt,
        edge_sz,
        muted,
        true,
    );
    let hint_y = (label_y + edge_sz * 0.9 + 4.0).min(y + h - 8.0);
    if alert {
        label(
            ui,
            Pos2::new(x + w * 0.5, hint_y),
            Align2::CENTER_CENTER,
            "LOW FUEL",
            (h * 0.12).clamp(9.0, 12.0),
            ctx.cfg.color(SECTION, "box_warn", "#e23b3b"),
            true,
        );
    } else if let Some(hint) = &d.pit_hint {
        label(
            ui,
            Pos2::new(x + w * 0.5, hint_y),
            Align2::CENTER_CENTER,
            hint,
            (h * 0.11).clamp(8.0, 11.0),
            muted,
            false,
        );
    } else if show_live_burn {
        if let Some(burn) = d.live_burn {
            let b = ctx.cfg.conv_fuel(burn);
            label(
                ui,
                Pos2::new(x + w * 0.5, hint_y),
                Align2::CENTER_CENTER,
                &format!("{b:.2}{}/lap", ctx.cfg.fuel_unit()),
                (h * 0.11).clamp(8.0, 11.0),
                muted,
                false,
            );
        }
    }
}

fn stats_row_metrics(ctx: &WidgetCtx<'_>, h: f32) -> (f32, f32) {
    let n = STAT_ROWS.len() as f32;
    let fixed_rh = ctx.cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    if fixed_rh > 0.0 {
        let head = (fixed_rh * 1.1).min(h * 0.3);
        let row = ((h - head) / n).min(fixed_rh).max(1.0);
        return (head, row);
    }
    // Always fit within allocated height — never overflow into the PIT strip.
    let head_h = (h * 0.22).min(h * 0.28).max(1.0).min(h * 0.4);
    let row_h = ((h - head_h) / n).max(1.0);
    (head_h, row_h)
}

fn stats_content_height(ctx: &WidgetCtx<'_>, allocated_h: f32) -> f32 {
    let (head_h, row_h) = stats_row_metrics(ctx, allocated_h);
    // Prefer a readable minimum when space allows; caller shrink-wraps down.
    let n = STAT_ROWS.len() as f32;
    let min_row = 18.0_f32;
    let min_head = 20.0_f32;
    let comfortable = min_head + n * min_row;
    if comfortable <= allocated_h {
        comfortable
    } else {
        head_h + n * row_h
    }
}

fn scenario_for<'a>(d: &'a FuelCalcState, key: &str) -> &'a FuelScenario {
    match key {
        "max" => &d.max,
        "min" => &d.min,
        _ => &d.avg,
    }
}

fn draw_stats(ui: &mut Ui, ctx: &WidgetCtx<'_>, d: &FuelCalcState, x: f32, y: f32, w: f32, h: f32) {
    let label_w = w * 0.13;
    let col_w = (w - label_w) / STAT_COLS.len() as f32;
    let (head_h, row_h) = stats_row_metrics(ctx, h);
    let hscale = ctx.cfg.f64_key(SECTION, "stats_header_font_scale", 1.0) as f32;
    let rscale = ctx.cfg.f64_key(SECTION, "stats_row_font_scale", 1.0) as f32;
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let header = ctx.cfg.color(SECTION, "header", "#8b93a1");

    let band = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, head_h));
    ui.painter().rect_filled(
        band,
        CornerRadius::ZERO,
        ctx.cfg.color(SECTION, "header_bg", "#0b0e12bb"),
    );
    let headers = ["USAGE", "LAPS", "PITS", "REFUEL"];
    for (i, hdr) in headers.iter().enumerate() {
        let cx = x + label_w + i as f32 * col_w;
        label(
            ui,
            Pos2::new(cx + col_w * 0.5, y + head_h * 0.5),
            Align2::CENTER_CENTER,
            hdr,
            (head_h * 0.48 * hscale).clamp(8.0, 14.0),
            header,
            false,
        );
    }

    let labels = ["AVG", "HIGH", "LOW"];
    for (r, rk) in STAT_ROWS.iter().enumerate() {
        let ry = y + head_h + r as f32 * row_h;
        if ry + row_h > y + h + 0.5 {
            break;
        }
        if r % 2 == 1 {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(x, ry), Vec2::new(w, row_h)),
                CornerRadius::same(4),
                ctx.cfg.color(SECTION, "row_alt", "#ffffff08"),
            );
        }
        label(
            ui,
            Pos2::new(x + 4.0, ry + row_h * 0.5),
            Align2::LEFT_CENTER,
            labels[r],
            (row_h * 0.40 * rscale).clamp(8.0, 14.0),
            muted,
            true,
        );
        let data = scenario_for(d, rk);
        for (i, ck) in STAT_COLS.iter().enumerate() {
            let cx = x + label_w + i as f32 * col_w;
            let cell = fmt_stat_cell(ctx, ck, data);
            label(
                ui,
                Pos2::new(cx + col_w * 0.5, ry + row_h * 0.5),
                Align2::CENTER_CENTER,
                &cell,
                (row_h * 0.44 * rscale).clamp(8.0, 15.0),
                text,
                false,
            );
        }
        if ctx.cfg.bool_key(SECTION, "row_dividers", true) && r + 1 < STAT_ROWS.len() {
            let line = ctx.cfg.color(SECTION, "border", "#ffffff28");
            ui.painter().line_segment(
                [Pos2::new(x, ry + row_h), Pos2::new(x + w, ry + row_h)],
                Stroke::new(1.0_f32, line),
            );
        }
    }
}

fn draw_strip(ui: &mut Ui, ctx: &WidgetCtx<'_>, d: &FuelCalcState, x: f32, y: f32, w: f32, h: f32) {
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let lbl_w = w * 0.10;
    label(
        ui,
        Pos2::new(x, y + h * 0.5),
        Align2::LEFT_CENTER,
        "PIT",
        (h * 0.45).clamp(9.0, 14.0),
        muted,
        true,
    );
    let total = d.strip.total;
    if total <= 0 {
        return;
    }
    let sx = x + lbl_w;
    let sw = w - lbl_w;
    let gap = sw / total as f32 * 0.22;
    let seg_w = sw / total as f32 - gap;
    let bar_h = (h * 0.55).min(h - 2.0);
    let by = y + (h - bar_h) * 0.5;
    let win = d.strip.window;
    let now = d.strip.now;
    for i in 0..total {
        let cx = sx + i as f32 * (seg_w + gap);
        let color = if now == Some(i) {
            ctx.cfg.color(SECTION, "strip_now", "#ffd23a")
        } else if win.map(|(a, b)| i >= a && i <= b).unwrap_or(false) {
            ctx.cfg.color(SECTION, "strip_window", "#46df7a")
        } else {
            ctx.cfg.color(SECTION, "strip_none", "#333a42")
        };
        ui.painter().rect_filled(
            Rect::from_min_size(Pos2::new(cx, by), Vec2::new(seg_w.max(2.0), bar_h)),
            CornerRadius::same(2),
            color,
        );
    }
}

fn draw_box_elegant(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    title: &str,
    value: &str,
    margin_txt: &str,
    margin_val: Option<f32>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(8),
        color_with_alpha(ctx.cfg.color(SECTION, "cell_dark", "#0b0e12"), 55),
    );
    let pad = 10.0;
    let warn = margin_val.map(|v| v < 0.0).unwrap_or(false);
    let mcol = if warn {
        ctx.cfg.color(SECTION, "box_warn", "#e23b3b")
    } else {
        color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200)
    };
    // Single horizontal row with three zones — full width, no stacking collisions.
    label(
        ui,
        Pos2::new(x + pad, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        11.0,
        color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 210),
        false,
    );
    label(
        ui,
        Pos2::new(x + w * 0.62, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        15.0,
        ctx.cfg.color(SECTION, "box_value", "#f4f6f8"),
        true,
    );
    label(
        ui,
        Pos2::new(x + w - pad, rect.center().y),
        Align2::RIGHT_CENTER,
        margin_txt,
        12.0,
        mcol,
        false,
    );
}

fn draw_box(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    title: &str,
    value: &str,
    margin_txt: &str,
    margin_val: Option<f32>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
) {
    let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    draw_dark_cell(ui, ctx.cfg, SECTION, rect, 6.0);
    let pad = w * 0.02;
    let gap = w * 0.02;
    let label_w = w * 0.34;
    let margin_w = w * 0.22;
    let value_w = w - label_w - margin_w - pad * 2.0 - gap * 2.0;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let warn = margin_val.map(|v| v < 0.0).unwrap_or(false);
    let mcol = if warn {
        ctx.cfg.color(SECTION, "box_warn", "#e23b3b")
    } else {
        ctx.cfg.color(SECTION, "muted", "#8b93a1")
    };
    label(
        ui,
        Pos2::new(x + pad, rect.center().y),
        Align2::LEFT_CENTER,
        title,
        (h * 0.30).min(18.0),
        text,
        true,
    );
    label(
        ui,
        Pos2::new(x + label_w + gap + value_w, rect.center().y),
        Align2::RIGHT_CENTER,
        value,
        (h * 0.52).min(28.0),
        ctx.cfg.color(SECTION, "box_value", "#f4f6f8"),
        true,
    );
    label(
        ui,
        Pos2::new(x + w - pad, rect.center().y),
        Align2::RIGHT_CENTER,
        margin_txt,
        (h * 0.32).min(20.0),
        mcol,
        false,
    );
}

fn fmt1(x: Option<f32>) -> String {
    x.map(|v| format!("{v:.1}")).unwrap_or_else(|| "–".into())
}

fn signed1(x: Option<f32>) -> String {
    match x {
        Some(v) if v >= 0.0 => format!("+{:.1}", v),
        Some(v) => format!("-{:.1}", v.abs()),
        None => "–".into(),
    }
}

fn fmt_hms(sec: Option<f32>) -> String {
    let Some(sec) = sec else {
        return "--:--:--".into();
    };
    let sec = sec.max(0.0) as i32;
    let h = sec / 3600;
    let m = (sec % 3600) / 60;
    let s = sec % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn fmt_signed_hms(sec: Option<f32>) -> String {
    let Some(sec) = sec else {
        return "--:--:--".into();
    };
    let sign = if sec < 0.0 { "-" } else { "+" };
    format!("{sign}{}", fmt_hms(Some(sec.abs())))
}

fn fmt_stat_cell(ctx: &WidgetCtx<'_>, col: &str, data: &FuelScenario) -> String {
    match col {
        "usage" => data
            .usage
            .map(|u| format!("{:.1}", ctx.cfg.conv_fuel(u)))
            .unwrap_or_else(|| "–".into()),
        "refuel" => data
            .refuel
            .map(|u| format!("{:.1}", ctx.cfg.conv_fuel(u)))
            .unwrap_or_else(|| "–".into()),
        "laps" => fmt1(data.laps),
        "pits" => fmt1(data.pits),
        _ => "–".into(),
    }
}
