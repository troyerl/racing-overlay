//! Track JSON load + path sampling (map MVP).

use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

/// One pit road: entry blend, lane, exit blend + lap-% extents.
#[derive(Debug, Clone, Default)]
pub struct PitLane {
    pub path: Vec<(f32, f32)>,
    pub entry: Vec<(f32, f32)>,
    pub exit: Vec<(f32, f32)>,
    /// Full route extents (diverge → rejoin), wrapping OK.
    pub in_pct: Option<f32>,
    pub out_pct: Option<f32>,
    /// Lane-only OnPitRoad span when known.
    pub span: Option<(f32, f32)>,
    pub speed_ms: Option<f32>,
    #[allow(dead_code)]
    pub lane_speed_pct: f32,
    #[allow(dead_code)]
    pub source: Option<String>,
}

impl PitLane {
    pub fn has_drawable(&self) -> bool {
        self.path.len() >= 2 || self.entry.len() >= 2 || self.exit.len() >= 2
    }

    /// Concatenated route for car placement (entry + lane + exit when blends on).
    pub fn route(&self, include_blends: bool) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        if include_blends && self.entry.len() >= 2 {
            out.extend_from_slice(&self.entry);
        }
        if self.path.len() >= 2 {
            out.extend_from_slice(&self.path);
        }
        if include_blends && self.exit.len() >= 2 {
            out.extend_from_slice(&self.exit);
        }
        out
    }

    pub fn all_points(&self) -> Vec<(f32, f32)> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.entry);
        out.extend_from_slice(&self.path);
        out.extend_from_slice(&self.exit);
        out
    }
}

/// Corner marker from track JSON.
#[derive(Debug, Clone, Default)]
pub struct CornerMark {
    pub pct: f32,
    pub label: String,
    pub ox: f32,
    pub oy: f32,
}

/// Closed track polyline in normalized (or raw) XY space.
#[derive(Debug, Clone, Default)]
pub struct TrackPath {
    pub points: Vec<(f32, f32)>,
    pub name: String,
    /// LapDistPct of start/finish (0..1).
    pub start_finish: f32,
    /// LapDistPct of pit exit merge onto the racing loop (0..1), when known.
    pub pit_out_pct: Option<f32>,
    /// Primary pit road geometry.
    pub pit: PitLane,
    /// Optional second pit road.
    pub pit2: PitLane,
    /// DRS activation ranges as (lo, hi) lap fractions.
    pub drs_zones: Vec<(f32, f32)>,
    /// Push-to-pass ranges as (lo, hi) lap fractions.
    pub p2p_zones: Vec<(f32, f32)>,
    pub corners: Vec<CornerMark>,
}

/// Directories to search for track JSON (user data tracks only).
pub fn tracks_search_dirs() -> Vec<PathBuf> {
    vec![crate::paths::tracks_dir()]
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

fn parse_polyline(v: Option<&Value>) -> Vec<(f32, f32)> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in arr {
        let Some(pair) = p.as_array() else {
            continue;
        };
        if pair.len() < 2 {
            continue;
        }
        let x = pair[0].as_f64().unwrap_or(f64::NAN) as f32;
        let y = pair[1].as_f64().unwrap_or(f64::NAN) as f32;
        if x.is_finite() && y.is_finite() {
            out.push((x, y));
        }
    }
    out
}

fn parse_span(v: Option<&Value>) -> Option<(f32, f32)> {
    let arr = v?.as_array()?;
    if arr.len() < 2 {
        return None;
    }
    let a = arr[0].as_f64()? as f32;
    let b = arr[1].as_f64()? as f32;
    if a.is_finite() && b.is_finite() {
        Some((a.rem_euclid(1.0), b.rem_euclid(1.0)))
    } else {
        None
    }
}

