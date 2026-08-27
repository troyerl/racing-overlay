//! Local + Mongo persistence for race lap docs and track personal bests.

use std::fs;
use std::path::PathBuf;

use crate::cloud;
use crate::paths;
use crate::telemetry::{RaceDoc, StoredLap, TrackPbDoc};

pub fn races_dir() -> PathBuf {
    let d = paths::data_dir().join("races");
    let _ = fs::create_dir_all(&d);
    d
}

pub fn track_pbs_dir() -> PathBuf {
    let d = paths::data_dir().join("track_pbs");
    let _ = fs::create_dir_all(&d);
    d
}

fn sanitize_car(car_path: &str) -> String {
    car_path
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn track_pb_path(track_id: i32, car_path: &str) -> PathBuf {
    track_pbs_dir().join(format!("{track_id}_{}.json", sanitize_car(car_path)))
}

pub fn race_path(subsession_id: i32, track_id: i32) -> PathBuf {
    let key = if subsession_id > 0 {
        subsession_id.to_string()
    } else {
        format!(
            "local_{track_id}_{}",
            chrono::Utc::now().format("%Y%m%d%H%M%S")
        )
    };
    races_dir().join(format!("{key}.json"))
}

pub fn load_track_pb(track_id: i32, car_path: &str) -> Option<StoredLap> {
    if track_id <= 0 || car_path.is_empty() {
        return None;
    }
    let path = track_pb_path(track_id, car_path);
    if let Ok(text) = fs::read_to_string(&path) {
        if let Ok(doc) = serde_json::from_str::<TrackPbDoc>(&text) {
            return Some(StoredLap {
                driver_name: "Me".into(),
                car_number: String::new(),
                is_player: true,
                lap_time_s: doc.lap_time_s,
                lap_number: 0,
                samples: doc.samples,
                markers: doc.markers,
            });
        }
    }
    // Optional cloud pull when local missing.
    if cloud::read_available() {
        if let Ok(Some(remote)) = cloud::fetch_track_pb(track_id, car_path) {
            let _ = save_track_pb_doc(&remote, false);
            return Some(StoredLap {
                driver_name: "Me".into(),
                car_number: String::new(),
                is_player: true,
                lap_time_s: remote.lap_time_s,
                lap_number: 0,
                samples: remote.samples,
                markers: remote.markers,
            });
        }
    }
    None
}

pub fn save_track_pb_doc(doc: &TrackPbDoc, upload: bool) -> anyhow::Result<()> {
    let path = track_pb_path(doc.track_id, &doc.car_path);
    let v = serde_json::to_value(doc)?;
    cloud::write_json_atomic(&path, &v)?;
    if upload && cloud::can_write() {
        cloud::upsert_track_pb(&v)?;
    }
    Ok(())
}

pub fn save_race_doc(doc: &RaceDoc, upload: bool) -> anyhow::Result<()> {
    let path = race_path(doc.subsession_id, doc.track_id);
    let v = serde_json::to_value(doc)?;
    cloud::write_json_atomic(&path, &v)?;
    if upload && cloud::can_write() {
        cloud::upsert_race(&v)?;
    }
    Ok(())
}
