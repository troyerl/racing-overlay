//! Shared JSON-RPC protocol for GridGlance overlay (Rust) ↔ settings (Python).
//!
//! Wire format: newline-delimited JSON objects over a local TCP socket
//! (`127.0.0.1:19847` by default). Each request has `id`, `method`, `params`;
//! each response has `id`, `ok`, and either `result` or `error`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Default localhost port for overlay IPC.
pub const DEFAULT_IPC_PORT: u16 = 19847;

/// Protocol version advertised in `ping`.
pub const PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, msg: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(msg.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingResult {
    pub version: u32,
    pub backend: String,
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConfigApplyParams {
    /// Sparse or full CFG dict matching Python `config.CFG` shape.
    #[serde(default)]
    pub cfg: Value,
    /// Optional generation from the writer (monotonic).
    #[serde(default)]
    pub generation: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OverlayModeParams {
    #[serde(default)]
    pub edit_mode: Option<bool>,
    #[serde(default)]
    pub running: Option<bool>,
    #[serde(default)]
    pub click_through: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutGeom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayoutSetParams {
    pub key: String,
    pub geom: LayoutGeom,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapPitEditParams {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_phase")]
    pub phase: String,
    #[serde(default = "default_lane")]
    pub lane: String,
}

fn default_phase() -> String {
    "road".into()
}
fn default_lane() -> String {
    "primary".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapBoolParams {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapSpeedParams {
    pub speed_ms: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapLaneSpeedParams {
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapNumTurnsParams {
    pub n: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapClearPitParams {
    /// When set, clear only that phase (`entry` / `road` / `merge` / `all`).
    /// When omitted, clear the currently active phase.
    #[serde(default)]
    pub phase: Option<String>,
    /// `"primary"` / `"secondary"` / `"1"` / `"2"`. Omit = active lane (or both for `all`).
    #[serde(default)]
    pub lane: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapLoadPitParams {
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapSetLoopParams {
    pub track_id: i32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub points: Vec<[f64; 2]>,
    #[serde(default)]
    pub start_finish: f64,
    #[serde(default)]
    pub corners: Vec<Value>,
    #[serde(default)]
    pub num_turns: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapSetCornersParams {
    #[serde(default)]
    pub corners: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapSetStartFinishParams {
    pub pct: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MapAliasIdsParams {
    #[serde(default)]
    pub ids: Vec<i32>,
}

/// Well-known method names.
pub mod methods {
    pub const PING: &str = "ping";
    pub const CONFIG_RELOAD: &str = "config.reload";
    pub const CONFIG_APPLY: &str = "config.apply";
    pub const OVERLAY_START: &str = "overlay.start";
    pub const OVERLAY_STOP: &str = "overlay.stop";
    pub const OVERLAY_SET_EDIT_MODE: &str = "overlay.set_edit_mode";
    pub const LAYOUT_GET: &str = "layout.get";
    pub const LAYOUT_SET: &str = "layout.set";
    pub const MAP_SET_PIT_EDIT: &str = "map.set_pit_edit";
    pub const MAP_UNDO_POINT: &str = "map.undo_point";
    pub const MAP_CLEAR_PIT: &str = "map.clear_pit";
    pub const MAP_RESET_VIEW: &str = "map.reset_view";
    pub const MAP_SAVE_PIT: &str = "map.save_pit";
    pub const MAP_SAVE_LOOP: &str = "map.save_loop";
    pub const MAP_SET_INTERACTIVE: &str = "map.set_interactive";
    pub const MAP_GET_STATE: &str = "map.get_state";
    pub const MAP_SET_CORNER_EDIT: &str = "map.set_corner_edit";
    pub const MAP_SET_SF_EDIT: &str = "map.set_sf_edit";
    pub const MAP_SET_PIT_SPEED: &str = "map.set_pit_speed";
    pub const MAP_SET_PIT_LANE_SPEED: &str = "map.set_pit_lane_speed";
    pub const MAP_SET_NUM_TURNS: &str = "map.set_num_turns";
    pub const MAP_SET_ALIAS_IDS: &str = "map.set_alias_ids";
    pub const MAP_INVALIDATE_TRACK: &str = "map.invalidate_track";
    pub const MAP_LOAD_PIT: &str = "map.load_pit";
    pub const MAP_SET_LOOP: &str = "map.set_loop";
    pub const MAP_SET_CORNERS: &str = "map.set_corners";
    pub const MAP_SET_START_FINISH: &str = "map.set_start_finish";
    pub const TRACK_AUTHORING_STATE: &str = "track.authoring_state";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_request() {
        let req = Request {
            id: 1,
            method: methods::PING.into(),
            params: Value::Null,
        };
        let s = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&s).unwrap();
        assert_eq!(back.method, methods::PING);
    }
}
