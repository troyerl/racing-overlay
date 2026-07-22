use super::WidgetCtx;
use crate::chrome::{
    anim_dt, color_with_alpha, draw_dark_cell, ease, full_rect, label, panel_card,
};
use crate::config::OverlayConfig;
use crate::icons;
use crate::map_markers::{self, TrafficMarker};
use crate::state::{MapAuthoring, TrackPathStatus};
use crate::telemetry::CarRow;
use crate::track_path;
use egui::{
    epaint::{PathShape, PathStroke},
    Align2, Color32, CornerRadius, CursorIcon, FontFamily, FontId, PointerButton, Pos2, Rect,
    Sense, Shape, Stroke, Ui, Vec2,
};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

const SECTION: &str = "map";
/// Bump when changing map car motion; shown in `--perf` as `map_motion_rev=`.
/// Keep in sync with `docs/map-wiki.md`.
pub const MAP_MOTION_REV: u32 = 23;
/// Screen ease for pit-route / non-pct modes only (Python `_smooth_marker_point`).
const SCREEN_EASE_TAU: f32 = 0.09;
const SCREEN_EASE_SNAP: f32 = 120.0;
/// Keep anim if a car drops out of `frame.cars` briefly.
const PCT_SEEN_GRACE_SECS: f64 = 2.0;
/// Teleport / tow: snap instead of easing across the track.
const PCT_SNAP_ERR: f32 = 0.40;
/// Ignore micro LapDistPct jitter when measuring vel / refreshing anchor.
const TELEM_EPS: f32 = 0.00008;
/// EMA blend for measured lap-%/s.
const VEL_EMA: f32 = 0.45;
const VEL_DT_MIN: f64 = 0.004;
const VEL_DT_MAX: f64 = 3.5;
/// Decay vel only after a long pause (not 2.5s — that matched old stutter).
const QUIET_DECAY_SECS: f64 = 20.0;
/// Ease display toward wall-clock predict (Python used 0.09 lag-only).
const PCT_FOLLOW_TAU: f32 = 0.055;
/// Cap predict age so a long telem gap does not run away.
const PREDICT_AGE_MAX: f32 = 1.5;
/// Still-easing / coasting threshold for `panel_animating`.
const PCT_MOVING_EPS: f32 = 1e-5;

/// Aspect-preserving map from path bounds → plot pixels (after model xform).
#[derive(Clone, Copy)]
struct PlotXform {
    origin: Pos2,
    scale: f32,
    min: (f32, f32),
}

impl PlotXform {
    fn fit(plot: Rect, pts: &[(f32, f32)], pad_px: f32) -> Self {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &(x, y) in pts {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let w = (max_x - min_x).max(1e-6);
        let h = (max_y - min_y).max(1e-6);
        let pad = pad_px.max(8.0);
        let avail_w = (plot.width() - 2.0 * pad).max(1.0);
        let avail_h = (plot.height() - 2.0 * pad).max(1.0);
        let scale = (avail_w / w).min(avail_h / h);
        let drawn_w = w * scale;
        let drawn_h = h * scale;
        let origin = Pos2::new(
            plot.left() + pad + (avail_w - drawn_w) * 0.5,
            plot.top() + pad + (avail_h - drawn_h) * 0.5,
        );
        Self {
            origin,
            scale,
            min: (min_x, min_y),
        }
    }

    fn map(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(
            self.origin.x + (x - self.min.0) * self.scale,
            self.origin.y + (y - self.min.1) * self.scale,
        )
    }

    fn unmap(&self, p: Pos2) -> (f32, f32) {
        let x = self.min.0 + (p.x - self.origin.x) / self.scale;
        let y = self.min.1 + (p.y - self.origin.y) / self.scale;
        (x, y)
    }

    /// Apply pit-edit zoom/pan on top of a fit transform.
    fn with_view(&self, zoom: f32, pan: (f32, f32)) -> Self {
        Self {
            origin: Pos2::new(self.origin.x + pan.0, self.origin.y + pan.1),
            scale: self.scale * zoom.max(1e-6),
            min: self.min,
        }
    }
}

/// Python model-space: mirror then 90° rotation steps.
fn model_point(x: f32, y: f32, mirror: bool, rot: i32) -> (f32, f32) {
    let mut x = x;
    if mirror {
        x = -x;
    }
    match rot.rem_euclid(360) {
        90 => (y, -x),
        180 => (-x, -y),
        270 => (-y, x),
        _ => (x, y),
    }
}

fn apply_loaded_track(ctx: &mut WidgetCtx<'_>, id: i32, tp: track_path::TrackPath) {
    ctx.map.cached_track_id = Some(id);
    ctx.map.path_status = TrackPathStatus::Ready;
    ctx.map.cached_track_name = tp.name.clone();
    ctx.map.cached_start_finish = tp.start_finish;
    ctx.map.cached_pit_out_pct = tp.pit_out_pct;
    ctx.map.cached_path = tp.points;
    ctx.map.cached_pit = tp.pit;
    ctx.map.cached_pit2 = tp.pit2;
    ctx.map.cached_drs_zones = tp.drs_zones;
    ctx.map.cached_p2p_zones = tp.p2p_zones;
    ctx.map.cached_corners = tp.corners;
    if ctx.map.cached_pit.lane_speed_pct > 0.0 {
        ctx.map.pit_lane_speed_pct = ctx.map.cached_pit.lane_speed_pct as f64;
    }
}

fn install_oval_demo(ctx: &mut WidgetCtx<'_>) {
    // Demo-only placeholder. Pin TrackID so we do not rebuild every frame.
    ctx.map.cached_track_id = ctx.frame.track_id;
    ctx.map.path_status = TrackPathStatus::Ready;
    ctx.map.cached_path = track_path::oval_path(64);
    ctx.map.cached_start_finish = 0.0;
    if let Some(synth) = track_path::synthesize_demo_pit(&ctx.map.cached_path) {
        ctx.map.cached_pit_out_pct = synth.out_pct;
        ctx.map.pit_lane_speed_pct = synth.lane_speed_pct as f64;
        ctx.map.cached_pit = synth;
    } else {
        ctx.map.cached_pit_out_pct = Some(0.08);
    }
}

fn live_status_for_miss(id: i32) -> TrackPathStatus {
    if crate::cloud::fetch_missed(id as i64) || !crate::cloud::read_available() {
        TrackPathStatus::Unavailable
    } else {
        TrackPathStatus::Loading
    }
}

fn ensure_path_cached(ctx: &mut WidgetCtx<'_>) {
    let tid = ctx.frame.track_id;
    // Background cloud fetch finished — force reload from disk.
    if let Some(id) = tid {
        if crate::cloud::take_fetch_ready(id as i64) {
            ctx.map.invalidate_track_cache();
        }
    }
    if tid == ctx.map.cached_track_id
        && ctx.map.path_status == TrackPathStatus::Ready
        && !ctx.map.cached_path.is_empty()
    {
        return;
    }
    // Soft poll while waiting / after a miss — keep trying JSON without wiping latches.
    if ctx.map.cached_track_id.is_none()
        && matches!(
            ctx.map.path_status,
            TrackPathStatus::Loading | TrackPathStatus::Unavailable
        )
    {
        if let Some(id) = tid {
            if let Some(tp) = track_path::load_for_track_id(id) {
                apply_loaded_track(ctx, id, tp);
                return;
            }
            crate::cloud::fetch_track_async(id as i64);
            ctx.map.path_status = live_status_for_miss(id);
            return;
        }
    }
    ctx.map.cached_path.clear();
    ctx.map.cached_track_name.clear();
    ctx.map.cached_start_finish = 0.0;
    ctx.map.cached_pit_out_pct = None;
    ctx.map.cached_pit = track_path::PitLane::default();
    ctx.map.cached_pit2 = track_path::PitLane::default();
    ctx.map.cached_drs_zones.clear();
    ctx.map.cached_p2p_zones.clear();
    ctx.map.cached_corners.clear();
    ctx.map.pit_route_latch.clear();
    ctx.map.pit_prev_on.clear();
    ctx.map.pit_exit_latch.clear();
    ctx.map.pit_latch_seed_pending = true;
    if let Some(id) = tid {
        if let Some(tp) = track_path::load_for_track_id(id) {
            apply_loaded_track(ctx, id, tp);
            return;
        }
        crate::cloud::fetch_track_async(id as i64);
        if ctx.demo {
            install_oval_demo(ctx);
            return;
        }
        ctx.map.cached_track_id = None;
        ctx.map.path_status = live_status_for_miss(id);
        return;
    }
    if ctx.demo {
        install_oval_demo(ctx);
        return;
    }
    ctx.map.cached_track_id = None;
    ctx.map.path_status = TrackPathStatus::None;
}

fn paint_path_status(ui: &mut Ui, ctx: &WidgetCtx<'_>, rect: Rect) {
    let (text, alpha, size) = match ctx.map.path_status {
        TrackPathStatus::Unavailable => ("Track unavailable", 110u8, 11.0_f32),
        TrackPathStatus::Loading | TrackPathStatus::None => ("Loading track…", 170u8, 13.0_f32),
        TrackPathStatus::Ready => return,
    };
    let scale = ctx.cfg.text_scale(SECTION);
    let font = FontId::new((size * scale).max(9.0), FontFamily::Proportional);
    let color = Color32::from_rgba_unmultiplied(200, 205, 215, alpha);
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, text, font, color);
}

fn fill_infield(ui: &mut Ui, screen: &[Pos2], fill: Color32) {
    if screen.len() < 3 {
        return;
    }
    // Drop duplicate closing vertex if the path repeats the start point.
    let mut pts: Vec<Pos2> = screen.to_vec();
    if pts.len() >= 2 {
        let a = pts[0];
        let b = *pts.last().unwrap();
        if (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3 {
            pts.pop();
        }
    }
    if pts.len() < 3 {
        return;
    }
    // Concave race outlines: centroid fan leaks triangles outside the polygon
    // (visible as black shards once the static map bg is cached for composite).
    let tris = earcut_triangles(&pts);
    if tris.is_empty() {
        return;
    }
    let mut mesh = egui::Mesh::default();
    let base = mesh.vertices.len() as u32;
    for p in &pts {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: egui::epaint::WHITE_UV,
            color: fill,
        });
    }
    for [a, b, c] in tris {
        mesh.indices
            .extend_from_slice(&[base + a, base + b, base + c]);
    }
    ui.painter().add(Shape::mesh(mesh));
}

/// Ear-clip a simple polygon into triangles (no holes). Returns index triples.
fn earcut_triangles(pts: &[Pos2]) -> Vec<[u32; 3]> {
    let n = pts.len();
    if n < 3 {
        return Vec::new();
    }
    let cross = |i: usize, j: usize, k: usize| -> f32 {
        let a = pts[i];
        let b = pts[j];
        let c = pts[k];
        (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y)
    };
    let point_in_tri = |p: Pos2, i: usize, j: usize, k: usize| -> bool {
        let a = pts[i];
        let b = pts[j];
        let c = pts[k];
        let area = (b.x - a.x) * (c.y - a.y) - (c.x - a.x) * (b.y - a.y);
        if area.abs() < 1e-8 {
            return false;
        }
        let s = if area > 0.0 { 1.0 } else { -1.0 };
        let d1 = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
        let d2 = (c.x - b.x) * (p.y - b.y) - (c.y - b.y) * (p.x - b.x);
        let d3 = (a.x - c.x) * (p.y - c.y) - (a.y - c.y) * (p.x - c.x);
        d1 * s >= -1e-5 && d2 * s >= -1e-5 && d3 * s >= -1e-5
    };

    // Ensure CCW for consistent ear tests.
    let mut idx: Vec<usize> = (0..n).collect();
    let signed: f32 = (0..n)
        .map(|i| {
            let j = (i + 1) % n;
            pts[i].x * pts[j].y - pts[j].x * pts[i].y
        })
        .sum();
    if signed < 0.0 {
        idx.reverse();
    }

    let mut out = Vec::with_capacity(n.saturating_sub(2));
    let mut guard = 0;
    let max_guard = n * n + 8;
    while idx.len() > 3 && guard < max_guard {
        guard += 1;
        let m = idx.len();
        let mut clipped = false;
        for i in 0..m {
            let i0 = idx[(i + m - 1) % m];
            let i1 = idx[i];
            let i2 = idx[(i + 1) % m];
            // Convex vertex in CCW polygon: positive cross.
            if cross(i0, i1, i2) <= 1e-8 {
                continue;
            }
            let mut ear = true;
            for &t in &idx {
                if t == i0 || t == i1 || t == i2 {
                    continue;
                }
                if point_in_tri(pts[t], i0, i1, i2) {
                    ear = false;
                    break;
                }
            }
            if !ear {
                continue;
            }
            out.push([i0 as u32, i1 as u32, i2 as u32]);
            idx.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            break;
        }
    }
    if idx.len() == 3 {
        out.push([idx[0] as u32, idx[1] as u32, idx[2] as u32]);
    }
    out
}

fn stroke_closed(ui: &mut Ui, screen: &[Pos2], width: f32, color: Color32) {
    if screen.len() < 2 {
        return;
    }
    ui.painter().add(Shape::Path(PathShape::closed_line(
        screen.to_vec(),
        PathStroke::new(width, color),
    )));
}

fn draw_start_finish(ui: &mut Ui, path: &[(f32, f32)], xform: &PlotXform, pct: f32) {
    draw_loop_tick(ui, path, xform, pct, 7.0, 3.0, Color32::WHITE, false);
}

/// Radial tick on the racing loop (S/F, pit exit, pace safety).
fn draw_loop_tick(
    ui: &mut Ui,
    path: &[(f32, f32)],
    xform: &PlotXform,
    pct: f32,
    tick: f32,
    width: f32,
    color: Color32,
    dashed: bool,
) {
    let (nx, ny) = track_path::point_at(path, pct);
    let (tx, ty) = track_path::tangent_at(path, pct);
    let px = -ty;
    let py = tx;
    let c = xform.map(nx, ny);
    let a = Pos2::new(c.x + px * tick, c.y + py * tick);
    let b = Pos2::new(c.x - px * tick, c.y - py * tick);
    if dashed {
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let segs = 5;
        for i in 0..segs {
            if i % 2 == 1 {
                continue;
            }
            let t0 = i as f32 / segs as f32;
            let t1 = (i + 1) as f32 / segs as f32;
            ui.painter().line_segment(
                [
                    Pos2::new(a.x + dx * t0, a.y + dy * t0),
                    Pos2::new(a.x + dx * t1, a.y + dy * t1),
                ],
                Stroke::new(width, color),
            );
        }
        let _ = len;
    } else {
        ui.painter().line_segment([a, b], Stroke::new(width, color));
    }
}

