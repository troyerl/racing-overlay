//! Track JSON load + path sampling (map MVP).

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// Closed track polyline in normalized (or raw) XY space.
#[derive(Debug, Clone, Default)]
pub struct TrackPath {
    pub points: Vec<(f32, f32)>,
    pub name: String,
    /// LapDistPct of start/finish (0..1).
    pub start_finish: f32,
    /// DRS activation ranges as (lo, hi) lap fractions.
    pub drs_zones: Vec<(f32, f32)>,
    /// Push-to-pass ranges as (lo, hi) lap fractions.
    pub p2p_zones: Vec<(f32, f32)>,
}

/// Directories to search for track JSON (user data + common relative paths).
pub fn tracks_search_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![crate::paths::tracks_dir()];
    if let Ok(cwd) = std::env::current_dir() {
        for rel in ["tracks", "../tracks", "../../tracks"] {
            let p = cwd.join(rel);
            if p.is_dir() {
                dirs.push(p);
            }
        }
    }
    // Dedup
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Find a track file for a numeric iRacing TrackID (or demo id).
pub fn find_track_file(track_id: i32) -> Option<PathBuf> {
    let id_str = track_id.to_string();
    for dir in tracks_search_dirs() {
        // Fast path: <id>.json
        let direct = dir.join(format!("{id_str}.json"));
        if direct.is_file() {
            return Some(direct);
        }
        // Demo convenience: id 1 → _demo.json
        if track_id == 1 {
            let demo = dir.join("_demo.json");
            if demo.is_file() {
                return Some(demo);
            }
        }
        // Scan JSON for matching track_id / aliases
        if let Ok(rd) = fs::read_dir(&dir) {
            for ent in rd.flatten() {
                let path = ent.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if json_matches_track_id(&path, track_id, &id_str) {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn json_matches_track_id(path: &Path, track_id: i32, id_str: &str) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return false;
    };
    if let Some(tid) = v.get("track_id") {
        if tid.as_i64() == Some(track_id as i64) {
            return true;
        }
        if tid.as_str() == Some(id_str) {
            return true;
        }
        // "_demo" accepted for demo track_id 1
        if track_id == 1 && tid.as_str() == Some("_demo") {
            return true;
        }
    }
    if let Some(arr) = v.get("alias_track_ids").and_then(|a| a.as_array()) {
        for a in arr {
            if a.as_i64() == Some(track_id as i64) {
                return true;
            }
            if a.as_str() == Some(id_str) {
                return true;
            }
        }
    }
    // Filename stem equals id
    path.file_stem().and_then(|s| s.to_str()) == Some(id_str)
}

/// Load `points` from a track JSON; resample to ~n points by arc length.
pub fn load_points(path: &Path, n: usize) -> Option<TrackPath> {
    let text = fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let raw = v.get("points")?.as_array()?;
    let mut pts = Vec::with_capacity(raw.len());
    for p in raw {
        let arr = p.as_array()?;
        if arr.len() < 2 {
            continue;
        }
        let x = arr[0].as_f64()? as f32;
        let y = arr[1].as_f64()? as f32;
        if x.is_finite() && y.is_finite() {
            pts.push((x, y));
        }
    }
    if pts.len() < 2 {
        return None;
    }
    let name = v
        .get("name")
        .and_then(|n| n.as_str())
        .unwrap_or("")
        .to_string();
    let start_finish = v
        .get("start_finish")
        .and_then(|x| x.as_f64())
        .unwrap_or(0.0) as f32;
    let drs_zones = parse_zone_ranges(v.get("drs_zones"));
    let p2p_zones = parse_zone_ranges(v.get("p2p_zones"));
    let target = n.clamp(64, 720);
    let resampled = resample_closed(&pts, target);
    Some(TrackPath {
        points: resampled,
        name,
        start_finish: start_finish.fract().rem_euclid(1.0),
        drs_zones,
        p2p_zones,
    })
}

fn parse_zone_ranges(v: Option<&Value>) -> Vec<(f32, f32)> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for z in arr {
        let Some(pair) = z.as_array() else {
            continue;
        };
        if pair.len() < 2 {
            continue;
        }
        let lo = pair[0].as_f64().unwrap_or(f64::NAN) as f32;
        let hi = pair[1].as_f64().unwrap_or(f64::NAN) as f32;
        if lo.is_finite() && hi.is_finite() {
            out.push((lo.rem_euclid(1.0), hi.rem_euclid(1.0)));
        }
    }
    out
}

/// Load track for id, or None if not found / invalid.
pub fn load_for_track_id(track_id: i32) -> Option<TrackPath> {
    let path = find_track_file(track_id)?;
    load_points(&path, 360)
}

fn arc_lengths(pts: &[(f32, f32)], closed: bool) -> (Vec<f32>, f32) {
    let mut cum = Vec::with_capacity(pts.len() + 1);
    cum.push(0.0);
    let mut total = 0.0;
    for w in pts.windows(2) {
        let dx = w[1].0 - w[0].0;
        let dy = w[1].1 - w[0].1;
        total += (dx * dx + dy * dy).sqrt();
        cum.push(total);
    }
    if closed && pts.len() >= 2 {
        let a = pts.last().unwrap();
        let b = pts.first().unwrap();
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        total += (dx * dx + dy * dy).sqrt();
        cum.push(total);
    }
    (cum, total)
}

fn resample_closed(pts: &[(f32, f32)], n: usize) -> Vec<(f32, f32)> {
    if pts.len() < 2 || n < 2 {
        return pts.to_vec();
    }
    let (cum, total) = arc_lengths(pts, true);
    if total <= 1e-6 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let target = total * (i as f32 / n as f32);
        out.push(sample_at_dist(pts, &cum, target, true));
    }
    out
}

fn sample_at_dist(pts: &[(f32, f32)], cum: &[f32], target: f32, closed: bool) -> (f32, f32) {
    let nseg = if closed {
        pts.len()
    } else {
        pts.len().saturating_sub(1)
    };
    for i in 0..nseg {
        let d0 = cum[i];
        let d1 = cum[i + 1];
        if target >= d0 && target <= d1 + 1e-6 {
            let (a, b) = if i + 1 < pts.len() {
                (pts[i], pts[i + 1])
            } else {
                (pts[i], pts[0])
            };
            let span = (d1 - d0).max(1e-9);
            let t = ((target - d0) / span).clamp(0.0, 1.0);
            return (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
        }
    }
    pts[0]
}

pub fn point_at(pts: &[(f32, f32)], pct: f32) -> (f32, f32) {
    if pts.len() < 2 {
        return pts.first().copied().unwrap_or((0.5, 0.5));
    }
    let mut p = pct.fract();
    if p < 0.0 {
        p += 1.0;
    }
    let (cum, total) = arc_lengths(pts, true);
    if total <= 1e-6 {
        return pts[0];
    }
    sample_at_dist(pts, &cum, p * total, true)
}

/// Unit tangent along the closed path at `pct` (for S/F tick).
pub fn tangent_at(pts: &[(f32, f32)], pct: f32) -> (f32, f32) {
    if pts.len() < 2 {
        return (1.0, 0.0);
    }
    let eps = 1.0 / pts.len().max(8) as f32;
    let a = point_at(pts, pct - eps);
    let b = point_at(pts, pct + eps);
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = (dx * dx + dy * dy).sqrt().max(1e-6);
    (dx / len, dy / len)
}

/// Fallback oval in 0..1 space (same as previous map stub).
pub fn oval_path(n: usize) -> Vec<(f32, f32)> {
    let n = n.max(16);
    (0..n)
        .map(|i| {
            let a = (i as f32 / n as f32) * std::f32::consts::TAU;
            (0.5 + 0.38 * a.cos(), 0.5 + 0.28 * a.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn point_at_wraps() {
        let oval = oval_path(64);
        let a = point_at(&oval, 0.0);
        let b = point_at(&oval, 1.0);
        assert!((a.0 - b.0).abs() < 0.05);
        assert!((a.1 - b.1).abs() < 0.05);
        let mid = point_at(&oval, 0.25);
        assert!(mid.0.is_finite() && mid.1.is_finite());
    }

    #[test]
    fn resample_keeps_count() {
        let oval = oval_path(32);
        let r = resample_closed(&oval, 100);
        assert_eq!(r.len(), 100);
    }
}
