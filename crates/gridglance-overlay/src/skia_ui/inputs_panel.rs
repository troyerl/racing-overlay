//! Skia painter for the scrolling input telemetry panel (Phase C).
//!
//! Ports `widgets/inputs.rs` (egui) onto the Skia `Canvas`. History is
//! host-owned (`InputsHistory`, passed in by the caller) instead of a
//! process-wide `Mutex`, since the Skia panel host already owns per-panel
//! animation state (see `skia_ui::anim`).

use super::canvas::{Canvas, TextAlign};
use super::chrome::{is_elegant, panel_card, section_color, text_at};
use super::types::Rect;
use crate::config::OverlayConfig;
use crate::telemetry::TelemetryFrame;
use std::collections::VecDeque;

const SECTION: &str = "inputs";
/// Sample cadence for the scrolling trace (~present rate).
const SAMPLE_HZ: f64 = 60.0;

/// (t, thr, brk, clt, steer, abs, gear)
pub type Sample = (f64, f32, f32, f32, f32, f32, i32);

/// Host-owned scrolling sample history for the inputs trace.
#[derive(Clone)]
pub struct InputsHistory {
    pub samples: VecDeque<Sample>,
}

impl InputsHistory {
    pub fn new() -> Self {
        Self {
            samples: VecDeque::with_capacity(512),
        }
    }
}

impl Default for InputsHistory {
    fn default() -> Self {
        Self::new()
    }
}

fn full_bounds(c: &Canvas) -> Rect {
    Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32)
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