/// Dashed open polyline with a continuous dash phase along the whole path
/// (Python QPen DashLine). Resetting per segment looks solid on dense tracks.
fn stroke_open_dashed(
    ui: &mut Ui,
    screen: &[Pos2],
    width: f32,
    color: Color32,
    dash: f32,
    gap: f32,
) {
    if screen.len() < 2 {
        return;
    }
    let dash = dash.max(1.0);
    let gap = gap.max(0.5);
    let pattern = dash + gap;
    let stroke = Stroke::new(width, color);
    let mut phase = 0.0_f32; // distance into current dash+gap cycle
    for w in screen.windows(2) {
        let a = w[0];
        let b = w[1];
        let dx = b.x - a.x;
        let dy = b.y - a.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-3 {
            continue;
        }
        let ux = dx / len;
        let uy = dy / len;
        let mut consumed = 0.0_f32;
        while consumed < len {
            let in_dash = phase < dash;
            let remain = if in_dash {
                dash - phase
            } else {
                pattern - phase
            };
            let step = remain.min(len - consumed);
            if in_dash && step > 1e-4 {
                let t0 = consumed;
                let t1 = consumed + step;
                ui.painter().line_segment(
                    [
                        Pos2::new(a.x + ux * t0, a.y + uy * t0),
                        Pos2::new(a.x + ux * t1, a.y + uy * t1),
                    ],
                    stroke,
                );
            }
            consumed += step;
            phase += step;
            if phase >= pattern - 1e-6 {
                phase = 0.0;
            }
        }
    }
}

fn stroke_open(ui: &mut Ui, screen: &[Pos2], width: f32, color: Color32) {
    if screen.len() < 2 {
        return;
    }
    for w in screen.windows(2) {
        ui.painter()
            .line_segment([w[0], w[1]], Stroke::new(width, color));
    }
}

fn model_poly(pts: &[(f32, f32)], mirror: bool, rot: i32) -> Vec<(f32, f32)> {
    pts.iter()
        .map(|&(x, y)| model_point(x, y, mirror, rot))
        .collect()
}

fn screen_poly(xform: &PlotXform, modeled: &[(f32, f32)]) -> Vec<Pos2> {
    modeled.iter().map(|&(x, y)| xform.map(x, y)).collect()
}

fn draw_pit_lane(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    lane: &track_path::PitLane,
    xform: &PlotXform,
    mirror: bool,
    rot: i32,
    asphalt: Color32,
    show_entry: bool,
    show_road: bool,
    show_exit: bool,
) {
    if !lane.has_drawable() {
        return;
    }
    let opacity = ctx
        .cfg
        .f64_key(SECTION, "pit_lane_opacity", 1.0)
        .clamp(0.05, 1.0) as f32;
    let a = (opacity * 255.0) as u8;
    let a_asphalt = (opacity * 0.85 * 255.0) as u8;
    let show_blends = ctx.cfg.bool_key(SECTION, "show_pit_blends", true);
    let show_speed = ctx.cfg.bool_key(SECTION, "show_pit_speed", true);

    let pit_col = color_with_alpha(ctx.cfg.color(SECTION, "pit", "#ff4d4d"), a);
    let blend_in = color_with_alpha(ctx.cfg.color(SECTION, "pit_blend", "#ffd23a"), a);
    let blend_out = color_with_alpha(ctx.cfg.color(SECTION, "pit_blend_out", "#3aa0ff"), a);
    let asphalt_u = color_with_alpha(asphalt, a_asphalt);

    if show_blends && show_entry && lane.entry.len() >= 2 {
        let m = model_poly(&lane.entry, mirror, rot);
        let s = screen_poly(xform, &m);
        stroke_open_dashed(ui, &s, 2.5, blend_in, 3.0, 4.0);
    }
    if show_blends && show_exit && lane.exit.len() >= 2 {
        let m = model_poly(&lane.exit, mirror, rot);
        let s = screen_poly(xform, &m);
        stroke_open_dashed(ui, &s, 2.5, blend_out, 3.0, 4.0);
    }
    if show_road && lane.path.len() >= 2 {
        let m = model_poly(&lane.path, mirror, rot);
        let s = screen_poly(xform, &m);
        stroke_open(ui, &s, 7.0, asphalt_u);
        stroke_open_dashed(ui, &s, 2.2, pit_col, 4.0, 3.0);

        if show_speed {
            if let Some(ms) = lane.speed_ms.filter(|v| *v > 0.0) {
                let (val, unit) = (ctx.cfg.conv_speed(ms), ctx.cfg.speed_unit());
                let anchor = s[s.len() / 2];
                let txt = format!("PIT {val:.0} {unit}");
                let bg = color_with_alpha(ctx.cfg.color(SECTION, "pit", "#ff4d4d"), 235);
                let fg = ctx.cfg.color(SECTION, "pit_text", "#ffffff");
                let font = FontId::new(11.0, FontFamily::Proportional);
                let galley = ui.fonts(|f| f.layout_no_wrap(txt, font, fg));
                let pad = 4.0;
                let rect = Rect::from_min_size(
                    Pos2::new(
                        anchor.x - galley.size().x * 0.5 - pad,
                        anchor.y - galley.size().y - pad * 2.0,
                    ),
                    egui::vec2(galley.size().x + pad * 2.0, galley.size().y + pad),
                );
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(4), bg);
                ui.painter().galley(
                    Pos2::new(rect.left() + pad, rect.top() + pad * 0.5),
                    galley,
                    fg,
                );
            }
        }
    }
}

fn pct_in_interval(pct: f32, lo: f32, hi: f32) -> bool {
    let span = (hi - lo).rem_euclid(1.0);
    if span <= 1e-6 {
        return false;
    }
    ((pct - lo).rem_euclid(1.0)) <= span
}

/// Lap-% → loop fraction along the racing polyline (Python `_loop_frac_for_pct`).
/// `reverse_path`: SVG driving direction opposite iRacing LapDistPct.
fn loop_frac_for_pct(pct: f32, start_finish: f32, reverse: bool) -> f32 {
    let d = (pct - start_finish).rem_euclid(1.0);
    if reverse {
        (1.0 - d).rem_euclid(1.0)
    } else {
        d
    }
}

/// Loop arc fraction where lap pct 0 (S/F) sits.
fn sf_loop_frac(start_finish: f32, reverse: bool) -> f32 {
    loop_frac_for_pct(0.0, start_finish, reverse)
}

/// Choose `start_finish` so live `telem` maps to `loop_frac` (click-to-calibrate).
fn sf_for_player_at(telem: f32, loop_frac: f32, reverse: bool) -> f32 {
    if reverse {
        (loop_frac + telem).rem_euclid(1.0)
    } else {
        (telem - loop_frac).rem_euclid(1.0)
    }
}

fn map_reverse(ctx: &WidgetCtx<'_>) -> bool {
    ctx.cfg.bool_key(SECTION, "reverse_path", false)
}

fn hypot2(a: (f32, f32), b: (f32, f32)) -> f32 {
    let dx = a.0 - b.0;
    let dy = a.1 - b.1;
    dx * dx + dy * dy
}

fn blend_xy(a: (f32, f32), b: (f32, f32), w: f32) -> (f32, f32) {
    let w = w.clamp(0.0, 1.0);
    (a.0 + (b.0 - a.0) * w, a.1 + (b.1 - a.1) * w)
}

fn open_poly_length(pts: &[(f32, f32)]) -> f32 {
    if pts.len() < 2 {
        return 0.0;
    }
    pts.windows(2)
        .map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1))
        .sum()
}

/// Python `_pit_arc_length`.
fn pit_arc_length(segments: &[&[(f32, f32)]]) -> f32 {
    segments
        .iter()
        .filter(|s| s.len() >= 2)
        .map(|s| open_poly_length(s))
        .sum()
}

/// Python `_loop_arc_between`: racing-loop arc over wrapping [lo, hi].
fn loop_arc_between(racing: &[(f32, f32)], lo: f32, hi: f32) -> f32 {
    if racing.len() < 2 {
        return 0.0;
    }
    let span = (hi - lo).rem_euclid(1.0);
    if span <= 1e-9 {
        return 0.0;
    }
    let mut total = 0.0_f32;
    for i in 0..racing.len() {
        let a = racing[i];
        let b = racing[(i + 1) % racing.len()];
        total += (b.0 - a.0).hypot(b.1 - a.1);
    }
    span * total
}

/// Python `_pit_phase_pos` (entry/exit blends only).
fn pit_phase_pos(
    pct: f32,
    lo: f32,
    hi: f32,
    seg: &[(f32, f32)],
    racing: &[(f32, f32)],
    speed: f32,
) -> Option<(f32, f32)> {
    if seg.len() < 2 {
        return None;
    }
    let span = (hi - lo).rem_euclid(1.0);
    if span <= 1e-6 {
        return None;
    }
    let linear = ((pct - lo).rem_euclid(1.0) / span).clamp(0.0, 1.0);
    let speed = if speed > 0.0 { speed } else { 1.0 };
    let pit_arc = pit_arc_length(&[seg]);
    let loop_arc = loop_arc_between(racing, lo, hi);
    let t = if pit_arc > 1e-9 && loop_arc > 1e-9 {
        (linear * (loop_arc / pit_arc) * speed).clamp(0.0, 1.0)
    } else {
        (linear * speed).clamp(0.0, 1.0)
    };
    Some(track_path::point_on_open(seg, t))
}

/// Python `_pit_blend_weight` (schematic).
fn pit_blend_weight(
    pct: f32,
    on_route: bool,
    on_pit: bool,
    in_entry: bool,
    in_exit: bool,
    route_lo: f32,
    route_hi: f32,
) -> f32 {
    if !on_route && !on_pit {
        return 0.0;
    }
    let span = (route_hi - route_lo).rem_euclid(1.0);
    if span <= 1e-6 {
        return 0.0;
    }
    let feather = (span * 0.12).clamp(0.012, span * 0.35);
    if in_entry {
        let d_entry = (pct - route_lo).rem_euclid(1.0);
        if d_entry < feather {
            return d_entry / feather;
        }
        return 1.0;
    }
    if in_exit {
        let d_exit = (route_hi - pct).rem_euclid(1.0);
        if d_exit < feather {
            return if on_route { d_exit / feather } else { 0.0 };
        }
        return if on_route { 1.0 } else { 0.0 };
    }
    if on_pit || on_route {
        return 1.0;
    }
    0.0
}

/// Python `_pit_path_handoff_point`: where pit_in meets pit_path.
fn pit_path_handoff(lane: &track_path::PitLane) -> Option<(f32, f32)> {
    if let Some(p) = lane.entry.last() {
        return Some(*p);
    }
    lane.path.first().copied()
}

/// Python `_pit_path_needs_reverse`.
fn pit_path_needs_reverse(lane: &track_path::PitLane) -> bool {
    if lane.path.len() < 2 {
        return false;
    }
    let Some(handoff) = pit_path_handoff(lane) else {
        return false;
    };
    let p0 = lane.path[0];
    let p1 = *lane.path.last().unwrap();
    hypot2(p1, handoff) < hypot2(p0, handoff)
}

/// Project pit_path endpoints onto the racing loop; prefer authored span when
/// it lies inside that projection (Python `_pit_lane_bounds`).
fn pit_lane_bounds(racing: &[(f32, f32)], lane: &track_path::PitLane) -> Option<(f32, f32)> {
    if lane.path.len() < 2 || racing.len() < 2 {
        return lane.span;
    }
    let mut path_lo = track_path::nearest_pct_on_loop(racing, lane.path[0].0, lane.path[0].1);
    let mut path_hi = {
        let last = *lane.path.last().unwrap();
        track_path::nearest_pct_on_loop(racing, last.0, last.1)
    };
    if let Some((lane_lo, lane_hi)) = lane.span {
        if pct_in_interval(lane_lo, path_lo, path_hi) {
            path_lo = lane_lo;
        }
        if pct_in_interval(lane_hi, path_lo, path_hi) {
            path_hi = lane_hi;
        }
    }
    Some((path_lo, path_hi))
}

/// Python `_pit_lane_mapping_interval`: wide oval spans use path projection.
fn pit_lane_mapping_interval(
    racing: &[(f32, f32)],
    lane: &track_path::PitLane,
) -> Option<(f32, f32)> {
    let path_bounds = pit_lane_bounds(racing, lane);
    let Some((lane_lo, lane_hi)) = lane.span else {
        return path_bounds;
    };
    let span = (lane_hi - lane_lo).rem_euclid(1.0);
    if span > 0.5 {
        if let Some(pb) = path_bounds {
            return Some(pb);
        }
    }
    Some((lane_lo, lane_hi))
}

/// Python `_pit_path_pos_for_route_pct` (linear * speed; no tip-pin).
fn pit_path_pos_for_route_pct(
    lane: &track_path::PitLane,
    pct: f32,
    lo: f32,
    hi: f32,
    speed: f32,
) -> Option<(f32, f32)> {
    if lane.path.len() < 2 {
        return None;
    }
    let span = (hi - lo).rem_euclid(1.0);
    if span <= 1e-6 {
        return None;
    }
    let speed = if speed > 0.0 { speed } else { 1.0 };
    let linear = ((pct - lo).rem_euclid(1.0) / span).clamp(0.0, 1.0);
    let mut t = (linear * speed).clamp(0.0, 1.0);
    if pit_path_needs_reverse(lane) {
        t = 1.0 - t;
    }
    Some(track_path::point_on_open(&lane.path, t))
}

fn authoring_lane_speed(ctx: &WidgetCtx<'_>, lane: &track_path::PitLane) -> f32 {
    let authored = ctx.map.pit_lane_speed_pct as f32;
    if authored > 0.0 {
        authored
    } else if lane.lane_speed_pct > 0.0 {
        lane.lane_speed_pct
    } else {
        1.0
    }
}

