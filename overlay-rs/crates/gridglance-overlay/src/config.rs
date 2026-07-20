//! Load / merge `overlay_config.json` (schema 2) into a live CFG map.

use anyhow::{Context, Result};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::Path;

use crate::paths;

pub const GARAGE_KEY: &str = "garage";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConfigContext {
    #[default]
    Race,
    Garage,
}

impl ConfigContext {
    pub fn label(self) -> &'static str {
        match self {
            Self::Race => "On track",
            Self::Garage => "In garage",
        }
    }
}

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
        "fuel_calc" => (440, 500, 460, 380),
        "inputs" => (340, 760, 280, 160),
        "delta_bar" => (640, 760, 280, 120),
        "flags" => (940, 660, 220, 100),
        "sector_timing" => (1160, 40, 280, 200),
        "lap_compare" => (1160, 260, 280, 220),
        "tire_panel" => (1160, 500, 280, 200),
        "pit_board" => (40, 980, 320, 140),
        "weather_panel" => (380, 980, 260, 180),
        "leaderboard_strip" => (660, 980, 480, 80),
        "radio_tower" => (1160, 720, 220, 56),
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
                "auto_switch_by_league": false,
                "auto_switch_by_car": false,
                "auto_switch_to_default": false,
                "active_preset": "Default",
                "presets": {
                    "Default": {
                        "config": {},
                        "layout": {},
                        "layout_garage": {},
                        "cars": [],
                        "leagues": [],
                        "default": true
                    }
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
        let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        let mut doc: Value = serde_json::from_str(&text).context("parse overlay_config.json")?;
        sanitize_loaded_config(&mut doc);
        let active = doc
            .get("active_preset")
            .and_then(|v| v.as_str())
            .unwrap_or("Default")
            .to_string();
        let cfg = if doc.get("schema").and_then(|s| s.as_u64()) == Some(2) {
            Self::cfg_from_doc(&doc, &active, ConfigContext::Race)
        } else {
            // Legacy flat config
            let mut cfg = default_cfg();
            deep_merge(&mut cfg, &doc);
            sanitize_config_sections(&mut cfg);
            doc = json!({
                "schema": 2,
                "active_preset": "Default",
                "auto_switch_by_league": false,
                "auto_switch_by_car": false,
                "auto_switch_to_default": false,
                "presets": {
                    "Default": {
                        "config": sparse_diff(&default_cfg(), &cfg),
                        "layout": {},
                        "layout_garage": {},
                        "cars": [],
                        "leagues": [],
                        "default": true
                    }
                }
            });
            cfg
        };
        Ok(Self {
            cfg,
            doc,
            generation: 0,
            active_preset: active,
        })
    }

    fn cfg_from_doc(doc: &Value, preset_name: &str, context: ConfigContext) -> Value {
        let mut cfg = default_cfg();
        if let Some(preset) = doc
            .get("presets")
            .and_then(|p| p.as_object())
            .and_then(|presets| presets.get(preset_name))
        {
            if let Some(overrides) = preset.get("config") {
                let mut base = overrides.clone();
                let garage = base.as_object_mut().and_then(|obj| obj.remove(GARAGE_KEY));
                deep_merge(&mut cfg, &base);
                if context == ConfigContext::Garage {
                    if let Some(garage) = garage {
                        deep_merge(&mut cfg, &garage);
                    }
                }
            }
        }
        cfg
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

    pub fn context_cfg(&self, context: ConfigContext) -> Value {
        Self::cfg_from_doc(&self.doc, &self.active_preset, context)
    }

    pub fn apply_context(&mut self, context: ConfigContext) {
        self.cfg = self.context_cfg(context);
        self.generation = self.generation.saturating_add(1);
    }

    pub fn auto_switch_flag(&self, key: &str) -> bool {
        self.doc.get(key).and_then(|v| v.as_bool()).unwrap_or(false)
    }

    pub fn set_auto_switch_flag(&mut self, key: &str, on: bool) {
        if matches!(
            key,
            "auto_switch_by_league" | "auto_switch_by_car" | "auto_switch_to_default"
        ) {
            if let Some(doc) = self.doc.as_object_mut() {
                doc.insert(key.into(), json!(on));
            }
        }
    }

    /// Resolve which preset a session should use (Python `preset_for_session`).
    pub fn preset_for_session(&self, league_id: Option<i32>, car_path: &str) -> Option<String> {
        let by_league = self.auto_switch_flag("auto_switch_by_league");
        let by_car = self.auto_switch_flag("auto_switch_by_car");
        let to_default = self.auto_switch_flag("auto_switch_to_default");
        if !by_league && !by_car && !to_default {
            return None;
        }
        let presets = self.doc.get("presets").and_then(|p| p.as_object())?;
        if by_league {
            if let Some(lid) = league_id {
                if lid > 0 {
                    for (name, entry) in presets {
                        if let Some(arr) = entry.get("leagues").and_then(|a| a.as_array()) {
                            if arr.iter().any(|v| {
                                v.as_i64() == Some(lid as i64)
                                    || v.as_u64() == Some(lid as u64)
                                    || v.as_str().and_then(|s| s.parse().ok()) == Some(lid)
                            }) {
                                return Some(name.clone());
                            }
                        }
                    }
                }
            }
        }
        if by_car && !car_path.is_empty() {
            for (name, entry) in presets {
                if let Some(arr) = entry.get("cars").and_then(|a| a.as_array()) {
                    if arr.iter().any(|v| v.as_str() == Some(car_path)) {
                        return Some(name.clone());
                    }
                }
            }
        }
        if to_default {
            for (name, entry) in presets {
                if entry.get("default").and_then(|d| d.as_bool()) == Some(true) {
                    return Some(name.clone());
                }
            }
        }
        None
    }

    pub fn active_preset_cars(&self) -> Vec<String> {
        self.doc
            .get("presets")
            .and_then(|p| p.get(&self.active_preset))
            .and_then(|e| e.get("cars"))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn active_preset_leagues(&self) -> Vec<i32> {
        self.doc
            .get("presets")
            .and_then(|p| p.get(&self.active_preset))
            .and_then(|e| e.get("leagues"))
            .and_then(|a| a.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| {
                        v.as_i64()
                            .map(|i| i as i32)
                            .or_else(|| v.as_u64().map(|u| u as i32))
                            .or_else(|| v.as_str()?.parse().ok())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn set_active_preset_cars(&mut self, cars: Vec<String>) {
        let active = self.active_preset.clone();
        if let Some(entry) = self
            .doc
            .get_mut("presets")
            .and_then(|p| p.as_object_mut())
            .and_then(|p| p.get_mut(&active))
            .and_then(|e| e.as_object_mut())
        {
            entry.insert("cars".into(), json!(cars));
            self.generation = self.generation.saturating_add(1);
        }
    }

    pub fn set_active_preset_leagues(&mut self, leagues: Vec<i32>) {
        let active = self.active_preset.clone();
        if let Some(entry) = self
            .doc
            .get_mut("presets")
            .and_then(|p| p.as_object_mut())
            .and_then(|p| p.get_mut(&active))
            .and_then(|e| e.as_object_mut())
        {
            entry.insert("leagues".into(), json!(leagues));
            self.generation = self.generation.saturating_add(1);
        }
    }

    /// Preset names from the schema-2 document.
    pub fn preset_names(&self) -> Vec<String> {
        self.doc
            .get("presets")
            .and_then(|p| p.as_object())
            .map(|m| {
                let mut names: Vec<_> = m.keys().cloned().collect();
                names.sort();
                names
            })
            .unwrap_or_else(|| vec!["Default".into()])
    }

    /// Switch active preset and rebuild live CFG from its on-track config overrides.
    pub fn set_active_preset(&mut self, name: &str) -> Result<()> {
        if !self
            .doc
            .get("presets")
            .and_then(|p| p.as_object())
            .map(|p| p.contains_key(name))
            .unwrap_or(false)
        {
            anyhow::bail!("unknown preset: {name}");
        }
        self.active_preset = name.to_string();
        if let Some(doc) = self.doc.as_object_mut() {
            doc.insert("active_preset".into(), Value::String(name.into()));
        }
        self.cfg = self.context_cfg(ConfigContext::Race);
        self.generation = self.generation.saturating_add(1);
        Ok(())
    }

    fn ensure_presets_object(&mut self) -> &mut Map<String, Value> {
        if !self.doc.is_object() {
            self.doc = json!({ "schema": 2, "presets": {} });
        }
        let doc = self.doc.as_object_mut().unwrap();
        if !doc.contains_key("presets") {
            doc.insert(
                "presets".into(),
                json!({
                    "Default": {
                        "config": {},
                        "layout": {},
                        "layout_garage": {},
                        "cars": [],
                        "leagues": [],
                        "default": true
                    }
                }),
            );
        }
        doc.get_mut("presets")
            .and_then(|v| v.as_object_mut())
            .expect("presets object")
    }

    fn blank_preset_entry() -> Value {
        json!({
            "config": {},
            "layout": {},
            "layout_garage": {},
            "cars": [],
            "leagues": [],
            "default": false
        })
    }

    /// Create a new empty preset and switch to it.
    pub fn create_preset(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("preset name required");
        }
        {
            let presets = self.ensure_presets_object();
            if presets.contains_key(name) {
                anyhow::bail!("preset already exists: {name}");
            }
            presets.insert(name.to_string(), Self::blank_preset_entry());
        }
        self.set_active_preset(name)?;
        Ok(())
    }

    /// Duplicate an existing preset under a new name.
    pub fn duplicate_preset(&mut self, from: &str, to: &str) -> Result<()> {
        let to = to.trim();
        if to.is_empty() {
            anyhow::bail!("preset name required");
        }
        let src = self
            .doc
            .get("presets")
            .and_then(|p| p.get(from))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown preset: {from}"))?;
        {
            let presets = self.ensure_presets_object();
            if presets.contains_key(to) {
                anyhow::bail!("preset already exists: {to}");
            }
            presets.insert(to.to_string(), src);
        }
        self.set_active_preset(to)?;
        Ok(())
    }

    /// Rename a preset (updates active_preset if needed).
    pub fn rename_preset(&mut self, from: &str, to: &str) -> Result<()> {
        let to = to.trim();
        if to.is_empty() {
            anyhow::bail!("preset name required");
        }
        if from == to {
            return Ok(());
        }
        let became_active = self.active_preset == from;
        {
            let presets = self.ensure_presets_object();
            if !presets.contains_key(from) {
                anyhow::bail!("unknown preset: {from}");
            }
            if presets.contains_key(to) {
                anyhow::bail!("preset already exists: {to}");
            }
            let entry = presets.remove(from).unwrap();
            presets.insert(to.to_string(), entry);
        }
        if became_active {
            self.active_preset = to.to_string();
            if let Some(doc) = self.doc.as_object_mut() {
                doc.insert("active_preset".into(), Value::String(to.into()));
            }
        }
        Ok(())
    }

    /// Delete a preset (refuses if it is the only one).
    pub fn delete_preset(&mut self, name: &str) -> Result<()> {
        let was_active = self.active_preset == name;
        let fallback = {
            let presets = self.ensure_presets_object();
            if presets.len() <= 1 {
                anyhow::bail!("cannot delete the only preset");
            }
            if !presets.contains_key(name) {
                anyhow::bail!("unknown preset: {name}");
            }
            presets.remove(name);
            if was_active {
                presets.keys().next().cloned()
            } else {
                None
            }
        };
        if let Some(fallback) = fallback {
            self.set_active_preset(&fallback)?;
        }
        Ok(())
    }

    pub fn save_for_context(&mut self, context: ConfigContext) -> Result<()> {
        self.sync_active_preset_for_context(context);
        self.save_doc()
    }

    /// Atomic write of the schema-2 document.
    pub fn save_doc(&self) -> Result<()> {
        let path = paths::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("mkdir {}", parent.display()))?;
        }
        let text = serde_json::to_string_pretty(&self.doc).context("serialize config")?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, &text).with_context(|| format!("write {}", tmp.display()))?;
        fs::rename(&tmp, &path).with_context(|| format!("rename to {}", path.display()))?;
        Ok(())
    }

    /// Diff live CFG against defaults and store under the active preset.
    pub fn sync_active_preset_config(&mut self) {
        let defaults = default_cfg();
        let mut sparse = sparse_diff(&defaults, &self.cfg);
        // Preserve garage overrides already on the preset if present.
        if let Some(presets) = self.doc.get("presets").and_then(|p| p.as_object()) {
            if let Some(preset) = presets.get(&self.active_preset) {
                if let Some(garage) = preset.get("config").and_then(|c| c.get("garage")).cloned() {
                    if let Some(obj) = sparse.as_object_mut() {
                        obj.insert("garage".into(), garage);
                    }
                }
            }
        }
        if let Some(presets) = self.doc.get_mut("presets").and_then(|p| p.as_object_mut()) {
            let entry = presets
                .entry(self.active_preset.clone())
                .or_insert_with(|| json!({ "config": {}, "layout": {}, "layout_garage": {} }));
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("config".into(), sparse);
            }
        }
        if let Some(doc) = self.doc.as_object_mut() {
            doc.insert("schema".into(), json!(2));
            doc.entry("auto_switch_by_league").or_insert(json!(false));
            doc.entry("auto_switch_by_car").or_insert(json!(false));
            doc.entry("auto_switch_to_default").or_insert(json!(false));
            doc.insert(
                "active_preset".into(),
                Value::String(self.active_preset.clone()),
            );
        }
        self.ensure_one_default();
    }

    pub fn sync_active_preset_for_context(&mut self, context: ConfigContext) {
        match context {
            ConfigContext::Race => self.sync_active_preset_config(),
            ConfigContext::Garage => {
                let race = self.context_cfg(ConfigContext::Race);
                let garage = sparse_diff(&race, &self.cfg);
                if let Some(presets) = self.doc.get_mut("presets").and_then(|p| p.as_object_mut()) {
                    let entry = presets
                        .entry(self.active_preset.clone())
                        .or_insert_with(Self::blank_preset_entry);
                    let obj = entry.as_object_mut().expect("preset object");
                    let cfg = obj.entry("config").or_insert_with(|| json!({}));
                    if !cfg.is_object() {
                        *cfg = json!({});
                    }
                    if let Some(cfg_obj) = cfg.as_object_mut() {
                        if garage == Value::Null
                            || garage.as_object().map(|o| o.is_empty()).unwrap_or(false)
                        {
                            cfg_obj.remove(GARAGE_KEY);
                        } else {
                            cfg_obj.insert(GARAGE_KEY.into(), garage);
                        }
                    }
                }
            }
        }
    }

    pub fn reset_context(&mut self, context: ConfigContext) {
        match context {
            ConfigContext::Race => {
                self.cfg = default_cfg();
                self.sync_active_preset_config();
            }
            ConfigContext::Garage => {
                if let Some(cfg) = self.active_preset_config_mut() {
                    if let Some(obj) = cfg.as_object_mut() {
                        obj.remove(GARAGE_KEY);
                    }
                }
                self.cfg = self.context_cfg(ConfigContext::Garage);
            }
        }
        self.generation = self.generation.saturating_add(1);
    }

    pub fn reset_section(&mut self, context: ConfigContext, section: &str) {
        let default = default_cfg().get(section).cloned().unwrap_or(Value::Null);
        if default != Value::Null {
            if let Some(obj) = self.cfg.as_object_mut() {
                obj.insert(section.into(), default);
            }
            self.sync_active_preset_for_context(context);
            self.generation = self.generation.saturating_add(1);
        }
    }

    pub fn default_preset(&self) -> Option<String> {
        self.doc
            .get("presets")
            .and_then(|p| p.as_object())
            .and_then(|presets| {
                presets.iter().find_map(|(name, entry)| {
                    entry
                        .get("default")
                        .and_then(|v| v.as_bool())
                        .filter(|v| *v)
                        .map(|_| name.clone())
                })
            })
    }

    pub fn set_default_preset(&mut self, name: &str) -> Result<()> {
        let presets = self.ensure_presets_object();
        if !presets.contains_key(name) {
            anyhow::bail!("unknown preset: {name}");
        }
        for (preset_name, entry) in presets.iter_mut() {
            if let Some(obj) = entry.as_object_mut() {
                obj.insert("default".into(), json!(preset_name == name));
            }
        }
        Ok(())
    }

    fn ensure_one_default(&mut self) {
        let names = self.preset_names();
        if names.is_empty() || self.default_preset().is_some() {
            return;
        }
        let fallback = if names.iter().any(|n| n == "Default") {
            "Default".to_string()
        } else {
            names[0].clone()
        };
        let _ = self.set_default_preset(&fallback);
    }

    fn active_preset_config_mut(&mut self) -> Option<&mut Value> {
        let active = self.active_preset.clone();
        let presets = self.ensure_presets_object();
        let entry = presets
            .entry(active)
            .or_insert_with(Self::blank_preset_entry);
        let obj = entry.as_object_mut()?;
        Some(obj.entry("config").or_insert_with(|| json!({})))
    }

    pub fn active_layout_doc(&self, context: ConfigContext) -> Value {
        let preset = self
            .doc
            .get("presets")
            .and_then(|p| p.get(&self.active_preset));
        let race = preset
            .and_then(|p| p.get("layout"))
            .cloned()
            .unwrap_or_else(|| json!({}));
        if context == ConfigContext::Race {
            return race;
        }
        let mut garage = race;
        if let Some(delta) = preset.and_then(|p| p.get("layout_garage")) {
            deep_merge(&mut garage, delta);
        }
        garage
    }

    pub fn store_active_layout_doc(&mut self, context: ConfigContext, layout: Value) {
        let active = self.active_preset.clone();
        let presets = self.ensure_presets_object();
        let entry = presets
            .entry(active)
            .or_insert_with(Self::blank_preset_entry);
        let obj = entry.as_object_mut().expect("preset object");
        match context {
            ConfigContext::Race => {
                obj.insert("layout".into(), layout);
            }
            ConfigContext::Garage => {
                let race = obj.get("layout").cloned().unwrap_or_else(|| json!({}));
                obj.insert("layout_garage".into(), sparse_diff(&race, &layout));
            }
        }
    }

    pub fn export_preset_value(&self, name: &str) -> Result<Value> {
        let mut preset = self
            .doc
            .get("presets")
            .and_then(|p| p.get(name))
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown preset: {name}"))?;
        if let Some(obj) = preset.as_object_mut() {
            obj.remove("default");
        }
        Ok(json!({
            "kind": "gridglance.preset",
            "version": 1,
            "name": name,
            "preset": preset
        }))
    }

    pub fn import_preset_value(
        &mut self,
        name: &str,
        payload: &Value,
        overwrite: bool,
    ) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("preset name required");
        }
        let mut preset =
            if payload.get("kind").and_then(|v| v.as_str()) == Some("gridglance.preset") {
                payload
                    .get("preset")
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("preset payload missing preset field"))?
            } else if payload.get("config").is_some() || payload.get("layout").is_some() {
                payload.clone()
            } else if let Some(presets) = payload.get("presets").and_then(|p| p.as_object()) {
                presets
                    .values()
                    .next()
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("no presets in import payload"))?
            } else {
                json!({ "config": payload })
            };
        if !preset.is_object() {
            preset = json!({ "config": preset });
        }
        let preset_obj = preset.as_object_mut().unwrap();
        preset_obj.entry("config").or_insert_with(|| json!({}));
        preset_obj.entry("layout").or_insert_with(|| json!({}));
        preset_obj
            .entry("layout_garage")
            .or_insert_with(|| json!({}));
        preset_obj.entry("cars").or_insert_with(|| json!([]));
        preset_obj.entry("leagues").or_insert_with(|| json!([]));
        preset_obj.insert("default".into(), json!(false));

        {
            let presets = self.ensure_presets_object();
            if presets.contains_key(name) && !overwrite {
                anyhow::bail!("preset already exists: {name}");
            }
            presets.insert(name.into(), preset);
        }
        self.set_active_preset(name)?;
        Ok(())
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
                    self.section(section)
                        .get("colors")
                        .and_then(|c| c.get(alias))
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

    /// Nested string under `section[group][key]` (e.g. `header.left`).
    pub fn nested_str(&self, section: &str, group: &str, key: &str, default: &str) -> String {
        self.section(section)
            .get(group)
            .and_then(|g| g.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }

    /// Nested bool under `section[group][key]` (e.g. `header_icons.left`).
    pub fn nested_bool(&self, section: &str, group: &str, key: &str, default: bool) -> bool {
        self.section(section)
            .get(group)
            .and_then(|g| g.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(default)
    }

    /// Whether `column_order` includes `col`.
    pub fn has_column(&self, section: &str, col: &str) -> bool {
        self.section(section)
            .get("column_order")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().any(|x| x.as_str() == Some(col)))
            .unwrap_or(false)
    }

    /// Dash slot keys that may hold `"irating"`.
    pub fn dash_uses_irating(&self) -> bool {
        const SLOTS: &[&str] = &[
            "top_left",
            "top_right",
            "primary_left",
            "primary_right",
            "stat_left",
            "stat_right",
            "strip_left",
            "strip_center",
            "strip_right",
        ];
        SLOTS
            .iter()
            .any(|k| self.str_key("dash", k, "") == "irating")
    }

    pub fn conv_temp(&self, celsius: f32) -> f32 {
        if self.imperial_units() {
            celsius * 9.0 / 5.0 + 32.0
        } else {
            celsius
        }
    }

    pub fn temp_unit(&self) -> &'static str {
        if self.imperial_units() {
            "°F"
        } else {
            "°C"
        }
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
            self.global_str("units", "metric")
                .to_ascii_lowercase()
                .as_str(),
            "imperial" | "us" | "mph"
        )
    }

    /// Litres → display units (L or US gal).
    pub fn conv_fuel(&self, litres: f32) -> f32 {
        if self.imperial_units() {
            litres * 0.264_172_05
        } else {
            litres
        }
    }

    pub fn fuel_unit(&self) -> &'static str {
        if self.imperial_units() {
            "Gal"
        } else {
            "L"
        }
    }

    /// m/s → display speed (mph or km/h).
    pub fn conv_speed(&self, mps: f32) -> f32 {
        if self.imperial_units() {
            mps * 2.236_936_3
        } else {
            mps * 3.6
        }
    }

    pub fn speed_unit(&self) -> &'static str {
        if self.imperial_units() {
            "mph"
        } else {
            "km/h"
        }
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

