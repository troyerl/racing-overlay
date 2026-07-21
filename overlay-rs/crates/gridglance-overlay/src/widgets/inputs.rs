//! Scrolling input telemetry (Python `inputs.py` parity).

use super::WidgetCtx;
use crate::chrome::{anim_dt, ease, full_rect, label, panel_card, still_easing};
use egui::{Align2, Color32, CornerRadius, Pos2, Rect, Shape, Stroke, Ui, Vec2};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

const SECTION: &str = "inputs";
const BAR_EASE_TAU: f32 = 0.07;

/// (t, thr, brk, clt, steer, abs, gear)
type Sample = (f64, f32, f32, f32, f32, f32, i32);

#[derive(Clone, Default)]
struct BarAnim {
    thr: f32,
    brk: f32,
    clt: f32,
    last_secs: f64,
}

struct Hist {
    clock: Instant,
    samples: VecDeque<Sample>,
}

impl Hist {
    fn new() -> Self {
        Self {
            clock: Instant::now(),
            samples: VecDeque::with_capacity(512),
        }
    }
}

fn hist() -> &'static Mutex<Hist> {
    use std::sync::OnceLock;
    static CELL: OnceLock<Mutex<Hist>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(Hist::new()))
}

fn gear_str(g: i32) -> String {
    if g < 0 {
        "R".into()
    } else if g == 0 {
        "N".into()
    } else {
        g.to_string()
    }
}

fn push_sample(ctx: &WidgetCtx<'_>) {
    let f = ctx.frame;
    let cfg = ctx.cfg;
    let mut h = hist().lock().unwrap_or_else(|e| e.into_inner());
    let t = h.clock.elapsed().as_secs_f64();
    let thr = f.throttle.clamp(0.0, 1.0);
    let brk = f.brake.clamp(0.0, 1.0);
    let clt = f.clutch.clamp(0.0, 1.0);
    let steer = ((f.steering + 1.0) * 0.5).clamp(0.0, 1.0);
    let abs_on = if f.abs_active { 1.0 } else { 0.0 };
    let gear = f.gear;
    // Always append on a short cadence so the graph scrolls smoothly even when
    // pedals are held steady (otherwise the trace freezes then jumps).
    let force = match h.samples.back() {
        Some(last) => t - last.0 >= 1.0 / 30.0,
        None => true,
    };
    if let Some(last) = h.samples.back() {
        if !force
            && (last.1 - thr).abs() < 1e-4
            && (last.2 - brk).abs() < 1e-4
            && (last.3 - clt).abs() < 1e-4
            && (last.4 - steer).abs() < 1e-4
            && last.6 == gear
        {
            return;
        }
    }
    h.samples.push_back((t, thr, brk, clt, steer, abs_on, gear));
    let window = cfg.f64_key(SECTION, "history_seconds", 6.0);
    let cutoff = t - window;
    while h.samples.len() > 2 && h.samples.front().map(|s| s.0 < cutoff).unwrap_or(false) {
        h.samples.pop_front();
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    push_sample(ctx);
    let rect = full_rect(ui);
    panel_card(ui, ctx.cfg, SECTION, rect);
    let h = rect.height();
    let pad = if crate::chrome::is_elegant(ctx.cfg, SECTION) {
        (h * 0.06).max(5.0)
    } else {
        (h * 0.08).max(6.0)
    };
    let gap = (h * 0.06).max(6.0);
    let mut left = rect.left() + pad;
    let mut right = rect.right() - pad;

    if ctx.cfg.bool_key(SECTION, "show_label", true) && !crate::chrome::is_elegant(ctx.cfg, SECTION)
    {
        left = draw_label(ui, ctx, left, pad, h) + gap;
    }
    if ctx.cfg.bool_key(SECTION, "show_gauge", true) {
        let gd = h - 2.0 * pad;
        draw_gauge(
            ui,
            ctx,
            Rect::from_min_size(Pos2::new(right - gd, rect.top() + pad), Vec2::new(gd, gd)),
        );
        right -= gd + gap;
    }
    let chans = bar_channels(ctx);
    if ctx.cfg.bool_key(SECTION, "show_bars", true) && !chans.is_empty() {
        let bw = (h * 0.13).max(14.0);
        let bgap = (h * 0.12).max(10.0);
        let block = chans.len() as f32 * bw + (chans.len().saturating_sub(1) as f32) * bgap;
        draw_bars(
            ui,
            ctx,
            Rect::from_min_size(
                Pos2::new(right - block, rect.top() + pad),
                Vec2::new(block, h - 2.0 * pad),
            ),
            bw,
            bgap,
            &chans,
        );
        right -= block + gap;
    }
    if ctx.cfg.bool_key(SECTION, "show_graph", true) {
        draw_graph(
            ui,
            ctx,
            Rect::from_min_size(
                Pos2::new(left, rect.top() + pad),
                Vec2::new((right - left).max(1.0), h - 2.0 * pad),
            ),
        );
    }
}

fn bar_channels(ctx: &WidgetCtx<'_>) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    if ctx.cfg.bool_key(SECTION, "show_throttle", true) {
        out.push((1, "throttle"));
    }
    if ctx.cfg.bool_key(SECTION, "show_brake", true) {
        out.push((2, "brake"));
    }
    if ctx.cfg.bool_key(SECTION, "show_clutch", false) {
        out.push((3, "clutch"));
    }
    out
}

