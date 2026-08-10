//! Skia map painter — visual parity with egui `widgets/map.rs`.

use super::canvas::{Canvas, TextAlign};
use super::chrome::{draw_card, draw_dark_cell, text_at};
use super::icons;
use super::tokens::DesignTokens;
use super::types::{FontSpec, Rect, Rgba};
use crate::chrome::{anim_dt, color_with_alpha};
use crate::config::OverlayConfig;
use crate::map_markers;
use crate::state::{MapAuthoring, TrackPathStatus};
use crate::telemetry::{CarRow, TelemetryFrame};
use crate::track_path;
use crate::widgets::{self, map as emap, MapPaintMode, WidgetCtx};
use egui::Pos2;
use std::collections::HashMap;
use std::f32::consts::PI;

const SECTION: &str = "map";
/// Skia measures glyph width only; approximate egui's galley height so corner
/// pills are the same size in both paths.
const CORNER_LINE_HEIGHT: f32 = 1.25;

/// Paint the map into a Skia canvas. Returns `true` while motion/status needs high cadence.
pub fn paint(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    map: &mut MapAuthoring,
    mono_secs: f64,
    demo: bool,
    _edit_mode: bool,
) -> bool {
    paint_with_mode(c, cfg, frame, map, mono_secs, demo, MapPaintMode::Full)
}

/// Capture static track chrome only (no cars) for the CPU bg cache.
pub fn paint_static(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    map: &mut MapAuthoring,
    mono_secs: f64,
    demo: bool,
) -> bool {
    paint_with_mode(
        c,
        cfg,
        frame,
        map,
        mono_secs,
        demo,
        MapPaintMode::StaticOnly,
    )
}

/// Hot path: cars / markers / wind only, over a canvas already seeded with the
/// cached static track. Same renderer as `paint`, so dots keep their numbers,
/// player ring, traffic-marker rings and status badges.
pub fn paint_dynamic(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    map: &mut MapAuthoring,
    mono_secs: f64,
    demo: bool,
) -> bool {
    paint_with_mode(
        c,
        cfg,
        frame,
        map,
        mono_secs,
        demo,
        MapPaintMode::DynamicOnly,
    )
}

fn paint_with_mode(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    map: &mut MapAuthoring,
    mono_secs: f64,
    demo: bool,
    mode: MapPaintMode,
) -> bool {
    let mut animating = false;
    let mut wctx = WidgetCtx {
        cfg,
        frame,
        edit_mode: false,
        demo,
        map,
        mono_secs,
        panel_animating: &mut animating,
        map_paint_mode: mode,
    };
    // Car-bearing paints may tick; StaticOnly does not need coast advance.
    if mode != MapPaintMode::StaticOnly {
        let _ = widgets::tick_car_motion(&mut wctx);
    }
    paint_inner(c, &mut wctx);
    animating
}