fn push_sample(hist: &mut InputsHistory, cfg: &OverlayConfig, frame: &TelemetryFrame, t: f64) {
    let thr = frame.throttle.clamp(0.0, 1.0);
    let brk = frame.brake.clamp(0.0, 1.0);
    let clt = frame.clutch.clamp(0.0, 1.0);
    let steer = ((frame.steering + 1.0) * 0.5).clamp(0.0, 1.0);
    let abs_on = if frame.abs_active { 1.0 } else { 0.0 };
    let gear = frame.gear;
    // Always append on a short cadence so the graph scrolls smoothly even when
    // pedals are held steady (otherwise the trace freezes then jumps).
    let force = match hist.samples.back() {
        Some(last) => t - last.0 >= 1.0 / SAMPLE_HZ,
        None => true,
    };
    if let Some(last) = hist.samples.back() {
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
    hist.samples
        .push_back((t, thr, brk, clt, steer, abs_on, gear));
    let window = cfg.f64_key(SECTION, "history_seconds", 6.0);
    let cutoff = t - window;
    while hist.samples.len() > 2 && hist.samples.front().map(|s| s.0 < cutoff).unwrap_or(false) {
        hist.samples.pop_front();
    }
}

/// Paint the inputs panel. Returns `true` when the panel should keep a live
/// present cadence (mirrors egui `panel_animating`).
pub fn paint_inputs(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    hist: &mut InputsHistory,
    mono_secs: f64,
    edit_mode: bool,
) -> bool {
    let _ = edit_mode;
    c.clear_transparent();
    let show_graph = cfg.bool_key(SECTION, "show_graph", true);
    let animating = frame.connected || show_graph;
    push_sample(hist, cfg, frame, mono_secs);

    let bounds = full_bounds(c);
    panel_card(c, cfg, SECTION, bounds);
    let h = bounds.height();
    let elegant = is_elegant(cfg, SECTION);
    let pad = if elegant {
        (h * 0.06).max(5.0)
    } else {
        (h * 0.08).max(6.0)
    };
    let gap = (h * 0.06).max(6.0);
    let mut left = bounds.left() + pad;
    let mut right = bounds.right() - pad;

    if cfg.bool_key(SECTION, "show_label", true) && !elegant {
        left = draw_label(c, cfg, left, pad, h) + gap;
    }
    if cfg.bool_key(SECTION, "show_gauge", true) {
        let gd = h - 2.0 * pad;
        draw_gauge(
            c,
            cfg,
            frame,
            Rect::from_xywh(right - gd, bounds.top() + pad, gd, gd),
        );
        right -= gd + gap;
    }
    let chans = bar_channels(cfg);
    if cfg.bool_key(SECTION, "show_bars", true) && !chans.is_empty() {
        let bw = (h * 0.13).max(14.0);
        let bgap = (h * 0.12).max(10.0);
        let block = chans.len() as f32 * bw + (chans.len().saturating_sub(1) as f32) * bgap;
        draw_bars(
            c,
            cfg,
            hist,
            Rect::from_xywh(right - block, bounds.top() + pad, block, h - 2.0 * pad),
            bw,
            bgap,
            &chans,
        );
        right -= block + gap;
    }
    if show_graph {
        draw_graph(
            c,
            cfg,
            hist,
            mono_secs,
            Rect::from_xywh(
                left,
                bounds.top() + pad,
                (right - left).max(1.0),
                h - 2.0 * pad,
            ),
        );
    }
    animating
}

fn bar_channels(cfg: &OverlayConfig) -> Vec<(usize, &'static str)> {
    let mut out = Vec::new();
    if cfg.bool_key(SECTION, "show_throttle", true) {
        out.push((1, "throttle"));
    }
    if cfg.bool_key(SECTION, "show_brake", true) {
        out.push((2, "brake"));
    }
    if cfg.bool_key(SECTION, "show_clutch", false) {
        out.push((3, "clutch"));
    }
    out
}

fn brake_threshold(cfg: &OverlayConfig) -> f32 {
    if !cfg.bool_key(SECTION, "show_brake_threshold", false) {
        return 0.0;
    }
    (cfg.f64_key(SECTION, "brake_threshold", 85.0) as f32 / 100.0).clamp(0.0, 1.0)
}

fn brake_color(cfg: &OverlayConfig, value: f32, abs_on: bool, thr: f32) -> super::types::Rgba {
    if abs_on {
        return section_color(cfg, SECTION, "brake_abs", "#ffd23a");
    }
    if thr > 0.0 && value > thr {
        return section_color(cfg, SECTION, "brake_over", "#ff7a1a");
    }
    section_color(cfg, SECTION, "brake", "#e23b3b")
}

fn draw_label(c: &mut Canvas, cfg: &OverlayConfig, x: f32, pad: f32, h: f32) -> f32 {
    let bar_w = (h * 0.035).max(3.0);
    let bar = Rect::from_xywh(x, pad, bar_w, h - 2.0 * pad);
    c.fill_rect(bar, section_color(cfg, SECTION, "accent", "#e23b3b"), 2.0);
    let text = cfg.str_key(SECTION, "label_text", "TELEMETRY");
    let tab_w = (h * 0.20).max(14.0);
    let cx = x + bar_w + tab_w * 0.5;
    let fs = (h * 0.10).max(8.0);
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(1) as f32;
    let step = (h - 2.0 * pad) / n;
    for (i, ch) in chars.iter().enumerate() {
        text_at(
            c,
            cx,
            pad + step * (i as f32 + 0.5),
            &ch.to_string(),
            fs,
            section_color(cfg, SECTION, "label", "#cdd3db"),
            true,
            TextAlign::Center,
        );
    }
    x + bar_w + tab_w
}

fn to_pt(rect: Rect, now: f64, window: f64, t: f64, frac: f32) -> (f32, f32) {
    let x = rect.right() - ((now - t) / window).clamp(0.0, 1.0) as f32 * rect.width();
    let y = rect.bottom() - frac * rect.height();
    (x, y)
}

fn draw_chan(
    c: &mut Canvas,
    hist: &InputsHistory,
    di: usize,
    color: super::types::Rgba,
    now: f64,
    window: f64,
    rect: Rect,
    lw: f32,
) {
    let pts: Vec<(f32, f32)> = hist
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
            to_pt(rect, now, window, s.0, v)
        })
        .collect();
    if pts.len() >= 2 {
        c.polyline(&pts, color, lw, false);
    }
}