fn brake_threshold(ctx: &WidgetCtx<'_>) -> f32 {
    if !ctx.cfg.bool_key(SECTION, "show_brake_threshold", false) {
        return 0.0;
    }
    (ctx.cfg.f64_key(SECTION, "brake_threshold", 85.0) as f32 / 100.0).clamp(0.0, 1.0)
}

fn brake_color(ctx: &WidgetCtx<'_>, value: f32, abs_on: bool, thr: f32) -> Color32 {
    if abs_on {
        return ctx.cfg.color(SECTION, "brake_abs", "#ffd23a");
    }
    if thr > 0.0 && value > thr {
        return ctx.cfg.color(SECTION, "brake_over", "#ff7a1a");
    }
    ctx.cfg.color(SECTION, "brake", "#e23b3b")
}

fn draw_label(ui: &mut Ui, ctx: &WidgetCtx<'_>, x: f32, pad: f32, h: f32) -> f32 {
    let bar_w = (h * 0.035).max(3.0);
    let bar = Rect::from_min_size(Pos2::new(x, pad), Vec2::new(bar_w, h - 2.0 * pad));
    ui.painter().rect_filled(
        bar,
        CornerRadius::same(2),
        ctx.cfg.color(SECTION, "accent", "#e23b3b"),
    );
    let text = ctx.cfg.str_key(SECTION, "label_text", "TELEMETRY");
    let tab_w = (h * 0.20).max(14.0);
    let cx = x + bar_w + tab_w * 0.5;
    // egui has no easy rotate; draw stacked letters vertically.
    let fs = (h * 0.10).max(8.0);
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(1) as f32;
    let step = (h - 2.0 * pad) / n;
    for (i, ch) in chars.iter().enumerate() {
        label(
            ui,
            Pos2::new(cx, pad + step * (i as f32 + 0.5)),
            Align2::CENTER_CENTER,
            &ch.to_string(),
            fs,
            ctx.cfg.color(SECTION, "label", "#cdd3db"),
            true,
        );
    }
    x + bar_w + tab_w
}