fn paint_inner(c: &mut Canvas, ctx: &mut WidgetCtx<'_>) {
    emap::ensure_path_cached(ctx);

    let bounds = Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32);
    // DynamicOnly paints on top of a pre-seeded static bg; clearing would wipe it.
    let draw_static = ctx.map_paint_mode != MapPaintMode::DynamicOnly;
    if draw_static {
        c.clear_transparent();
        if ctx.cfg.bool_key(SECTION, "show_panel", false) {
            let tokens = DesignTokens::for_section(ctx.cfg, SECTION, c.height() as f32);
            draw_card(c, &tokens, bounds.inset(1.0, 1.0));
        }
    }

    if !ctx.demo && ctx.map.path_status != TrackPathStatus::Ready {
        if matches!(
            ctx.map.path_status,
            TrackPathStatus::Loading | TrackPathStatus::None
        ) {
            *ctx.panel_animating = true;
        }
        paint_path_status(c, ctx, bounds);
        return;
    }
    if ctx.map.cached_path.len() < 3 {
        if !ctx.demo {
            *ctx.panel_animating = true;
            paint_path_status(c, ctx, bounds);
        }
        return;
    }

    emap::seed_pit_latches(ctx);

    let asphalt_w = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let outline_w = ctx.cfg.f64_key(SECTION, "outline_width", 6.0) as f32;
    let show_infield = ctx.cfg.bool_key(SECTION, "show_infield", true);
    let show_sf = ctx.cfg.bool_key(SECTION, "show_start_finish", true);
    let mirror = ctx.cfg.bool_key(SECTION, "mirror", false);
    let rot = ctx.cfg.f64_key(SECTION, "rotation", 0.0) as i32;

    let asphalt = rgba_cfg(ctx.cfg, "asphalt", "#333a42");
    let outline = rgba_cfg(ctx.cfg, "outline", "#8b93a1");
    let infield = rgba_cfg(ctx.cfg, "infield", "#0f1216c8");
    let accent = rgba_cfg(ctx.cfg, "accent", "#70df7a");

    let text_scale = ctx.cfg.text_scale(SECTION);
    let pad_px = emap::layout_pad(ctx.cfg, asphalt_w, text_scale);
    let plot = egui::Rect::from_min_size(
        egui::pos2(bounds.x, bounds.y),
        egui::vec2(bounds.w, bounds.h),
    );

    let path = ctx.map.cached_path.clone();
    let mut modeled: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| emap::model_point(x, y, mirror, rot))
        .collect();
    if ctx.cfg.bool_key(SECTION, "show_pit", true) {
        for lane in [&ctx.map.cached_pit, &ctx.map.cached_pit2] {
            for &(x, y) in &lane.all_points() {
                modeled.push(emap::model_point(x, y, mirror, rot));
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
                modeled.push(emap::model_point(x, y, mirror, rot));
            }
        }
    }
    let base_xform = emap::PlotXform::fit(plot, &modeled, pad_px);
    let xform = if ctx.map.pit_edit {
        base_xform.with_view(ctx.map.pit_edit_zoom, ctx.map.pit_edit_pan)
    } else {
        base_xform
    };
    let screen: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| {
            let (mx, my) = emap::model_point(x, y, mirror, rot);
            xform.map_xy(mx, my)
        })
        .collect();
    let modeled: Vec<(f32, f32)> = path
        .iter()
        .map(|&(x, y)| emap::model_point(x, y, mirror, rot))
        .collect();

    let zone_w = (asphalt_w * 1.35).max(4.0);
    let sf = ctx.map.cached_start_finish;
    let reverse = emap::map_reverse(ctx);
    let cal = ctx.map.cached_pct_map.as_deref();
    let screen_centroid = if screen.is_empty() {
        (bounds.center().0, bounds.center().1)
    } else {
        let n = screen.len() as f32;
        (
            screen.iter().map(|p| p.0).sum::<f32>() / n,
            screen.iter().map(|p| p.1).sum::<f32>() / n,
        )
    };
    let centroid = Pos2::new(screen_centroid.0, screen_centroid.1);

    // —— Static track (already baked into the bg cache on the hot path) ——
    if draw_static {
        if show_infield {
            fill_infield(c, &screen, infield, ctx.map.cached_self_crossing);
        }
        c.polyline(&screen, asphalt, asphalt_w, true);
        c.polyline(&screen, outline, outline_w, true);

        if ctx.cfg.bool_key(SECTION, "show_pit", true) {
            let pit = ctx.map.cached_pit.clone();
            let pit2 = ctx.map.cached_pit2.clone();
            let hide = ctx.map.pit_edit;
            let show1_entry = !(hide && !ctx.map.entry_pts.is_empty());
            let show1_road = !(hide && !ctx.map.road_pts.is_empty());
            let show1_exit = !(hide && !ctx.map.merge_pts.is_empty());
            let show_lane1 = !(hide
                && (!ctx.map.road_pts.is_empty()
                    || !ctx.map.merge_pts.is_empty()
                    || !ctx.map.entry_pts.is_empty()));
            if show_lane1 {
                draw_pit_lane(
                    c,
                    ctx.cfg,
                    &pit,
                    &xform,
                    mirror,
                    rot,
                    asphalt,
                    show1_entry,
                    show1_road,
                    show1_exit,
                    text_scale,
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
                        c,
                        ctx.cfg,
                        &pit2,
                        &xform,
                        mirror,
                        rot,
                        asphalt,
                        show2_entry,
                        show2_road,
                        show2_exit,
                        text_scale,
                    );
                }
            }
        }

        if ctx.cfg.bool_key(SECTION, "show_drs_zones", false)
            && !ctx.map.cached_drs_zones.is_empty()
        {
            let col = rgba_cfg(ctx.cfg, "drs_zone", "#46df7a88");
            let zones = ctx.map.cached_drs_zones.clone();
            draw_zones(
                c, &path, &xform, mirror, rot, &zones, zone_w, col, sf, reverse, cal,
            );
        }
        if ctx.cfg.bool_key(SECTION, "show_p2p_zones", false)
            && !ctx.map.cached_p2p_zones.is_empty()
        {
            let col = rgba_cfg(ctx.cfg, "p2p_zone", "#3aa0ff88");
            let zones = ctx.map.cached_p2p_zones.clone();
            draw_zones(
                c, &path, &xform, mirror, rot, &zones, zone_w, col, sf, reverse, cal,
            );
        }
        if ctx
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
            let col = rgba_cfg(ctx.cfg, "active_sector", "#ffd23a66");
            draw_zones(
                c,
                &path,
                &xform,
                mirror,
                rot,
                &[(lo, hi)],
                zone_w,
                col,
                sf,
                reverse,
                cal,
            );
        }

        if show_sf && !modeled.is_empty() {
            let (tick, width) = emap::sf_tick_style(ctx.map.sf_edit);
            draw_loop_tick(
                c,
                &modeled,
                &xform,
                emap::sf_loop_frac(sf, reverse, ctx.map.cached_pct_map.as_deref()),
                tick,
                width,
                Rgba::WHITE,
                false,
            );
        }

        if ctx.cfg.bool_key(SECTION, "show_sector_boundaries", true) && !modeled.is_empty() {
            draw_sector_boundaries(c, ctx, &modeled, &xform, centroid);
        }
        if ctx.cfg.bool_key(SECTION, "show_corners", true) && !ctx.map.cached_corners.is_empty() {
            draw_corners(c, ctx, &path, &xform, mirror, rot, centroid);
        }

        if ctx.cfg.bool_key(SECTION, "show_pace_safety_line", true)
            && emap::is_caution_flag(ctx.frame.flag.as_deref())
            && !modeled.is_empty()
        {
            let exit_col = rgba_cfg(ctx.cfg, "pit_exit_mark", "#ffd23acc");
            let safety_col = rgba_cfg(ctx.cfg, "pace_safety", "#ff9416ee");
            if let Some(exit_pct) = ctx.map.cached_pit_out_pct {
                draw_loop_tick(c, &modeled, &xform, exit_pct, 9.0, 2.5, exit_col, true);
            }
            if let Some(pace) = ctx
                .frame
                .cars
                .iter()
                .find(|car| car.is_pace_car && car.lap_dist_pct >= 0.0)
            {
                draw_loop_tick(
                    c,
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
    }

    // Static BG capture stops here (cars / markers / wind / authoring are hot or Full).
    if ctx.map_paint_mode == MapPaintMode::StaticOnly {
        return;
    }

    // —— Dynamic ——
    let player_scale = emap::dot_scale(ctx.cfg.f64_key(SECTION, "dot_radius_frac", 0.05));
    let other_scale = emap::dot_scale(ctx.cfg.f64_key(SECTION, "other_dot_radius_frac", 0.05));
    let pit_opacity = ctx.cfg.f64_key(SECTION, "pit_dot_opacity", 0.45) as f32;
    let car_label_mode = ctx.cfg.str_key(SECTION, "car_label", "number");
    let map_text_scale = text_scale;
    let show_markers = ctx.cfg.bool_key(SECTION, "show_traffic_markers", true);
    let hold_sec = ctx.cfg.f64_key(SECTION, "marker_hold_seconds", 3.0);
    let show_status = ctx.cfg.bool_key(SECTION, "show_car_status", true);

    let screen_dt = anim_dt(ctx.mono_secs, &mut ctx.map.last_screen_secs);
    let eased: HashMap<i32, f32> = ctx
        .map
        .car_anim
        .iter()
        .map(|(&idx, st)| (idx, st.pct))
        .collect();
    let focus_idx = emap::focus_car_idx(ctx);
    let markers = if show_markers {
        map_markers::resolve_traffic_markers(
            &mut ctx.map.marker_hold,
            &ctx.frame.cars,
            ctx.frame.session_time,
            hold_sec,
            focus_idx,
            &car_label_mode,
        )
    } else {
        HashMap::new()
    };
    let marker_slots = map_markers::marker_slots_by_idx(&markers);

    let mut cars: Vec<&CarRow> = ctx.frame.cars.iter().collect();
    cars.sort_by_key(|car| (car.is_speaking, Some(car.car_idx) == focus_idx));

    let show_blends = ctx.cfg.bool_key(SECTION, "show_pit_blends", true);
    let pit_lane = ctx.map.cached_pit.clone();
    let pit_lane2 = ctx.map.cached_pit2.clone();

    emap::update_pit_route_latches(ctx, &cars);

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
        let on_route = emap::car_on_route(ctx, car);
        let (nx, ny) = emap::car_model_xy(ctx, car, pct, &path, &pit_lane, &pit_lane2, show_blends);
        let (mx, my) = emap::model_point(nx, ny, mirror, rot);
        let p = xform.map(mx, my);
        let key = emap::car_motion_key(on_route, car.on_pit);
        targets.insert(car.car_idx, (p, key));
    }
    let (car_pts, screen_animating) = emap::smooth_car_screen_pts(ctx, &targets, screen_dt);
    if screen_animating || emap::cars_pct_animating(ctx) {
        *ctx.panel_animating = true;
    }

    for car in &cars {
        if car.is_pace_car && car.lap_dist_pct < 0.0 {
            continue;
        }
        let Some(&p) = car_pts.get(&car.car_idx) else {
            continue;
        };
        let on_route = emap::car_on_route(ctx, car);
        let is_focus = Some(car.car_idx) == focus_idx;
        let mut r = if is_focus {
            12.5 * player_scale
        } else {
            9.0 * other_scale
        };
        if is_focus && (car.on_pit || on_route) {
            r *= 1.15;
        }
        if let Some(slot) = marker_slots.get(&car.car_idx) {
            if !is_focus {
                let (col_key, fallback) = emap::marker_color_key(slot);
                let ring = rgba_cfg(ctx.cfg, col_key, fallback);
                c.circle_ex(p.x, p.y, r + 5.0, ring, false, 2.6);
            }
        }
        let fill = Rgba::from_egui(emap::car_fill(
            ctx.cfg,
            car,
            pit_opacity,
            on_route,
            is_focus,
        ));
        if is_focus {
            draw_player_dot(c, p.x, p.y, r, fill);
        } else {
            draw_other_dot(c, p.x, p.y, r, fill);
        }
        if car.is_speaking && !car.is_pace_car {
            draw_speaking(c, ctx.cfg, p.x, p.y, r);
        }
        if show_status && !car.is_pace_car {
            if let Some(kind) = car.status_kind.as_deref() {
                draw_status_badge(c, ctx.cfg, p.x, p.y, r, kind);
            }
        }
        let show_label = is_focus || car.is_pace_car || !marker_slots.contains_key(&car.car_idx);
        if show_label {
            let label_text = emap::car_label_text(car, &car_label_mode);
            draw_car_number_label(
                c,
                p.x,
                p.y,
                &label_text,
                is_focus,
                car.is_pace_car,
                map_text_scale,
            );
        }
    }

    if show_markers {
        draw_traffic_markers(
            c,
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
            reverse,
            ctx.map.cached_pct_map.as_deref(),
        );
    }

    if ctx.cfg.bool_key(SECTION, "show_wind", true) {
        if let (Some(dir), Some(vel)) = (ctx.frame.wind_dir, ctx.frame.wind_vel) {
            let screen_pos: Vec<Pos2> = screen.iter().map(|&(x, y)| Pos2::new(x, y)).collect();
            draw_wind(
                c,
                ctx.cfg,
                plot,
                &screen_pos,
                dir,
                vel,
                ctx.frame.track_wetness,
                ctx.frame.rain_intensity,
                ctx.cfg.bool_key(SECTION, "show_expanded_weather", false),
                map_text_scale,
            );
        }
    }

    // —— Authoring overlays (visual) ——
    if ctx.map.pit_edit {
        draw_pit_edit_drafts(c, ctx.map, &xform, mirror, rot, asphalt_w);
        let phase = ctx.map.phase_key();
        let lane2 = ctx.map.lane_is_2();
        let label_col = match phase {
            "entry" => {
                if lane2 {
                    Rgba::rgb(200, 240, 120)
                } else {
                    Rgba::rgb(255, 210, 58)
                }
            }
            "merge" => {
                if lane2 {
                    Rgba::rgb(100, 200, 255)
                } else {
                    Rgba::rgb(90, 160, 255)
                }
            }
            _ => {
                if lane2 {
                    Rgba::rgb(70, 210, 170)
                } else {
                    Rgba::rgb(255, 90, 90)
                }
            }
        };
        text_at(
            c,
            bounds.left() + 6.0,
            bounds.top() + 12.0,
            &format!("PIT EDIT ({} L{})", phase, if lane2 { 2 } else { 1 }),
            12.0,
            label_col,
            true,
            TextAlign::Left,
        );
    } else if ctx.map.corner_edit {
        text_at(
            c,
            bounds.left() + 6.0,
            bounds.top() + 12.0,
            "CORNER EDIT",
            12.0,
            Rgba::rgb(255, 200, 60),
            true,
            TextAlign::Left,
        );
    } else if ctx.map.sf_edit {
        let hint = if ctx
            .frame
            .cars
            .iter()
            .any(|car| car.is_player && car.lap_dist_pct >= 0.0)
        {
            "S/F EDIT — click where YOU are"
        } else {
            "S/F EDIT — click S/F on map"
        };
        text_at(
            c,
            bounds.left() + 6.0,
            bounds.top() + 12.0,
            hint,
            12.0,
            accent,
            true,
            TextAlign::Left,
        );
    }
}