const DEAD_TABLE_COLUMNS: &[&str] = &["qual_pos", "qual_best", "gap_pole"];

/// Strip removed widget keys from a loaded config document (schema 2 presets).
fn sanitize_loaded_config(doc: &mut Value) {
    if let Some(presets) = doc.get_mut("presets").and_then(|p| p.as_object_mut()) {
        for preset in presets.values_mut() {
            if let Some(config) = preset.get_mut("config") {
                sanitize_config_sections(config);
            }
        }
    }
}

fn sanitize_config_sections(cfg: &mut Value) {
    for section in ["relative", "standings"] {
        if let Some(arr) = cfg
            .get_mut(section)
            .and_then(|s| s.get_mut("column_order"))
            .and_then(|v| v.as_array_mut())
        {
            arr.retain(|v| {
                v.as_str()
                    .map(|s| !DEAD_TABLE_COLUMNS.contains(&s))
                    .unwrap_or(true)
            });
        }
        normalize_table_row_counts(cfg, section);
    }
    strip_section_keys(cfg, "ers_hybrid", &["show_lap_energy", "label_lap"]);
    strip_section_keys(cfg, "inputs", &["show_handbrake", "show_steering_torque"]);
}

/// Keep `rows == rows_ahead + rows_behind` for Relative / Standings.
fn normalize_table_row_counts(cfg: &mut Value, section: &str) {
    let Some(obj) = cfg.get_mut(section).and_then(|v| v.as_object_mut()) else {
        return;
    };
    let ahead = obj
        .get("rows_ahead")
        .and_then(|v| v.as_f64())
        .unwrap_or(if section == "relative" { 3.0 } else { 4.0 })
        .max(0.0)
        .round() as i64;
    let behind = obj
        .get("rows_behind")
        .and_then(|v| v.as_f64())
        .unwrap_or(if section == "relative" { 3.0 } else { 5.0 })
        .max(0.0)
        .round() as i64;
    let sum = ahead + behind;
    // Ahead/behind drive the live table; `rows` is their total. If an older
    // preset has a mismatched top-N `rows`, sync total to the split.
    obj.insert("rows_ahead".into(), json!(ahead.max(0)));
    obj.insert("rows_behind".into(), json!(behind.max(0)));
    obj.insert("rows".into(), json!(sum.max(0)));
}