fn draw_graph(ui: &mut Ui, ctx: &WidgetCtx<'_>, rect: Rect) {
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(6),
        ctx.cfg.color(SECTION, "graph_bg", "#0b0d11"),
    );
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        Stroke::new(1.0_f32, ctx.cfg.color(SECTION, "cell_border", "#ffffff20")),
        egui::StrokeKind::Inside,
    );
    let grid = ctx.cfg.color(SECTION, "grid", "#ffffff14");
    for fr in [0.0_f32, 0.5, 1.0] {
        let y = rect.bottom() - fr * rect.height();
        ui.painter().line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0_f32, grid),
        );
    }

    let h = hist().lock().unwrap_or_else(|e| e.into_inner());
    if h.samples.len() < 2 {
        return;
    }
    let window = ctx.cfg.f64_key(SECTION, "history_seconds", 6.0);
    // Wall clock (not last sample) so the trace keeps scrolling while pedals are held.
    let now = h.clock.elapsed().as_secs_f64();
    let lw = ctx.cfg.f64_key(SECTION, "line_width", 2.4) as f32;
    let to_pt = |t: f64, frac: f32| -> Pos2 {
        let x = rect.right() - ((now - t) / window).min(1.0) as f32 * rect.width();
        let y = rect.bottom() - frac * rect.height();
        Pos2::new(x, y)
    };

    let thr = brake_threshold(ctx);
    if thr > 0.0 {
        let y = rect.bottom() - thr * rect.height();
        ui.painter().line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.4_f32, ctx.cfg.color(SECTION, "threshold", "#ffffff66")),
        );
    }

    let draw_chan = |ui: &mut Ui, di: usize, color: Color32| {
        let pts: Vec<Pos2> = h
            .samples
            .iter()
            .map(|s| {
                let v = match di {
                    1 => s.1,
                    2 => s.2,
                    3 => s.3,
                    4 => s.4,
                    _ => 0.0,
                };
                to_pt(s.0, v)
            })
            .collect();
        if pts.len() >= 2 {
            ui.painter().add(Shape::line(pts, Stroke::new(lw, color)));
        }
    };

    if ctx.cfg.bool_key(SECTION, "show_throttle", true) {
        draw_chan(ui, 1, ctx.cfg.color(SECTION, "throttle", "#46df7a"));
    }
    if ctx.cfg.bool_key(SECTION, "show_clutch", false) {
        draw_chan(ui, 3, ctx.cfg.color(SECTION, "clutch", "#3aa0ff"));
    }
    if ctx.cfg.bool_key(SECTION, "show_steering", false) {
        draw_chan(ui, 4, ctx.cfg.color(SECTION, "steering", "#c08bff"));
    }

    // Brake segment-by-segment for ABS / over colors.
    if ctx.cfg.bool_key(SECTION, "show_brake", true) {
        let mut prev: Option<Pos2> = None;
        for s in h.samples.iter() {
            let cur = to_pt(s.0, s.2);
            if let Some(p0) = prev {
                let col = brake_color(ctx, s.2, s.5 > 0.5, thr);
                ui.painter().line_segment([p0, cur], Stroke::new(lw, col));
            }
            prev = Some(cur);
        }
    }

    if ctx.cfg.bool_key(SECTION, "show_shift_markers", false) {
        let mut prev_g: Option<i32> = None;
        for s in h.samples.iter() {
            if let Some(pg) = prev_g {
                if s.6 != pg {
                    let pt = to_pt(s.0, 0.5);
                    ui.painter().line_segment(
                        [
                            Pos2::new(pt.x, rect.top() + 2.0),
                            Pos2::new(pt.x, rect.bottom() - 2.0),
                        ],
                        Stroke::new(1.2_f32, ctx.cfg.color(SECTION, "text", "#f4f6f8")),
                    );
                }
            }
            prev_g = Some(s.6);
        }
    }
}