fn rgba_cfg(cfg: &OverlayConfig, key: &str, fallback: &str) -> Rgba {
    Rgba::from_egui(cfg.color(SECTION, key, fallback))
}

fn paint_path_status(c: &mut Canvas, ctx: &WidgetCtx<'_>, rect: Rect) {
    let (text, alpha, size) = match ctx.map.path_status {
        TrackPathStatus::Unavailable => ("Track unavailable", 110u8, 11.0_f32),
        TrackPathStatus::Loading | TrackPathStatus::None => ("Loading track…", 170u8, 13.0_f32),
        TrackPathStatus::Ready => return,
    };
    let scale = ctx.cfg.text_scale(SECTION);
    let color = Rgba::new(200, 205, 215, alpha);
    text_at(
        c,
        rect.center().0,
        rect.center().1,
        text,
        (size * scale).max(9.0),
        color,
        false,
        TextAlign::Center,
    );
}

fn fill_infield(c: &mut Canvas, screen: &[(f32, f32)], fill: Rgba, _self_crossing: bool) {
    if screen.len() < 3 {
        return;
    }
    let mut pts = screen.to_vec();
    if pts.len() >= 2 {
        let a = pts[0];
        let b = *pts.last().unwrap();
        if (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3 {
            pts.pop();
        }
    }
    if pts.len() < 3 {
        return;
    }
    // Always use Skia's winding fill. Earcut triangles drawn as separate
    // subpaths leave AA cracks along long diagonals — visible as a black
    // "tear" across the infield (and looking like it cuts the asphalt).
    c.fill_closed_path(&pts, fill);
}

fn draw_pit_lane(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    lane: &track_path::PitLane,
    xform: &emap::PlotXform,
    mirror: bool,
    rot: i32,
    asphalt: Rgba,
    show_entry: bool,
    show_road: bool,
    show_exit: bool,
    text_scale: f32,
) {
    if !lane.has_drawable() {
        return;
    }
    let opacity = cfg
        .f64_key(SECTION, "pit_lane_opacity", 1.0)
        .clamp(0.05, 1.0) as f32;
    let a = (opacity * 255.0) as u8;
    let a_asphalt = (opacity * 0.85 * 255.0) as u8;
    let show_blends = cfg.bool_key(SECTION, "show_pit_blends", true);
    let show_speed = cfg.bool_key(SECTION, "show_pit_speed", true);

    let pit_col = Rgba::from_egui(color_with_alpha(cfg.color(SECTION, "pit", "#ff4d4d"), a));
    let blend_in = Rgba::from_egui(color_with_alpha(
        cfg.color(SECTION, "pit_blend", "#ffd23a"),
        a,
    ));
    let blend_out = Rgba::from_egui(color_with_alpha(
        cfg.color(SECTION, "pit_blend_out", "#3aa0ff"),
        a,
    ));
    let asphalt_u = asphalt.with_alpha(a_asphalt);

    if show_blends && show_entry && lane.entry.len() >= 2 {
        let m = emap::model_poly(&lane.entry, mirror, rot);
        let s = emap::screen_poly_xy(xform, &m);
        c.dashed_polyline(&s, blend_in, 2.5, 3.0, 4.0);
    }
    if show_blends && show_exit && lane.exit.len() >= 2 {
        let m = emap::model_poly(&lane.exit, mirror, rot);
        let s = emap::screen_poly_xy(xform, &m);
        c.dashed_polyline(&s, blend_out, 2.5, 3.0, 4.0);
    }
    if show_road && lane.path.len() >= 2 {
        let m = emap::model_poly(&lane.path, mirror, rot);
        let s = emap::screen_poly_xy(xform, &m);
        c.polyline(&s, asphalt_u, 7.0, false);
        c.dashed_polyline(&s, pit_col, 2.2, 4.0, 3.0);

        if show_speed {
            if let Some(ms) = lane.speed_ms.filter(|v| *v > 0.0) {
                let (val, unit) = (cfg.conv_speed(ms), cfg.speed_unit());
                let anchor = s[s.len() / 2];
                let txt = format!("PIT {val:.0} {unit}");
                let bg =
                    Rgba::from_egui(color_with_alpha(cfg.color(SECTION, "pit", "#ff4d4d"), 235));
                let fg = rgba_cfg(cfg, "pit_text", "#ffffff");
                let sz = (11.0 * text_scale).max(9.0);
                let tw = c.measure_text(&txt, FontSpec::new(sz));
                let pad = 4.0;
                let rect = Rect::from_xywh(
                    anchor.0 - tw * 0.5 - pad,
                    anchor.1 - sz - pad * 2.0,
                    tw + pad * 2.0,
                    sz + pad,
                );
                c.fill_rect(rect, bg, 4.0);
                text_at(
                    c,
                    rect.left() + pad,
                    rect.center().1,
                    &txt,
                    sz,
                    fg,
                    false,
                    TextAlign::Left,
                );
            }
        }
    }
}

fn draw_zones(
    c: &mut Canvas,
    path: &[(f32, f32)],
    xform: &emap::PlotXform,
    mirror: bool,
    rot: i32,
    zones: &[(f32, f32)],
    width: f32,
    color: Rgba,
    start_finish: f32,
    reverse: bool,
    cal: Option<&[f32]>,
) {
    for &(lo, hi) in zones {
        let pts = emap::zone_screen_pts(path, xform, mirror, rot, lo, hi, start_finish, reverse, cal);
        if pts.len() >= 2 {
            c.polyline(&pts, color, width, false);
        }
    }
}

fn draw_loop_tick(
    c: &mut Canvas,
    path: &[(f32, f32)],
    xform: &emap::PlotXform,
    pct: f32,
    tick: f32,
    width: f32,
    color: Rgba,
    dashed: bool,
) {
    let (a, b) = emap::loop_tick_ends(path, xform, pct, tick);
    if dashed {
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let segs = 5;
        for i in 0..segs {
            if i % 2 == 1 {
                continue;
            }
            let t0 = i as f32 / segs as f32;
            let t1 = (i + 1) as f32 / segs as f32;
            c.line(
                a.0 + dx * t0,
                a.1 + dy * t0,
                a.0 + dx * t1,
                a.1 + dy * t1,
                color,
                width,
            );
        }
    } else {
        c.line(a.0, a.1, b.0, b.1, color, width);
    }
}

fn draw_sector_boundaries(
    c: &mut Canvas,
    ctx: &WidgetCtx<'_>,
    modeled: &[(f32, f32)],
    xform: &emap::PlotXform,
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
    let reverse = emap::map_reverse(ctx);
    let cal = ctx.map.cached_pct_map.as_deref();
    let sf_frac = emap::sf_loop_frac(sf, reverse, cal);
    let line = rgba_cfg(ctx.cfg, "sector_line", "#a78bfa");
    let text_col = rgba_cfg(ctx.cfg, "sector_text", "#c4b5fd");
    let asph = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let text_scale = ctx.cfg.text_scale(SECTION);
    let sz = (7.0 * text_scale).max(5.0);
    let label_off = asph * 0.5 + sz + 6.0;
    let mut label_num = 2_i32;

    for start in starts {
        let frac = emap::loop_frac_for_pct(start, sf, reverse, cal);
        if frac.abs() < 1e-4 || (frac - sf_frac).abs() < 0.01 {
            continue;
        }
        draw_loop_tick(c, modeled, xform, frac, 6.0, 2.0, line, false);

        let (nx, ny) = track_path::point_at(modeled, frac);
        let pt = xform.map(nx, ny);
        let outward = emap::outward_from(pt, centroid, label_off);
        let tag = format!("S{label_num}");
        label_num += 1;

        let tw = c.measure_text(&tag, FontSpec::bold(sz)).max(sz + 2.0) + 8.0;
        let th = sz + 2.0;
        let rect = Rect::from_xywh(outward.x - tw * 0.5, outward.y - th * 0.5, tw, th);
        c.fill_rect(rect, line, 3.0);
        text_at(
            c,
            rect.center().0,
            rect.center().1,
            &tag,
            sz,
            text_col,
            true,
            TextAlign::Center,
        );
    }
}

fn draw_corners(
    c: &mut Canvas,
    ctx: &WidgetCtx<'_>,
    path: &[(f32, f32)],
    xform: &emap::PlotXform,
    mirror: bool,
    rot: i32,
    centroid: Pos2,
) {
    let asphalt_w = ctx.cfg.f64_key(SECTION, "asphalt_width", 12.0) as f32;
    let text_scale = ctx.cfg.text_scale(SECTION);
    let sz = (8.0 * text_scale).max(5.0);
    let off = asphalt_w * 0.5 + sz + 8.0;
    let sf = ctx.map.cached_start_finish;
    let reverse = emap::map_reverse(ctx);
    let corners = ctx.map.cached_corners.clone();
    for (idx, corner) in corners.iter().enumerate() {
        let (nx, ny) = track_path::point_at(
            path,
            emap::loop_frac_for_pct(corner.pct, sf, reverse, ctx.map.cached_pct_map.as_deref()),
        );
        let (mx, my) = emap::model_point(nx, ny, mirror, rot);
        let s = xform.map(mx, my);
        let dx = s.x - centroid.x;
        let dy = s.y - centroid.y;
        let ln = (dx * dx + dy * dy).sqrt().max(1.0);
        let mut ax = s.x + dx / ln * off;
        let mut ay = s.y + dy / ln * off;
        if corner.ox != 0.0 || corner.oy != 0.0 {
            let (omx, omy) = emap::model_point(corner.ox, corner.oy, mirror, rot);
            ax += omx * xform.scale * 0.15;
            ay += omy * xform.scale * 0.15;
        }
        let label_txt = &corner.label;
        // Square-ish minimum so single digits don't render as thin slivers.
        let th = sz * CORNER_LINE_HEIGHT + 4.0;
        let tw = (c.measure_text(label_txt, FontSpec::new(sz)) + 12.0).max(th);
        let rect = Rect::from_xywh(ax - tw * 0.5, ay - th * 0.5, tw, th);
        let radius = emap::CORNER_CELL_RADIUS as f32;
        if ctx.map.corner_edit {
            let a = if ctx.map.drag_corner == Some(idx) {
                220
            } else {
                160
            };
            c.fill_rect(rect, Rgba::new(255, 200, 60, a), radius);
        } else {
            draw_dark_cell(c, ctx.cfg, SECTION, rect, radius);
        }
        text_at(
            c,
            rect.center().0,
            rect.center().1,
            label_txt,
            sz,
            rgba_cfg(ctx.cfg, "corner_text", emap::CORNER_TEXT),
            false,
            TextAlign::Center,
        );
    }
}

fn draw_player_dot(c: &mut Canvas, x: f32, y: f32, r: f32, fill: Rgba) {
    let glow = fill.with_alpha(70);
    c.circle(x, y, r + 6.0, glow, true);
    c.circle(x, y, r, fill, true);
    c.circle_ex(x, y, r, Rgba::BLACK, false, 2.0);
    c.circle_ex(x, y, r + 2.4, Rgba::WHITE, false, 2.4);
}

fn draw_other_dot(c: &mut Canvas, x: f32, y: f32, r: f32, fill: Rgba) {
    c.circle(x, y, r, fill, true);
    c.circle_ex(x, y, r, Rgba::BLACK.with_alpha(fill.a), false, 1.0);
}

fn draw_speaking(c: &mut Canvas, cfg: &OverlayConfig, x: f32, y: f32, r: f32) {
    let ring = rgba_cfg(cfg, "speaking_ring", "#46df7a");
    let glow = rgba_cfg(cfg, "speaking_glow", "#46df7a55");
    c.circle(x, y, r + 7.5, glow, true);
    c.circle_ex(x, y, r + 4.8, ring, false, 2.8);
    let sz = (r * 1.05).max(7.0);
    let side = sz + 6.0;
    let bx = x + r * 0.95;
    let by = y - r * 1.05;
    let bg = rgba_cfg(cfg, "speaking_badge_bg", "#22c55e");
    let fg = rgba_cfg(cfg, "speaking_badge_text", "#ffffff");
    c.circle(bx, by, side * 0.5, bg, true);
    c.circle_ex(bx, by, side * 0.5, fg, false, 1.2);
    icons::paint(c, "speaking", bx, by, sz, fg, TextAlign::Center);
}

fn draw_status_badge(c: &mut Canvas, cfg: &OverlayConfig, x: f32, y: f32, r: f32, kind: &str) {
    let Some(fill_egui) = emap::status_fill(cfg, kind) else {
        return;
    };
    let fill = Rgba::from_egui(fill_egui);
    let br = (r * 0.55).max(5.0);
    let bx = x + r * 0.65;
    let by = y - r * 0.65;
    c.circle(bx, by, br, fill, true);
    c.circle_ex(bx, by, br, Rgba::new(0, 0, 0, 180), false, 1.0);
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
        Rgba::rgb(20, 20, 20)
    } else {
        Rgba::WHITE
    };
    text_at(c, bx, by, glyph, sz, text_col, true, TextAlign::Center);
}