fn draw_graph(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    hist: &InputsHistory,
    mono_secs: f64,
    rect: Rect,
) {
    c.fill_rect(
        rect,
        section_color(cfg, SECTION, "graph_bg", "#0b0d11"),
        6.0,
    );
    c.stroke_rect(
        rect,
        section_color(cfg, SECTION, "cell_border", "#ffffff20"),
        6.0,
        1.0,
    );
    let grid = section_color(cfg, SECTION, "grid", "#ffffff14");
    for fr in [0.0_f32, 0.5, 1.0] {
        let y = rect.bottom() - fr * rect.height();
        c.line(rect.left(), y, rect.right(), y, grid, 1.0);
    }

    if hist.samples.len() < 2 {
        return;
    }
    let window = cfg.f64_key(SECTION, "history_seconds", 6.0);
    // Host mono clock (not last sample) so the trace keeps scrolling while pedals are held.
    let now = mono_secs;
    let lw = cfg.f64_key(SECTION, "line_width", 2.4) as f32;

    let thr = brake_threshold(cfg);
    if thr > 0.0 {
        let y = rect.bottom() - thr * rect.height();
        c.line(
            rect.left(),
            y,
            rect.right(),
            y,
            section_color(cfg, SECTION, "threshold", "#ffffff66"),
            1.4,
        );
    }

    if cfg.bool_key(SECTION, "show_throttle", true) {
        draw_chan(
            c,
            hist,
            1,
            section_color(cfg, SECTION, "throttle", "#46df7a"),
            now,
            window,
            rect,
            lw,
        );
    }
    if cfg.bool_key(SECTION, "show_clutch", false) {
        draw_chan(
            c,
            hist,
            3,
            section_color(cfg, SECTION, "clutch", "#3aa0ff"),
            now,
            window,
            rect,
            lw,
        );
    }
    if cfg.bool_key(SECTION, "show_steering", false) {
        draw_chan(
            c,
            hist,
            4,
            section_color(cfg, SECTION, "steering", "#c08bff"),
            now,
            window,
            rect,
            lw,
        );
    }

    // Brake segment-by-segment for ABS / over colors.
    if cfg.bool_key(SECTION, "show_brake", true) {
        let mut prev: Option<(f32, f32)> = None;
        for s in hist.samples.iter() {
            let cur = to_pt(rect, now, window, s.0, s.2);
            if let Some(p0) = prev {
                let col = brake_color(cfg, s.2, s.5 > 0.5, thr);
                c.line(p0.0, p0.1, cur.0, cur.1, col, lw);
            }
            prev = Some(cur);
        }
    }

    if cfg.bool_key(SECTION, "show_shift_markers", false) {
        let mut prev_g: Option<i32> = None;
        for s in hist.samples.iter() {
            if let Some(pg) = prev_g {
                if s.6 != pg {
                    let pt = to_pt(rect, now, window, s.0, 0.5);
                    c.line(
                        pt.0,
                        rect.top() + 2.0,
                        pt.0,
                        rect.bottom() - 2.0,
                        section_color(cfg, SECTION, "text", "#f4f6f8"),
                        1.2,
                    );
                }
            }
            prev_g = Some(s.6);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_bars(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    hist: &InputsHistory,
    rect: Rect,
    bw: f32,
    bgap: f32,
    chans: &[(usize, &str)],
) {
    let latest = hist
        .samples
        .back()
        .copied()
        .unwrap_or((0.0, 0.0, 0.0, 0.0, 0.5, 0.0, 0));
    let abs_on = latest.5 > 0.5;
    let thr = brake_threshold(cfg);
    let eased = (latest.1, latest.2, latest.3);

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
            brake_color(cfg, val, abs_on, thr)
        } else {
            section_color(cfg, SECTION, colk, "#46df7a")
        };
        let track = Rect::from_xywh(x, track_top, bw, track_h);
        let r = bw * 0.4;
        c.fill_rect(
            track,
            section_color(cfg, SECTION, "bar_track", "#262b34"),
            r,
        );
        let fh = val * track_h;
        if fh > 0.5 {
            c.fill_rect(
                Rect::from_xywh(x, track_top + track_h - fh, bw, fh),
                fill,
                r,
            );
        }
        text_at(
            c,
            x + bw * 0.5,
            rect.top() + label_h * 0.5,
            &format!("{:.0}", val * 100.0),
            (rect.height() * 0.14).max(8.0),
            section_color(cfg, SECTION, "text", "#f4f6f8"),
            true,
            TextAlign::Center,
        );
        x += bw + bgap;
    }
}

fn draw_gauge(c: &mut Canvas, cfg: &OverlayConfig, frame: &TelemetryFrame, rect: Rect) {
    let (cx, cy) = rect.center();
    let rad = rect.width() * 0.5;
    let ring = rect.inset(rad * 0.06, rad * 0.06);
    let (rcx, rcy) = ring.center();
    c.circle(
        rcx,
        rcy,
        ring.width() * 0.5,
        section_color(cfg, SECTION, "gauge_bg", "#0b0d11"),
        true,
    );
    c.circle_ex(
        rcx,
        rcy,
        ring.width() * 0.5,
        section_color(cfg, SECTION, "gauge_ring", "#333a42"),
        false,
        (rad * 0.10).max(2.0),
    );
    text_at(
        c,
        cx,
        cy - rad * 0.15,
        &gear_str(frame.gear),
        rad * 0.85,
        section_color(cfg, SECTION, "text", "#f4f6f8"),
        true,
        TextAlign::Center,
    );
    text_at(
        c,
        cx,
        cy + rad * 0.28,
        cfg.speed_unit(),
        rad * 0.22,
        section_color(cfg, SECTION, "muted", "#8b93a1"),
        true,
        TextAlign::Center,
    );
    let spd = cfg.conv_speed(frame.speed_mps);
    text_at(
        c,
        cx,
        cy + rad * 0.55,
        &format!("{:.0}", spd),
        rad * 0.30,
        section_color(cfg, SECTION, "text", "#f4f6f8"),
        true,
        TextAlign::Center,
    );
}
