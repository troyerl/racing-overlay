//! Shared mutable overlay state (config, telemetry, map authoring, layout).

use crate::config::{default_geom, ConfigContext, OverlayConfig, WIDGET_KEYS};
use crate::paths;
use crate::telemetry::TelemetryFrame;
use crate::track_path::{CornerMark, PitLane};
use parking_lot::RwLock;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

/// Per-car screen-space ease state (Python track_map `_car_anim` screen pts).
#[derive(Debug, Clone, Copy)]
pub struct CarScreenAnim {
    pub x: f32,
    pub y: f32,
    /// Packed motion key: mode (pct=0/route=1) | on_route<<1 | on_pit<<2.
    pub key: u8,
}

/// Predict-to-now lap-% animation between discrete telem samples.
#[derive(Debug, Clone, Copy)]
pub struct CarPctAnim {
    pub pct: f32,
    /// Lap fractions per second (extrapolate between telem updates).
    pub vel: f32,
    pub last_telem: f32,
    pub last_telem_secs: f64,
}

#[derive(Debug, Clone)]
pub struct MapAuthoring {
    pub pit_edit: bool,
    pub phase: String,
    pub lane: String,
    pub corner_edit: bool,
    pub sf_edit: bool,
    pub interactive: bool,
    /// Draft polylines for Track Scan pit authoring (lane 1).
    pub entry_pts: Vec<(f32, f32)>,
    pub road_pts: Vec<(f32, f32)>,
    pub merge_pts: Vec<(f32, f32)>,
    /// Draft polylines for lane 2.
    pub entry_pts_2: Vec<(f32, f32)>,
    pub road_pts_2: Vec<(f32, f32)>,
    pub merge_pts_2: Vec<(f32, f32)>,
    pub pit_speed_ms: f64,
    pub pit_lane_speed_pct: f64,
    pub num_turns: i32,
    pub alias_ids: Vec<i32>,
    /// Cached track polyline for map MVP (`None` = try load / oval fallback).
    pub cached_track_id: Option<i32>,
    pub cached_path: Vec<(f32, f32)>,
    pub cached_track_name: String,
    pub cached_start_finish: f32,
    pub cached_pit_out_pct: Option<f32>,
    pub cached_pit: PitLane,
    pub cached_pit2: PitLane,
    pub cached_drs_zones: Vec<(f32, f32)>,
    pub cached_p2p_zones: Vec<(f32, f32)>,
    /// Corner markers from track JSON / authoring.
    pub cached_corners: Vec<CornerMark>,
    /// Corner being dragged in edit mode (`None` = none).
    pub drag_corner: Option<usize>,
    /// Pit-edit view: zoom factor (1 = fit).
    pub pit_edit_zoom: f32,
    /// Pit-edit view: screen-space pan (pixels).
    pub pit_edit_pan: (f32, f32),
    /// Active handle drag: (lane 1|2, phase code, idx).
    /// Phase: 0=entry 1=road 2=merge 3=joint 4=entry_joint.
    pub pit_drag: Option<(u8, u8, usize)>,
    /// Hold-before-switch state for ahead/behind/leader markers.
    pub marker_hold: crate::map_markers::HoldStates,
    /// Predict-to-now lap_dist_pct per car_idx.
    pub car_anim: HashMap<i32, CarPctAnim>,
    /// Screen-space eased car dots (Python `_car_anim` screen pts).
    pub car_screen: HashMap<i32, CarScreenAnim>,
    /// Wall/egui time of last map paint (for car easing dt; not SessionTime).
    pub last_paint_secs: f64,
    /// Hold car on pit route after OnPitRoad clears until past pit_out.
    pub pit_route_latch: HashMap<i32, bool>,
    /// Previous OnPitRoad per car (for exit edge).
    pub pit_prev_on: HashMap<i32, bool>,
    /// Lap % when OnPitRoad fell (schematic exit placement).
    pub pit_exit_latch: HashMap<i32, f32>,
    /// Seed route latches once after a track load (Python `_seed_pit_latches`).
    pub pit_latch_seed_pending: bool,
}

/// Pit-edit zoom clamp (match Python `_PIT_EDIT_ZOOM_*`).
pub const PIT_EDIT_ZOOM_MIN: f32 = 0.5;
pub const PIT_EDIT_ZOOM_MAX: f32 = 12.0;