/// On-dot number — white ink, black halo, sizes shared with the egui path.
fn draw_car_number_label(
    c: &mut Canvas,
    x: f32,
    y: f32,
    text: &str,
    is_focus: bool,
    is_pace: bool,
    text_scale: f32,
) {
    if text.is_empty() {
        return;
    }
    let (size, w) = emap::car_label_metrics(is_focus, text_scale, text);
    let stroke = Rgba::new(0, 0, 0, if is_pace { 160 } else { 220 });
    let offsets: &[(f32, f32)] = if is_focus || is_pace {
        &emap::CAR_LABEL_RICH
    } else {
        &emap::CAR_LABEL_PLAIN
    };
    let spec = FontSpec::bold(size);
    for &(ox, oy) in offsets {
        c.text_ink_centered(text, x + ox * w, y + oy * w, spec, stroke);
    }
    c.text_ink_centered(text, x, y, spec, Rgba::WHITE);
}

fn draw_traffic_markers(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    markers: &HashMap<&'static str, Option<map_markers::TrafficMarker>>,
    car_pts: &HashMap<i32, Pos2>,
    path: &[(f32, f32)],
    xform: &emap::PlotXform,
    mirror: bool,
    rot: i32,
    centroid: Pos2,
    asphalt_w: f32,
    text_scale: f32,
    start_finish: f32,
    reverse: bool,
    cal: Option<&[f32]>,
) {
    let sz = (10.0 * text_scale).max(8.0);
    let icon_off = asphalt_w * 1.2 + sz + 10.0;
    for slot in ["leader", "ahead", "behind"] {
        let Some(Some(m)) = markers.get(slot) else {
            continue;
        };
        let car_pt = car_pts.get(&m.idx).copied().unwrap_or_else(|| {
            let (nx, ny) =
                track_path::point_at(path, emap::loop_frac_for_pct(m.pct, start_finish, reverse, cal));
            let (mx, my) = emap::model_point(nx, ny, mirror, rot);
            xform.map(mx, my)
        });
        let icon_pt = emap::outward_from(car_pt, centroid, icon_off);
        let (col_key, fallback) = emap::marker_color_key(slot);
        let col = rgba_cfg(cfg, col_key, fallback);
        c.line(car_pt.x, car_pt.y, icon_pt.x, icon_pt.y, col, 2.0);
        let side = (sz + 6.0).max(22.0);
        let icon_rect = Rect::from_xywh(icon_pt.x - side * 0.5, icon_pt.y - side * 0.5, side, side);
        draw_dark_cell(c, cfg, SECTION, icon_rect, 5.0);
        icons::paint(
            c,
            emap::marker_glyph_name(slot),
            icon_pt.x,
            icon_pt.y,
            sz,
            col,
            TextAlign::Center,
        );
        if !m.label.is_empty() {
            let fsz = (sz - 2.0).max(7.0);
            let tw = c.measure_text(&m.label, FontSpec::new(fsz)) + 8.0;
            let th = fsz + 4.0;
            let pill = Rect::from_xywh(icon_pt.x - tw * 0.5, icon_rect.bottom() + 2.0, tw, th);
            c.fill_rect(pill, col, 3.0);
            text_at(
                c,
                pill.center().0,
                pill.center().1,
                &m.label,
                fsz,
                Rgba::rgb(20, 20, 20),
                false,
                TextAlign::Center,
            );
        }
    }
}