fn draw_bars(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    rect: Rect,
    bw: f32,
    bgap: f32,
    chans: &[(usize, &str)],
) {
    let h = hist().lock().unwrap_or_else(|e| e.into_inner());
    let latest = h
        .samples
        .back()
        .copied()
        .unwrap_or((0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0));
    drop(h);
    let abs_on = latest.5 > 0.5;
    let thr = brake_threshold(ctx);

    let id = egui::Id::new("inputs_bar_anim");
    let mut anim = ui
        .ctx()
        .data_mut(|d| d.get_temp::<BarAnim>(id).unwrap_or_default());
    let dt = anim_dt(ctx.mono_secs, &mut anim.last_secs);
    anim.thr = ease(anim.thr, latest.1, dt, BAR_EASE_TAU);
    anim.brk = ease(anim.brk, latest.2, dt, BAR_EASE_TAU);
    anim.clt = ease(anim.clt, latest.3, dt, BAR_EASE_TAU);
    let animating = still_easing(anim.thr, latest.1, 0.005)
        || still_easing(anim.brk, latest.2, 0.005)
        || still_easing(anim.clt, latest.3, 0.005);
    if animating {
        *ctx.panel_animating = true;
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1));
    }
    let eased = (anim.thr, anim.brk, anim.clt);
    ui.ctx().data_mut(|d| d.insert_temp(id, anim));

    let label_h = (rect.height() * 0.16).max(10.0);
    let track_top = rect.top() + label_h;
    let track_h = rect.height() - label_h;
    let mut x = rect.left();
    for &(di, colk) in chans {
        let val = match di {
            1 => eased.0,
            2 => eased.1,
            3 => eased.2,
            _ => 0.0,
        }
        .clamp(0.0, 1.0);
        let fill = if di == 2 {
            brake_color(ctx, val, abs_on, thr)
        } else {
            ctx.cfg.color(SECTION, colk, "#46df7a")
        };
        let track = Rect::from_min_size(Pos2::new(x, track_top), Vec2::new(bw, track_h));
        let r = (bw * 0.4) as u8;
        ui.painter().rect_filled(
            track,
            CornerRadius::same(r),
            ctx.cfg.color(SECTION, "bar_track", "#262b34"),
        );
        let fh = val * track_h;
        if fh > 0.5 {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(x, track_top + track_h - fh), Vec2::new(bw, fh)),
                CornerRadius::same(r),
                fill,
            );
        }
        label(
            ui,
            Pos2::new(x + bw * 0.5, rect.top() + label_h * 0.5),
            Align2::CENTER_CENTER,
            &format!("{:.0}", val * 100.0),
            (rect.height() * 0.14).max(8.0),
            ctx.cfg.color(SECTION, "text", "#f4f6f8"),
            true,
        );
        x += bw + bgap;
    }
}

fn draw_gauge(ui: &mut Ui, ctx: &WidgetCtx<'_>, rect: Rect) {
    let cx = rect.center().x;
    let cy = rect.center().y;
    let rad = rect.width() * 0.5;
    let ring = rect.shrink(rad * 0.06);
    ui.painter().circle_filled(
        ring.center(),
        ring.width() * 0.5,
        ctx.cfg.color(SECTION, "gauge_bg", "#0b0d11"),
    );
    ui.painter().circle_stroke(
        ring.center(),
        ring.width() * 0.5,
        Stroke::new(
            (rad * 0.10).max(2.0),
            ctx.cfg.color(SECTION, "gauge_ring", "#333a42"),
        ),
    );
    label(
        ui,
        Pos2::new(cx, cy - rad * 0.15),
        Align2::CENTER_CENTER,
        &gear_str(ctx.frame.gear),
        rad * 0.85,
        ctx.cfg.color(SECTION, "text", "#f4f6f8"),
        true,
    );
    label(
        ui,
        Pos2::new(cx, cy + rad * 0.28),
        Align2::CENTER_CENTER,
        ctx.cfg.speed_unit(),
        rad * 0.22,
        ctx.cfg.color(SECTION, "muted", "#8b93a1"),
        true,
    );
    let spd = ctx.cfg.conv_speed(ctx.frame.speed_mps);
    label(
        ui,
        Pos2::new(cx, cy + rad * 0.55),
        Align2::CENTER_CENTER,
        &format!("{:.0}", spd),
        rad * 0.30,
        ctx.cfg.color(SECTION, "text", "#f4f6f8"),
        true,
    );
}
