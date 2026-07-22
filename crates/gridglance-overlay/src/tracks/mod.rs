//! Track authoring: geometry helpers, save, HTML import.

mod geom;
mod html_import;
mod layers;
mod path_sample;

use chrono::Utc;
use serde_json::{json, Value};
use std::path::Path;

use crate::cloud;

pub use html_import::import_track_source;

pub fn cloud_blocks_track_save(canonical: &Value) -> Option<String> {
    if !cloud::can_write() {
        return None;
    }
    match cloud::cloud_track_exists(canonical) {
        Some(true) => Some(format!(
            "TrackID {canonical} is already in the shared library — save skipped."
        )),
        _ => None,
    }
}

pub fn write_track_json(
    tracks_dir: &Path,
    tid: &Value,
    doc: &Value,
) -> anyhow::Result<std::path::PathBuf> {
    let mut stamped = doc.clone();
    if let Some(obj) = stamped.as_object_mut() {
        obj.insert("updated_at".into(), json!(Utc::now().to_rfc3339()));
    }
    let path = cloud::track_file_path(tracks_dir, tid);
    cloud::write_json_atomic(&path, &stamped)?;
    Ok(path)
}

pub fn build_loop_doc(
    tid: &Value,
    loop_pts: &[(f32, f32)],
    name: Option<&str>,
    start_finish: f32,
    corners: &[Value],
    num_turns: Option<i64>,
    alias_track_ids: &[i32],
    map_rotation: i32,
    map_mirror: bool,
) -> Value {
    let points: Vec<Value> = loop_pts
        .iter()
        .map(|(x, y)| json!([round7(*x), round7(*y)]))
        .collect();
    let mut doc = json!({
        "schema": 2,
        "import_version": 2,
        "pit_source": "manual",
        "track_id": tid,
        "name": name.unwrap_or("track"),
        "start_finish": start_finish,
        "points": points,
        "corners": corners,
        "map_rotation": map_rotation,
        "map_mirror": map_mirror,
    });
    if let Some(n) = num_turns {
        if n > 0 {
            doc.as_object_mut()
                .unwrap()
                .insert("num_turns".into(), json!(n));
        }
    }
    if !alias_track_ids.is_empty() {
        doc.as_object_mut()
            .unwrap()
            .insert("alias_track_ids".into(), json!(alias_track_ids));
    }
    doc
}

fn round7(v: f32) -> f64 {
    (v as f64 * 1e7).round() / 1e7
}

fn pts_json(pts: &[(f32, f32)]) -> Vec<Value> {
    pts.iter()
        .map(|(x, y)| json!([round7(*x), round7(*y)]))
        .collect()
}

/// Build pit lane fields from manual entry/road/merge polylines.
pub fn build_manual_pit_lane_fields(
    loop_pts: &[(f32, f32)],
    entry: &[(f32, f32)],
    road: &[(f32, f32)],
    merge: &[(f32, f32)],
) -> Option<serde_json::Map<String, Value>> {
    if road.len() < 2 || merge.len() < 2 {
        return None;
    }
    let pit_path = geom::resample_open(road, 140);
    let pit_out_raw = geom::resample_open(merge, 41);
    let pit_out =
        geom::connect_blend_to_loop(&pit_out_raw, loop_pts, true, 20, None, Some(&pit_path));
    let pit_out = geom::resample_open(&pit_out, 41);
    let (lane_lo, lane_hi) = geom::pit_span_on_loop(loop_pts, &pit_path);
    let pit_out_pct = geom::pct_on_loop(loop_pts, *pit_out.last()?);

    let mut fields = serde_json::Map::new();
    fields.insert("pit_path".into(), Value::Array(pts_json(&pit_path)));
    fields.insert("pit_out".into(), Value::Array(pts_json(&pit_out)));
    fields.insert("pit_in_pct".into(), Value::Null);
    fields.insert("pit_span".into(), json!([round5(lane_lo), round5(lane_hi)]));
    fields.insert("pit_out_pct".into(), json!(round5(pit_out_pct)));

    if entry.len() >= 2 {
        let pit_in_seed = geom::resample_open(entry, 24);
        let mut pit_in =
            geom::connect_blend_to_loop(&pit_in_seed, loop_pts, false, 12, Some(24), None);
        if let Some(first) = pit_path.first() {
            if let Some(last) = pit_in.last_mut() {
                *last = *first;
            }
        }
        let pit_in = geom::resample_open(&pit_in, 24);
        fields.insert("pit_in".into(), Value::Array(pts_json(&pit_in)));
        if let Some(p0) = pit_in.first() {
            fields.insert(
                "pit_in_pct".into(),
                json!(round5(geom::pct_on_loop(loop_pts, *p0))),
            );
        }
    } else {
        fields.insert("pit_in_pct".into(), json!(round5(lane_lo)));
    }
    Some(fields)
}