impl Default for MapAuthoring {
    fn default() -> Self {
        Self {
            pit_edit: false,
            phase: String::new(),
            lane: String::new(),
            corner_edit: false,
            sf_edit: false,
            interactive: false,
            entry_pts: Vec::new(),
            road_pts: Vec::new(),
            merge_pts: Vec::new(),
            entry_pts_2: Vec::new(),
            road_pts_2: Vec::new(),
            merge_pts_2: Vec::new(),
            pit_speed_ms: 0.0,
            pit_lane_speed_pct: 1.0,
            num_turns: 0,
            alias_ids: Vec::new(),
            cached_track_id: None,
            cached_path: Vec::new(),
            cached_track_name: String::new(),
            cached_start_finish: 0.0,
            cached_pit_out_pct: None,
            cached_pit: PitLane::default(),
            cached_pit2: PitLane::default(),
            cached_drs_zones: Vec::new(),
            cached_p2p_zones: Vec::new(),
            cached_corners: Vec::new(),
            drag_corner: None,
            pit_edit_zoom: 1.0,
            pit_edit_pan: (0.0, 0.0),
            pit_drag: None,
            marker_hold: crate::map_markers::fresh_hold_states(),
            car_anim: HashMap::new(),
            car_screen: HashMap::new(),
            last_paint_secs: 0.0,
            pit_route_latch: HashMap::new(),
            pit_prev_on: HashMap::new(),
            pit_exit_latch: HashMap::new(),
            pit_latch_seed_pending: false,
        }
    }
}

impl MapAuthoring {
    pub fn phase_key(&self) -> &str {
        if self.phase.is_empty() {
            "road"
        } else {
            self.phase.as_str()
        }
    }

    pub fn lane_is_2(&self) -> bool {
        matches!(self.lane.as_str(), "2" | "secondary")
    }

    pub fn active_pts_mut(&mut self) -> &mut Vec<(f32, f32)> {
        let lane2 = self.lane_is_2();
        match (self.phase_key(), lane2) {
            ("entry", false) => &mut self.entry_pts,
            ("merge", false) => &mut self.merge_pts,
            (_, false) => &mut self.road_pts,
            ("entry", true) => &mut self.entry_pts_2,
            ("merge", true) => &mut self.merge_pts_2,
            (_, true) => &mut self.road_pts_2,
        }
    }

    pub fn clear_phase(&mut self, phase: &str, lane2: Option<bool>) {
        let lanes: &[bool] = match lane2 {
            Some(true) => &[true],
            Some(false) => &[false],
            None => {
                if phase == "all" {
                    &[false, true]
                } else if self.lane_is_2() {
                    &[true]
                } else {
                    &[false]
                }
            }
        };
        for &l2 in lanes {
            let (entry, road, merge) = if l2 {
                (
                    &mut self.entry_pts_2,
                    &mut self.road_pts_2,
                    &mut self.merge_pts_2,
                )
            } else {
                (&mut self.entry_pts, &mut self.road_pts, &mut self.merge_pts)
            };
            match phase {
                "entry" => entry.clear(),
                "merge" => merge.clear(),
                "road" | "pit" | "pit_road" => road.clear(),
                "all" => {
                    entry.clear();
                    road.clear();
                    merge.clear();
                }
                _ => road.clear(),
            }
        }
    }