fn parse_pit_lane(v: &Value, suffix: &str) -> PitLane {
    let key = |base: &str| -> String {
        if suffix.is_empty() {
            base.to_string()
        } else {
            format!("{base}{suffix}")
        }
    };
    let mut path = parse_polyline(v.get(&key("pit_path")));
    if path.len() < 2 && suffix.is_empty() {
        path = parse_polyline(v.get("pit_lane_points"));
    }
    let entry = parse_polyline(v.get(&key("pit_in")));
    let exit = parse_polyline(v.get(&key("pit_out")));
    let in_pct = v
        .get(&key("pit_in_pct"))
        .and_then(|x| x.as_f64())
        .map(|p| (p as f32).rem_euclid(1.0));
    let out_pct = v
        .get(&key("pit_out_pct"))
        .and_then(|x| x.as_f64())
        .map(|p| (p as f32).rem_euclid(1.0));
    let span = parse_span(v.get(&key("pit_span")));
    let speed_ms = v
        .get("pit_speed")
        .and_then(|x| x.as_f64())
        .map(|s| s as f32)
        .filter(|s| *s > 0.0);
    let lane_speed_pct = v
        .get(&key("pit_lane_speed_pct"))
        .and_then(|x| x.as_f64())
        .map(|s| s as f32)
        .filter(|s| *s > 0.0)
        .unwrap_or(1.0);
    let source = if suffix.is_empty() {
        v.get("pit_source")
            .and_then(|x| x.as_str())
            .map(|s| s.to_string())
    } else {
        None
    };
    PitLane {
        path,
        entry,
        exit,
        in_pct,
        out_pct,
        span,
        speed_ms,
        lane_speed_pct,
        source,
    }
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
    let mut pit = parse_pit_lane(&v, "");
    let pit2 = parse_pit_lane(&v, "_2");
    let mut pit_out_pct = parse_pit_out_pct(&v, &pts);
    if pit_out_pct.is_none() {
        pit_out_pct = pit.out_pct;
    }
    if pit.out_pct.is_none() {
        pit.out_pct = pit_out_pct;
    }
    let target = n.clamp(64, 720);
    let resampled = resample_closed(&pts, target);
    let is_demo = path
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s == "_demo" || s == "1")
        .unwrap_or(false)
        || v.get("track_id")
            .and_then(|t| t.as_str())
            .map(|s| s == "_demo")
            .unwrap_or(false)
        || v.get("alias_track_ids")
            .and_then(|a| a.as_array())
            .map(|a| a.iter().any(|x| x.as_i64() == Some(1)))
            .unwrap_or(false);

    let corners = parse_corners(v.get("corners"));
    let mut tp = TrackPath {
        points: resampled,
        name,
        start_finish: start_finish.fract().rem_euclid(1.0),
        pit_out_pct,
        pit,
        pit2,
        drs_zones,
        p2p_zones,
        corners,
    };
    if is_demo && !tp.pit.has_drawable() {
        if let Some(synth) = synthesize_demo_pit(&tp.points) {
            tp.pit = synth;
            tp.pit_out_pct = tp.pit.out_pct.or(tp.pit_out_pct);
        }
    }
    Some(tp)
}

fn parse_corners(v: Option<&Value>) -> Vec<CornerMark> {
    let Some(arr) = v.and_then(|x| x.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (i, c) in arr.iter().enumerate() {
        let pct = if let Some(p) = c.get("pct").and_then(|x| x.as_f64()) {
            p as f32
        } else if let Some(a) = c.as_array() {
            a.first().and_then(|x| x.as_f64()).unwrap_or(0.0) as f32
        } else {
            continue;
        };
        let label = if let Some(s) = c.get("label").and_then(|x| x.as_str()) {
            s.to_string()
        } else if let Some(n) = c.get("label").and_then(|x| x.as_i64()) {
            n.to_string()
        } else if let Some(a) = c.as_array() {
            a.get(1)
                .and_then(|x| {
                    x.as_str()
                        .map(|s| s.to_string())
                        .or_else(|| x.as_i64().map(|n| n.to_string()))
                })
                .unwrap_or_else(|| (i + 1).to_string())
        } else {
            (i + 1).to_string()
        };
        let ox = c.get("ox").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        let oy = c.get("oy").and_then(|x| x.as_f64()).unwrap_or(0.0) as f32;
        if pct.is_finite() {
            out.push(CornerMark {
                pct: pct.rem_euclid(1.0),
                label,
                ox,
                oy,
            });
        }
    }
    out
}

fn parse_pit_out_pct(v: &Value, loop_pts: &[(f32, f32)]) -> Option<f32> {
    if let Some(p) = v.get("pit_out_pct").and_then(|x| x.as_f64()) {
        if p.is_finite() {
            return Some((p as f32).rem_euclid(1.0));
        }
    }
    // Fallback: project last pit_out point onto the racing loop.
    let arr = v.get("pit_out")?.as_array()?;
    let last = arr.last()?.as_array()?;
    if last.len() < 2 {
        return None;
    }
    let x = last[0].as_f64()? as f32;
    let y = last[1].as_f64()? as f32;
    if !x.is_finite() || !y.is_finite() || loop_pts.len() < 2 {
        return None;
    }
    Some(nearest_pct_on_loop(loop_pts, x, y))
}

/// Demo pit spans matching Python `demo_data.DEMO_PIT_*` + `_demo_pit_geometry`.
pub const DEMO_PIT_IN_PCT: f32 = 0.90;
pub const DEMO_PIT_OUT_PCT: f32 = 0.12;
pub const DEMO_PIT_LANE_LO: f32 = 0.95;
pub const DEMO_PIT_LANE_HI: f32 = 0.06;

/// Build inward-offset entry / lane / exit from a racing loop (demo / oval).
pub fn synthesize_demo_pit(pts: &[(f32, f32)]) -> Option<PitLane> {
    let n = pts.len();
    if n < 24 {
        return None;
    }
    let cx: f32 = pts.iter().map(|p| p.0).sum::<f32>() / n as f32;
    let cy: f32 = pts.iter().map(|p| p.1).sum::<f32>() / n as f32;
    let min_x = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let diag = ((max_x - min_x).hypot(max_y - min_y)).max(1e-6);
    let off = 0.045 * diag;

    let at = |pct: f32| -> (f32, f32) {
        let i = (((pct.rem_euclid(1.0)) * n as f32) as usize) % n;
        pts[i]
    };
    let inward = |p: (f32, f32), frac: f32| -> (f32, f32) {
        let dx = cx - p.0;
        let dy = cy - p.1;
        let ln = dx.hypot(dy).max(1e-6);
        (p.0 + dx / ln * off * frac, p.1 + dy / ln * off * frac)
    };
    let span_pts = |a: f32, b: f32, steps: usize, f0: f32, f1: f32| -> Vec<(f32, f32)> {
        let s = (b - a).rem_euclid(1.0);
        let mut out = Vec::with_capacity(steps + 1);
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            out.push(inward(at(a + s * t), f0 + (f1 - f0) * t));
        }
        out
    };

    Some(PitLane {
        entry: span_pts(DEMO_PIT_IN_PCT, DEMO_PIT_LANE_LO, 14, 0.0, 1.0),
        path: span_pts(DEMO_PIT_LANE_LO, DEMO_PIT_LANE_HI, 44, 1.0, 1.0),
        exit: span_pts(DEMO_PIT_LANE_HI, DEMO_PIT_OUT_PCT, 14, 1.0, 0.0),
        in_pct: Some(DEMO_PIT_IN_PCT),
        out_pct: Some(DEMO_PIT_OUT_PCT),
        span: Some((DEMO_PIT_LANE_LO, DEMO_PIT_LANE_HI)),
        speed_ms: Some(22.0),
        lane_speed_pct: 0.38,
        source: Some("schematic".into()),
    })
}