fn update_pit_route_latches(ctx: &mut WidgetCtx<'_>, cars: &[&CarRow]) {
    let lane = &ctx.map.cached_pit;
    let route_lo = lane
        .in_pct
        .or(lane.span.map(|(a, _)| a))
        .unwrap_or(track_path::DEMO_PIT_IN_PCT);
    let route_hi = lane
        .out_pct
        .or(ctx.map.cached_pit_out_pct)
        .or(lane.span.map(|(_, b)| b))
        .unwrap_or(track_path::DEMO_PIT_OUT_PCT);

    let mut seen = std::collections::HashSet::new();
    for car in cars {
        let idx = car.car_idx;
        seen.insert(idx);
        // Latch on CarIdxOnPitRoad only — not ApproachingPits / stall.
        let on = car.on_pit;
        let pct = car.lap_dist_pct;
        let prev = ctx.map.pit_prev_on.get(&idx).copied().unwrap_or(false);
        if prev && !on && pct >= 0.0 {
            ctx.map.pit_exit_latch.insert(idx, pct.rem_euclid(1.0));
        }
        ctx.map.pit_prev_on.insert(idx, on);

        if on {
            ctx.map.pit_route_latch.insert(idx, true);
            continue;
        }
        let latched = ctx.map.pit_route_latch.get(&idx).copied().unwrap_or(false);
        if latched {
            if pct >= 0.0 && pct_in_interval(pct, route_lo, route_hi) {
                ctx.map.pit_route_latch.insert(idx, true);
            } else {
                ctx.map.pit_route_latch.insert(idx, false);
                ctx.map.pit_exit_latch.remove(&idx);
            }
        }
    }
    ctx.map.pit_route_latch.retain(|k, _| seen.contains(k));
    ctx.map.pit_prev_on.retain(|k, _| seen.contains(k));
    ctx.map.pit_exit_latch.retain(|k, _| seen.contains(k));
}

fn car_on_route(ctx: &WidgetCtx<'_>, car: &CarRow) -> bool {
    if car.on_pit
        || ctx
            .map
            .pit_route_latch
            .get(&car.car_idx)
            .copied()
            .unwrap_or(false)
    {
        return true;
    }
    // Python: player ApproachingPits during entry lap-% is on-route.
    if car.is_player && car.approaching_pits && car.lap_dist_pct >= 0.0 {
        let lane = &ctx.map.cached_pit;
        if lane.has_drawable() {
            let in_pct = lane
                .in_pct
                .or(lane.span.map(|(a, _)| a))
                .unwrap_or(track_path::DEMO_PIT_IN_PCT);
            let lane_lo = lane
                .span
                .map(|(a, _)| a)
                .unwrap_or(track_path::DEMO_PIT_LANE_LO);
            if pct_in_interval(car.lap_dist_pct.rem_euclid(1.0), in_pct, lane_lo) {
                return true;
            }
        }
    }
    false
}

/// Seed latches for cars already on the pit route after a mid-session track load.
fn seed_pit_latches(ctx: &mut WidgetCtx<'_>) {
    if !ctx.map.pit_latch_seed_pending {
        return;
    }
    ctx.map.pit_latch_seed_pending = false;
    let lane = ctx.map.cached_pit.clone();
    if !lane.has_drawable() {
        return;
    }
    let route_lo = lane
        .in_pct
        .or(lane.span.map(|(a, _)| a))
        .unwrap_or(track_path::DEMO_PIT_IN_PCT);
    let route_hi = lane
        .out_pct
        .or(ctx.map.cached_pit_out_pct)
        .or(lane.span.map(|(_, b)| b))
        .unwrap_or(track_path::DEMO_PIT_OUT_PCT);
    for car in &ctx.frame.cars {
        let pct = car.lap_dist_pct;
        if pct < 0.0 {
            continue;
        }
        if car.on_pit || pct_in_interval(pct, route_lo, route_hi) {
            ctx.map.pit_route_latch.insert(car.car_idx, true);
        }
    }
}

/// Python `_pos_for_schematic_route` (raw=True) — phase poly only.
fn schematic_route_pos(
    ctx: &WidgetCtx<'_>,
    car: &CarRow,
    pct: f32,
    racing: &[(f32, f32)],
    lane: &track_path::PitLane,
    show_blends: bool,
) -> Option<(f32, f32)> {
    let on_pit = car.on_pit;
    let on_route = car_on_route(ctx, car);
    if !on_route {
        return None;
    }
    if !lane.has_drawable() {
        return None;
    }
    let in_pct = lane
        .in_pct
        .or(lane.span.map(|(a, _)| a))
        .unwrap_or(track_path::DEMO_PIT_IN_PCT);
    let out_pct = lane
        .out_pct
        .or(ctx.map.cached_pit_out_pct)
        .or(lane.span.map(|(_, b)| b))
        .unwrap_or(track_path::DEMO_PIT_OUT_PCT);
    let (lane_lo, lane_hi) = lane
        .span
        .unwrap_or((track_path::DEMO_PIT_LANE_LO, track_path::DEMO_PIT_LANE_HI));
    let speed = authoring_lane_speed(ctx, lane);

    // Exit blend: pit lane end -> rejoin (off pit road).
    if show_blends && !on_pit && lane.exit.len() >= 2 {
        let exit_start = ctx
            .map
            .pit_exit_latch
            .get(&car.car_idx)
            .copied()
            .unwrap_or(lane_hi);
        if pct_in_interval(pct, exit_start, out_pct) {
            return pit_phase_pos(pct, exit_start, out_pct, &lane.exit, racing, speed);
        }
    }

    // Entry blend (also while OnPitRoad — telem often asserts before lane_lo).
    let entry_end = lane_lo;
    if show_blends && lane.entry.len() >= 2 && pct_in_interval(pct, in_pct, entry_end) {
        return pit_phase_pos(pct, in_pct, entry_end, &lane.entry, racing, speed);
    }

    // Pit road while OnPitRoad.
    if on_pit && lane.path.len() >= 2 {
        let (rlo, rhi) = pit_lane_mapping_interval(racing, lane)
            .or(Some((in_pct, out_pct)))
            .unwrap_or((lane_lo, lane_hi));
        return pit_path_pos_for_route_pct(lane, pct, rlo, rhi, speed);
    }
    None
}

/// Python `_resolve_car_point` schematic branch: route + blend weight vs track.
fn car_model_xy(
    ctx: &WidgetCtx<'_>,
    car: &CarRow,
    pct: f32,
    racing: &[(f32, f32)],
    pit: &track_path::PitLane,
    pit2: &track_path::PitLane,
    show_blends: bool,
) -> (f32, f32) {
    let on_pit = car.on_pit;
    let on_route = car_on_route(ctx, car);
    let sf = ctx.map.cached_start_finish;
    let track = track_path::point_at(racing, loop_frac_for_pct(pct, sf, map_reverse(ctx)));
    if !on_route && !on_pit {
        return track;
    }

    let lane = if pit.has_drawable() {
        pit
    } else if pit2.has_drawable() {
        pit2
    } else {
        return track;
    };

    let in_pct = lane
        .in_pct
        .or(lane.span.map(|(a, _)| a))
        .unwrap_or(track_path::DEMO_PIT_IN_PCT);
    let out_pct = lane
        .out_pct
        .or(ctx.map.cached_pit_out_pct)
        .or(lane.span.map(|(_, b)| b))
        .unwrap_or(track_path::DEMO_PIT_OUT_PCT);
    let (lane_lo, lane_hi) = lane
        .span
        .unwrap_or((track_path::DEMO_PIT_LANE_LO, track_path::DEMO_PIT_LANE_HI));

    // Python `_pit_route_phases`: entry/exit false while OnPitRoad.
    let in_entry = !on_pit && pct_in_interval(pct, in_pct, lane_lo);
    let in_exit = !on_pit && pct_in_interval(pct, lane_hi, out_pct);

    let weight = pit_blend_weight(pct, on_route, on_pit, in_entry, in_exit, in_pct, out_pct);
    if weight <= 0.0 {
        return track;
    }

    let Some(route) = schematic_route_pos(ctx, car, pct, racing, lane, show_blends) else {
        return track;
    };
    if weight >= 1.0 {
        route
    } else {
        blend_xy(track, route, weight)
    }
}

fn is_caution_flag(flag: Option<&str>) -> bool {
    matches!(
        flag,
        Some("yellow") | Some("caution") | Some("yellow_waving") | Some("caution_waving")
    )
}

/// Python `_dot_scale`: frac relative to 0.05 default, clamped.
fn dot_scale(frac: f64) -> f32 {
    let mut f = if frac <= 0.0 { 0.05 } else { frac };
    if f <= 0.0 {
        f = 0.05;
    }
    ((f / 0.05) as f32).clamp(0.2, 4.0)
}

/// Python map colors: player / competitor / lap tint / pit / pace — not class color.
fn car_fill(cfg: &OverlayConfig, car: &CarRow, pit_opacity: f32, on_route: bool) -> Color32 {
    if car.is_pace_car {
        return cfg.color(SECTION, "pace_car", "#0b0e12");
    }
    // Python `_car_dot_style`: grey when on_pit or on_route (non-player).
    if (car.on_pit || on_route) && !car.is_player {
        let pit = cfg.color(SECTION, "pit_car", "#6e747d");
        return color_with_alpha(pit, (pit_opacity.clamp(0.05, 1.0) * 255.0) as u8);
    }
    if car.is_player {
        return cfg.color(SECTION, "player", "#46df7a");
    }
    if car.lapping && car.lap_ahead {
        // Blue = this car is lapping the player/focus car.
        return cfg.color(SECTION, "lapped", "#2563eb");
    }
    if car.lapping {
        // Red = this is lapped traffic for the player/focus car.
        return cfg.color(SECTION, "lapping", "#ff5050");
    }
    cfg.color(SECTION, "competitor", "#b06bff")
}

fn draw_player_dot(ui: &mut Ui, c: Pos2, r: f32, fill: Color32) {
    let glow = color_with_alpha(fill, 70);
    ui.painter().circle_filled(c, r + 6.0, glow);
    ui.painter().circle_filled(c, r, fill);
    ui.painter()
        .circle_stroke(c, r, Stroke::new(2.0_f32, Color32::BLACK));
    ui.painter()
        .circle_stroke(c, r + 2.4, Stroke::new(2.4_f32, Color32::WHITE));
}

fn draw_other_dot(ui: &mut Ui, c: Pos2, r: f32, fill: Color32) {
    ui.painter().circle_filled(c, r, fill);
    let ring = color_with_alpha(Color32::BLACK, fill.a());
    ui.painter().circle_stroke(c, r, Stroke::new(1.0_f32, ring));
}

fn car_label_text(car: &CarRow, mode: &str) -> String {
    if car.is_pace_car {
        return "PC".into();
    }
    if mode == "position" && car.position > 0 {
        return car.position.to_string();
    }
    if !car.car_number.is_empty() {
        return car.car_number.clone();
    }
    "?".into()
}

/// Stroked centered label (Python `_draw_stroked_center_text`).
fn draw_car_number_label(
    ui: &mut Ui,
    c: Pos2,
    text: &str,
    _r: f32,
    is_player: bool,
    is_pace: bool,
    text_scale: f32,
    dot_fill: Color32,
) {
    if text.is_empty() {
        return;
    }
    let base = if is_player { 10.0 } else { 8.5 };
    let size = (base * text_scale).max(7.0);
    let font = FontId::new(size, FontFamily::Proportional);
    // Always pick high-contrast ink for the dot fill (white on blue/black/purple).
    let fill = crate::chrome::contrast_text(dot_fill);
    let stroke = if fill == Color32::WHITE {
        Color32::from_rgba_unmultiplied(0, 0, 0, 230)
    } else {
        Color32::from_rgba_unmultiplied(255, 255, 255, 220)
    };
    let stroke_w = if is_player || is_pace {
        1.35_f32
    } else {
        1.15
    };
    // Use the same subpixel center as the eased dot (no pixel-snap lag).
    let pos = c;
    let offsets: &[(f32, f32)] = &[
        (-stroke_w, -stroke_w),
        (-stroke_w, stroke_w),
        (stroke_w, -stroke_w),
        (stroke_w, stroke_w),
        (-stroke_w, 0.0),
        (stroke_w, 0.0),
        (0.0, -stroke_w),
        (0.0, stroke_w),
    ];
    let painter = ui.painter();
    for &(ox, oy) in offsets {
        painter.text(
            Pos2::new(pos.x + ox, pos.y + oy),
            Align2::CENTER_CENTER,
            text,
            font.clone(),
            stroke,
        );
    }
    painter.text(pos, Align2::CENTER_CENTER, text, font, fill);
}

fn wrap_lap_delta(them: f32, me: f32) -> f32 {
    let mut delta = them - me;
    if delta > 0.5 {
        delta -= 1.0;
    } else if delta < -0.5 {
        delta += 1.0;
    }
    delta
}

