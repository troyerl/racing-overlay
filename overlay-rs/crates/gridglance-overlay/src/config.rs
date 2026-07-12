//! Load / merge `overlay_config.json` (schema 2) into a live CFG map.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

use crate::paths;

/// Widget section keys owned by the Rust overlay.
pub const WIDGET_KEYS: &[&str] = &[
    "standings",
    "relative",
    "radar",
    "map",
    "dash",
    "laptime_log",
    "fuel_calc",
    "inputs",
    "delta_bar",
    "flags",
    "sector_timing",
    "lap_compare",
    "tire_panel",
    "pit_board",
    "weather_panel",
    "leaderboard_strip",
    "radio_tower",
    "ers_hybrid",
    "system_panel",
    "pit_advisor",
];

/// Default geometries (x, y, w, h) matching Python panel defaults loosely.
pub fn default_geom(key: &str) -> (i32, i32, i32, i32) {
    match key {
        "standings" => (40, 40, 420, 520),
        "relative" => (480, 40, 420, 360),
        "radar" => (920, 40, 220, 220),
        "map" => (920, 280, 360, 360),
        "dash" => (40, 580, 520, 160),
        "laptime_log" => (580, 580, 320, 220),
        "fuel_calc" => (40, 760, 280, 200),
        "inputs" => (340, 760, 280, 160),
        "delta_bar" => (640, 760, 280, 120),
        "flags" => (940, 660, 220, 100),
        "sector_timing" => (1160, 40, 280, 200),
        "lap_compare" => (1160, 260, 280, 220),
        "tire_panel" => (1160, 500, 280, 200),
        "pit_board" => (40, 980, 320, 140),
        "weather_panel" => (380, 980, 260, 180),
        "leaderboard_strip" => (660, 980, 480, 80),
        "radio_tower" => (1160, 720, 200, 200),
        "ers_hybrid" => (920, 780, 220, 140),
        "system_panel" => (1160, 940, 220, 180),
        "pit_advisor" => (660, 1080, 320, 160),
        _ => (100, 100, 280, 160),
    }
}

#[derive(Debug, Clone)]
pub struct OverlayConfig {
    /// Live merged CFG (same shape as Python `config.CFG`).
    pub cfg: Value,
    /// Raw schema-2 document (presets etc.).
    pub doc: Value,
    pub generation: u64,
    pub active_preset: String,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            cfg: default_cfg(),
            doc: json!({
                "schema": 2,
                "active_preset": "Default",
                "presets": {
                    "Default": { "config": {}, "layout": {}, "layout_garage": {} }
                }
            }),
            generation: 0,
            active_preset: "Default".into(),
        }
    }
}