/// Open polyline sample: `t` in 0..1 along arc length.
pub fn point_on_open(pts: &[(f32, f32)], t: f32) -> (f32, f32) {
    if pts.is_empty() {
        return (0.5, 0.5);
    }
    if pts.len() == 1 {
        return pts[0];
    }
    let (cum, total) = arc_lengths(pts, false);
    if total <= 1e-6 {
        return pts[0];
    }
    let target = t.clamp(0.0, 1.0) * total;
    sample_at_dist(pts, &cum, target, false)
}

/// Map lap% through a wrapping [in_pct, out_pct] span onto 0..1 along the route.
pub fn route_t_for_pct(pct: f32, in_pct: f32, out_pct: f32) -> f32 {
    let span = (out_pct - in_pct).rem_euclid(1.0);
    if span <= 1e-6 {
        return 0.0;
    }
    ((pct - in_pct).rem_euclid(1.0) / span).clamp(0.0, 1.0)
}

/// Closest arc-length fraction on a closed loop to a model-space point.
pub fn nearest_pct_on_loop(pts: &[(f32, f32)], x: f32, y: f32) -> f32 {
    if pts.len() < 2 {
        return 0.0;
    }
    let (cum, total) = arc_lengths(pts, true);
    if total <= 1e-6 {
        return 0.0;
    }
    let mut best_d = f32::MAX;
    let mut best_s = 0.0_f32;
    let nseg = pts.len();
    for i in 0..nseg {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        let abx = b.0 - a.0;
        let aby = b.1 - a.1;
        let apx = x - a.0;
        let apy = y - a.1;
        let ab2 = abx * abx + aby * aby;
        let t = if ab2 > 1e-12 {
            ((apx * abx + apy * aby) / ab2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let px = a.0 + abx * t;
        let py = a.1 + aby * t;
        let dx = x - px;
        let dy = y - py;
        let d2 = dx * dx + dy * dy;
        if d2 < best_d {
            best_d = d2;
            let seg0 = cum[i];
            let seg1 = cum[i + 1];
            best_s = seg0 + (seg1 - seg0) * t;
        }
    }
    (best_s / total).rem_euclid(1.0)
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
    let mut tp = load_points(&path, 360)?;
    if track_id == 1 && !tp.pit.has_drawable() {
        if let Some(synth) = synthesize_demo_pit(&tp.points) {
            tp.pit = synth;
            tp.pit_out_pct = tp.pit.out_pct.or(tp.pit_out_pct);
        }
    }
    Some(tp)
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

    #[test]
    fn synthesize_demo_has_blends_and_lane() {
        let oval = oval_path(64);
        let pit = synthesize_demo_pit(&oval).expect("synth");
        assert!(pit.entry.len() >= 2);
        assert!(pit.path.len() >= 2);
        assert!(pit.exit.len() >= 2);
        assert!(pit.has_drawable());
        let route = pit.route(true);
        assert!(route.len() > pit.path.len());
        let t = route_t_for_pct(0.0, DEMO_PIT_IN_PCT, DEMO_PIT_OUT_PCT);
        assert!((0.0..=1.0).contains(&t));
    }
}