/// Smooth lap-% for map dots (motion rev 23).
///
/// Rev 22: presents matched host fps (~70) but cars still stuttered — coast +
/// `TELEM_CORRECT` yank (ahead then pull back each sample). Rev 15-style predict
/// also froze when `age` used `SessionTime` (held between SDK ticks → age=0).
/// Rev 23: `predict = last_telem + vel * wall_age`, ease toward predict; update
/// anchor/vel only on meaningful telem Δ; no hard blend. Present ~30 Hz (Python).
fn advance_car_pcts(ctx: &mut WidgetCtx<'_>, dt: f32, wall_secs: f64) -> HashMap<i32, f32> {
    let mut out = HashMap::new();
    let telem_clock = wall_secs;
    let session_now = ctx.frame.session_time;
    let player_pct = ctx.frame.player_lap_dist_pct;
    let lap_est = ctx.frame.lap_est_time;
    let vel_cap = if lap_est > 10.0 {
        1.08 / lap_est
    } else {
        0.045
    };

    for car in &ctx.frame.cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        if car.lap_dist_pct < 0.0 {
            continue;
        }
        let telem = if car.is_player && player_pct >= 0.0 && player_pct.is_finite() {
            player_pct.rem_euclid(1.0)
        } else {
            car.lap_dist_pct.rem_euclid(1.0)
        };

        let mut st = ctx
            .map
            .car_anim
            .get(&car.car_idx)
            .copied()
            .unwrap_or_else(|| fresh_pct_anim(telem, telem_clock, session_now));
        st.last_seen_secs = telem_clock;

        let pin = car.on_pit || ctx.demo;
        if pin {
            st = fresh_pct_anim(telem, telem_clock, session_now);
            st.last_seen_secs = telem_clock;
            ctx.map.car_anim.insert(car.car_idx, st);
            out.insert(car.car_idx, telem);
            continue;
        }

        if dt <= 0.0 {
            ctx.map.car_anim.insert(car.car_idx, st);
            out.insert(car.car_idx, st.pct);
            continue;
        }

        let d_telem = wrap_lap_delta(telem, st.last_telem);
        if d_telem.abs() > TELEM_EPS {
            let sess_dt = session_now - st.last_telem_session;
            let wall_dt = telem_clock - st.last_telem_secs;
            let sample_dt = if st.last_telem_session > 0.0
                && sess_dt > VEL_DT_MIN
                && sess_dt < VEL_DT_MAX
            {
                sess_dt
            } else if st.last_telem_secs > 0.0 && wall_dt > VEL_DT_MIN && wall_dt < VEL_DT_MAX {
                wall_dt
            } else {
                0.0
            };
            if sample_dt > 0.0 {
                let inst = d_telem / sample_dt as f32;
                st.vel += (inst - st.vel) * VEL_EMA;
                st.vel = st.vel.clamp(-vel_cap, vel_cap);
            }
            st.last_telem = telem;
            st.last_telem_secs = telem_clock;
            st.last_telem_session = session_now;
        } else {
            let quiet = telem_clock - st.last_telem_secs;
            if quiet > QUIET_DECAY_SECS {
                st.vel = 0.0;
            }
        }

        // Wall-clock age — SessionTime is flat between SDK ticks, so using it
        // for predict age freezes the target (looks like move/stop stutter).
        let age = ((telem_clock - st.last_telem_secs) as f32)
            .max(0.0)
            .min(PREDICT_AGE_MAX);
        let predict = (st.last_telem + st.vel * age).rem_euclid(1.0);
        let err = wrap_lap_delta(predict, st.pct);
        if err.abs() > PCT_SNAP_ERR {
            st.pct = telem;
            st.vel = 0.0;
            st.last_telem = telem;
            st.last_telem_secs = telem_clock;
            st.last_telem_session = session_now;
        } else {
            let a = 1.0 - (-dt / PCT_FOLLOW_TAU).exp();
            st.pct = (st.pct + err * a).rem_euclid(1.0);
        }

        ctx.map.car_anim.insert(car.car_idx, st);
        out.insert(car.car_idx, st.pct);
    }

    ctx.map
        .car_anim
        .retain(|_, st| (telem_clock - st.last_seen_secs) < PCT_SEEN_GRACE_SECS);
    out
}

/// True while any racing-line car still has predict/ease error or coast vel.
fn cars_pct_animating(ctx: &WidgetCtx<'_>) -> bool {
    if ctx.demo {
        return false;
    }
    let wall = ctx.mono_secs;
    for car in &ctx.frame.cars {
        if car.on_pit || car.lap_dist_pct < 0.0 {
            continue;
        }
        let Some(st) = ctx.map.car_anim.get(&car.car_idx) else {
            continue;
        };
        if st.vel.abs() > PCT_MOVING_EPS {
            return true;
        }
        let age = ((wall - st.last_telem_secs) as f32)
            .max(0.0)
            .min(PREDICT_AGE_MAX);
        let predict = (st.last_telem + st.vel * age).rem_euclid(1.0);
        if wrap_lap_delta(predict, st.pct).abs() > PCT_MOVING_EPS {
            return true;
        }
    }
    false
}

fn fresh_pct_anim(telem: f32, wall_secs: f64, session_now: f64) -> crate::state::CarPctAnim {
    crate::state::CarPctAnim {
        pct: telem,
        vel: 0.0,
        last_telem: telem,
        last_telem_secs: wall_secs,
        last_telem_session: session_now,
        last_seen_secs: wall_secs,
    }
}

/// Advance map car positions on the host clock. Idempotent within the same
/// `mono_secs` tick (`dt≈0` keeps eased pct). Call every host frame.
/// Returns the dt used this tick (0 on re-entry).
pub fn tick_car_motion(ctx: &mut WidgetCtx<'_>) -> f32 {
    // Resolve track JSON even when GL paint is skipped (CPU-composite hot path).
    ensure_path_cached(ctx);
    let wall_secs = ctx.mono_secs;
    let dt = if ctx.map.last_paint_secs > 0.0 {
        ((wall_secs - ctx.map.last_paint_secs) as f32).clamp(0.0, 0.1)
    } else {
        1.0 / 60.0
    };
    // Already advanced this mono instant — do not bump the clock again.
    if dt <= 1e-6 && ctx.map.last_paint_secs > 0.0 {
        let _ = advance_car_pcts(ctx, 0.0, wall_secs);
        return 0.0;
    }
    ctx.map.last_paint_secs = wall_secs;
    let _ = advance_car_pcts(ctx, dt, wall_secs);
    dt
}

/// Python `_car_motion_key` (schematic): pct vs route + pit flags.
fn car_motion_key(on_route: bool, on_pit: bool) -> u8 {
    let mode = if on_route { 1u8 } else { 0u8 };
    mode | ((on_route as u8) << 1) | ((on_pit as u8) << 2)
}

fn is_on_pit_key(key: u8) -> bool {
    key & 4 != 0
}

fn is_pct_mode_key(key: u8) -> bool {
    // mode nibble 0 = racing-line pct (Python key[0] == "pct")
    (key & 1) == 0
}

/// Python `_smooth_marker_point`.
fn smooth_marker_point(cur: Pos2, tgt: Pos2, dt: f32, tau: f32) -> (Pos2, bool) {
    let dx = tgt.x - cur.x;
    let dy = tgt.y - cur.y;
    if dx * dx + dy * dy > SCREEN_EASE_SNAP * SCREEN_EASE_SNAP {
        return (tgt, false);
    }
    let nx = ease(cur.x, tgt.x, dt, tau);
    let ny = ease(cur.y, tgt.y, dt, tau);
    let moving = (nx - tgt.x).abs() > 0.35 || (ny - tgt.y).abs() > 0.35;
    (Pos2::new(nx, ny), moving)
}

/// Place car dots like Python `_build_smooth_car_screen_points`:
/// - **pct** mode: use target from eased lap-% (no second screen ease)
/// - **route** mode: screen-space ease toward route targets
/// - **pit**: snap to target
/// Returns `(points, still_animating)`.
fn smooth_car_screen_pts(
    ctx: &mut WidgetCtx<'_>,
    targets: &HashMap<i32, (Pos2, u8)>,
    dt: f32,
) -> (HashMap<i32, Pos2>, bool) {
    let mut pts = HashMap::new();
    let mut seen = std::collections::HashSet::new();
    let mut animating = false;
    for (&idx, &(target, key)) in targets {
        seen.insert(idx);
        let prev = ctx.map.car_screen.get(&idx).copied();
        let start = match prev {
            Some(st) => Pos2::new(st.x, st.y),
            None => target,
        };
        let key_changed = prev.map(|st| st.key != key).unwrap_or(true);
        let (pt, moving) = if key_changed && start == target {
            (target, false)
        } else if is_on_pit_key(key) || is_pct_mode_key(key) {
            // Pct / pit: position already from eased % or telem — no XY lag.
            (target, false)
        } else {
            smooth_marker_point(start, target, dt, SCREEN_EASE_TAU)
        };
        if moving {
            animating = true;
        }
        ctx.map.car_screen.insert(
            idx,
            crate::state::CarScreenAnim {
                x: pt.x,
                y: pt.y,
                key,
            },
        );
        pts.insert(idx, pt);
    }
    ctx.map.car_screen.retain(|k, _| seen.contains(k));
    (pts, animating)
}

fn outward_from(pt: Pos2, cc: Pos2, extra: f32) -> Pos2 {
    let dx = pt.x - cc.x;
    let dy = pt.y - cc.y;
    let ln = (dx * dx + dy * dy).sqrt().max(1.0);
    Pos2::new(pt.x + dx / ln * extra, pt.y + dy / ln * extra)
}

/// Screen-space inset so outward labels/icons are not clipped (Python `_layout_pad`).
fn layout_pad(cfg: &OverlayConfig, asphalt_w: f32, text_scale: f32) -> f32 {
    let pad = 26.0_f32;
    let mut outward = 0.0_f32;
    if cfg.bool_key(SECTION, "show_traffic_markers", true) {
        let sz = (10.0 * text_scale).max(8.0);
        let icon_off = asphalt_w * 1.2 + sz + 10.0;
        let side = (sz + 6.0).max(22.0);
        let pill_h = sz + 14.0;
        outward = outward.max(icon_off + side * 0.5 + pill_h + 4.0);
    }
    if cfg.bool_key(SECTION, "show_corners", true) {
        let sz = (8.0 * text_scale).max(5.0);
        let off = asphalt_w * 0.5 + sz + 8.0;
        outward = outward.max(off + sz + 8.0);
    }
    if cfg.bool_key(SECTION, "show_sector_boundaries", true) {
        let sz = (7.0 * text_scale).max(5.0);
        let off = asphalt_w * 0.5 + sz + 6.0;
        outward = outward.max(off + sz + 6.0);
    }
    pad + outward
}

fn marker_color_key(slot: &str) -> (&'static str, &'static str) {
    match slot {
        "leader" => ("marker_leader", "#ffd23a"),
        "ahead" => ("marker_ahead", "#46df7a"),
        "behind" => ("marker_behind", "#ff5050"),
        _ => ("marker_ahead", "#46df7a"),
    }
}

fn marker_glyph_name(slot: &str) -> &'static str {
    match slot {
        "leader" => "leader",
        "ahead" => "car_ahead",
        "behind" => "car_behind",
        _ => "car_ahead",
    }
}

fn draw_traffic_markers(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    markers: &HashMap<&'static str, Option<TrafficMarker>>,
    car_pts: &HashMap<i32, Pos2>,
    path: &[(f32, f32)],
    xform: &PlotXform,
    mirror: bool,
    rot: i32,
    centroid: Pos2,
    asphalt_w: f32,
    text_scale: f32,
    start_finish: f32,
    reverse: bool,
) {
    let sz = (10.0 * text_scale).max(8.0);
    let icon_off = asphalt_w * 1.2 + sz + 10.0;
    let specs = ["leader", "ahead", "behind"];
    for slot in specs {
        let Some(Some(m)) = markers.get(slot) else {
            continue;
        };
        let car_pt = car_pts.get(&m.idx).copied().unwrap_or_else(|| {
            let (nx, ny) = track_path::point_at(path, loop_frac_for_pct(m.pct, start_finish, reverse));
            let (mx, my) = model_point(nx, ny, mirror, rot);
            xform.map(mx, my)
        });
        let icon_pt = outward_from(car_pt, centroid, icon_off);
        let (col_key, fallback) = marker_color_key(slot);
        let col = cfg.color(SECTION, col_key, fallback);
        ui.painter()
            .line_segment([car_pt, icon_pt], Stroke::new(2.0_f32, col));
        let side = (sz + 6.0).max(22.0);
        let icon_rect = Rect::from_center_size(icon_pt, egui::vec2(side, side));
        draw_dark_cell(ui, cfg, SECTION, icon_rect, 5.0);
        if let Some(g) = icons::glyph(marker_glyph_name(slot)) {
            ui.painter()
                .text(icon_pt, Align2::CENTER_CENTER, g, icons::font_id(sz), col);
        }
        if !m.label.is_empty() {
            let font = FontId::new((sz - 2.0).max(7.0), FontFamily::Proportional);
            let galley = ui.painter().layout_no_wrap(
                m.label.clone(),
                font.clone(),
                Color32::from_rgb(20, 20, 20),
            );
            let pw = galley.size().x + 8.0;
            let ph = galley.size().y + 4.0;
            let pill = Rect::from_min_size(
                Pos2::new(icon_pt.x - pw / 2.0, icon_rect.bottom() + 2.0),
                egui::vec2(pw, ph),
            );
            ui.painter()
                .rect_filled(pill, egui::CornerRadius::same(3), col);
            ui.painter().galley(
                Pos2::new(
                    pill.center().x - galley.size().x / 2.0,
                    pill.center().y - galley.size().y / 2.0,
                ),
                galley,
                Color32::from_rgb(20, 20, 20),
            );
        }
    }
}

fn draw_speaking(ui: &mut Ui, cfg: &OverlayConfig, c: Pos2, r: f32) {
    let ring = cfg.color(SECTION, "speaking_ring", "#46df7a");
    let glow = cfg.color(SECTION, "speaking_glow", "#46df7a55");
    ui.painter().circle_filled(c, r + 7.5, glow);
    ui.painter()
        .circle_stroke(c, r + 4.8, Stroke::new(2.8_f32, ring));
    let sz = (r * 1.05).max(7.0);
    let side = sz + 6.0;
    let badge_c = Pos2::new(c.x + r * 0.95, c.y - r * 1.05);
    let bg = cfg.color(SECTION, "speaking_badge_bg", "#22c55e");
    let fg = cfg.color(SECTION, "speaking_badge_text", "#ffffff");
    ui.painter().circle_filled(badge_c, side * 0.5, bg);
    ui.painter()
        .circle_stroke(badge_c, side * 0.5, Stroke::new(1.2_f32, fg));
    if let Some(g) = icons::glyph("speaking") {
        ui.painter()
            .text(badge_c, Align2::CENTER_CENTER, g, icons::font_id(sz), fg);
    }
}

fn status_fill(cfg: &OverlayConfig, kind: &str) -> Option<Color32> {
    match kind {
        "pit" => Some(cfg.color(SECTION, "status_pit", "#ffd23a")),
        "off" => Some(cfg.color(SECTION, "status_off", "#ff5050")),
        "garage" => Some(cfg.color(SECTION, "status_garage", "#8b93a1")),
        "black" => Some(cfg.color(SECTION, "status_black", "#1a1a1a")),
        "meatball" => Some(cfg.color(SECTION, "status_meatball", "#ff9416")),
        "dq" => Some(cfg.color(SECTION, "status_dq", "#ff5050")),
        "furled" => Some(cfg.color(SECTION, "status_furled", "#ffd23a")),
        _ => None,
    }
}