impl OverlayConfig {
    pub fn load() -> Result<Self> {
        let path = paths::config_path();
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from(&path)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("read {}", path.display()))?;
        let doc: Value = serde_json::from_str(&text).context("parse overlay_config.json")?;
        let mut cfg = default_cfg();
        let active = doc
            .get("active_preset")
            .and_then(|v| v.as_str())
            .unwrap_or("Default")
            .to_string();
        if let Some(presets) = doc.get("presets").and_then(|p| p.as_object()) {
            if let Some(preset) = presets.get(&active) {
                if let Some(overrides) = preset.get("config") {
                    deep_merge(&mut cfg, overrides);
                }
            }
        } else if doc.get("schema").and_then(|s| s.as_u64()) != Some(2) {
            // Legacy flat config
            deep_merge(&mut cfg, &doc);
        }
        Ok(Self {
            cfg,
            doc,
            generation: 0,
            active_preset: active,
        })
    }

    pub fn section(&self, key: &str) -> &Value {
        self.cfg.get(key).unwrap_or(&Value::Null)
    }

    pub fn widget_shown(&self, key: &str) -> bool {
        self.section(key)
            .get("show")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    }

    pub fn apply_cfg_patch(&mut self, patch: &Value) {
        deep_merge(&mut self.cfg, patch);
        self.generation = self.generation.saturating_add(1);
    }

    pub fn color(&self, section: &str, key: &str, fallback: &str) -> egui::Color32 {
        let raw = self
            .section(section)
            .get("colors")
            .and_then(|c| c.get(key))
            .or_else(|| {
                // border aliases
                let alias = match key {
                    "border" => "panel_border",
                    "panel_border" => "border",
                    _ => "",
                };
                if alias.is_empty() {
                    None
                } else {
                    self.section(section).get("colors").and_then(|c| c.get(alias))
                }
            });
        parse_color(raw, fallback)
    }

    pub fn f64_key(&self, section: &str, key: &str, default: f64) -> f64 {
        self.section(section)
            .get(key)
            .and_then(|v| v.as_f64())
            .unwrap_or(default)
    }

    pub fn bool_key(&self, section: &str, key: &str, default: bool) -> bool {
        self.section(section)
            .get(key)
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    pub fn str_key(&self, section: &str, key: &str, default: &str) -> String {
        self.section(section)
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }

    pub fn global_str(&self, key: &str, default: &str) -> String {
        self.cfg
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }

    pub fn imperial_units(&self) -> bool {
        matches!(
            self.global_str("units", "metric").to_ascii_lowercase().as_str(),
            "imperial" | "us" | "mph"
        )
    }

    /// Global `text_scale` × per-section `text_scale` (Python `text_scale_for`).
    pub fn text_scale(&self, section: &str) -> f32 {
        let g = self
            .cfg
            .get("text_scale")
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32;
        let w = self.f64_key(section, "text_scale", 1.0) as f32;
        (g * w).max(0.1)
    }
}

pub fn parse_color(v: Option<&Value>, fallback: &str) -> egui::Color32 {
    match v {
        Some(Value::String(s)) => parse_color_str(s),
        Some(Value::Array(arr)) if arr.len() >= 3 => {
            let r = arr[0].as_u64().unwrap_or(255) as u8;
            let g = arr[1].as_u64().unwrap_or(0) as u8;
            let b = arr[2].as_u64().unwrap_or(255) as u8;
            let a = arr.get(3).and_then(|x| x.as_u64()).unwrap_or(255) as u8;
            egui::Color32::from_rgba_unmultiplied(r, g, b, a)
        }
        _ => parse_color_str(fallback),
    }
}