fn strip_section_keys(cfg: &mut Value, section: &str, keys: &[&str]) {
    if let Some(obj) = cfg.get_mut(section).and_then(|v| v.as_object_mut()) {
        for key in keys {
            obj.remove(*key);
        }
    }
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

/// Keep only keys/values in `current` that differ from `defaults` (Python `diff_from_defaults`).
fn sparse_diff(defaults: &Value, current: &Value) -> Value {
    match (defaults, current) {
        (Value::Object(d), Value::Object(c)) => {
            let mut out = Map::new();
            for (k, cv) in c {
                match d.get(k) {
                    Some(dv) => {
                        let child = sparse_diff(dv, cv);
                        if child != Value::Null
                            && !(child.is_object() && child.as_object().unwrap().is_empty())
                        {
                            out.insert(k.clone(), child);
                        }
                    }
                    None => {
                        out.insert(k.clone(), cv.clone());
                    }
                }
            }
            Value::Object(out)
        }
        (d, c) if d == c => Value::Null,
        (_, c) => c.clone(),
    }
}

fn default_colors() -> Map<String, Value> {
    const PAIRS: &[(&str, &str)] = &[
        ("bg", "#1b1f26f2"),
        ("bg_top", "#1b1f26f2"),
        ("bg_bottom", "#0f1216f2"),
        ("border", "#ffffff28"),
        ("text", "#f4f6f8"),
        ("title", "#f4f6f8"),
        ("muted", "#8b93a1"),
        ("row_dim_text", "#5a616c"),
        ("header_bg", "#0b0e12bb"),
        ("cell_dark", "#0b0e12"),
        ("faster", "#46df7a"),
        ("slower", "#e23b3b"),
        ("track", "#262b34"),
        ("center", "#8b93a1"),
        ("fill", "#46df7a"),
        ("idle_bg", "#1f242c"),
        ("idle_text", "#9fb0a4"),
        ("flag_yellow", "#ffd23a"),
        ("flag_yellow_text", "#1a1400"),
        ("flag_black", "#0a0a0a"),
        ("flag_black_text", "#ffffff"),
        ("flag_green", "#46df7a"),
        ("flag_green_text", "#06210f"),
        ("flag_white_bg", "#eef1f4"),
        ("flag_white_text", "#14171c"),
        ("flag_red", "#d11f2d"),
        ("flag_red_text", "#ffffff"),
        ("flag_blue", "#2f6bd8"),
        ("flag_blue_text", "#ffffff"),
        ("flag_checker_bg", "#14171c"),
        ("flag_checker_text", "#f4f6f8"),
        ("flag_meatball", "#ff7a1a"),
        ("flag_meatball_text", "#1a0d00"),
        ("flag_furled", "#caa23a"),
        ("flag_furled_text", "#1a1400"),
        ("flag_dq", "#c0392b"),
        ("flag_dq_text", "#ffffff"),
        ("flag_debris", "#e0a72e"),
        ("flag_debris_text", "#1a1400"),
        ("flag_crossed", "#2a2f38"),
        ("flag_crossed_text", "#f4f6f8"),
        ("player_row", "#ff941670"),
        ("row_alt", "#ffffff01"),
        ("accent", "#e23b3b"),
        ("shift_green", "#46df7a"),
        ("shift_yellow", "#ffd23a"),
        ("shift_red", "#e23b3b"),
        ("shift_off", "#333a42"),
        ("shift_idle", "#ffffff18"),
        ("throttle", "#46df7a"),
        ("brake", "#e23b3b"),
        ("clutch", "#4a8cff"),
        ("abs", "#ffd23a"),
        ("ring_track", "#333a42"),
        ("pedal_track", "#333a42"),
        ("cell_border", "#ffffff20"),
        ("orange", "#ff9416"),
        ("warn", "#e0a93a"),
        ("gear", "#ffffff"),
        ("label", "#8b93a1"),
        ("value", "#f4f6f8"),
        ("irating_bg", "#0b0d11cc"),
        ("irating_border", "#ffffff20"),
        ("irating_text", "#f4f6f8"),
        ("irating_delta_up", "#46df7a"),
        ("irating_delta_down", "#e23b3b"),
        ("threat", "#ff505060"),
        ("lapped", "#2563eb60"),
        ("pit_row", "#8b93a118"),
        ("inactive_row", "#8b93a128"),
        ("speaking_row", "#22c55e50"),
        ("footer_bg", "#0f1216"),
        ("badge_player", "#ff9416"),
        ("badge_pit_bg", "#ebeef0"),
        ("badge_pit_text", "#141414"),
        ("badge_lap", "#7638c4"),
        ("badge_speaking_bg", "#22c55e"),
        ("badge_speaking_text", "#ffffff"),
        ("badge_speaking_border", "#ffffffcc"),
        ("badge_empty_border", "#ffffff28"),
        ("badge_empty_fill", "#00000078"),
        ("pro_name", "#f5c542"),
        ("pro_badge", "#f5c542"),
        ("strip_none", "#333a42"),
        ("strip_window", "#46df7a"),
        ("strip_now", "#ffd23a"),
        ("pill_open", "#46df7a"),
        ("pill_closed", "#6e747d"),
        ("pill_text", "#06210f"),
        ("add_text", "#f4f6f8"),
        ("gauge_fill", "#f4f6f8"),
        ("gauge_bg", "#0b0e12"),
        ("box_value", "#f4f6f8"),
        ("box_warn", "#e23b3b"),
        ("header", "#8b93a1"),
        ("asphalt", "#333a42"),
        ("outline", "#8b93a1"),
        ("infield", "#0f1216c8"),
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
        section.insert("corner_radius_frac".into(), json!(0.0));
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
        section.insert("primary_left".into(), Value::String("lap_count".into()));
        section.insert("primary_right".into(), Value::String("speed".into()));
        section.insert("stat_left".into(), Value::String("tires".into()));
        section.insert("stat_right".into(), Value::String("fuel_stack".into()));
        section.insert("strip_left".into(), Value::String("air_temp".into()));
        section.insert("strip_center".into(), Value::String("track_temp".into()));
        section.insert("strip_right".into(), Value::String("last_lap".into()));
        section.insert("text_scale".into(), json!(1.0));
        section.insert("shift_blink".into(), Value::Bool(true));
        section.insert("shift_blink_hz".into(), json!(7.0));
        section.insert("shift_blink_pct".into(), json!(0.99));
        section.insert("shift_blink_max_sec".into(), json!(3.0));
        section.insert("shift_segments".into(), json!(20));
        section.insert("shift_red_frac".into(), json!(0.16));
        section.insert("shift_yellow_frac".into(), json!(0.24));
        section.insert("ring_segments".into(), json!(16));
        section.insert("delta_bar_range".into(), json!(1.0));
        section.insert("show_irating_projection".into(), Value::Bool(false));
        section.insert("irating_abbreviate".into(), Value::Bool(true));
        section.insert("irating_show_icon".into(), Value::Bool(true));
        // Table / radar defaults (Python parity) so demo works without a full CFG dump.
        if *key == "relative" || *key == "standings" {
            section.insert(
                "rows_ahead".into(),
                json!(if *key == "relative" { 3 } else { 4 }),
            );
            section.insert(
                "rows_behind".into(),
                json!(if *key == "relative" { 3 } else { 5 }),
            );
            // Total neighbor slots: rows_ahead + rows_behind.
            section.insert(
                "rows".into(),
                json!(if *key == "relative" { 6 } else { 9 }),
            );
            section.insert("center_on_player".into(), Value::Bool(true));
            section.insert("show_footer".into(), Value::Bool(true));
            section.insert("row_ease_tau".into(), json!(0.16));
            section.insert("fade_ease_tau".into(), json!(0.12));
            section.insert("header_font_scale".into(), json!(1.0));
            section.insert("footer_font_scale".into(), json!(1.0));
            section.insert("alt_row_shading".into(), Value::Bool(true));
            section.insert("row_dividers".into(), Value::Bool(true));
            section.insert("name_font_bold".into(), Value::Bool(true));
            section.insert("gap_font_scale".into(), json!(1.12));
            section.insert("pit_mode".into(), Value::String("laps_since".into()));
            section.insert(
                "column_order".into(),
                json!(["badge", "position", "name", "license", "irating", "gap"]),
            );
            section.insert("columns".into(), json!({ "stripe": true }));
            if *key == "relative" {
                section.insert("show_strategy_hints".into(), Value::Bool(true));
                section.insert("strategy_fuel_pct_thresh".into(), json!(0.18));
                section.insert("undercut_gap_max_s".into(), json!(12.0));
                section.insert("cover_gap_max_s".into(), json!(8.0));
                section.insert("pit_loss_seconds".into(), json!(25.0));
                if let Some(Value::Object(colors)) = section.get_mut("colors") {
                    colors.insert("undercut_row".into(), Value::String("#3aa0ff44".into()));
                    colors.insert("cover_row".into(), Value::String("#ff941644".into()));
                    colors.insert("undercut_gap".into(), Value::String("#3aa0ff".into()));
                    colors.insert("cover_gap".into(), Value::String("#ff9416".into()));
                    colors.insert("badge_undercut".into(), Value::String("#3aa0ff".into()));
                    colors.insert("badge_cover".into(), Value::String("#ff9416".into()));
                }
                section.insert(
                    "header".into(),
                    json!({"left": "sof", "center": "none", "right": "position"}),
                );
                section.insert(
                    "footer".into(),
                    json!({"left": "race_time", "center": "lap", "right": "incidents"}),
                );
                section.insert(
                    "header_icons".into(),
                    json!({"left": false, "center": false, "right": false}),
                );
                section.insert(
                    "footer_icons".into(),
                    json!({"left": false, "center": false, "right": false}),
                );
            }
            if *key == "standings" {
                section.insert("rows".into(), json!(9));
                section.insert("pin_podium".into(), Value::Bool(false));
                section.insert("title".into(), Value::String("Standings".into()));
                if let Some(Value::Object(colors)) = section.get_mut("colors") {
                    colors.insert(
                        "podium_separator".into(),
                        Value::String("#22c55e".into()),
                    );
                }
                section.insert(
                    "header".into(),
                    json!({"left": "order_pill", "center": "title", "right": "count"}),
                );
                section.insert(
                    "footer".into(),
                    json!({"left": "track_temp", "center": "session_time", "right": "air_temp"}),
                );
                section.insert(
                    "header_icons".into(),
                    json!({"left": false, "center": false, "right": false}),
                );
                section.insert(
                    "footer_icons".into(),
                    json!({"left": false, "center": false, "right": false}),
                );
            }
        }
        if *key == "map" {
            section.insert("show_panel".into(), Value::Bool(false));
            section.insert("show_infield".into(), Value::Bool(true));
            section.insert("show_start_finish".into(), Value::Bool(true));
            section.insert("asphalt_width".into(), json!(12));
            section.insert("outline_width".into(), json!(6));
            section.insert("rotation".into(), json!(0));
            section.insert("mirror".into(), Value::Bool(false));
            section.insert("dot_radius_frac".into(), json!(0.05));
            section.insert("other_dot_radius_frac".into(), json!(0.05));
            section.insert("pit_dot_opacity".into(), json!(0.45));
            section.insert("show_pit".into(), Value::Bool(true));
            section.insert("show_pit_blends".into(), Value::Bool(true));
            section.insert("show_pit_speed".into(), Value::Bool(true));
            section.insert("pit_lane_opacity".into(), json!(1.0));
            section.insert("car_label".into(), Value::String("number".into()));
            section.insert("show_traffic_markers".into(), Value::Bool(true));
            section.insert("marker_hold_seconds".into(), json!(3.0));
            section.insert("show_car_status".into(), Value::Bool(true));
            section.insert("show_drs_zones".into(), Value::Bool(false));
            section.insert("show_p2p_zones".into(), Value::Bool(false));
            section.insert("show_wind".into(), Value::Bool(true));
            section.insert("show_expanded_weather".into(), Value::Bool(false));
            section.insert("show_pace_safety_line".into(), Value::Bool(true));
            if let Some(Value::Object(colors)) = section.get_mut("colors") {
                colors.insert("player".into(), Value::String("#46df7a".into()));
                colors.insert("competitor".into(), Value::String("#b06bff".into()));
                colors.insert("lapped".into(), Value::String("#2563eb".into()));
                colors.insert("lapping".into(), Value::String("#ff5050".into()));
                colors.insert("pit_car".into(), Value::String("#6e747d".into()));
                colors.insert("pit".into(), Value::String("#ff4d4d".into()));
                colors.insert("pit_text".into(), Value::String("#ffffff".into()));
                colors.insert("pit_blend".into(), Value::String("#ffd23a".into()));
                colors.insert("pit_blend_out".into(), Value::String("#3aa0ff".into()));
                colors.insert("pace_car".into(), Value::String("#0b0e12".into()));
                colors.insert("pace_safety".into(), Value::String("#ff9416ee".into()));
                colors.insert("pit_exit_mark".into(), Value::String("#ffd23acc".into()));
                colors.insert("asphalt".into(), Value::String("#333a42".into()));
                colors.insert("outline".into(), Value::String("#8b93a1".into()));
                colors.insert("infield".into(), Value::String("#0f1216c8".into()));
                colors.insert("marker_leader".into(), Value::String("#ffd23a".into()));
                colors.insert("marker_ahead".into(), Value::String("#46df7a".into()));
                colors.insert("marker_behind".into(), Value::String("#ff5050".into()));
                colors.insert("marker_line".into(), Value::String("#ffffff40".into()));
                colors.insert("speaking_ring".into(), Value::String("#46df7a".into()));
                colors.insert("speaking_glow".into(), Value::String("#46df7a55".into()));
                colors.insert("speaking_badge_bg".into(), Value::String("#22c55e".into()));
                colors.insert(
                    "speaking_badge_text".into(),
                    Value::String("#ffffff".into()),
                );
                colors.insert("status_pit".into(), Value::String("#ffd23a".into()));
                colors.insert("status_off".into(), Value::String("#ff5050".into()));
                colors.insert("status_garage".into(), Value::String("#8b93a1".into()));
                colors.insert("status_black".into(), Value::String("#1a1a1a".into()));
                colors.insert("status_meatball".into(), Value::String("#ff9416".into()));
                colors.insert("status_dq".into(), Value::String("#ff5050".into()));
                colors.insert("status_furled".into(), Value::String("#ffd23a".into()));
                colors.insert("drs_zone".into(), Value::String("#46df7a88".into()));
                colors.insert("p2p_zone".into(), Value::String("#3aa0ff88".into()));
                colors.insert("active_sector".into(), Value::String("#ffd23a66".into()));
                colors.insert("sector_line".into(), Value::String("#a78bfa".into()));
                colors.insert("sector_text".into(), Value::String("#c4b5fd".into()));
                colors.insert("wind".into(), Value::String("#9fd0ff".into()));
                colors.insert("wind_text".into(), Value::String("#eaf3ff".into()));
            }
            section.insert("show_sector_boundaries".into(), Value::Bool(true));
            section.insert("show_corners".into(), Value::Bool(true));
        }
        if *key == "radar" {
            section.insert("show_panel".into(), Value::Bool(false));
            section.insert("show_front".into(), Value::Bool(true));
            section.insert("show_rear".into(), Value::Bool(true));
            section.insert("show_axis".into(), Value::Bool(true));
            section.insert("show_nose".into(), Value::Bool(true));
            section.insert("range_pct".into(), json!(0.03));
            section.insert("alongside_zone_pct".into(), json!(0.004));
            section.insert("side_span_pct".into(), json!(0.0045));
            section.insert("ease_side_tau".into(), json!(0.12));
            section.insert("ease_glow_tau".into(), json!(0.16));
            section.insert(
                "sizes".into(),
                json!({
                    "car_w": 0.13,
                    "car_h": 0.20,
                    "bar_h": 0.78,
                    "glow_w": 0.17,
                    "nose_len": 0.16
                }),
            );
            if let Some(Value::Object(colors)) = section.get_mut("colors") {
                colors.insert("car".into(), Value::String("#f4f6f8".into()));
                colors.insert("axis".into(), Value::String("#46df7a3a".into()));
                colors.insert("nose".into(), Value::String("#46df7ae6".into()));
                colors.insert("red".into(), Value::String("#e23b3b".into()));
                colors.insert("yellow".into(), Value::String("#ffd23a".into()));
            }
        }
        if *key == "radio_tower" {
            section.insert("show_title".into(), Value::Bool(true));
            section.insert("title".into(), Value::String("RADIO".into()));
            section.insert("show_position".into(), Value::Bool(true));
            section.insert("show_car_number".into(), Value::Bool(true));
            section.insert("show_name".into(), Value::Bool(true));
            section.insert("highlight_player".into(), Value::Bool(true));
            section.insert("row_height_px".into(), json!(0));
        }
        if *key == "fuel_calc" {
            section.insert("title".into(), Value::String("FUEL CALCULATOR".into()));
            section.insert("show_title".into(), Value::Bool(true));
            section.insert("show_pill".into(), Value::Bool(true));
            section.insert("show_add".into(), Value::Bool(true));
            section.insert("show_gauge".into(), Value::Bool(true));
            section.insert("show_stats".into(), Value::Bool(true));
            section.insert("show_strip".into(), Value::Bool(true));
            section.insert("show_time".into(), Value::Bool(true));
            section.insert("show_laps".into(), Value::Bool(true));
            section.insert("show_live_burn".into(), Value::Bool(false));
            section.insert("show_tank_pct".into(), Value::Bool(false));
            section.insert("show_low_fuel_alert".into(), Value::Bool(true));
            section.insert("history_laps".into(), json!(10));
            section.insert("low_fuel_laps_threshold".into(), json!(2.0));
            section.insert("low_fuel_time_threshold".into(), json!(120.0));
            section.insert("max_row_height_frac".into(), json!(0.14));
            section.insert("stats_header_font_scale".into(), json!(1.0));
            section.insert("stats_row_font_scale".into(), json!(1.0));
        }
        if *key == "inputs" {
            section.insert("history_seconds".into(), json!(6.0));
            section.insert("show_throttle".into(), Value::Bool(true));
            section.insert("show_brake".into(), Value::Bool(true));
            section.insert("show_clutch".into(), Value::Bool(false));
            section.insert("show_steering".into(), Value::Bool(false));
            section.insert("show_shift_markers".into(), Value::Bool(false));
            section.insert("show_brake_threshold".into(), Value::Bool(false));
            section.insert("brake_threshold".into(), json!(85));
            section.insert("show_label".into(), Value::Bool(true));
            section.insert("show_graph".into(), Value::Bool(true));
            section.insert("show_bars".into(), Value::Bool(true));
            section.insert("show_gauge".into(), Value::Bool(true));
            section.insert("label_text".into(), Value::String("TELEMETRY".into()));
            section.insert("line_width".into(), json!(2.4));
            if let Some(Value::Object(colors)) = section.get_mut("colors") {
                colors.insert("accent".into(), Value::String("#e23b3b".into()));
                colors.insert("label".into(), Value::String("#cdd3db".into()));
                colors.insert("graph_bg".into(), Value::String("#0b0d11".into()));
                colors.insert("grid".into(), Value::String("#ffffff14".into()));
                colors.insert("throttle".into(), Value::String("#46df7a".into()));
                colors.insert("brake".into(), Value::String("#e23b3b".into()));
                colors.insert("clutch".into(), Value::String("#3aa0ff".into()));
                colors.insert("steering".into(), Value::String("#c08bff".into()));
                colors.insert("brake_abs".into(), Value::String("#ffd23a".into()));
                colors.insert("brake_over".into(), Value::String("#ff7a1a".into()));
                colors.insert("threshold".into(), Value::String("#ffffff66".into()));
                colors.insert("bar_track".into(), Value::String("#262b34".into()));
                colors.insert("gauge_bg".into(), Value::String("#0b0d11".into()));
                colors.insert("gauge_ring".into(), Value::String("#333a42".into()));
            }
        }
        if *key == "pit_board" {
            section.insert("show_title".into(), Value::Bool(true));
            section.insert("title".into(), Value::String("PIT SERVICES".into()));
            section.insert("show_pit_banner".into(), Value::Bool(true));
            section.insert(
                "pit_banner_text".into(),
                Value::String("PIT STOP ACTIVE".into()),
            );
            section.insert("show_compound".into(), Value::Bool(true));
            section.insert("show_fast_repairs".into(), Value::Bool(true));
            section.insert("show_pressures".into(), Value::Bool(false));
            section.insert("row_dividers".into(), Value::Bool(true));
            if let Some(Value::Object(colors)) = section.get_mut("colors") {
                colors.insert("checked".into(), Value::String("#46df7a".into()));
            }
        }
        if *key == "tire_panel" {
            section.insert("show_title".into(), Value::Bool(true));
            section.insert("title".into(), Value::String("TIRES".into()));
            section.insert("show_wear".into(), Value::Bool(true));
            section.insert("show_temp".into(), Value::Bool(true));
            section.insert("show_pressure".into(), Value::Bool(false));
            section.insert("warn_wear_pct".into(), json!(30.0));
            if let Some(Value::Object(colors)) = section.get_mut("colors") {
                colors.insert("wear".into(), Value::String("#46df7a".into()));
                colors.insert("warn".into(), Value::String("#ff9416".into()));
                colors.insert("bar_bg".into(), Value::String("#262b34".into()));
                colors.insert("header".into(), Value::String("#8b93a1".into()));
            }
        }
        if *key == "ers_hybrid" {
            section.insert("show_title".into(), Value::Bool(true));
            section.insert("title".into(), Value::String("HYBRID".into()));
            section.insert("empty_text".into(), Value::String("No hybrid data".into()));
            section.insert("show_battery".into(), Value::Bool(true));
            section.insert("show_boost".into(), Value::Bool(true));
            section.insert("show_p2p".into(), Value::Bool(true));
            section.insert("label_battery".into(), Value::String("ERS".into()));
            section.insert("label_boost".into(), Value::String("BOOST".into()));
            section.insert("label_p2p".into(), Value::String("P2P".into()));
        }
        if *key == "leaderboard_strip" {
            section.insert("rows".into(), json!(10));
            section.insert("show_position".into(), Value::Bool(true));
            section.insert("show_car_number".into(), Value::Bool(true));
            section.insert("show_name".into(), Value::Bool(false));
            section.insert("show_gap".into(), Value::Bool(false));
            section.insert("show_lap".into(), Value::Bool(false));
            section.insert("show_mph".into(), Value::Bool(false));
            section.insert("highlight_player".into(), Value::Bool(true));
            if let Some(Value::Object(colors)) = section.get_mut("colors") {
                colors.insert("pylon_bg".into(), Value::String("#000000".into()));
                colors.insert("digit".into(), Value::String("#ff9416".into()));
                colors.insert("player".into(), Value::String("#ffffff".into()));
            }
        }
        if *key == "laptime_log" {
            section.insert("rows".into(), json!(8));
            section.insert("show_header".into(), Value::Bool(true));
            section.insert("delta_mode".into(), Value::String("previous".into()));
            section.insert("temp_icon".into(), Value::Bool(true));
            section.insert("alt_row_shading".into(), Value::Bool(true));
            section.insert("row_dividers".into(), Value::Bool(true));
            section.insert(
                "column_order".into(),
                json!(["lap", "time", "delta", "temp"]),
            );
            if let Some(Value::Object(colors)) = section.get_mut("colors") {
                colors.insert("faster".into(), Value::String("#46df7a".into()));
                colors.insert("slower".into(), Value::String("#e23b3b".into()));
                colors.insert("header".into(), Value::String("#8b93a1".into()));
            }
        }
        if *key == "sector_timing" {
            section.insert("sectors".into(), json!(3));
            section.insert("show_sector_delta".into(), Value::Bool(false));
            section.insert("show_predicted_lap".into(), Value::Bool(false));
            section.insert("highlight_active_sector_on_map".into(), Value::Bool(false));
        }
        if *key == "lap_compare" {
            section.insert("reference_mode".into(), Value::String("best".into()));
            section.insert("show_graph".into(), Value::Bool(true));
            section.insert("show_brake_markers".into(), Value::Bool(true));
            section.insert("show_lift_markers".into(), Value::Bool(true));
            section.insert("max_turns".into(), json!(6));
            section.insert("alt_row_shading".into(), Value::Bool(true));
        }
        if *key == "pit_advisor" {
            section.insert("show_title".into(), Value::Bool(true));
            section.insert("title".into(), Value::String("PIT ENGINEER".into()));
            section.insert("show_only_when_actionable".into(), Value::Bool(true));
            section.insert("pit_loss_seconds".into(), json!(25.0));
            section.insert("undercut_gap_max_s".into(), json!(12.0));
            section.insert("cover_gap_max_s".into(), json!(8.0));
            section.insert("low_fuel_laps_threshold".into(), json!(2.0));
        }
        m.insert((*key).into(), Value::Object(section));
    }
    m.insert("start_overlay_on_launch".into(), Value::Bool(false));
    m.insert("check_updates_on_launch".into(), Value::Bool(true));
    m.insert("start_at_login".into(), Value::Bool(false));
    m.insert("driver_groups".into(), json!([]));
    m.insert("units".into(), Value::String("imperial".into()));
    m.insert("text_scale".into(), json!(1.20));
    Value::Object(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_removed_keys() {
        let mut cfg = json!({
            "relative": {
                "column_order": ["position", "qual_pos", "gap"]
            },
            "ers_hybrid": { "show_lap_energy": true, "label_lap": "LAP" },
            "inputs": { "show_handbrake": true }
        });
        sanitize_config_sections(&mut cfg);
        let cols = cfg["relative"]["column_order"].as_array().unwrap();
        assert_eq!(cols.len(), 2);
        assert!(cfg["ers_hybrid"].get("show_lap_energy").is_none());
        assert!(cfg["inputs"].get("show_handbrake").is_none());
    }

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

    #[test]
    fn sparse_diff_drops_equal_leaves() {
        let defaults = json!({"a": 1, "b": {"c": true, "d": 2}});
        let current = json!({"a": 1, "b": {"c": false, "d": 2}, "e": 9});
        let diff = sparse_diff(&defaults, &current);
        assert_eq!(diff, json!({"b": {"c": false}, "e": 9}));
    }

    #[test]
    fn garage_context_merges_sparse_over_race() {
        let mut oc = OverlayConfig::default();
        oc.doc["presets"]["Default"]["config"] = json!({
            "units": "metric",
            "garage": { "units": "imperial", "dash": { "show": false } }
        });
        oc.set_active_preset("Default").unwrap();
        assert_eq!(oc.context_cfg(ConfigContext::Race)["units"], "metric");
        let garage = oc.context_cfg(ConfigContext::Garage);
        assert_eq!(garage["units"], "imperial");
        assert_eq!(garage["dash"]["show"], false);
    }

    #[test]
    fn default_preset_is_unique() {
        let mut oc = OverlayConfig::default();
        oc.create_preset("Alt").unwrap();
        oc.set_default_preset("Alt").unwrap();
        assert_eq!(oc.default_preset().as_deref(), Some("Alt"));
        assert_eq!(oc.doc["presets"]["Default"]["default"], false);
        assert_eq!(oc.doc["presets"]["Alt"]["default"], true);
    }

    #[test]
    fn import_export_round_trip_forces_import_non_default() {
        let mut oc = OverlayConfig::default();
        oc.doc["presets"]["Default"]["default"] = json!(true);
        let payload = oc.export_preset_value("Default").unwrap();
        let mut next = OverlayConfig::default();
        next.import_preset_value("Imported", &payload, false)
            .unwrap();
        assert!(next.doc["presets"]["Imported"].get("config").is_some());
        assert_eq!(next.doc["presets"]["Imported"]["default"], false);
    }

    #[test]
    fn garage_layout_stores_sparse_delta() {
        let mut oc = OverlayConfig::default();
        let race = json!({"dash": [1, 2, 3, 4], "map": [5, 6, 7, 8]});
        let garage = json!({"dash": [9, 2, 3, 4], "map": [5, 6, 7, 8]});
        oc.store_active_layout_doc(ConfigContext::Race, race);
        oc.store_active_layout_doc(ConfigContext::Garage, garage);
        assert_eq!(
            oc.doc["presets"]["Default"]["layout_garage"],
            json!({"dash": [9, 2, 3, 4]})
        );
        assert_eq!(
            oc.active_layout_doc(ConfigContext::Garage)["dash"],
            json!([9, 2, 3, 4])
        );
    }
}