fn round5(v: f32) -> f64 {
    (v as f64 * 1e5).round() / 1e5
}

fn suffix_pit_keys(
    fields: serde_json::Map<String, Value>,
    suffix: &str,
) -> serde_json::Map<String, Value> {
    if suffix.is_empty() {
        return fields;
    }
    fields
        .into_iter()
        .map(|(k, v)| (format!("{k}{suffix}"), v))
        .collect()
}

pub struct SaveResult {
    pub ok: bool,
    pub msg: String,
}

pub fn save_manual_track(
    tracks_dir: &Path,
    tid: Option<&Value>,
    loop_pts: &[(f32, f32)],
    entry: &[(f32, f32)],
    road: &[(f32, f32)],
    merge: &[(f32, f32)],
    entry2: &[(f32, f32)],
    road2: &[(f32, f32)],
    merge2: &[(f32, f32)],
    name: Option<&str>,
    start_finish: f32,
    corners: &[Value],
    num_turns: Option<i64>,
    alias_track_ids: &[i32],
    pit_speed_ms: f32,
    pit_lane_speed_pct: f32,
    pit_lane_speed_pct_2: f32,
    map_rotation: i32,
    map_mirror: bool,
    upload: bool,
) -> SaveResult {
    let Some(tid) = tid else {
        return SaveResult {
            ok: false,
            msg: "No TrackID — join a session on track, or import members HTML.".into(),
        };
    };
    if loop_pts.len() < 3 {
        return SaveResult {
            ok: false,
            msg: "No track loop loaded.".into(),
        };
    }
    if road.len() < 2 {
        return SaveResult {
            ok: false,
            msg: "Need at least 2 pit road points.".into(),
        };
    }
    if merge.len() < 2 {
        return SaveResult {
            ok: false,
            msg: "Need at least 2 merge points.".into(),
        };
    }
    let canonical = cloud::resolve_track_id(tracks_dir, tid).unwrap_or_else(|| tid.clone());
    if let Some(block) = cloud_blocks_track_save(&canonical) {
        return SaveResult {
            ok: false,
            msg: block,
        };
    }
    let Some(lane1) = build_manual_pit_lane_fields(loop_pts, entry, road, merge) else {
        return SaveResult {
            ok: false,
            msg: "Could not build pit geometry.".into(),
        };
    };
    let mut doc = build_loop_doc(
        tid,
        loop_pts,
        name,
        start_finish,
        corners,
        num_turns,
        alias_track_ids,
        map_rotation,
        map_mirror,
    );
    if let Some(obj) = doc.as_object_mut() {
        obj.extend(lane1.clone());
        if pit_speed_ms > 0.0 {
            obj.insert(
                "pit_speed".into(),
                json!((pit_speed_ms as f64 * 1000.0).round() / 1000.0),
            );
        }
        if (pit_lane_speed_pct - 1.0).abs() > 1e-6 {
            obj.insert(
                "pit_lane_speed_pct".into(),
                json!((pit_lane_speed_pct as f64 * 1e4).round() / 1e4),
            );
        }
        for k in [
            "pit_path_2",
            "pit_in_2",
            "pit_out_2",
            "pit_span_2",
            "pit_in_pct_2",
            "pit_out_pct_2",
            "pit_lane_speed_pct_2",
        ] {
            obj.remove(k);
        }
        if let Some(lane2) = build_manual_pit_lane_fields(loop_pts, entry2, road2, merge2) {
            obj.extend(suffix_pit_keys(lane2, "_2"));
            if (pit_lane_speed_pct_2 - 1.0).abs() > 1e-6 {
                obj.insert(
                    "pit_lane_speed_pct_2".into(),
                    json!((pit_lane_speed_pct_2 as f64 * 1e4).round() / 1e4),
                );
            }
        }
    }
    match write_track_json(tracks_dir, &canonical, &doc) {
        Ok(path) => {
            let mut msg = format!("Saved {}", path.display());
            if upload && cloud::can_write() {
                match cloud::upload_local(tracks_dir, &canonical) {
                    Ok(()) => msg.push_str(" Uploaded to cloud."),
                    Err(e) => msg.push_str(&format!(" Upload failed: {e}")),
                }
            }
            SaveResult { ok: true, msg }
        }
        Err(e) => SaveResult {
            ok: false,
            msg: e.to_string(),
        },
    }
}

