use super::WidgetCtx;
use crate::chrome::{color_with_alpha, draw_card, draw_dark_cell, ease, full_rect, label};
use crate::config::OverlayConfig;
use crate::icons;
use crate::map_markers::{self, TrafficMarker};
use crate::telemetry::CarRow;
use crate::track_path;
use egui::{
    epaint::{PathShape, PathStroke},
    Align2, Color32, FontFamily, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui,
};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};

const SECTION: &str = "map";
const CAR_EASE_TAU: f32 = 0.09;

/// Aspect-preserving map from path bounds → plot pixels (after model xform).
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
}

/// Python model-space: mirror then 90° rotation steps.
fn model_point(x: f32, y: f32, mirror: bool, rot: i32) -> (f32, f32) {
    let mut x = x;
    let y = y;
    if mirror {
        x = -x;
    }
    match ((rot % 360) + 360) % 360 {
        90 => (y, -x),
        180 => (-x, -y),
        270 => (-y, x),
        _ => (x, y),
    }
}

fn ensure_path_cached(ctx: &mut WidgetCtx<'_>) {
    let tid = ctx.frame.track_id;
    if tid == ctx.map.cached_track_id && !ctx.map.cached_path.is_empty() {
        return;
    }
    ctx.map.cached_track_id = tid;
    ctx.map.cached_path.clear();
    ctx.map.cached_track_name.clear();
    ctx.map.cached_start_finish = 0.0;
    ctx.map.cached_drs_zones.clear();
    ctx.map.cached_p2p_zones.clear();
    if let Some(id) = tid {
        if let Some(tp) = track_path::load_for_track_id(id) {
            ctx.map.cached_track_name = tp.name.clone();
            ctx.map.cached_start_finish = tp.start_finish;
            ctx.map.cached_path = tp.points;
            ctx.map.cached_drs_zones = tp.drs_zones;
            ctx.map.cached_p2p_zones = tp.p2p_zones;
            return;
        }
    }
    ctx.map.cached_path = track_path::oval_path(64);
    ctx.map.cached_start_finish = 0.0;
}

fn fill_infield(ui: &mut Ui, screen: &[Pos2], fill: Color32) {
    if screen.len() < 3 {
        return;
    }
    // Centroid fan — works for typical race outlines (centroid inside).
    let mut cx = 0.0_f32;
    let mut cy = 0.0_f32;
    for p in screen {
        cx += p.x;
        cy += p.y;
    }
    let n = screen.len() as f32;
    let c = Pos2::new(cx / n, cy / n);
    let mut mesh = egui::Mesh::default();
    let ci = mesh.vertices.len() as u32;
    mesh.vertices.push(egui::epaint::Vertex {
        pos: c,
        uv: egui::epaint::WHITE_UV,
        color: fill,
    });
    for p in screen {
        mesh.vertices.push(egui::epaint::Vertex {
            pos: *p,
            uv: egui::epaint::WHITE_UV,
            color: fill,
        });
    }
    for i in 0..screen.len() {
        let a = ci + 1 + i as u32;
        let b = ci + 1 + ((i + 1) % screen.len()) as u32;
        mesh.indices.extend_from_slice(&[ci, a, b]);
    }
    ui.painter().add(Shape::mesh(mesh));
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
    let (nx, ny) = track_path::point_at(path, pct);
    let (tx, ty) = track_path::tangent_at(path, pct);
    // Perpendicular in screen space (same orientation as model after uniform scale).
    let px = -ty;
    let py = tx;
    let c = xform.map(nx, ny);
    // Tick length in pixels (±7).
    let tick = 7.0_f32;
    // Convert model-space unit tangent to screen by rotating perpendicular already unit
    // in model; after uniform scale direction is preserved.
    let a = Pos2::new(c.x + px * tick, c.y + py * tick);
    let b = Pos2::new(c.x - px * tick, c.y - py * tick);
    ui.painter()
        .line_segment([a, b], Stroke::new(3.0_f32, Color32::WHITE));
}