fn draw_status_badge(ui: &mut Ui, cfg: &OverlayConfig, c: Pos2, r: f32, kind: &str) {
    let Some(fill) = status_fill(cfg, kind) else {
        return;
    };
    let br = (r * 0.55).max(5.0);
    let bx = c.x + r * 0.65;
    let by = c.y - r * 0.65;
    let bp = Pos2::new(bx, by);
    ui.painter().circle_filled(bp, br, fill);
    ui.painter().circle_stroke(
        bp,
        br,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 180)),
    );
    let glyph = match kind {
        "pit" => "P",
        "off" => "!",
        "garage" => "G",
        "black" => "B",
        "meatball" => "M",
        "dq" => "X",
        "furled" => "W",
        _ => "",
    };
    if glyph.is_empty() {
        return;
    }
    let sz = (br * 1.1).max(5.0);
    let text_col = if kind == "furled" {
        Color32::from_rgb(20, 20, 20)
    } else {
        Color32::WHITE
    };
    ui.painter().text(
        bp,
        Align2::CENTER_CENTER,
        glyph,
        FontId::new(sz, FontFamily::Proportional),
        text_col,
    );
}

fn draw_zones(
    ui: &mut Ui,
    path: &[(f32, f32)],
    xform: &PlotXform,
    mirror: bool,
    rot: i32,
    zones: &[(f32, f32)],
    width: f32,
    color: Color32,
    start_finish: f32,
    reverse: bool,
) {
    if path.len() < 2 || zones.is_empty() {
        return;
    }
    for &(lo, hi) in zones {
        let span = (hi - lo).rem_euclid(1.0);
        if span <= 1e-5 {
            continue;
        }
        let steps = (span * path.len() as f32).round().max(8.0) as usize;
        let mut pts = Vec::with_capacity(steps + 1);
        for i in 0..=steps {
            let pct = (lo + span * (i as f32 / steps as f32)).rem_euclid(1.0);
            let (nx, ny) =
                track_path::point_at(path, loop_frac_for_pct(pct, start_finish, reverse));
            let (mx, my) = model_point(nx, ny, mirror, rot);
            pts.push(xform.map(mx, my));
        }
        if pts.len() < 2 {
            continue;
        }
        ui.painter().add(Shape::Path(PathShape::line(
            pts,
            PathStroke::new(width, color),
        )));
    }
}

fn wind_dir_radians(dir: f32) -> f32 {
    if dir.abs() > TAU {
        dir.to_radians()
    } else {
        dir
    }
}

fn wind_center(screen: &[Pos2], rect: Rect) -> (Pos2, f32) {
    let r = (rect.width().min(rect.height()) * 0.034).max(8.0);
    let label_h = r + 14.0;
    let total_h = r + label_h + 6.0;
    let total_w = 2.0 * r + 4.0;
    let gap = 4.0;
    if screen.is_empty() {
        return (
            Pos2::new(rect.right() - gap - r, rect.top() + gap + r + 6.0),
            r,
        );
    }
    let minx = screen.iter().map(|p| p.x).fold(f32::MAX, f32::min);
    let maxx = screen.iter().map(|p| p.x).fold(f32::MIN, f32::max);
    let miny = screen.iter().map(|p| p.y).fold(f32::MAX, f32::min);
    let maxy = screen.iter().map(|p| p.y).fold(f32::MIN, f32::max);
    let candidates = [
        ("tr", maxx + gap + r, miny + r + gap),
        ("tl", minx - gap - r, miny + r + gap),
        ("br", maxx + gap + r, maxy - r - gap),
        ("bl", minx - gap - r, maxy - r - gap),
    ];
    let hits = |cx: f32, cy: f32| {
        let box_r = Rect::from_min_size(
            Pos2::new(cx - total_w / 2.0, cy - r - 6.0),
            egui::vec2(total_w, total_h),
        );
        screen.iter().filter(|p| box_r.contains(**p)).count()
    };
    let best = candidates
        .iter()
        .enumerate()
        .min_by_key(|(i, &(_, cx, cy))| (hits(cx, cy), *i))
        .map(|(_, &(_, cx, cy))| (cx, cy))
        .unwrap_or((candidates[0].1, candidates[0].2));
    let cx = best.0.clamp(rect.left() + gap + r, rect.right() - gap - r);
    let cy = best
        .1
        .clamp(rect.top() + gap + r + 6.0, rect.bottom() - gap - label_h);
    (Pos2::new(cx, cy), r)
}

