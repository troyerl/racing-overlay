//! Fuel calculator — Python `fuel_calc.py` parity.

use super::WidgetCtx;
use crate::chrome::{draw_card, draw_dark_cell, draw_section_header, full_rect, label};
use crate::telemetry::{FuelCalcState, FuelScenario};
use egui::{Align2, CornerRadius, Pos2, Rect, Stroke, Ui, Vec2};

const SECTION: &str = "fuel_calc";
const STAT_COLS: &[&str] = &["usage", "laps", "pits", "refuel"];
const STAT_ROWS: &[&str] = &["avg", "max", "min"];

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let w = card.width();
    let h = card.height();
    let m = w * 0.04;
    let inner = w - 2.0 * m;
    let d = &ctx.frame.fuel;

    // Accent bar
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
    if ctx.cfg.bool_key(SECTION, "show_time", true) {
        blocks.push(("time", 0.95));
    }
    if ctx.cfg.bool_key(SECTION, "show_laps", true) {
        blocks.push(("laps", 0.95));
    }
    if blocks.is_empty() {
        return;
    }

    let content_top = card.top() + h * 0.012 + bar_h + h * 0.015;
    let content_bottom = card.top() + h * 0.985;
    let gap = h * 0.02;
    let sumw: f32 = blocks.iter().map(|(_, wt)| wt).sum();
    let avail = (content_bottom - content_top) - gap * (blocks.len() as f32 - 1.0);
    let mut heights: Vec<f32> = blocks.iter().map(|(_, wt)| avail * wt / sumw).collect();
    // Shrink-wrap stats
    if let Some(i) = blocks.iter().position(|(k, _)| *k == "stats") {
        let needed = stats_content_height(ctx, heights[i]);
        if needed < heights[i] {
            heights[i] = needed;
        }
    }

    let mut cy = content_top;
    for (i, (key, _)) in blocks.iter().enumerate() {
        let bh = heights[i];
        let x = card.left() + m;
        match *key {
            "title" => {
                let band = Rect::from_min_size(Pos2::new(x, cy), Vec2::new(inner, bh));
                draw_section_header(
                    ui,
                    ctx.cfg,
                    SECTION,
                    band,
                    &ctx.cfg.str_key(SECTION, "title", "FUEL CALCULATOR"),
                    radius,
                );
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
        cy += bh + gap;
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
    label(
        ui,
        Pos2::new(pill.center().x, pill.top() + h * 0.32),
        Align2::CENTER_CENTER,
        if d.window_open { "OPEN" } else { "CLOSED" },
        h * 0.34,
        fg,
        true,
    );
    let sub = match d.window {
        Some((a, b)) => format!("L{a}-{b}"),
        None => "—".into(),
    };
    label(
        ui,
        Pos2::new(pill.center().x, pill.top() + h * 0.72),
        Align2::CENTER_CENTER,
        &sub,
        h * 0.22,
        fg,
        true,
    );
}

fn draw_add(ui: &mut Ui, ctx: &WidgetCtx<'_>, d: &FuelCalcState, x: f32, y: f32, w: f32, h: f32) {
    let rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));
    draw_dark_cell(ui, ctx.cfg, SECTION, rect, 8.0);
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
        h * 0.46,
        ctx.cfg.color(SECTION, "add_text", "#f4f6f8"),
        true,
    );
}

