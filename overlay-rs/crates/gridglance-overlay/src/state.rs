//! Shared mutable overlay state (config, telemetry, map authoring, layout).

use crate::config::{default_geom, OverlayConfig, WIDGET_KEYS};
use crate::paths;
use crate::telemetry::TelemetryFrame;
use crate::track_path::PitLane;
use parking_lot::RwLock;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct MapAuthoring {
    pub pit_edit: bool,
    pub phase: String,
    pub lane: String,
    pub corner_edit: bool,
    pub sf_edit: bool,
    pub interactive: bool,
    pub pit_points: Vec<(f32, f32)>,
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
    /// Hold-before-switch state for ahead/behind/leader markers.
    pub marker_hold: crate::map_markers::HoldStates,
    /// Eased lap_dist_pct per car_idx for smooth map motion.
    pub car_anim: HashMap<i32, f32>,
    /// Session time of last map paint (for smoothing dt).
    pub last_paint_secs: f64,
}

impl Default for MapAuthoring {
    fn default() -> Self {
        Self {
            pit_edit: false,
            phase: String::new(),
            lane: String::new(),
            corner_edit: false,
            sf_edit: false,
            interactive: false,
            pit_points: Vec::new(),
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
            marker_hold: crate::map_markers::fresh_hold_states(),
            car_anim: HashMap::new(),
            last_paint_secs: 0.0,
        }
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

pub struct SharedState {
    pub config: Arc<OverlayConfig>,
    pub frame: Arc<TelemetryFrame>,
    pub running: bool,
    pub edit_mode: bool,
    pub click_through: bool,
    pub demo: bool,
    pub layout: HashMap<String, PanelLayout>,
    pub map: MapAuthoring,
}

impl SharedState {
    pub fn new(config: OverlayConfig, click_through: bool, demo: bool) -> Self {
        let mut layout = HashMap::new();
        // Load overlay_layout.json if present.
        if let Ok(text) = fs::read_to_string(paths::layout_path()) {
            if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) {
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
        }
        for key in WIDGET_KEYS {
            layout.entry((*key).to_string()).or_insert_with(|| {
                let (x, y, w, h) = default_geom(key);
                PanelLayout { x, y, w, h }
            });
        }
        // Merge layout from active preset if present.
        if let Some(presets) = config.doc.get("presets").and_then(|p| p.as_object()) {
            if let Some(preset) = presets.get(&config.active_preset) {
                if let Some(Value::Object(lay)) = preset.get("layout") {
                    for (k, v) in lay {
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
                        }
                    }
                }
            }
        }
        Self {
            config: Arc::new(config),
            frame: Arc::new(TelemetryFrame::default()),
            running: true,
            edit_mode: !click_through,
            click_through,
            demo,
            layout,
            map: MapAuthoring {
                phase: "road".into(),
                lane: "primary".into(),
                pit_speed_ms: 22.0,
                pit_lane_speed_pct: 1.0,
                ..Default::default()
            },
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

    /// Resolve loop / pit geometry for Track Scan IPC (may load track JSON).
    fn authoring_geom(&self) -> (usize, bool, bool) {
        if self.map.cached_path.len() >= 3 {
            return (
                self.map.cached_path.len(),
                self.map.cached_pit.path.len() >= 2,
                self.map.cached_pit2.path.len() >= 2,
            );
        }
        if let Some(id) = self.frame.track_id {
            if let Some(tp) = crate::track_path::load_for_track_id(id) {
                return (
                    tp.points.len(),
                    tp.pit.path.len() >= 2,
                    tp.pit2.path.len() >= 2,
                );
            }
            // Paint falls back to an oval when TrackID has no JSON.
            return (64, false, false);
        }
        (0, false, false)
    }

    pub fn map_state_json(&self) -> Value {
        let (loop_n, has_saved_pit, has_saved_pit_2) = self.authoring_geom();
        let has_loop = loop_n >= 3;
        let tid = self.frame.track_id;
        let has_geom = has_loop;
        let can_author = tid.is_some() && has_geom;
        let lane_num = match self.map.lane.as_str() {
            "2" | "secondary" => 2,
            _ => 1,
        };
        let phase = if self.map.phase.is_empty() {
            "road"
        } else {
            self.map.phase.as_str()
        };
        let n_pts = self.map.pit_points.len();
        let (entry_count, road_count, merge_count) = match phase {
            "entry" => (n_pts, 0, 0),
            "merge" => (0, 0, n_pts),
            _ => (0, n_pts, 0),
        };
        json!({
            "pit_edit": self.map.pit_edit,
            "pit_edit_mode": self.map.pit_edit,
            "phase": phase,
            "pit_edit_phase": phase,
            "lane": self.map.lane,
            "pit_edit_lane": lane_num,
            "corner_edit": self.map.corner_edit,
            "sf_edit": self.map.sf_edit,
            "interactive": self.map.interactive,
            "pit_points": n_pts,
            "entry_count": entry_count,
            "road_count": road_count,
            "merge_count": merge_count,
            "entry_count_2": 0,
            "road_count_2": 0,
            "merge_count_2": 0,
            "has_loop": has_loop,
            "has_saved_pit": has_saved_pit,
            "has_saved_pit_2": has_saved_pit_2,
            "pit_speed_ms": self.map.pit_speed_ms,
            "pit_lane_speed_pct": self.map.pit_lane_speed_pct,
            "num_turns": self.map.num_turns,
            "alias_ids": self.map.alias_ids,
            "alias_track_ids": self.map.alias_ids,
            "track_id": tid,
            "track_name": self.frame.track_name,
            "authoring_track_id": tid,
            "canonical_track_id": tid,
            "in_sim": !self.demo && tid.is_some(),
            "demo": self.demo,
            "has_track": can_author && !self.demo,
            "can_author_map": can_author,
            "corner_count": 0,
            "has_pit_geometry": has_saved_pit,
        })
    }
}

pub type StateHandle = Arc<RwLock<SharedState>>;

pub fn new_state(config: OverlayConfig, click_through: bool, demo: bool) -> StateHandle {
    Arc::new(RwLock::new(SharedState::new(config, click_through, demo)))
}