fn draw_wind(
    ui: &mut Ui,
    cfg: &OverlayConfig,
    rect: Rect,
    screen: &[Pos2],
    wind_dir: f32,
    wind_vel: f32,
    wet: Option<f32>,
    rain: Option<f32>,
    expanded: bool,
    text_scale: f32,
) {
    let (center, r) = wind_center(screen, rect);
    let col = cfg.color(SECTION, "wind", "#9fd0ff");
    let text_col = cfg.color(SECTION, "wind_text", "#eaf3ff");
    ui.painter()
        .circle_filled(center, r, Color32::from_rgba_unmultiplied(10, 13, 17, 190));
    ui.painter().circle_stroke(
        center,
        r,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 40)),
    );
    let nsz = (6.0 * text_scale).max(5.0);
    ui.painter().text(
        Pos2::new(center.x, center.y - r - nsz * 0.5 - 1.0),
        Align2::CENTER_CENTER,
        "N",
        FontId::new(nsz, FontFamily::Proportional),
        Color32::from_rgb(170, 178, 188),
    );
    let b = wind_dir_radians(wind_dir) + PI;
    let ux = b.sin();
    let uy = -b.cos();
    let px = -uy;
    let py = ux;
    let tip = Pos2::new(center.x + ux * r * 0.78, center.y + uy * r * 0.78);
    let tail = Pos2::new(center.x - ux * r * 0.70, center.y - uy * r * 0.70);
    ui.painter()
        .line_segment([tail, tip], Stroke::new((r * 0.14).max(1.5), col));
    let hl = r * 0.42;
    let hw = r * 0.26;
    let base = Pos2::new(tip.x - ux * hl, tip.y - uy * hl);
    let head = vec![
        tip,
        Pos2::new(base.x + px * hw, base.y + py * hw),
        Pos2::new(base.x - px * hw, base.y - py * hw),
    ];
    ui.painter()
        .add(Shape::convex_polygon(head, col, Stroke::NONE));

    let spd = cfg.conv_speed(wind_vel).round();
    let spd_text = format!("{spd:.0} {}", cfg.speed_unit());
    let ssz = (6.0 * text_scale).max(5.0);
    let font = FontId::new(ssz, FontFamily::Proportional);
    let galley = ui
        .painter()
        .layout_no_wrap(spd_text, font.clone(), text_col);
    let tw = galley.size().x + 6.0;
    let th = galley.size().y + 2.0;
    let lr = Rect::from_min_size(
        Pos2::new(center.x - tw / 2.0, center.y + r + 1.0),
        egui::vec2(tw, th),
    );
    ui.painter().rect_filled(
        lr,
        egui::CornerRadius::same(2),
        Color32::from_rgba_unmultiplied(10, 13, 17, 190),
    );
    ui.painter().galley(
        Pos2::new(
            lr.center().x - galley.size().x / 2.0,
            lr.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_col,
    );

    if expanded {
        let mut lines = Vec::new();
        if let Some(w) = wet {
            lines.push(format!("Wet {w:.0}%"));
        }
        if let Some(rn) = rain {
            if rn > 0.0 {
                lines.push(format!("Rain {rn:.0}%"));
            }
        }
        if !lines.is_empty() {
            let ssz2 = (6.0 * text_scale).max(5.0);
            let font2 = FontId::new(ssz2, FontFamily::Proportional);
            let galleys: Vec<_> = lines
                .iter()
                .map(|s| {
                    ui.painter()
                        .layout_no_wrap(s.clone(), font2.clone(), text_col)
                })
                .collect();
            let tw2 = galleys.iter().map(|g| g.size().x).fold(0.0_f32, f32::max) + 6.0;
            let line_h = galleys.first().map(|g| g.size().y).unwrap_or(ssz2);
            let th2 = line_h * galleys.len() as f32 + 4.0;
            let lr2 = Rect::from_min_size(
                Pos2::new(center.x - tw2 / 2.0, lr.bottom() + 2.0),
                egui::vec2(tw2, th2),
            );
            ui.painter().rect_filled(
                lr2,
                egui::CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(10, 13, 17, 190),
            );
            let mut y = lr2.top() + 2.0;
            for g in galleys {
                ui.painter()
                    .galley(Pos2::new(lr2.center().x - g.size().x / 2.0, y), g, text_col);
                y += line_h;
            }
        }
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    ensure_path_cached(ctx);

    let rect = full_rect(ui);
    if ctx.cfg.bool_key(SECTION, "show_panel", false) {
        panel_card(ui, ctx.cfg, SECTION, rect);
    }

    if !ctx.demo && ctx.map.path_status != TrackPathStatus::Ready {
        if matches!(
            ctx.map.path_status,
            TrackPathStatus::Loading | TrackPathStatus::None
        ) {
            *ctx.panel_animating = true;
        }
        paint_path_status(ui, ctx, rect);
        return;
    }
    if ctx.map.cached_path.len() < 3 {
        if !ctx.demo {
            *ctx.panel_animating = true;
            paint_path_status(ui, ctx, rect);
        }
        return;
    }

    seed_pit_latches(ctx);

    let asphalt_w = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let outline_w = ctx.cfg.f64_key(SECTION, "outline_width", 6.0) as f32;
    let show_infield = ctx.cfg.bool_key(SECTION, "show_infield", true);
    let show_sf = ctx.cfg.bool_key(SECTION, "show_start_finish", true);
    let mirror = ctx.cfg.bool_key(SECTION, "mirror", false);
    let rot = ctx.cfg.f64_key(SECTION, "rotation", 0.0) as i32;

    let asphalt = ctx.cfg.color(SECTION, "asphalt", "#333a42");
    let outline = ctx.cfg.color(SECTION, "outline", "#8b93a1");
    let infield = ctx.cfg.color(SECTION, "infield", "#0f1216c8");
    let accent = ctx.cfg.color(SECTION, "accent", "#70df7a");

    let text_scale = ctx.cfg.text_scale(SECTION);
    let pad_px = layout_pad(ctx.cfg, asphalt_w, text_scale);
    let plot = rect; // fit uses absolute pad inside

    let path = ctx.map.cached_path.clone();
    let mut modeled: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| model_point(x, y, mirror, rot))
        .collect();
    // Expand fit bbox so pit polylines are not clipped off the plot.
    if ctx.cfg.bool_key(SECTION, "show_pit", true) {
        for lane in [&ctx.map.cached_pit, &ctx.map.cached_pit2] {
            for &(x, y) in &lane.all_points() {
                modeled.push(model_point(x, y, mirror, rot));
            }
        }
    }
    if ctx.map.pit_edit {
        for pts in [
            &ctx.map.entry_pts,
            &ctx.map.road_pts,
            &ctx.map.merge_pts,
            &ctx.map.entry_pts_2,
            &ctx.map.road_pts_2,
            &ctx.map.merge_pts_2,
        ] {
            for &(x, y) in pts {
                modeled.push(model_point(x, y, mirror, rot));
            }
        }
    }
    let base_xform = PlotXform::fit(plot, &modeled, pad_px);
    let xform = if ctx.map.pit_edit {
        base_xform.with_view(ctx.map.pit_edit_zoom, ctx.map.pit_edit_pan)
    } else {
        base_xform
    };
    let screen: Vec<Pos2> = path
        .iter()
        .map(|&(x, y)| {
            let (mx, my) = model_point(x, y, mirror, rot);
            xform.map(mx, my)
        })
        .collect();
    // Modeled racing loop only (for S/F / ticks) — drop pit expanders.
    let modeled: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| model_point(x, y, mirror, rot))
        .collect();

    if show_infield {
        fill_infield(ui, &screen, infield);
    }
    // Dual stroke: thick asphalt under thinner outline (Python recipe).
    stroke_closed(ui, &screen, asphalt_w, asphalt);
    stroke_closed(ui, &screen, outline_w, outline);

    if ctx.cfg.bool_key(SECTION, "show_pit", true) {
        let pit = ctx.map.cached_pit.clone();
        let pit2 = ctx.map.cached_pit2.clone();
        let hide = ctx.map.pit_edit;
        // Python: hide saved segments while corresponding edit buffers are active.
        let show1_entry = !(hide && !ctx.map.entry_pts.is_empty());
        let show1_road = !(hide && !ctx.map.road_pts.is_empty());
        let show1_exit = !(hide && !ctx.map.merge_pts.is_empty());
        let show_lane1 = !(hide
            && (!ctx.map.road_pts.is_empty()
                || !ctx.map.merge_pts.is_empty()
                || !ctx.map.entry_pts.is_empty()));
        if show_lane1 {
            draw_pit_lane(
                ui,
                ctx,
                &pit,
                &xform,
                mirror,
                rot,
                asphalt,
                show1_entry,
                show1_road,
                show1_exit,
            );
        }
        if pit2.has_drawable() {
            let show2_entry = !(hide && !ctx.map.entry_pts_2.is_empty());
            let show2_road = !(hide && !ctx.map.road_pts_2.is_empty());
            let show2_exit = !(hide && !ctx.map.merge_pts_2.is_empty());
            let show_lane2 = !(hide
                && (!ctx.map.road_pts_2.is_empty()
                    || !ctx.map.merge_pts_2.is_empty()
                    || !ctx.map.entry_pts_2.is_empty()));
            if show_lane2 {
                draw_pit_lane(
                    ui,
                    ctx,
                    &pit2,
                    &xform,
                    mirror,
                    rot,
                    asphalt,
                    show2_entry,
                    show2_road,
                    show2_exit,
                );
            }
        }
    }

    let zone_w = (asphalt_w * 1.35).max(4.0);
    let sf = ctx.map.cached_start_finish;
    let reverse = map_reverse(ctx);
    if ctx.cfg.bool_key(SECTION, "show_drs_zones", false) && !ctx.map.cached_drs_zones.is_empty() {
        let col = ctx.cfg.color(SECTION, "drs_zone", "#46df7a88");
        let zones = ctx.map.cached_drs_zones.clone();
        draw_zones(
            ui, &path, &xform, mirror, rot, &zones, zone_w, col, sf, reverse,
        );
    }
    if ctx.cfg.bool_key(SECTION, "show_p2p_zones", false) && !ctx.map.cached_p2p_zones.is_empty() {
        let col = ctx.cfg.color(SECTION, "p2p_zone", "#3aa0ff88");
        let zones = ctx.map.cached_p2p_zones.clone();
        draw_zones(
            ui, &path, &xform, mirror, rot, &zones, zone_w, col, sf, reverse,
        );
    }
    // Active sector wash (Python `_draw_active_sector`).
    // Skip on static bg capture (changes often).
    if ctx.map_paint_mode == super::MapPaintMode::Full
        && ctx
            .cfg
            .bool_key("sector_timing", "highlight_active_sector_on_map", false)
        && !path.is_empty()
        && !ctx.frame.sectors_ui.starts.is_empty()
    {
        let starts = &ctx.frame.sectors_ui.starts;
        let n = starts.len();
        let idx = ctx.frame.sectors_ui.active_idx.min(n.saturating_sub(1));
        let lo = starts[idx] as f32;
        let hi = if idx + 1 >= n {
            1.0
        } else {
            starts[(idx + 1) % n] as f32
        };
        let col = ctx.cfg.color(SECTION, "active_sector", "#ffd23a66");
        draw_zones(
            ui,
            &path,
            &xform,
            mirror,
            rot,
            &[(lo, hi)],
            zone_w,
            col,
            sf,
            reverse,
        );
    }

    if show_sf && !modeled.is_empty() {
        // Transform tangent via model: evaluate on modeled path.
        draw_start_finish(ui, &modeled, &xform, sf_loop_frac(sf, reverse));
    }

    let screen_centroid = if screen.is_empty() {
        plot.center()
    } else {
        let n = screen.len() as f32;
        Pos2::new(
            screen.iter().map(|p| p.x).sum::<f32>() / n,
            screen.iter().map(|p| p.y).sum::<f32>() / n,
        )
    };
    if ctx.cfg.bool_key(SECTION, "show_sector_boundaries", true) && !modeled.is_empty() {
        draw_sector_boundaries(ui, ctx, &modeled, &xform, screen_centroid);
    }
    if ctx.cfg.bool_key(SECTION, "show_corners", true) && !ctx.map.cached_corners.is_empty() {
        draw_corners(ui, ctx, &path, &xform, mirror, rot, screen_centroid);
    }

    // Caution: pit-exit reference + moving pace-car safety line ("not a lap down").
    // Skip on static bg capture — pace line moves every frame.
    if ctx.map_paint_mode == super::MapPaintMode::Full
        && ctx.cfg.bool_key(SECTION, "show_pace_safety_line", true)
        && is_caution_flag(ctx.frame.flag.as_deref())
        && !modeled.is_empty()
    {
        let exit_col = ctx.cfg.color(SECTION, "pit_exit_mark", "#ffd23acc");
        let safety_col = ctx.cfg.color(SECTION, "pace_safety", "#ff9416ee");
        if let Some(exit_pct) = ctx.map.cached_pit_out_pct {
            draw_loop_tick(ui, &modeled, &xform, exit_pct, 9.0, 2.5, exit_col, true);
        }
        if let Some(pace) = ctx
            .frame
            .cars
            .iter()
            .find(|c| c.is_pace_car && c.lap_dist_pct >= 0.0)
        {
            // Moving line: pace car position. If you're still in pits when this
            // passes the pit-exit mark, rejoining puts you a lap down.
            draw_loop_tick(
                ui,
                &modeled,
                &xform,
                pace.lap_dist_pct,
                11.0,
                3.0,
                safety_col,
                true,
            );
        }
    }

    if ctx.map_paint_mode == super::MapPaintMode::StaticOnly {
        return;
    }

    let player_scale = dot_scale(ctx.cfg.f64_key(SECTION, "dot_radius_frac", 0.05));
    let other_scale = dot_scale(ctx.cfg.f64_key(SECTION, "other_dot_radius_frac", 0.05));
    let pit_opacity = ctx.cfg.f64_key(SECTION, "pit_dot_opacity", 0.45) as f32;
    let car_label_mode = ctx.cfg.str_key(SECTION, "car_label", "number");
    let map_text_scale = text_scale;
    let show_markers = ctx.cfg.bool_key(SECTION, "show_traffic_markers", true);
    let hold_sec = ctx.cfg.f64_key(SECTION, "marker_hold_seconds", 3.0);
    let show_status = ctx.cfg.bool_key(SECTION, "show_car_status", true);

    // Pct coast (idempotent if host already ticked). Screen ease uses its own clock
    // so a same-mono re-entry does not freeze dots at dt=0.
    let _ = tick_car_motion(ctx);
    let screen_dt = anim_dt(ctx.mono_secs, &mut ctx.map.last_screen_secs);
    let eased: HashMap<i32, f32> = ctx
        .map
        .car_anim
        .iter()
        .map(|(&idx, st)| (idx, st.pct))
        .collect();
    // Screen-ease uses an independent wall clock (host may already have ticked pct).
    let markers = if show_markers {
        map_markers::resolve_traffic_markers(
            &mut ctx.map.marker_hold,
            &ctx.frame.cars,
            ctx.frame.session_time,
            hold_sec,
        )
    } else {
        HashMap::new()
    };
    let marker_slots = map_markers::marker_slots_by_idx(&markers);

    // Screen centroid of modeled path (outward icon direction).
    let centroid = if screen.is_empty() {
        plot.center()
    } else {
        let n = screen.len() as f32;
        Pos2::new(
            screen.iter().map(|p| p.x).sum::<f32>() / n,
            screen.iter().map(|p| p.y).sum::<f32>() / n,
        )
    };

    // Draw order: field first, then player, speaking on top (Python sort key).
    let mut cars: Vec<&CarRow> = ctx.frame.cars.iter().collect();
    cars.sort_by_key(|c| (c.is_speaking, c.is_player));

    let show_blends = ctx.cfg.bool_key(SECTION, "show_pit_blends", true);
    let pit_lane = ctx.map.cached_pit.clone();
    let pit_lane2 = ctx.map.cached_pit2.clone();

    update_pit_route_latches(ctx, &cars);

    // Raw telem targets, then screen-space ease (Python `_build_smooth_car_screen_points`).
    let mut targets: HashMap<i32, (Pos2, u8)> = HashMap::new();
    for car in &cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        // OnPitRoad / demo: raw telem (coast is for sparse live SDK samples).
        let pct = if ctx.demo || car.on_pit {
            car.lap_dist_pct.rem_euclid(1.0)
        } else {
            eased.get(&car.car_idx).copied().unwrap_or(car.lap_dist_pct)
        };
        if pct < 0.0 {
            continue;
        }
        let on_route = car_on_route(ctx, car);
        let (nx, ny) = car_model_xy(ctx, car, pct, &path, &pit_lane, &pit_lane2, show_blends);
        let (mx, my) = model_point(nx, ny, mirror, rot);
        let p = xform.map(mx, my);
        let key = car_motion_key(on_route, car.on_pit);
        targets.insert(car.car_idx, (p, key));
    }
    let (car_pts, screen_animating) = smooth_car_screen_pts(ctx, &targets, screen_dt);
    if screen_animating || cars_pct_animating(ctx) {
        *ctx.panel_animating = true;
    }

    for car in &cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        let Some(&p) = car_pts.get(&car.car_idx) else {
            continue;
        };
        let on_route = car_on_route(ctx, car);
        let mut r = if car.is_player {
            12.5 * player_scale
        } else {
            9.0 * other_scale
        };
        if car.is_player && (car.on_pit || on_route) {
            r *= 1.15;
        }
        if let Some(slot) = marker_slots.get(&car.car_idx) {
            if !car.is_player {
                let (col_key, fallback) = marker_color_key(slot);
                let ring = ctx.cfg.color(SECTION, col_key, fallback);
                ui.painter()
                    .circle_stroke(p, r + 5.0, Stroke::new(2.6_f32, ring));
            }
        }
        let fill = car_fill(ctx.cfg, car, pit_opacity, on_route);
        if car.is_player {
            draw_player_dot(ui, p, r, fill);
        } else {
            draw_other_dot(ui, p, r, fill);
        }
        if car.is_speaking && !car.is_pace_car {
            draw_speaking(ui, ctx.cfg, p, r);
        }
        if show_status && !car.is_pace_car {
            if let Some(kind) = car.status_kind.as_deref() {
                draw_status_badge(ui, ctx.cfg, p, r, kind);
            }
        }
        // Hide on-dot number when a traffic marker already labels the car.
        let show_label =
            car.is_player || car.is_pace_car || !marker_slots.contains_key(&car.car_idx);
        if show_label {
            let label_text = car_label_text(car, &car_label_mode);
            draw_car_number_label(
                ui,
                p,
                &label_text,
                r,
                car.is_player,
                car.is_pace_car,
                map_text_scale,
                fill,
            );
        }
    }

    if show_markers {
        draw_traffic_markers(
            ui,
            ctx.cfg,
            &markers,
            &car_pts,
            &path,
            &xform,
            mirror,
            rot,
            centroid,
            asphalt_w,
            map_text_scale,
            ctx.map.cached_start_finish,
            map_reverse(ctx),
        );
    }

    if ctx.cfg.bool_key(SECTION, "show_wind", true) {
        if let (Some(dir), Some(vel)) = (ctx.frame.wind_dir, ctx.frame.wind_vel) {
            draw_wind(
                ui,
                ctx.cfg,
                rect,
                &screen,
                dir,
                vel,
                ctx.frame.track_wetness,
                ctx.frame.rain_intensity,
                ctx.cfg.bool_key(SECTION, "show_expanded_weather", false),
                map_text_scale,
            );
        }
    }

    // Authoring drafts: only while Track Scan pit edit Enabled.
    if ctx.map.pit_edit {
        let phase = ctx.map.phase_key().to_string();
        let lane2 = ctx.map.lane_is_2();
        let entry_col = Color32::from_rgb(255, 210, 58);
        let road_col = Color32::from_rgb(255, 90, 90);
        let merge_col = Color32::from_rgb(90, 160, 255);
        let entry2_col = Color32::from_rgb(200, 240, 120);
        let road2_col = Color32::from_rgb(70, 210, 170);
        let merge2_col = Color32::from_rgb(100, 200, 255);
        let asphalt_w_f = asphalt_w;
        let base_r = (asphalt_w_f * 0.35).max(4.0);
        let handle_r = (base_r * ctx.map.pit_edit_zoom.max(1.0).sqrt()).max(8.0);
        let pit_hits = draw_pit_edit_drafts(
            ui,
            ctx.map,
            &xform,
            mirror,
            rot,
            handle_r,
            [
                (entry_col, road_col, merge_col),
                (entry2_col, road2_col, merge2_col),
            ],
        );

        if ctx.map.interactive {
            handle_pit_edit(
                ui,
                ctx,
                plot,
                &base_xform,
                &xform,
                mirror,
                rot,
                handle_r,
                &pit_hits,
            );
            let label_col = match phase.as_str() {
                "entry" => {
                    if lane2 {
                        entry2_col
                    } else {
                        entry_col
                    }
                }
                "merge" => {
                    if lane2 {
                        merge2_col
                    } else {
                        merge_col
                    }
                }
                _ => {
                    if lane2 {
                        road2_col
                    } else {
                        road_col
                    }
                }
            };
            label(
                ui,
                Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
                Align2::LEFT_TOP,
                &format!("PIT EDIT ({} L{})", phase, if lane2 { 2 } else { 1 }),
                12.0,
                label_col,
                true,
            );
        }
    } else if ctx.map.corner_edit && ctx.map.interactive {
        handle_corner_edit(ui, ctx, plot, &path, &xform, mirror, rot, screen_centroid);
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            "CORNER EDIT",
            12.0,
            Color32::from_rgb(255, 200, 60),
            true,
        );
    } else if ctx.map.sf_edit && ctx.map.interactive {
        let resp = ui.interact(plot, ui.id().with("map_sf"), Sense::click());
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let (mx, my) = xform.unmap(pos);
                let (rx, ry) = inverse_model(mx, my, mirror, rot);
                if path.len() >= 2 {
                    let loop_frac = track_path::nearest_pct_on_loop(&path, rx, ry);
                    let reverse = map_reverse(ctx);
                    // If driving: click means "I am here" (calibrate offset).
                    // Otherwise place S/F so lap pct 0 sits on the click.
                    let player_telem = ctx
                        .frame
                        .cars
                        .iter()
                        .find(|c| c.is_player && c.lap_dist_pct >= 0.0)
                        .map(|c| {
                            if ctx.frame.player_lap_dist_pct >= 0.0 {
                                ctx.frame.player_lap_dist_pct.rem_euclid(1.0)
                            } else {
                                c.lap_dist_pct.rem_euclid(1.0)
                            }
                        });
                    ctx.map.cached_start_finish = if let Some(telem) = player_telem {
                        sf_for_player_at(telem, loop_frac, reverse)
                    } else {
                        // lap pct 0 → loop_frac
                        sf_for_player_at(0.0, loop_frac, reverse)
                    };
                }
            }
        }
        let hint = if ctx.frame.cars.iter().any(|c| c.is_player && c.lap_dist_pct >= 0.0)
        {
            "S/F EDIT — click where YOU are"
        } else {
            "S/F EDIT — click S/F on map"
        };
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            hint,
            12.0,
            accent,
            true,
        );
    }
}