fn draw_gauge(ui: &mut Ui, ctx: &mut WidgetCtx<'_>, d: &FuelCalcState, x: f32, y: f32, w: f32, h: f32) {
    let bar = Rect::from_min_size(Pos2::new(x, y + h * 0.18), Vec2::new(w, h * 0.36));
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
    if let (Some(level), Some(cap)) = (d.level, d.cap) {
        if cap > 0.0 {
            let target = (level / cap).clamp(0.0, 1.0) as f32;
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
            ui.painter().rect_filled(
                fill,
                CornerRadius::same(3),
                ctx.cfg.color(SECTION, "gauge_fill", "#f4f6f8"),
            );
        }
    }
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let label_y = bar.bottom() + h * 0.2;
    label(
        ui,
        Pos2::new(x, label_y),
        Align2::LEFT_CENTER,
        "E",
        h * 0.26,
        muted,
        true,
    );
    let cur = d.level.map(|l| ctx.cfg.conv_fuel(l));
    let mut cur_txt = cur
        .map(|c| format!("{c:.1} {}", ctx.cfg.fuel_unit()))
        .unwrap_or_else(|| "—".into());
    if let Some(pct) = d.fuel_pct {
        cur_txt = format!("{cur_txt} ({pct:.0}%)");
    }
    label(
        ui,
        Pos2::new(x + w * 0.5, label_y),
        Align2::CENTER_CENTER,
        &cur_txt,
        h * 0.26,
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
        h * 0.26,
        muted,
        true,
    );
    if d.alert {
        label(
            ui,
            Pos2::new(x + w * 0.5, bar.bottom() + h * 0.48),
            Align2::CENTER_CENTER,
            "LOW FUEL",
            h * 0.22,
            ctx.cfg.color(SECTION, "box_warn", "#e23b3b"),
            true,
        );
    } else if let Some(hint) = &d.pit_hint {
        label(
            ui,
            Pos2::new(x + w * 0.5, bar.bottom() + h * 0.48),
            Align2::CENTER_CENTER,
            hint,
            h * 0.20,
            muted,
            false,
        );
    } else if let Some(burn) = d.live_burn {
        let b = ctx.cfg.conv_fuel(burn);
        label(
            ui,
            Pos2::new(x + w * 0.5, bar.bottom() + h * 0.48),
            Align2::CENTER_CENTER,
            &format!("{b:.2}{}/lap", ctx.cfg.fuel_unit()),
            h * 0.20,
            muted,
            false,
        );
    }
}

fn stats_row_metrics(ctx: &WidgetCtx<'_>, h: f32) -> (f32, f32) {
    let n = STAT_ROWS.len() as f32;
    let fixed_rh = ctx.cfg.f64_key(SECTION, "row_height_px", 0.0) as f32;
    if fixed_rh > 0.0 {
        return (fixed_rh * 1.1, fixed_rh);
    }
    let max_rh_frac = ctx.cfg.f64_key(SECTION, "max_row_height_frac", 0.14) as f32;
    if max_rh_frac > 0.0 {
        // Approximate panel height via allocated block; use h as proxy when frac binds.
        let capped = h.max(1.0) * 0.35; // soft bound inside stats slot
        let natural_head = capped * 1.1;
        let natural = natural_head + n * capped;
        if natural <= h + 1e-3 {
            return (natural_head, capped);
        }
    }
    let head_h = h * 0.22;
    let row_h = ((h - head_h) / n).min(h * 0.28).max(16.0);
    (head_h, row_h)
}

fn stats_content_height(ctx: &WidgetCtx<'_>, allocated_h: f32) -> f32 {
    let (head_h, row_h) = stats_row_metrics(ctx, allocated_h);
    head_h + STAT_ROWS.len() as f32 * row_h
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
            head_h * 0.5 * hscale,
            header,
            false,
        );
    }

    let labels = ["AVG", "HIGH", "LOW"];
    for (r, rk) in STAT_ROWS.iter().enumerate() {
        let ry = y + head_h + r as f32 * row_h;
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
            row_h * 0.4 * rscale,
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
                row_h * 0.46 * rscale,
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
        h * 0.5,
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
    let bar_h = h * 0.6;
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
    let warn = margin_val.map(|v| v < 0.0).unwrap_or(false);
    let mcol = if warn {
        ctx.cfg.color(SECTION, "box_warn", "#e23b3b")
    } else {
        ctx.cfg.color(SECTION, "muted", "#8b93a1")
    };
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