fn draw_wind(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: egui::Rect,
    screen: &[Pos2],
    wind_dir: f32,
    wind_vel: f32,
    wet: Option<f32>,
    rain: Option<f32>,
    expanded: bool,
    text_scale: f32,
) {
    let (center, r) = emap::wind_center(screen, rect);
    let col = rgba_cfg(cfg, "wind", "#9fd0ff");
    let text_col = rgba_cfg(cfg, "wind_text", "#eaf3ff");
    c.circle(center.x, center.y, r, Rgba::new(10, 13, 17, 190), true);
    c.circle_ex(
        center.x,
        center.y,
        r,
        Rgba::new(255, 255, 255, 40),
        false,
        1.0,
    );
    let nsz = (6.0 * text_scale).max(5.0);
    text_at(
        c,
        center.x,
        center.y - r - nsz * 0.5 - 1.0,
        "N",
        nsz,
        Rgba::rgb(170, 178, 188),
        false,
        TextAlign::Center,
    );
    let b = emap::wind_dir_radians(wind_dir) + PI;
    let ux = b.sin();
    let uy = -b.cos();
    let px = -uy;
    let py = ux;
    let tip = (center.x + ux * r * 0.78, center.y + uy * r * 0.78);
    let tail = (center.x - ux * r * 0.70, center.y - uy * r * 0.70);
    c.line(tail.0, tail.1, tip.0, tip.1, col, (r * 0.14).max(1.5));
    let hl = r * 0.42;
    let hw = r * 0.26;
    let base = (tip.0 - ux * hl, tip.1 - uy * hl);
    let head = [
        tip,
        (base.0 + px * hw, base.1 + py * hw),
        (base.0 - px * hw, base.1 - py * hw),
    ];
    c.fill_closed_path(&head, col);

    let spd = cfg.conv_speed(wind_vel).round();
    let spd_text = format!("{spd:.0} {}", cfg.speed_unit());
    let ssz = (6.0 * text_scale).max(5.0);
    let tw = c.measure_text(&spd_text, FontSpec::new(ssz)) + 6.0;
    let th = ssz + 2.0;
    let lr = Rect::from_xywh(center.x - tw * 0.5, center.y + r + 1.0, tw, th);
    c.fill_rect(lr, Rgba::new(10, 13, 17, 190), 2.0);
    text_at(
        c,
        lr.center().0,
        lr.center().1,
        &spd_text,
        ssz,
        text_col,
        false,
        TextAlign::Center,
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
            let tw2 = lines
                .iter()
                .map(|s| c.measure_text(s, FontSpec::new(ssz2)))
                .fold(0.0_f32, f32::max)
                + 6.0;
            let line_h = ssz2 + 1.0;
            let th2 = line_h * lines.len() as f32 + 4.0;
            let lr2 = Rect::from_xywh(center.x - tw2 * 0.5, lr.bottom() + 2.0, tw2, th2);
            c.fill_rect(lr2, Rgba::new(10, 13, 17, 190), 2.0);
            let mut y = lr2.top() + 2.0 + line_h * 0.5;
            for s in &lines {
                text_at(
                    c,
                    lr2.center().0,
                    y,
                    s,
                    ssz2,
                    text_col,
                    false,
                    TextAlign::Center,
                );
                y += line_h;
            }
        }
    }
}