/// Fingerprint of static map chrome — when this changes, recapture the bg cache.
pub fn bg_fingerprint(
    cfg: &OverlayConfig,
    map: &MapAuthoring,
    frame: &crate::telemetry::TelemetryFrame,
    w: i32,
    h: i32,
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    w.hash(&mut hasher);
    h.hash(&mut hasher);
    // Bust static BG when path sampling / alignment semantics change.
    11u32.hash(&mut hasher);
    map.cached_track_id.hash(&mut hasher);
    map.path_status.hash(&mut hasher);
    map.cached_path.len().hash(&mut hasher);
    map.cached_start_finish.to_bits().hash(&mut hasher);
    map.pit_edit.hash(&mut hasher);
    map.corner_edit.hash(&mut hasher);
    map.sf_edit.hash(&mut hasher);
    map.pit_edit_zoom.to_bits().hash(&mut hasher);
    map.pit_edit_pan.0.to_bits().hash(&mut hasher);
    map.pit_edit_pan.1.to_bits().hash(&mut hasher);
    cfg.bool_key(SECTION, "show_panel", false).hash(&mut hasher);
    cfg.bool_key(SECTION, "show_infield", true)
        .hash(&mut hasher);
    cfg.bool_key(SECTION, "show_start_finish", true)
        .hash(&mut hasher);
    cfg.bool_key(SECTION, "mirror", false).hash(&mut hasher);
    cfg.f64_key(SECTION, "rotation", 0.0)
        .to_bits()
        .hash(&mut hasher);
    cfg.f64_key(SECTION, "asphalt_width", 12.0)
        .to_bits()
        .hash(&mut hasher);
    cfg.f64_key(SECTION, "outline_width", 6.0)
        .to_bits()
        .hash(&mut hasher);
    cfg.bool_key(SECTION, "show_pit", true).hash(&mut hasher);
    cfg.bool_key(SECTION, "show_drs_zones", false)
        .hash(&mut hasher);
    cfg.bool_key(SECTION, "show_p2p_zones", false)
        .hash(&mut hasher);
    cfg.bool_key(SECTION, "show_sector_boundaries", true)
        .hash(&mut hasher);
    cfg.bool_key(SECTION, "show_corners", true)
        .hash(&mut hasher);
    cfg.str_key(SECTION, "asphalt", "#333a42").hash(&mut hasher);
    cfg.str_key(SECTION, "outline", "#8b93a1").hash(&mut hasher);
    cfg.str_key(SECTION, "infield", "#0f1216c8")
        .hash(&mut hasher);
    // Caution changes pit-exit mark visibility on the static layer.
    is_caution_flag(frame.flag.as_deref()).hash(&mut hasher);
    hasher.finish()
}

/// Advance car easing and return CPU-composite sprites.
/// `panel_w_px` / `panel_h_px` are client **pixels** (BGRA buffer size).
/// Plot fit uses egui **points** (`px / pixels_per_point`), then positions
/// are scaled back to pixels so they match the cached GL bg.
pub fn build_car_sprites(
    ctx: &mut WidgetCtx<'_>,
    panel_w_px: f32,
    panel_h_px: f32,
    pixels_per_point: f32,
) -> Vec<crate::layered::MapCarSprite> {
    ensure_path_cached(ctx);
    if ctx.map.path_status != TrackPathStatus::Ready || ctx.map.cached_path.len() < 3 {
        return Vec::new();
    }
    seed_pit_latches(ctx);

    let ppp = pixels_per_point.max(0.5);
    let rect = Rect::from_min_size(
        Pos2::ZERO,
        Vec2::new((panel_w_px / ppp).max(1.0), (panel_h_px / ppp).max(1.0)),
    );
    let asphalt_w = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let mirror = ctx.cfg.bool_key(SECTION, "mirror", false);
    let rot = ctx.cfg.f64_key(SECTION, "rotation", 0.0) as i32;
    let text_scale = ctx.cfg.text_scale(SECTION);
    let pad_px = layout_pad(ctx.cfg, asphalt_w, text_scale);
    let show_pit = ctx.cfg.bool_key(SECTION, "show_pit", true);

    let fit_key = {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        panel_w_px.to_bits().hash(&mut h);
        panel_h_px.to_bits().hash(&mut h);
        ppp.to_bits().hash(&mut h);
        mirror.hash(&mut h);
        rot.hash(&mut h);
        pad_px.to_bits().hash(&mut h);
        show_pit.hash(&mut h);
        ctx.map.cached_track_id.hash(&mut h);
        ctx.map.cached_path.len().hash(&mut h);
        ctx.map.pit_edit.hash(&mut h);
        ctx.map.pit_edit_zoom.to_bits().hash(&mut h);
        ctx.map.pit_edit_pan.0.to_bits().hash(&mut h);
        ctx.map.pit_edit_pan.1.to_bits().hash(&mut h);
        h.finish()
    };
    let xform = match (ctx.map.sprite_fit_key == fit_key, ctx.map.sprite_fit) {
        (true, Some((ox, oy, scale, min_x, min_y))) => PlotXform {
            origin: Pos2::new(ox, oy),
            scale,
            min: (min_x, min_y),
        },
        _ => {
            let mut modeled: Vec<(f32, f32)> = ctx
                .map
                .cached_path
                .iter()
                .map(|&(x, y)| model_point(x, y, mirror, rot))
                .collect();
            if show_pit {
                for lane in [&ctx.map.cached_pit, &ctx.map.cached_pit2] {
                    for &(x, y) in &lane.all_points() {
                        modeled.push(model_point(x, y, mirror, rot));
                    }
                }
            }
            let base = PlotXform::fit(rect, &modeled, pad_px);
            let x = if ctx.map.pit_edit {
                base.with_view(ctx.map.pit_edit_zoom, ctx.map.pit_edit_pan)
            } else {
                base
            };
            ctx.map.sprite_fit_key = fit_key;
            ctx.map.sprite_fit = Some((x.origin.x, x.origin.y, x.scale, x.min.0, x.min.1));
            x
        }
    };

    let player_scale = dot_scale(ctx.cfg.f64_key(SECTION, "dot_radius_frac", 0.05));
    let other_scale = dot_scale(ctx.cfg.f64_key(SECTION, "other_dot_radius_frac", 0.05));
    let pit_opacity = ctx.cfg.f64_key(SECTION, "pit_dot_opacity", 0.45) as f32;

    // Host may already have ticked pct this mono frame (idempotent).
    tick_car_motion(ctx);
    let screen_dt = anim_dt(ctx.mono_secs, &mut ctx.map.last_screen_secs);
    let eased: HashMap<i32, f32> = ctx
        .map
        .car_anim
        .iter()
        .map(|(&idx, st)| (idx, st.pct))
        .collect();

    let mut cars: Vec<&CarRow> = ctx.frame.cars.iter().collect();
    cars.sort_by_key(|c| (c.is_speaking, c.is_player));
    let show_blends = ctx.cfg.bool_key(SECTION, "show_pit_blends", true);
    update_pit_route_latches(ctx, &cars);

    let mut targets: HashMap<i32, (Pos2, u8)> = HashMap::new();
    for car in &cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        let pct = if ctx.demo || car.on_pit {
            car.lap_dist_pct.rem_euclid(1.0)
        } else {
            eased.get(&car.car_idx).copied().unwrap_or(car.lap_dist_pct)
        };
        if pct < 0.0 {
            continue;
        }
        let on_route = car_on_route(ctx, car);
        let (nx, ny) = car_model_xy(
            ctx,
            car,
            pct,
            &ctx.map.cached_path,
            &ctx.map.cached_pit,
            &ctx.map.cached_pit2,
            show_blends,
        );
        let (mx, my) = model_point(nx, ny, mirror, rot);
        let p = xform.map(mx, my);
        let key = car_motion_key(on_route, car.on_pit);
        targets.insert(car.car_idx, (p, key));
    }
    // Python parity: racing-line dots use eased % only (no second XY lag).
    let (car_pts, screen_animating) = smooth_car_screen_pts(ctx, &targets, screen_dt);
    if screen_animating || cars_pct_animating(ctx) {
        *ctx.panel_animating = true;
    }

    // Build candidates, then keep only the best-placed car when dots stack.
    // Player is always kept (clears anyone stacked on them).
    #[derive(Clone)]
    struct Cand {
        car_idx: i32,
        p: Pos2,
        r: f32,
        fill: Color32,
        player: bool,
        label: String,
        place: i32,
    }
    let mut cands: Vec<Cand> = Vec::with_capacity(cars.len());
    for car in &cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        let Some(&p) = car_pts.get(&car.car_idx) else {
            continue;
        };
        let on_route = car_on_route(ctx, car);
        let mut r = if car.is_player {
            12.5 * player_scale
        } else {
            9.0 * other_scale
        };
        if car.is_player && (car.on_pit || on_route) {
            r *= 1.15;
        }
        let fill = car_fill(ctx.cfg, car, pit_opacity, on_route);
        // Hot composite: no number labels (font atlas blit dominated ULW cost).
        let label = String::new();
        let place = if car.position > 0 {
            car.position
        } else if car.class_position > 0 {
            car.class_position
        } else {
            9999
        };
        cands.push(Cand {
            car_idx: car.car_idx,
            p,
            r,
            fill,
            player: car.is_player,
            label,
            place,
        });
    }
    let stacked = |a: &Cand, b: &Cand| -> bool {
        let dx = a.p.x - b.p.x;
        let dy = a.p.y - b.p.y;
        let lim = (a.r + b.r) * 0.72;
        dx * dx + dy * dy < lim * lim
    };
    cands.sort_by(|a, b| {
        a.place
            .cmp(&b.place)
            .then_with(|| a.car_idx.cmp(&b.car_idx))
    });
    let player_cand = cands.iter().find(|c| c.player).cloned();
    let mut kept: Vec<Cand> = Vec::with_capacity(cands.len());
    for c in cands {
        if kept.iter().any(|k| stacked(&c, k)) {
            continue;
        }
        kept.push(c);
    }
    if let Some(player) = player_cand {
        kept.retain(|k| k.car_idx == player.car_idx || !stacked(k, &player));
        if !kept.iter().any(|k| k.car_idx == player.car_idx) {
            kept.push(player);
        }
    }

    let mut out = Vec::with_capacity(kept.len());
    for c in kept {
        out.push(crate::layered::MapCarSprite {
            x: c.p.x * ppp,
            y: c.p.y * ppp,
            r: c.r * ppp,
            b: c.fill.b(),
            g: c.fill.g(),
            r_ch: c.fill.r(),
            a: c.fill.a(),
            player: c.player,
            label: c.label,
        });
    }
    out
}

/// Python `_draw_sector_boundaries`: purple ticks + S2/S3… pills at sector starts.
fn draw_sector_boundaries(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    modeled: &[(f32, f32)],
    xform: &PlotXform,
    centroid: Pos2,
) {
    let mut starts: Vec<f32> = ctx
        .frame
        .sectors_ui
        .starts
        .iter()
        .map(|s| *s as f32)
        .filter(|p| p.is_finite())
        .collect();
    if starts.is_empty() {
        let n = ctx
            .cfg
            .f64_key("sector_timing", "sectors", 3.0)
            .round()
            .max(1.0) as usize;
        starts = (0..n).map(|i| i as f32 / n as f32).collect();
    }
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    starts.dedup_by(|a, b| (*a - *b).abs() < 1e-5);

    let sf = ctx.map.cached_start_finish;
    let reverse = map_reverse(ctx);
    let sf_frac = sf_loop_frac(sf, reverse);
    let line = ctx.cfg.color(SECTION, "sector_line", "#a78bfa");
    let text_col = ctx.cfg.color(SECTION, "sector_text", "#c4b5fd");
    let asph = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let text_scale = ctx.cfg.text_scale(SECTION);
    let sz = (7.0 * text_scale).max(5.0);
    let label_off = asph * 0.5 + sz + 6.0;
    let mut label_num = 2_i32;

    for start in starts {
        let frac = loop_frac_for_pct(start, sf, reverse);
        // Skip start/finish (Python: near 0 or sf_frac).
        if frac.abs() < 1e-4 || (frac - sf_frac).abs() < 0.01 {
            continue;
        }
        draw_loop_tick(ui, modeled, xform, frac, 6.0, 2.0, line, false);

        let (nx, ny) = track_path::point_at(modeled, frac);
        let pt = xform.map(nx, ny);
        let outward = outward_from(pt, centroid, label_off);
        let tag = format!("S{label_num}");
        label_num += 1;

        let font = FontId::new(sz, FontFamily::Name(crate::icons::BOLD_FAMILY.into()));
        let galley = ui.fonts(|f| f.layout_no_wrap(tag.clone(), font, text_col));
        let bw = galley.size().x.max(sz + 2.0) + 8.0;
        let bh = galley.size().y + 2.0;
        let rect = Rect::from_center_size(outward, Vec2::new(bw, bh));
        ui.painter().rect_filled(rect, CornerRadius::same(3), line);
        label(
            ui,
            rect.center(),
            Align2::CENTER_CENTER,
            &tag,
            sz,
            text_col,
            true,
        );
    }
}

fn draw_corners(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    path: &[(f32, f32)],
    xform: &PlotXform,
    mirror: bool,
    rot: i32,
    centroid: Pos2,
) {
    let asphalt_w = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let text_scale = ctx.cfg.text_scale(SECTION);
    let sz = (8.0 * text_scale).max(5.0);
    let off = asphalt_w * 0.5 + sz + 8.0;
    let sf = ctx.map.cached_start_finish;
    let corners = ctx.map.cached_corners.clone();
    for (idx, c) in corners.iter().enumerate() {
        let (nx, ny) = track_path::point_at(path, loop_frac_for_pct(c.pct, sf, map_reverse(ctx)));
        let (mx, my) = model_point(nx, ny, mirror, rot);
        let s = xform.map(mx, my);
        let dx = s.x - centroid.x;
        let dy = s.y - centroid.y;
        let ln = (dx * dx + dy * dy).sqrt().max(1.0);
        let mut ax = s.x + dx / ln * off;
        let mut ay = s.y + dy / ln * off;
        // ox/oy are model-space deltas applied after model xform scale.
        if c.ox != 0.0 || c.oy != 0.0 {
            let (omx, omy) = model_point(c.ox, c.oy, mirror, rot);
            ax += omx * xform.scale * 0.15;
            ay += omy * xform.scale * 0.15;
        }
        let label_txt = &c.label;
        let font = FontId::new(sz, FontFamily::Proportional);
        let galley = ui.fonts(|f| {
            f.layout_no_wrap(
                label_txt.clone(),
                font,
                ctx.cfg.color(SECTION, "corner_text", "#d6dce2"),
            )
        });
        let bw = galley.size().x.max(sz + 4.0) + 12.0;
        let bh = galley.size().y + 4.0;
        let rect = Rect::from_center_size(Pos2::new(ax, ay), egui::vec2(bw, bh));
        let fill = if ctx.map.corner_edit {
            let a = if ctx.map.drag_corner == Some(idx) {
                220
            } else {
                160
            };
            Color32::from_rgba_unmultiplied(255, 200, 60, a)
        } else {
            Color32::from_rgba_unmultiplied(15, 18, 22, 200)
        };
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(4), fill);
        ui.painter().galley(
            Pos2::new(
                rect.center().x - galley.size().x * 0.5,
                rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            Color32::WHITE,
        );
    }
}