pub fn save_pit_patch(
    tracks_dir: &Path,
    tid: Option<&Value>,
    loop_pts: &[(f32, f32)],
    entry: &[(f32, f32)],
    road: &[(f32, f32)],
    merge: &[(f32, f32)],
    entry2: &[(f32, f32)],
    road2: &[(f32, f32)],
    merge2: &[(f32, f32)],
    pit_speed_ms: f32,
    pit_lane_speed_pct: f32,
    pit_lane_speed_pct_2: f32,
    upload: bool,
) -> SaveResult {
    let Some(tid) = tid else {
        return SaveResult {
            ok: false,
            msg: "No TrackID.".into(),
        };
    };
    if loop_pts.len() < 3 {
        return SaveResult {
            ok: false,
            msg: "No track loop loaded.".into(),
        };
    }
    if road.len() < 2 || merge.len() < 2 {
        return SaveResult {
            ok: false,
            msg: "Need pit road + merge points.".into(),
        };
    }
    let canonical = cloud::resolve_track_id(tracks_dir, tid).unwrap_or_else(|| tid.clone());
    let path = cloud::track_file_path(tracks_dir, &canonical);
    let mut doc: Value = if path.is_file() {
        match fs_read_json(&path) {
            Ok(v) => v,
            Err(e) => {
                return SaveResult {
                    ok: false,
                    msg: e.to_string(),
                }
            }
        }
    } else {
        return SaveResult {
            ok: false,
            msg: format!("No local track file for {canonical} — use Save track first."),
        };
    };
    let Some(lane1) = build_manual_pit_lane_fields(loop_pts, entry, road, merge) else {
        return SaveResult {
            ok: false,
            msg: "Could not build pit geometry.".into(),
        };
    };
    if let Some(obj) = doc.as_object_mut() {
        obj.extend(lane1);
        obj.insert("pit_source".into(), json!("manual"));
        if pit_speed_ms > 0.0 {
            obj.insert(
                "pit_speed".into(),
                json!((pit_speed_ms as f64 * 1000.0).round() / 1000.0),
            );
        }
        if (pit_lane_speed_pct - 1.0).abs() > 1e-6 {
            obj.insert(
                "pit_lane_speed_pct".into(),
                json!((pit_lane_speed_pct as f64 * 1e4).round() / 1e4),
            );
        }
        if let Some(lane2) = build_manual_pit_lane_fields(loop_pts, entry2, road2, merge2) {
            obj.extend(suffix_pit_keys(lane2, "_2"));
            if (pit_lane_speed_pct_2 - 1.0).abs() > 1e-6 {
                obj.insert(
                    "pit_lane_speed_pct_2".into(),
                    json!((pit_lane_speed_pct_2 as f64 * 1e4).round() / 1e4),
                );
            }
        }
    }
    match write_track_json(tracks_dir, &canonical, &doc) {
        Ok(path) => {
            let mut msg = format!("Saved pit {}", path.display());
            if upload && cloud::can_write() {
                match cloud::upload_local(tracks_dir, &canonical) {
                    Ok(()) => msg.push_str(" Uploaded to cloud."),
                    Err(e) => msg.push_str(&format!(" Upload failed: {e}")),
                }
            }
            SaveResult { ok: true, msg }
        }
        Err(e) => SaveResult {
            ok: false,
            msg: e.to_string(),
        },
    }
}