fn draw_pit_edit_drafts(
    c: &mut Canvas,
    map: &MapAuthoring,
    xform: &emap::PlotXform,
    mirror: bool,
    rot: i32,
    asphalt_w: f32,
) {
    let base_r = (asphalt_w * 0.35).max(4.0);
    let handle_r = (base_r * map.pit_edit_zoom.max(1.0).sqrt()).max(8.0);
    let colors = [
        (
            Rgba::rgb(255, 210, 58),
            Rgba::rgb(255, 90, 90),
            Rgba::rgb(90, 160, 255),
        ),
        (
            Rgba::rgb(200, 240, 120),
            Rgba::rgb(70, 210, 170),
            Rgba::rgb(100, 200, 255),
        ),
    ];
    let active_lane2 = map.lane_is_2();
    for (lane_i, (entry_col, road_col, merge_col)) in colors.into_iter().enumerate() {
        let lane2 = lane_i == 1;
        let lane_u = if lane2 { 2_u8 } else { 1_u8 };
        let (entry, road, merge) = if lane2 {
            (&map.entry_pts_2, &map.road_pts_2, &map.merge_pts_2)
        } else {
            (&map.entry_pts, &map.road_pts, &map.merge_pts)
        };
        if entry.is_empty() && road.is_empty() && merge.is_empty() {
            continue;
        }
        let active = lane2 == active_lane2;
        let fade = |col: Rgba| -> Rgba {
            if active {
                col
            } else {
                col.with_alpha(170)
            }
        };
        let entry_col = fade(entry_col);
        let road_col = fade(road_col);
        let merge_col = fade(merge_col);
        let road_w = if active { 3.5_f32 } else { 2.5_f32 };
        let blend_w = if active { 3.0_f32 } else { 2.0_f32 };

        let stroke_poly = |c: &mut Canvas, pts: &[(f32, f32)], col: Rgba, width: f32| {
            if pts.len() < 2 {
                return;
            }
            let s: Vec<(f32, f32)> = pts
                .iter()
                .map(|&(x, y)| {
                    let (mx, my) = emap::model_point(x, y, mirror, rot);
                    xform.map_xy(mx, my)
                })
                .collect();
            c.polyline(&s, col, width, false);
        };
        stroke_poly(c, entry, entry_col, blend_w);
        stroke_poly(c, road, road_col, road_w);
        stroke_poly(c, merge, merge_col, blend_w);

        let has_joint = map.has_joint(lane2);
        let has_entry_joint = map.has_entry_joint(lane2);
        let phases: [(&str, u8, &[(f32, f32)], Rgba); 3] = [
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
                let (mx, my) = emap::model_point(nx, ny, mirror, rot);
                let (px, py) = xform.map_xy(mx, my);
                let dragging = map.pit_drag == Some((lane_u, phase_code, idx));
                let fill = if dragging || active {
                    col
                } else {
                    col.with_alpha(170)
                };
                c.circle(px, py, handle_r, fill, true);
                c.circle_ex(
                    px,
                    py,
                    handle_r,
                    Rgba::rgb(
                        col.r.saturating_mul(3) / 4,
                        col.g.saturating_mul(3) / 4,
                        col.b.saturating_mul(3) / 4,
                    ),
                    false,
                    1.5,
                );
            }
        }
        if has_entry_joint {
            if let Some(&(nx, ny)) = entry.last() {
                let (mx, my) = emap::model_point(nx, ny, mirror, rot);
                let (px, py) = xform.map_xy(mx, my);
                let jcol = Rgba::rgb(
                    entry_col.r.saturating_add(20),
                    entry_col.g.saturating_add(20),
                    entry_col.b.saturating_add(10),
                );
                let dragging = map.pit_drag == Some((lane_u, 4, 0));
                let fill = if dragging || active {
                    jcol
                } else {
                    jcol.with_alpha(200)
                };
                c.circle(px, py, handle_r, fill, true);
                c.circle_ex(px, py, handle_r, jcol, false, 1.5);
            }
        }
        if has_joint {
            if let Some(&(nx, ny)) = road.last() {
                let (mx, my) = emap::model_point(nx, ny, mirror, rot);
                let (px, py) = xform.map_xy(mx, my);
                let jcol = if lane2 {
                    Rgba::rgb(120, 220, 180)
                } else {
                    Rgba::rgb(255, 170, 50)
                };
                let dragging = map.pit_drag == Some((lane_u, 3, 0));
                let fill = if dragging || active {
                    jcol
                } else {
                    jcol.with_alpha(200)
                };
                c.circle(px, py, handle_r, fill, true);
                c.circle_ex(px, py, handle_r, jcol, false, 1.5);
            }
        }
    }
}