pub fn parse_color_str(s: &str) -> egui::Color32 {
    let s = s.trim();
    if let Some(rest) = s.strip_prefix('#') {
        let (rgb, a) = match rest.len() {
            3 => {
                let r = u8::from_str_radix(&rest[0..1].repeat(2), 16).unwrap_or(255);
                let g = u8::from_str_radix(&rest[1..2].repeat(2), 16).unwrap_or(0);
                let b = u8::from_str_radix(&rest[2..3].repeat(2), 16).unwrap_or(255);
                ([r, g, b], 255)
            }
            6 => {
                let r = u8::from_str_radix(&rest[0..2], 16).unwrap_or(255);
                let g = u8::from_str_radix(&rest[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&rest[4..6], 16).unwrap_or(255);
                ([r, g, b], 255)
            }
            8 => {
                let r = u8::from_str_radix(&rest[0..2], 16).unwrap_or(255);
                let g = u8::from_str_radix(&rest[2..4], 16).unwrap_or(0);
                let b = u8::from_str_radix(&rest[4..6], 16).unwrap_or(255);
                let a = u8::from_str_radix(&rest[6..8], 16).unwrap_or(255);
                ([r, g, b], a)
            }
            _ => ([255, 0, 255], 255),
        };
        return egui::Color32::from_rgba_unmultiplied(rgb[0], rgb[1], rgb[2], a);
    }
    if let Some(inner) = s.strip_prefix("rgba(").and_then(|x| x.strip_suffix(')')) {
        let parts: Vec<_> = inner.split(',').map(|p| p.trim()).collect();
        if parts.len() >= 3 {
            let r: u8 = parts[0].parse().unwrap_or(255);
            let g: u8 = parts[1].parse().unwrap_or(0);
            let b: u8 = parts[2].parse().unwrap_or(255);
            let a_f: f64 = parts.get(3).and_then(|p| p.parse().ok()).unwrap_or(1.0);
            let a = (a_f.clamp(0.0, 1.0) * 255.0).round() as u8;
            return egui::Color32::from_rgba_unmultiplied(r, g, b, a);
        }
    }
    egui::Color32::from_rgb(255, 0, 255)
}

fn deep_merge(dst: &mut Value, src: &Value) {
    match (dst, src) {
        (Value::Object(d), Value::Object(s)) => {
            for (k, v) in s {
                match d.get_mut(k) {
                    Some(existing) => deep_merge(existing, v),
                    None => {
                        d.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (d, s) => *d = s.clone(),
    }
}

fn default_colors() -> Map<String, Value> {
    const PAIRS: &[(&str, &str)] = &[
        ("bg", "#1b1f26f2"),
        ("bg_top", "#1b1f26f2"),
        ("bg_bottom", "#0f1216f2"),
        ("border", "#ffffff28"),
        ("text", "#f4f6f8"),
        ("muted", "#8b93a1"),
        ("header_bg", "#0b0e12bb"),
        ("cell_dark", "#0b0e12"),
        ("faster", "#46df7a"),
        ("slower", "#ff5050"),
        ("track", "#ffffff18"),
        ("fill", "#46df7a"),
        ("idle_text", "#8b93a1"),
        ("flag_yellow", "#ffd23acc"),
        ("flag_yellow_text", "#141414"),
        ("flag_black", "#1a1a1acc"),
        ("flag_black_text", "#ffffff"),
        ("flag_green", "#46df7acc"),
        ("flag_green_text", "#141414"),
        ("flag_white_bg", "#ebeef0cc"),
        ("flag_white_text", "#141414"),
        ("flag_red", "#ff5050cc"),
        ("flag_red_text", "#ffffff"),
        ("flag_blue", "#4a8cffcc"),
        ("flag_blue_text", "#ffffff"),
        ("flag_checker_bg", "#1a1a1acc"),
        ("flag_checker_text", "#ffffff"),
        ("flag_meatball", "#ff9416cc"),
        ("flag_meatball_text", "#141414"),
        ("flag_furled", "#ffd23acc"),
        ("flag_furled_text", "#141414"),
        ("flag_dq", "#ff5050cc"),
        ("flag_dq_text", "#ffffff"),
        ("flag_debris", "#8b93a1cc"),
        ("flag_debris_text", "#ffffff"),
        ("flag_crossed", "#ff9416cc"),
        ("flag_crossed_text", "#141414"),
        ("player_row", "#ff941658"),
        ("row_alt", "#ffffff14"),
        ("accent", "#70df7a"),
        ("shift_green", "#46df7a"),
        ("shift_yellow", "#ffd23a"),
        ("shift_red", "#ff5050"),
        ("shift_off", "#333a42"),
        ("shift_idle", "#ffffff18"),
        ("throttle", "#46df7a"),
        ("brake", "#ff5050"),
        ("clutch", "#4a8cff"),
        ("abs", "#ffd23a"),
        ("ring_track", "#ffffff18"),
        ("pedal_track", "#ffffff18"),
        ("cell_border", "#ffffff20"),
        ("orange", "#ff9416"),
        ("warn", "#e0a93a"),
        ("gear", "#ffffff"),
        ("label", "#8b93a1"),
        ("value", "#f4f6f8"),
        ("irating_delta_up", "#46df7a"),
        ("irating_delta_down", "#ff5050"),
    ];
    let mut colors = Map::new();
    for (k, v) in PAIRS {
        colors.insert((*k).into(), Value::String((*v).into()));
    }
    colors
}

/// Minimal defaults so widgets render without a full Python DEFAULTS dump.
/// `show` flags must match Python `config.DEFAULTS` so sparse preset merges work.
fn default_show(key: &str) -> bool {
    !matches!(
        key,
        "tire_panel"
            | "pit_board"
            | "weather_panel"
            | "leaderboard_strip"
            | "radio_tower"
            | "ers_hybrid"
            | "system_panel"
            | "pit_advisor"
    )
}

fn default_cfg() -> Value {
    let mut m = Map::new();
    for key in WIDGET_KEYS {
        let mut section = Map::new();
        section.insert("show".into(), Value::Bool(default_show(key)));
        section.insert("corner_radius_frac".into(), json!(0.08));
        section.insert("colors".into(), Value::Object(default_colors()));
        section.insert(
            "title".into(),
            Value::String(key.to_uppercase().replace('_', " ")),
        );
        section.insert("show_title".into(), Value::Bool(true));
        section.insert("idle_text".into(), Value::String("TRACK CLEAR".into()));
        section.insert("range".into(), json!(1.0));
        section.insert("show_value".into(), Value::Bool(true));
        section.insert("row_height_px".into(), json!(36));
        section.insert("font_scale".into(), json!(0.40));
        section.insert("show_wind".into(), Value::Bool(true));
        // Dash defaults matching Python CFG.
        section.insert("show_shift_bar".into(), Value::Bool(true));
        section.insert("show_ring".into(), Value::Bool(true));
        section.insert("show_position".into(), Value::Bool(true));
        section.insert("show_flags".into(), Value::Bool(true));
        section.insert("show_delta_bar".into(), Value::Bool(false));
        section.insert("show_throttle".into(), Value::Bool(true));
        section.insert("show_brake".into(), Value::Bool(true));
        section.insert("show_clutch".into(), Value::Bool(false));
        section.insert("center_mode".into(), Value::String("ring".into()));
        section.insert("top_right".into(), Value::String("incidents".into()));
        section.insert("primary_left".into(), Value::String("laps_left".into()));
        section.insert("primary_right".into(), Value::String("speed".into()));
        section.insert("stat_left".into(), Value::String("tires".into()));
        section.insert("stat_right".into(), Value::String("fuel_stack".into()));
        section.insert("strip_left".into(), Value::String("car_number".into()));
        section.insert("strip_center".into(), Value::String("lap_count".into()));
        section.insert("strip_right".into(), Value::String("irating".into()));
        section.insert("text_scale".into(), json!(1.0));
        section.insert("shift_segments".into(), json!(20));
        section.insert("shift_red_frac".into(), json!(0.16));
        section.insert("shift_yellow_frac".into(), json!(0.24));
        section.insert("ring_segments".into(), json!(16));
        section.insert("delta_bar_range".into(), json!(1.0));
        section.insert("show_irating_projection".into(), Value::Bool(true));
        section.insert("irating_abbreviate".into(), Value::Bool(false));
        m.insert((*key).into(), Value::Object(section));
    }
    m.insert("start_overlay_on_launch".into(), Value::Bool(false));
    m.insert("units".into(), Value::String("imperial".into()));
    m.insert("text_scale".into(), json!(1.20));
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hex8() {
        let c = parse_color_str("#46df7aff");
        assert_eq!(c.r(), 0x46);
        assert_eq!(c.g(), 0xdf);
        assert_eq!(c.b(), 0x7a);
    }

    #[test]
    fn merge_preserves_nested() {
        let mut a = json!({"flags": {"show": true, "colors": {"text": "#fff"}}});
        deep_merge(&mut a, &json!({"flags": {"show": false}}));
        assert_eq!(a["flags"]["show"], false);
        assert_eq!(a["flags"]["colors"]["text"], "#fff");
    }

    #[test]
    fn sparse_preset_merge_matches_python_show_semantics() {
        let mut cfg = default_cfg();
        // Sparse patch: hide dash (diff from DEFAULTS). weather_panel omitted —
        // must stay false like Python DEFAULTS, not flip true.
        deep_merge(
            &mut cfg,
            &json!({
                "dash": { "show": false },
                "map": { "show": true },
            }),
        );
        let oc = OverlayConfig {
            cfg,
            doc: json!({}),
            generation: 0,
            active_preset: "Default".into(),
        };
        assert!(!oc.widget_shown("dash"));
        assert!(oc.widget_shown("map"));
        assert!(!oc.widget_shown("weather_panel"));
        assert!(!oc.widget_shown("pit_advisor"));
        assert!(oc.widget_shown("standings")); // Python default true, not patched
    }
}