pub fn save_loop_only(
    tracks_dir: &Path,
    tid: Option<&Value>,
    loop_pts: &[(f32, f32)],
    name: Option<&str>,
    start_finish: f32,
    corners: &[Value],
    num_turns: Option<i64>,
    alias_track_ids: &[i32],
    map_rotation: i32,
    map_mirror: bool,
    upload: bool,
) -> SaveResult {
    let Some(tid) = tid else {
        return SaveResult {
            ok: false,
            msg: "No TrackID.".into(),
        };
    };
    if loop_pts.len() < 3 {
        return SaveResult {
            ok: false,
            msg: "No track loop.".into(),
        };
    }
    let canonical = cloud::resolve_track_id(tracks_dir, tid).unwrap_or_else(|| tid.clone());
    if let Some(block) = cloud_blocks_track_save(&canonical) {
        // Allow overwriting local loop if file exists? Python blocks new track save.
        // For loop-only save on existing, patch instead.
        let path = cloud::track_file_path(tracks_dir, &canonical);
        if !path.is_file() {
            return SaveResult {
                ok: false,
                msg: block,
            };
        }
    }
    let path = cloud::track_file_path(tracks_dir, &canonical);
    let mut doc = if path.is_file() {
        fs_read_json(&path).unwrap_or_else(|_| {
            build_loop_doc(
                tid,
                loop_pts,
                name,
                start_finish,
                corners,
                num_turns,
                alias_track_ids,
                map_rotation,
                map_mirror,
            )
        })
    } else {
        if let Some(block) = cloud_blocks_track_save(&canonical) {
            return SaveResult {
                ok: false,
                msg: block,
            };
        }
        build_loop_doc(
            tid,
            loop_pts,
            name,
            start_finish,
            corners,
            num_turns,
            alias_track_ids,
            map_rotation,
            map_mirror,
        )
    };
    if let Some(obj) = doc.as_object_mut() {
        obj.insert("points".into(), Value::Array(pts_json(loop_pts)));
        obj.insert("start_finish".into(), json!(start_finish));
        obj.insert("corners".into(), json!(corners));
        obj.insert("map_rotation".into(), json!(map_rotation));
        obj.insert("map_mirror".into(), json!(map_mirror));
        if let Some(n) = name {
            obj.insert("name".into(), json!(n));
        }
    }
    match write_track_json(tracks_dir, &canonical, &doc) {
        Ok(path) => {
            let mut msg = format!("Saved loop {}", path.display());
            if upload && cloud::can_write() {
                match cloud::upload_local(tracks_dir, &canonical) {
                    Ok(()) => msg.push_str(" Uploaded to cloud."),
                    Err(e) => msg.push_str(&format!(" Upload failed: {e}")),
                }
            }
            SaveResult { ok: true, msg }
        }
        Err(e) => SaveResult {
            ok: false,
            msg: e.to_string(),
        },
    }
}

fn fs_read_json(path: &Path) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}