/// Hit target: lane(1|2), phase(0=entry,1=road,2=merge,3=joint,4=entry_joint), idx.
type PitHit = (u8, u8, usize);

fn draw_pit_edit_drafts(
    ui: &mut Ui,
    ctx: &MapAuthoring,
    xform: &PlotXform,
    mirror: bool,
    rot: i32,
    handle_r: f32,
    colors: [(Color32, Color32, Color32); 2],
) -> Vec<(Pos2, PitHit)> {
    let mut hits = Vec::new();
    let active_lane2 = ctx.lane_is_2();
    for (lane_i, (entry_col, road_col, merge_col)) in colors.into_iter().enumerate() {
        let lane2 = lane_i == 1;
        let lane_u = if lane2 { 2_u8 } else { 1_u8 };
        let (entry, road, merge) = if lane2 {
            (&ctx.entry_pts_2, &ctx.road_pts_2, &ctx.merge_pts_2)
        } else {
            (&ctx.entry_pts, &ctx.road_pts, &ctx.merge_pts)
        };
        if entry.is_empty() && road.is_empty() && merge.is_empty() {
            continue;
        }
        let active = lane2 == active_lane2;
        let fade = |c: Color32| -> Color32 {
            if active {
                c
            } else {
                Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 170)
            }
        };
        let entry_col = fade(entry_col);
        let road_col = fade(road_col);
        let merge_col = fade(merge_col);
        let road_w = if active { 3.5_f32 } else { 2.5_f32 };
        let blend_w = if active { 3.0_f32 } else { 2.0_f32 };

        let stroke_poly = |pts: &[(f32, f32)], col: Color32, width: f32| {
            for i in 1..pts.len() {
                let (mx0, my0) = model_point(pts[i - 1].0, pts[i - 1].1, mirror, rot);
                let (mx1, my1) = model_point(pts[i].0, pts[i].1, mirror, rot);
                ui.painter().line_segment(
                    [xform.map(mx0, my0), xform.map(mx1, my1)],
                    Stroke::new(width, col),
                );
            }
        };
        stroke_poly(entry, entry_col, blend_w);
        stroke_poly(road, road_col, road_w);
        stroke_poly(merge, merge_col, blend_w);

        let has_joint = ctx.has_joint(lane2);
        let has_entry_joint = ctx.has_entry_joint(lane2);
        type PhaseStroke<'a> = (&'a str, u8, &'a [(f32, f32)], Color32);
        let phases: [PhaseStroke; 3] = [
            ("entry", 0, entry, entry_col),
            ("road", 1, road, road_col),
            ("merge", 2, merge, merge_col),
        ];
        for (_name, phase_code, pts, col) in phases {
            for (idx, &(nx, ny)) in pts.iter().enumerate() {
                if has_entry_joint
                    && ((phase_code == 0 && idx + 1 == pts.len()) || (phase_code == 1 && idx == 0))
                {
                    continue;
                }
                if has_joint
                    && ((phase_code == 1 && idx + 1 == pts.len()) || (phase_code == 2 && idx == 0))
                {
                    continue;
                }
                let (mx, my) = model_point(nx, ny, mirror, rot);
                let p = xform.map(mx, my);
                let dragging = ctx.pit_drag == Some((lane_u, phase_code, idx));
                let fill = if dragging || active {
                    col
                } else {
                    Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 170)
                };
                ui.painter().circle_filled(p, handle_r, fill);
                ui.painter().circle_stroke(
                    p,
                    handle_r,
                    Stroke::new(
                        1.5_f32,
                        Color32::from_rgb(
                            col.r().saturating_mul(3) / 4,
                            col.g().saturating_mul(3) / 4,
                            col.b().saturating_mul(3) / 4,
                        ),
                    ),
                );
                hits.push((p, (lane_u, phase_code, idx)));
            }
        }
        if has_entry_joint {
            if let Some(&(nx, ny)) = entry.last() {
                let (mx, my) = model_point(nx, ny, mirror, rot);
                let p = xform.map(mx, my);
                let jcol = Color32::from_rgb(
                    entry_col.r().saturating_add(20),
                    entry_col.g().saturating_add(20),
                    entry_col.b().saturating_add(10),
                );
                let dragging = ctx.pit_drag == Some((lane_u, 4, 0));
                let fill = if dragging || active {
                    jcol
                } else {
                    Color32::from_rgba_unmultiplied(jcol.r(), jcol.g(), jcol.b(), 200)
                };
                ui.painter().circle_filled(p, handle_r, fill);
                ui.painter()
                    .circle_stroke(p, handle_r, Stroke::new(1.5_f32, jcol));
                hits.push((p, (lane_u, 4, 0)));
            }
        }
        if has_joint {
            if let Some(&(nx, ny)) = road.last() {
                let (mx, my) = model_point(nx, ny, mirror, rot);
                let p = xform.map(mx, my);
                let jcol = if lane2 {
                    Color32::from_rgb(120, 220, 180)
                } else {
                    Color32::from_rgb(255, 170, 50)
                };
                let dragging = ctx.pit_drag == Some((lane_u, 3, 0));
                let fill = if dragging || active {
                    jcol
                } else {
                    Color32::from_rgba_unmultiplied(jcol.r(), jcol.g(), jcol.b(), 200)
                };
                ui.painter().circle_filled(p, handle_r, fill);
                ui.painter()
                    .circle_stroke(p, handle_r, Stroke::new(1.5_f32, jcol));
                hits.push((p, (lane_u, 3, 0)));
            }
        }
    }
    hits
}

fn pit_handle_at(hits: &[(Pos2, PitHit)], pos: Pos2, r: f32) -> Option<PitHit> {
    let r2 = r * r;
    for &(p, hit) in hits.iter().rev() {
        let dx = p.x - pos.x;
        let dy = p.y - pos.y;
        if dx * dx + dy * dy <= r2 {
            return Some(hit);
        }
    }
    None
}

fn handle_pit_edit(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    plot: Rect,
    base_xform: &PlotXform,
    xform: &PlotXform,
    mirror: bool,
    rot: i32,
    handle_r: f32,
    hits: &[(Pos2, PitHit)],
) {
    // Scroll zoom toward pointer (Python wheelEvent).
    let scroll = ui.input(|i| i.smooth_scroll_delta);
    if scroll.y.abs() > 0.0 {
        if let Some(pos) = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| plot.contains(*p))
        {
            let (wx, wy) = xform.unmap(pos);
            let factor = (1.0015_f32).powf(scroll.y);
            let new_zoom = MapAuthoring::clamp_pit_zoom(ctx.map.pit_edit_zoom * factor);
            let new_scale = base_xform.scale * new_zoom;
            ctx.map.pit_edit_pan = (
                pos.x - base_xform.origin.x - (wx - base_xform.min.0) * new_scale,
                pos.y - base_xform.origin.y - (wy - base_xform.min.1) * new_scale,
            );
            ctx.map.pit_edit_zoom = new_zoom;
            ui.ctx().request_repaint();
        }
    }

    let resp = ui.interact(plot, ui.id().with("map_pit"), Sense::click_and_drag());
    let pointer = resp.interact_pointer_pos();
    let shift = ui.input(|i| i.modifiers.shift);
    let middle_down = ui.input(|i| i.pointer.button_down(PointerButton::Middle));
    let primary_down = ui.input(|i| i.pointer.button_down(PointerButton::Primary));

    // Start / continue handle drag (press on handle, not Shift-pan).
    let primary_pressed = ui.input(|i| i.pointer.primary_pressed());
    if let Some(pos) = pointer {
        if primary_pressed && !shift {
            if let Some(hit) = pit_handle_at(hits, pos, handle_r) {
                ctx.map.pit_drag = Some(hit);
            }
        }
        if primary_down && !shift {
            if let Some((lane_u, phase, idx)) = ctx.map.pit_drag {
                let (mx, my) = xform.unmap(pos);
                let (rx, ry) = inverse_model(mx, my, mirror, rot);
                ctx.map
                    .set_point_with_joints(lane_u == 2, phase, idx, rx, ry);
                ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            }
        }
    }
    if ctx.map.pit_drag.is_some() && !primary_down {
        ctx.map.pit_drag = None;
    }

    // Pan: middle-drag or Shift+primary (not on handle).
    let panning = middle_down || (primary_down && shift && ctx.map.pit_drag.is_none());
    if panning {
        let delta = ui.input(|i| i.pointer.delta());
        if delta != Vec2::ZERO {
            ctx.map.pit_edit_pan.0 += delta.x;
            ctx.map.pit_edit_pan.1 += delta.y;
            ui.ctx().set_cursor_icon(CursorIcon::Grabbing);
            ui.ctx().request_repaint();
        } else {
            ui.ctx().set_cursor_icon(CursorIcon::Grab);
        }
    } else if ctx.map.pit_drag.is_none() {
        if let Some(pos) = ui
            .input(|i| i.pointer.hover_pos())
            .filter(|p| plot.contains(*p))
        {
            if pit_handle_at(hits, pos, handle_r).is_some() || shift {
                ui.ctx().set_cursor_icon(CursorIcon::Grab);
            } else {
                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
            }
        }
    }

    // Primary click empty → append (avoid after handle drag).
    if resp.clicked() && ctx.map.pit_drag.is_none() && !shift {
        if let Some(pos) = pointer {
            if pit_handle_at(hits, pos, handle_r).is_none() {
                let (mx, my) = xform.unmap(pos);
                let (rx, ry) = inverse_model(mx, my, mirror, rot);
                ctx.map.append_pit_edit_at(rx, ry);
            }
        }
    }

    // Secondary click → pop last of active phase.
    if resp.secondary_clicked() {
        ctx.map.active_pts_mut().pop();
    }
}

fn handle_corner_edit(
    ui: &mut Ui,
    ctx: &mut WidgetCtx<'_>,
    plot: Rect,
    path: &[(f32, f32)],
    xform: &PlotXform,
    mirror: bool,
    rot: i32,
    centroid: Pos2,
) {
    let asphalt_w = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let text_scale = ctx.cfg.text_scale(SECTION);
    let sz = (8.0 * text_scale).max(5.0);
    let off = asphalt_w * 0.5 + sz + 8.0;
    let sf = ctx.map.cached_start_finish;
    let corners = ctx.map.cached_corners.clone();
    for (idx, c) in corners.iter().enumerate() {
        let (nx, ny) = track_path::point_at(path, loop_frac_for_pct(c.pct, sf, map_reverse(ctx)));
        let (mx, my) = model_point(nx, ny, mirror, rot);
        let s = xform.map(mx, my);
        let dx = s.x - centroid.x;
        let dy = s.y - centroid.y;
        let ln = (dx * dx + dy * dy).sqrt().max(1.0);
        let ax = s.x + dx / ln * off;
        let ay = s.y + dy / ln * off;
        let hit = Rect::from_center_size(Pos2::new(ax, ay), egui::vec2(28.0, 22.0));
        let resp = ui.interact(hit, ui.id().with(("corner", idx)), Sense::drag());
        if resp.drag_started() {
            ctx.map.drag_corner = Some(idx);
        }
        if resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                // Store screen delta back as ox/oy in a lightweight model space.
                let scale = xform.scale.max(1e-3);
                if let Some(c) = ctx.map.cached_corners.get_mut(idx) {
                    c.ox += resp.drag_delta().x / (scale * 0.15);
                    c.oy += resp.drag_delta().y / (scale * 0.15);
                    let _ = pos;
                }
            }
        }
        if resp.drag_stopped() {
            ctx.map.drag_corner = None;
        }
    }
    let resp = ui.interact(plot, ui.id().with("corner_bg"), Sense::click());
    if resp.clicked() && ctx.map.drag_corner.is_none() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (mx, my) = xform.unmap(pos);
            let (rx, ry) = inverse_model(mx, my, mirror, rot);
            if path.len() >= 2 {
                let loop_frac = track_path::nearest_pct_on_loop(path, rx, ry);
                // Store lap % so display via loop_frac_for_pct matches the click.
                let reverse = map_reverse(ctx);
                let pct = if reverse {
                    (sf - loop_frac).rem_euclid(1.0)
                } else {
                    (loop_frac + sf).rem_euclid(1.0)
                };
                let label = (ctx.map.cached_corners.len() + 1).to_string();
                ctx.map.cached_corners.push(track_path::CornerMark {
                    pct,
                    label,
                    ox: 0.0,
                    oy: 0.0,
                });
            }
        }
    }
}

fn inverse_model(x: f32, y: f32, mirror: bool, rot: i32) -> (f32, f32) {
    // Undo rotation first (inverse of rot), then undo mirror.
    let (mut x, y) = match rot.rem_euclid(360) {
        90 => (-y, x), // inverse of (y, -x)
        180 => (-x, -y),
        270 => (y, -x), // inverse of (-y, x)
        _ => (x, y),
    };
    if mirror {
        x = -x;
    }
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::earcut_triangles;
    use egui::Pos2;

    #[test]
    fn earcut_concave_quad_stays_inside() {
        // Arrowhead: concave at (1, 0.5). Centroid fan would spill outside.
        let pts = [
            Pos2::new(0.0, 0.0),
            Pos2::new(2.0, 0.0),
            Pos2::new(1.0, 0.5),
            Pos2::new(2.0, 1.0),
            Pos2::new(0.0, 1.0),
        ];
        let tris = earcut_triangles(&pts);
        assert_eq!(tris.len(), pts.len() - 2);
        // Every triangle vertex must be one of the polygon indices.
        for [a, b, c] in &tris {
            assert!((*a as usize) < pts.len());
            assert!((*b as usize) < pts.len());
            assert!((*c as usize) < pts.len());
        }
    }

    #[test]
    fn earcut_square() {
        let pts = [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
        ];
        assert_eq!(earcut_triangles(&pts).len(), 2);
    }
}