fn in_pit_lane(car: &CarRow) -> bool {
    car.on_pit || car.in_pit
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
fn car_fill(cfg: &OverlayConfig, car: &CarRow, pit_opacity: f32) -> Color32 {
    if car.is_pace_car {
        return cfg.color(SECTION, "pace_car", "#0b0e12");
    }
    if in_pit_lane(car) && !car.is_player {
        let pit = cfg.color(SECTION, "pit_car", "#6e747d");
        return color_with_alpha(pit, (pit_opacity.clamp(0.05, 1.0) * 255.0) as u8);
    }
    if car.is_player {
        return cfg.color(SECTION, "player", "#46df7a");
    }
    if car.lapping && car.lap_ahead {
        return cfg.color(SECTION, "lapping", "#ff5050");
    }
    if car.lapping {
        return cfg.color(SECTION, "lapped", "#4a8cff");
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
    ui.painter()
        .circle_stroke(c, r, Stroke::new(1.0_f32, ring));
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
) {
    if text.is_empty() {
        return;
    }
    let base = if is_player { 9.0 } else { 7.5 };
    let size = (base * text_scale).max(6.0);
    let font = FontId::new(size, FontFamily::Proportional);
    let fill = Color32::WHITE;
    let stroke = if is_pace {
        Color32::from_rgba_unmultiplied(0, 0, 0, 160)
    } else {
        Color32::from_rgba_unmultiplied(0, 0, 0, 220)
    };
    let stroke_w = if is_player { 1.2_f32 } else { 1.0 };
    let pos = c;
    let rich = is_player || is_pace;
    let offsets: &[(f32, f32)] = if rich {
        &[
            (-stroke_w, -stroke_w),
            (-stroke_w, stroke_w),
            (stroke_w, -stroke_w),
            (stroke_w, stroke_w),
            (-stroke_w, 0.0),
            (stroke_w, 0.0),
            (0.0, -stroke_w),
            (0.0, stroke_w),
        ]
    } else {
        &[
            (-stroke_w, 0.0),
            (stroke_w, 0.0),
            (0.0, -stroke_w),
            (0.0, stroke_w),
        ]
    };
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

fn ease_car_pcts(ctx: &mut WidgetCtx<'_>) -> HashMap<i32, f32> {
    let now = ctx.frame.session_time;
    let mut dt = if ctx.map.last_paint_secs > 0.0 {
        (now - ctx.map.last_paint_secs) as f32
    } else {
        1.0 / 60.0
    };
    dt = dt.clamp(0.0, 0.1);
    ctx.map.last_paint_secs = now;

    let mut seen = HashMap::new();
    let mut out = HashMap::new();
    for car in &ctx.frame.cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        if car.lap_dist_pct < 0.0 {
            continue;
        }
        seen.insert(car.car_idx, ());
        let target = car.lap_dist_pct.rem_euclid(1.0);
        let cur = ctx
            .map
            .car_anim
            .get(&car.car_idx)
            .copied()
            .unwrap_or(target);
        let delta = wrap_lap_delta(target, cur);
        let next = if delta.abs() > 0.35 {
            target
        } else if delta.abs() <= 1e-5 {
            target
        } else {
            (cur + ease(0.0, delta, dt, CAR_EASE_TAU)).rem_euclid(1.0)
        };
        ctx.map.car_anim.insert(car.car_idx, next);
        out.insert(car.car_idx, next);
    }
    ctx.map.car_anim.retain(|k, _| seen.contains_key(k));
    out
}

fn outward_from(pt: Pos2, cc: Pos2, extra: f32) -> Pos2 {
    let dx = pt.x - cc.x;
    let dy = pt.y - cc.y;
    let ln = (dx * dx + dy * dy).sqrt().max(1.0);
    Pos2::new(pt.x + dx / ln * extra, pt.y + dy / ln * extra)
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
) {
    let sz = (10.0 * text_scale).max(8.0);
    let icon_off = asphalt_w * 1.2 + sz + 10.0;
    let specs = ["leader", "ahead", "behind"];
    for slot in specs {
        let Some(Some(m)) = markers.get(slot) else {
            continue;
        };
        let car_pt = car_pts.get(&m.idx).copied().unwrap_or_else(|| {
            let (nx, ny) = track_path::point_at(path, m.pct);
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
            ui.painter().text(
                icon_pt,
                Align2::CENTER_CENTER,
                g,
                icons::font_id(sz),
                col,
            );
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
        ui.painter().text(
            badge_c,
            Align2::CENTER_CENTER,
            g,
            icons::font_id(sz),
            fg,
        );
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
            let (nx, ny) = track_path::point_at(path, pct);
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
    ui.painter().circle_filled(
        center,
        r,
        Color32::from_rgba_unmultiplied(10, 13, 17, 190),
    );
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
    ui.painter().line_segment(
        [tail, tip],
        Stroke::new((r * 0.14).max(1.5), col),
    );
    let hl = r * 0.42;
    let hw = r * 0.26;
    let base = Pos2::new(tip.x - ux * hl, tip.y - uy * hl);
    let head = vec![
        tip,
        Pos2::new(base.x + px * hw, base.y + py * hw),
        Pos2::new(base.x - px * hw, base.y - py * hw),
    ];
    ui.painter().add(Shape::convex_polygon(head, col, Stroke::NONE));

    let spd = cfg.conv_speed(wind_vel).round();
    let spd_text = format!("{spd:.0} {}", cfg.speed_unit());
    let ssz = (6.0 * text_scale).max(5.0);
    let font = FontId::new(ssz, FontFamily::Proportional);
    let galley = ui.painter().layout_no_wrap(spd_text, font.clone(), text_col);
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
                .map(|s| ui.painter().layout_no_wrap(s.clone(), font2.clone(), text_col))
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
                ui.painter().galley(
                    Pos2::new(lr2.center().x - g.size().x / 2.0, y),
                    g,
                    text_col,
                );
                y += line_h;
            }
        }
    }
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    ensure_path_cached(ctx);

    let rect = full_rect(ui);
    if ctx.cfg.bool_key(SECTION, "show_panel", false) {
        draw_card(ui, ctx.cfg, SECTION, rect);
    }

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

    // Pad so thick ribbon + S/F tick aren't clipped (~Python _layout_pad 26).
    let pad_px = 26.0_f32 + asphalt_w * 0.5;
    let plot = rect; // fit uses absolute pad inside

    let path = ctx.map.cached_path.clone();
    let modeled: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| model_point(x, y, mirror, rot))
        .collect();
    let xform = PlotXform::fit(plot, &modeled, pad_px);
    let screen: Vec<Pos2> = modeled
        .iter()
        .map(|&(x, y)| xform.map(x, y))
        .collect();

    if show_infield {
        fill_infield(ui, &screen, infield);
    }
    // Dual stroke: thick asphalt under thinner outline (Python recipe).
    stroke_closed(ui, &screen, asphalt_w, asphalt);
    stroke_closed(ui, &screen, outline_w, outline);

    let zone_w = (asphalt_w * 1.35).max(4.0);
    if ctx.cfg.bool_key(SECTION, "show_drs_zones", false) && !ctx.map.cached_drs_zones.is_empty()
    {
        let col = ctx.cfg.color(SECTION, "drs_zone", "#46df7a88");
        let zones = ctx.map.cached_drs_zones.clone();
        draw_zones(ui, &path, &xform, mirror, rot, &zones, zone_w, col);
    }
    if ctx.cfg.bool_key(SECTION, "show_p2p_zones", false) && !ctx.map.cached_p2p_zones.is_empty()
    {
        let col = ctx.cfg.color(SECTION, "p2p_zone", "#3aa0ff88");
        let zones = ctx.map.cached_p2p_zones.clone();
        draw_zones(ui, &path, &xform, mirror, rot, &zones, zone_w, col);
    }

    if show_sf && !modeled.is_empty() {
        // Transform tangent via model: evaluate on modeled path.
        draw_start_finish(ui, &modeled, &xform, ctx.map.cached_start_finish);
    }

    let player_scale = dot_scale(ctx.cfg.f64_key(SECTION, "dot_radius_frac", 0.05));
    let other_scale = dot_scale(ctx.cfg.f64_key(SECTION, "other_dot_radius_frac", 0.05));
    let pit_opacity = ctx.cfg.f64_key(SECTION, "pit_dot_opacity", 0.45) as f32;
    let car_label_mode = ctx.cfg.str_key(SECTION, "car_label", "number");
    let map_text_scale = ctx.cfg.text_scale(SECTION);
    let show_markers = ctx.cfg.bool_key(SECTION, "show_traffic_markers", true);
    let hold_sec = ctx.cfg.f64_key(SECTION, "marker_hold_seconds", 3.0);
    let show_status = ctx.cfg.bool_key(SECTION, "show_car_status", true);

    let eased = ease_car_pcts(ctx);

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

    let mut car_pts: HashMap<i32, Pos2> = HashMap::new();
    for car in &cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        let pct = eased
            .get(&car.car_idx)
            .copied()
            .unwrap_or(car.lap_dist_pct);
        if pct < 0.0 {
            continue;
        }
        let (nx, ny) = track_path::point_at(&path, pct);
        let (mx, my) = model_point(nx, ny, mirror, rot);
        let p = xform.map(mx, my);
        car_pts.insert(car.car_idx, p);
        let mut r = if car.is_player {
            12.5 * player_scale
        } else {
            9.0 * other_scale
        };
        if car.is_player && in_pit_lane(car) {
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
        let fill = car_fill(ctx.cfg, car, pit_opacity);
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

    // Authoring: pit points in path/raw space (pre-model). Map through model for display.
    let pit_col = accent;
    for (i, (nx, ny)) in ctx.map.pit_points.iter().enumerate() {
        let (mx, my) = model_point(*nx, *ny, mirror, rot);
        let p = xform.map(mx, my);
        ui.painter().circle_filled(p, 4.0, pit_col);
        if i > 0 {
            let (px, py) = ctx.map.pit_points[i - 1];
            let (pmx, pmy) = model_point(px, py, mirror, rot);
            let prev = xform.map(pmx, pmy);
            ui.painter()
                .line_segment([prev, p], Stroke::new(2.0_f32, pit_col));
        }
    }

    if ctx.map.pit_edit && ctx.map.interactive {
        let resp = ui.interact(plot, ui.id().with("map_pit"), Sense::click());
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let (mx, my) = xform.unmap(pos);
                // Inverse model (approx): undo rot then mirror.
                let (rx, ry) = inverse_model(mx, my, mirror, rot);
                ctx.map.pit_points.push((rx, ry));
            }
        }
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            &format!("PIT EDIT ({})", ctx.map.phase),
            12.0,
            accent,
            true,
        );
    } else if ctx.map.corner_edit {
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            "CORNER EDIT",
            12.0,
            accent,
            true,
        );
    } else if ctx.map.sf_edit {
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            "S/F EDIT",
            12.0,
            accent,
            true,
        );
    }
}

fn inverse_model(x: f32, y: f32, mirror: bool, rot: i32) -> (f32, f32) {
    // Undo rotation first (inverse of rot), then undo mirror.
    let (mut x, y) = match ((rot % 360) + 360) % 360 {
        90 => (-y, x),   // inverse of (y, -x)
        180 => (-x, -y),
        270 => (y, -x),  // inverse of (-y, x)
        _ => (x, y),
    };
    if mirror {
        x = -x;
    }
    (x, y)
}