    pub fn load_pit_from_cache(&mut self, force: bool) -> bool {
        let has_draft = self.entry_pts.len() >= 2
            || self.road_pts.len() >= 2
            || self.merge_pts.len() >= 2
            || self.entry_pts_2.len() >= 2
            || self.road_pts_2.len() >= 2
            || self.merge_pts_2.len() >= 2;
        if has_draft && !force {
            return true;
        }
        let mut loaded = false;
        for (lane2, pit) in [
            (false, &self.cached_pit.clone()),
            (true, &self.cached_pit2.clone()),
        ] {
            let mut entry = pit.entry.clone();
            if entry.len() >= 2 {
                let (a, b) = (entry[0], *entry.last().unwrap());
                if (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6 {
                    entry.clear();
                }
            }
            let road = pit.path.clone();
            let merge = pit.exit.clone();
            if road.len() < 2 && merge.len() < 2 && entry.len() < 2 {
                continue;
            }
            if lane2 {
                self.entry_pts_2 = entry;
                self.road_pts_2 = road;
                self.merge_pts_2 = merge;
            } else {
                self.entry_pts = entry;
                self.road_pts = road;
                self.merge_pts = merge;
            }
            self.sync_joints(lane2);
            loaded = true;
        }
        loaded
    }

    /// Drop cached track geometry so the next paint reloads from disk.
    pub fn invalidate_track_cache(&mut self) {
        self.cached_track_id = None;
        self.cached_path.clear();
        self.cached_track_name.clear();
        self.cached_start_finish = 0.0;
        self.cached_pit_out_pct = None;
        self.cached_pit = PitLane::default();
        self.cached_pit2 = PitLane::default();
        self.cached_drs_zones.clear();
        self.cached_p2p_zones.clear();
        self.cached_corners.clear();
        self.drag_corner = None;
    }

    pub fn reset_pit_edit_view(&mut self) {
        self.pit_edit_zoom = 1.0;
        self.pit_edit_pan = (0.0, 0.0);
        self.pit_drag = None;
    }

    fn bufs_mut(
        &mut self,
        lane2: bool,
    ) -> (
        &mut Vec<(f32, f32)>,
        &mut Vec<(f32, f32)>,
        &mut Vec<(f32, f32)>,
    ) {
        if lane2 {
            (
                &mut self.entry_pts_2,
                &mut self.road_pts_2,
                &mut self.merge_pts_2,
            )
        } else {
            (&mut self.entry_pts, &mut self.road_pts, &mut self.merge_pts)
        }
    }

    fn pts_coincide(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-5 && (a.1 - b.1).abs() < 1e-5
    }

    pub fn has_joint(&self, lane2: bool) -> bool {
        let (road, merge) = if lane2 {
            (&self.road_pts_2, &self.merge_pts_2)
        } else {
            (&self.road_pts, &self.merge_pts)
        };
        matches!((road.last(), merge.first()), (Some(&r), Some(&m)) if Self::pts_coincide(r, m))
    }

    pub fn has_entry_joint(&self, lane2: bool) -> bool {
        let (entry, road) = if lane2 {
            (&self.entry_pts_2, &self.road_pts_2)
        } else {
            (&self.entry_pts, &self.road_pts)
        };
        matches!((entry.last(), road.first()), (Some(&e), Some(&r)) if Self::pts_coincide(e, r))
    }

    /// Keep merge start tied to road end (Python `_sync_pit_joint`).
    pub fn sync_joints(&mut self, lane2: bool) {
        let (_, road, merge) = self.bufs_mut(lane2);
        if let (Some(r), Some(m0)) = (road.last().copied(), merge.first_mut()) {
            *m0 = r;
        }
    }

    /// Seed road start from entry end when switching to road with empty road.
    pub fn seed_road_from_entry(&mut self, lane2: bool) {
        let (entry, road, _) = self.bufs_mut(lane2);
        if road.is_empty() {
            if let Some(&pt) = entry.last() {
                road.push(pt);
            }
        }
    }

    /// Append a click in the active phase (Python `_append_pit_edit_at`).
    pub fn append_pit_edit_at(&mut self, x: f32, y: f32) {
        let lane2 = self.lane_is_2();
        let phase = self.phase_key().to_string();
        let has_ej = self.has_entry_joint(lane2);
        let (entry, road, merge) = self.bufs_mut(lane2);
        match phase.as_str() {
            "entry" => {
                if !road.is_empty() && !entry.is_empty() && has_ej {
                    let joint_idx = entry.len() - 1;
                    entry.insert(joint_idx, (x, y));
                } else if !road.is_empty() && entry.is_empty() {
                    entry.push((x, y));
                    entry.push(road[0]);
                } else {
                    entry.push((x, y));
                    if !road.is_empty() {
                        entry.push(road[0]);
                    }
                }
            }
            "merge" => {
                if merge.is_empty() {
                    if let Some(&r) = road.last() {
                        merge.push(r);
                    }
                }
                merge.push((x, y));
            }
            _ => {
                if road.is_empty() {
                    if let Some(&e) = entry.last() {
                        road.push(e);
                    }
                }
                road.push((x, y));
            }
        }
        if phase == "road" || phase == "pit" || phase == "pit_road" {
            self.sync_joints(lane2);
        }
    }

    /// Move one handle; joint endpoints stay linked (Python `_set_pit_edit_point`).
    pub fn set_point_with_joints(&mut self, lane2: bool, phase: u8, idx: usize, x: f32, y: f32) {
        let (entry, road, merge) = self.bufs_mut(lane2);
        match phase {
            3 => {
                // joint = road end + merge start
                if let Some(r) = road.last_mut() {
                    *r = (x, y);
                }
                if let Some(m) = merge.first_mut() {
                    *m = (x, y);
                }
            }
            4 => {
                // entry_joint = entry end + road start
                if let Some(e) = entry.last_mut() {
                    *e = (x, y);
                }
                if let Some(r) = road.first_mut() {
                    *r = (x, y);
                }
            }
            0 => {
                if idx < entry.len() {
                    entry[idx] = (x, y);
                    if idx + 1 == entry.len() {
                        if let Some(r) = road.first_mut() {
                            *r = (x, y);
                        }
                    }
                }
            }
            1 => {
                if idx < road.len() {
                    road[idx] = (x, y);
                    if idx == 0 {
                        if let Some(e) = entry.last_mut() {
                            *e = (x, y);
                        }
                    }
                    if idx + 1 == road.len() {
                        if let Some(m) = merge.first_mut() {
                            *m = (x, y);
                        }
                    }
                }
            }
            2 => {
                if idx < merge.len() {
                    merge[idx] = (x, y);
                    if idx == 0 {
                        if let Some(r) = road.last_mut() {
                            *r = (x, y);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    pub fn clamp_pit_zoom(z: f32) -> f32 {
        z.clamp(PIT_EDIT_ZOOM_MIN, PIT_EDIT_ZOOM_MAX)
    }
}

#[cfg(test)]
mod pit_edit_tests {
    use super::*;

    #[test]
    fn joint_drag_moves_road_end_and_merge_start() {
        let mut m = MapAuthoring::default();
        m.road_pts = vec![(0.0, 0.0), (1.0, 0.0)];
        m.merge_pts = vec![(1.0, 0.0), (2.0, 1.0)];
        m.set_point_with_joints(false, 3, 0, 5.0, 6.0);
        assert_eq!(*m.road_pts.last().unwrap(), (5.0, 6.0));
        assert_eq!(*m.merge_pts.first().unwrap(), (5.0, 6.0));
    }

    #[test]
    fn entry_joint_drag_moves_entry_end_and_road_start() {
        let mut m = MapAuthoring::default();
        m.entry_pts = vec![(0.0, 0.0), (1.0, 1.0)];
        m.road_pts = vec![(1.0, 1.0), (2.0, 2.0)];
        m.set_point_with_joints(false, 4, 0, 3.0, 4.0);
        assert_eq!(*m.entry_pts.last().unwrap(), (3.0, 4.0));
        assert_eq!(*m.road_pts.first().unwrap(), (3.0, 4.0));
    }

    #[test]
    fn seed_road_and_reset_view() {
        let mut m = MapAuthoring::default();
        m.entry_pts = vec![(1.0, 2.0)];
        m.phase = "road".into();
        m.seed_road_from_entry(false);
        assert_eq!(m.road_pts, vec![(1.0, 2.0)]);
        m.pit_edit_zoom = 2.5;
        m.pit_edit_pan = (10.0, 20.0);
        m.pit_drag = Some((1, 1, 0));
        m.reset_pit_edit_view();
        assert_eq!(m.pit_edit_zoom, 1.0);
        assert_eq!(m.pit_edit_pan, (0.0, 0.0));
        assert!(m.pit_drag.is_none());
        assert_eq!(MapAuthoring::clamp_pit_zoom(0.1), PIT_EDIT_ZOOM_MIN);
        assert_eq!(MapAuthoring::clamp_pit_zoom(99.0), PIT_EDIT_ZOOM_MAX);
    }
}

#[derive(Debug, Clone)]
pub struct PanelLayout {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            x: 100,
            y: 100,
            w: 280,
            h: 160,
        }
    }
}

fn layout_from_value(value: &Value) -> HashMap<String, PanelLayout> {
    let mut layout = HashMap::new();
    if let Some(map) = value.as_object() {
        for (k, v) in map {
            if let Some(arr) = v.as_array() {
                if arr.len() >= 4 {
                    layout.insert(
                        k.clone(),
                        PanelLayout {
                            x: arr[0].as_i64().unwrap_or(0) as i32,
                            y: arr[1].as_i64().unwrap_or(0) as i32,
                            w: arr[2].as_i64().unwrap_or(280) as i32,
                            h: arr[3].as_i64().unwrap_or(160) as i32,
                        },
                    );
                }
            } else if let Some(obj) = v.as_object() {
                layout.insert(
                    k.clone(),
                    PanelLayout {
                        x: obj.get("x").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
                        y: obj.get("y").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
                        w: obj.get("w").and_then(|x| x.as_i64()).unwrap_or(280) as i32,
                        h: obj.get("h").and_then(|x| x.as_i64()).unwrap_or(160) as i32,
                    },
                );
            }
        }
    }
    ensure_default_layouts(&mut layout);
    layout
}

/// Merge legacy `overlay_layout.json` into a race layout (not garage).
fn merge_overlay_layout_json(layout: &mut HashMap<String, PanelLayout>) {
    let Ok(text) = fs::read_to_string(paths::layout_path()) else {
        return;
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
        return;
    };
    for (k, v) in map {
        if let Some(arr) = v.as_array() {
            if arr.len() >= 4 {
                layout.insert(
                    k,
                    PanelLayout {
                        x: arr[0].as_i64().unwrap_or(0) as i32,
                        y: arr[1].as_i64().unwrap_or(0) as i32,
                        w: arr[2].as_i64().unwrap_or(280) as i32,
                        h: arr[3].as_i64().unwrap_or(160) as i32,
                    },
                );
            }
        } else if let Some(obj) = v.as_object() {
            layout.insert(
                k,
                PanelLayout {
                    x: obj.get("x").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
                    y: obj.get("y").and_then(|x| x.as_i64()).unwrap_or(0) as i32,
                    w: obj.get("w").and_then(|x| x.as_i64()).unwrap_or(280) as i32,
                    h: obj.get("h").and_then(|x| x.as_i64()).unwrap_or(160) as i32,
                },
            );
        }
    }
}

/// Preset layout for `context`, plus race-only legacy JSON merge (startup parity).
fn load_layout_for_context(
    config: &OverlayConfig,
    context: ConfigContext,
) -> HashMap<String, PanelLayout> {
    let mut layout = layout_from_value(&config.active_layout_doc(context));
    if context == ConfigContext::Race {
        merge_overlay_layout_json(&mut layout);
    }
    ensure_default_layouts(&mut layout);
    layout
}

fn ensure_default_layouts(layout: &mut HashMap<String, PanelLayout>) {
    for key in WIDGET_KEYS {
        layout.entry((*key).to_string()).or_insert_with(|| {
            let (x, y, w, h) = default_geom(key);
            PanelLayout { x, y, w, h }
        });
    }
}

pub struct SharedState {
    pub config: Arc<OverlayConfig>,
    pub frame: Arc<TelemetryFrame>,
    pub running: bool,
    pub edit_mode: bool,
    pub click_through: bool,
    pub demo: bool,
    pub layout: HashMap<String, PanelLayout>,
    pub config_context: ConfigContext,
    pub preview_context: Option<ConfigContext>,
    pub settings_apply_live: bool,
    pub settings_auto_save: bool,
    pub quit_requested: bool,
    pub map: MapAuthoring,
    /// In-overlay Settings window open.
    pub settings_open: bool,
    /// Active Settings nav section key.
    pub settings_section: String,
    /// Launch-time update notice: (version, download_url). Consumed by Settings UI.
    pub pending_update: Option<(String, Option<String>)>,
}

impl SharedState {
    pub fn new(config: OverlayConfig, click_through: bool, demo: bool) -> Self {
        let layout = load_layout_for_context(&config, ConfigContext::Race);
        Self {
            config: Arc::new(config),
            frame: Arc::new(TelemetryFrame::default()),
            running: true,
            edit_mode: !click_through,
            click_through,
            demo,
            layout,
            config_context: ConfigContext::Race,
            preview_context: None,
            settings_apply_live: true,
            settings_auto_save: true,
            quit_requested: false,
            map: MapAuthoring {
                phase: "road".into(),
                lane: "primary".into(),
                pit_speed_ms: 22.0,
                pit_lane_speed_pct: 1.0,
                ..Default::default()
            },
            settings_open: false,
            settings_section: "__general__".into(),
            pending_update: None,
        }
    }

    pub fn save_layout(&self) {
        let mut map = serde_json::Map::new();
        for (k, g) in &self.layout {
            map.insert(k.clone(), json!([g.x, g.y, g.w, g.h]));
        }
        let _ = fs::write(
            paths::layout_path(),
            serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default(),
        );
    }

    pub fn layout_doc(&self) -> Value {
        let mut map = serde_json::Map::new();
        for (k, g) in &self.layout {
            map.insert(k.clone(), json!([g.x, g.y, g.w, g.h]));
        }
        Value::Object(map)
    }

    pub fn save_layout_to_preset(&mut self) {
        let context = self.effective_context();
        let layout = self.layout_doc();
        let cfg = Arc::make_mut(&mut self.config);
        cfg.store_active_layout_doc(context, layout);
        let _ = cfg.save_doc();
    }

    pub fn effective_context(&self) -> ConfigContext {
        self.preview_context.unwrap_or(self.config_context)
    }

    pub fn set_preview_context(&mut self, context: Option<ConfigContext>) {
        self.preview_context = context;
        self.apply_effective_context();
    }

    pub fn set_config_context(&mut self, context: ConfigContext) {
        self.config_context = context;
        if self.preview_context.is_some() && self.preview_context != Some(context) {
            self.preview_context = None;
        }
        self.apply_effective_context();
    }

    pub fn apply_effective_context(&mut self) {
        let context = self.effective_context();
        let cfg = Arc::make_mut(&mut self.config);
        cfg.apply_context(context);
        self.layout = load_layout_for_context(cfg, context);
    }

    /// Resolve loop / pit geometry for Track Scan IPC (may load track JSON).
    fn authoring_loop_and_saved_pit(&self) -> (Vec<(f32, f32)>, bool, bool) {
        if self.map.cached_path.len() >= 3 {
            return (
                self.map.cached_path.clone(),
                self.map.cached_pit.path.len() >= 2,
                self.map.cached_pit2.path.len() >= 2,
            );
        }
        if let Some(id) = self.frame.track_id {
            if let Some(tp) = crate::track_path::load_for_track_id(id) {
                return (tp.points, tp.pit.path.len() >= 2, tp.pit2.path.len() >= 2);
            }
            // Paint falls back to an oval when TrackID has no JSON.
            return (crate::track_path::oval_path(64), false, false);
        }
        (Vec::new(), false, false)
    }

    fn pts_json(pts: &[(f32, f32)]) -> Value {
        Value::Array(
            pts.iter()
                .map(|(x, y)| json!([*x as f64, *y as f64]))
                .collect(),
        )
    }

    pub fn map_state_json(&self) -> Value {
        let (loop_pts, has_saved_pit, has_saved_pit_2) = self.authoring_loop_and_saved_pit();
        let has_loop = loop_pts.len() >= 3;
        // Prefer HTML-import / authoring cache over live/demo session TrackID.
        let tid = self.map.cached_track_id.or(self.frame.track_id);
        let track_name = if !self.map.cached_track_name.is_empty() {
            Some(self.map.cached_track_name.clone())
        } else {
            self.frame.track_name.clone()
        };
        let can_author = tid.is_some() && has_loop;
        let lane_num = match self.map.lane.as_str() {
            "2" | "secondary" => 2,
            _ => 1,
        };
        let phase = self.map.phase_key();
        let entry_count = self.map.entry_pts.len();
        let road_count = self.map.road_pts.len();
        let merge_count = self.map.merge_pts.len();
        let entry_count_2 = self.map.entry_pts_2.len();
        let road_count_2 = self.map.road_pts_2.len();
        let merge_count_2 = self.map.merge_pts_2.len();
        let n_pts = if lane_num == 2 {
            match phase {
                "entry" => entry_count_2,
                "merge" => merge_count_2,
                _ => road_count_2,
            }
        } else {
            match phase {
                "entry" => entry_count,
                "merge" => merge_count,
                _ => road_count,
            }
        };
        let corners = Value::Array(
            self.map
                .cached_corners
                .iter()
                .map(|c| {
                    let mut o = serde_json::Map::new();
                    o.insert("pct".into(), json!(c.pct));
                    o.insert("label".into(), json!(c.label));
                    o.insert("ox".into(), json!(c.ox));
                    o.insert("oy".into(), json!(c.oy));
                    Value::Object(o)
                })
                .collect(),
        );
        let mut m = serde_json::Map::new();
        m.insert("pit_edit".into(), json!(self.map.pit_edit));
        m.insert("pit_edit_mode".into(), json!(self.map.pit_edit));
        m.insert("phase".into(), json!(phase));
        m.insert("pit_edit_phase".into(), json!(phase));
        m.insert("lane".into(), json!(self.map.lane));
        m.insert("pit_edit_lane".into(), json!(lane_num));
        m.insert("corner_edit".into(), json!(self.map.corner_edit));
        m.insert("sf_edit".into(), json!(self.map.sf_edit));
        m.insert("interactive".into(), json!(self.map.interactive));
        m.insert("pit_points".into(), json!(n_pts));
        m.insert("entry_count".into(), json!(entry_count));
        m.insert("road_count".into(), json!(road_count));
        m.insert("merge_count".into(), json!(merge_count));
        m.insert("entry_count_2".into(), json!(entry_count_2));
        m.insert("road_count_2".into(), json!(road_count_2));
        m.insert("merge_count_2".into(), json!(merge_count_2));
        m.insert("entry_points".into(), Self::pts_json(&self.map.entry_pts));
        m.insert("road_points".into(), Self::pts_json(&self.map.road_pts));
        m.insert("merge_points".into(), Self::pts_json(&self.map.merge_pts));
        m.insert(
            "entry_points_2".into(),
            Self::pts_json(&self.map.entry_pts_2),
        );
        m.insert("road_points_2".into(), Self::pts_json(&self.map.road_pts_2));
        m.insert(
            "merge_points_2".into(),
            Self::pts_json(&self.map.merge_pts_2),
        );
        m.insert("loop_points".into(), Self::pts_json(&loop_pts));
        m.insert("start_finish".into(), json!(self.map.cached_start_finish));
        m.insert("corners".into(), corners);
        m.insert("has_loop".into(), json!(has_loop));
        m.insert("has_saved_pit".into(), json!(has_saved_pit));
        m.insert("has_saved_pit_2".into(), json!(has_saved_pit_2));
        m.insert("pit_speed_ms".into(), json!(self.map.pit_speed_ms));
        m.insert(
            "pit_lane_speed_pct".into(),
            json!(self.map.pit_lane_speed_pct),
        );
        m.insert("num_turns".into(), json!(self.map.num_turns));
        m.insert("alias_ids".into(), json!(self.map.alias_ids));
        m.insert("alias_track_ids".into(), json!(self.map.alias_ids));
        m.insert("track_id".into(), json!(tid));
        m.insert("track_name".into(), json!(track_name));
        m.insert("authoring_track_id".into(), json!(tid));
        m.insert("canonical_track_id".into(), json!(tid));
        m.insert("in_sim".into(), json!(!self.demo && tid.is_some()));
        m.insert("demo".into(), json!(self.demo));
        m.insert("has_track".into(), json!(can_author && !self.demo));
        m.insert("can_author_map".into(), json!(can_author));
        m.insert("corner_count".into(), json!(self.map.cached_corners.len()));
        m.insert("has_pit_geometry".into(), json!(has_saved_pit));
        Value::Object(m)
    }
}

pub type StateHandle = Arc<RwLock<SharedState>>;

pub fn new_state(config: OverlayConfig, click_through: bool, demo: bool) -> StateHandle {
    Arc::new(RwLock::new(SharedState::new(config, click_through, demo)))
}
