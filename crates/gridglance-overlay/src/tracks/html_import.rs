//! Members-site HTML/SVG → racing loop (Python `svg_layers_to_track_v2`).

use regex::Regex;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::OnceLock;

use super::layers::{
    align_loop_from_sf, apply_iracing_oval_labels, extract_layers_from_html, labels_run_backwards,
    normalize_loop, oval_corners, parse_turn_numbers,
};
use super::path_sample::sample_best_subpath;
use super::pit_import::{extract_pit_polylines_svg, normalize_pit_polylines};
use super::build_manual_pit_lane_fields;
use crate::track_path::PitLane;

static TRACK_MAP_RE: OnceLock<Regex> = OnceLock::new();

fn track_map_re() -> &'static Regex {
    TRACK_MAP_RE
        .get_or_init(|| Regex::new(r#"(?i)id\s*=\s*["']track-map-(\d+)["']"#).expect("regex"))
}

pub fn parse_track_id_from_html(html: &str) -> Option<i64> {
    if let Some(c) = track_map_re().captures(html) {
        return c.get(1)?.as_str().parse().ok();
    }
    let doc = Html::parse_document(html);
    let sel = Selector::parse("[id]").ok()?;
    let re = Regex::new(r"(?i)^track-map-(\d+)$").ok()?;
    for el in doc.select(&sel) {
        if let Some(id) = el.value().attr("id") {
            if let Some(c) = re.captures(id.trim()) {
                return c.get(1)?.as_str().parse().ok();
            }
        }
    }
    None
}

/// Full schema-2 import result (loop + optional HTML pit layer).
#[derive(Debug, Clone)]
pub struct ImportDoc {
    pub track_id: Option<i64>,
    pub name: String,
    pub points: Vec<(f32, f32)>,
    pub corners: Vec<Value>,
    pub num_turns: Option<i64>,
    pub start_finish: f32,
    /// Built from members `#Pitroad` / `#Mergeline` when stitching succeeds.
    pub pit: Option<PitLane>,
    /// Raw JSON pit fields for `to_json` (same shape as Track Scan save).
    pub pit_fields: Option<serde_json::Map<String, Value>>,
}

impl ImportDoc {
    pub fn to_json(&self) -> Value {
        let points: Vec<Value> = self
            .points
            .iter()
            .map(|(x, y)| {
                json!([
                    ((*x as f64) * 1e7).round() / 1e7,
                    ((*y as f64) * 1e7).round() / 1e7
                ])
            })
            .collect();
        let pit_source = if self.pit_fields.is_some() {
            "html"
        } else {
            "manual"
        };
        let mut doc = json!({
            "schema": 2,
            "import_version": 2,
            "pit_source": pit_source,
            "start_finish": self.start_finish,
            "points": points,
            "corners": self.corners,
            "name": self.name,
        });
        if let Some(tid) = self.track_id {
            doc.as_object_mut()
                .unwrap()
                .insert("track_id".into(), json!(tid));
        }
        if let Some(n) = self.num_turns {
            if n > 0 {
                doc.as_object_mut()
                    .unwrap()
                    .insert("num_turns".into(), json!(n));
            }
        }
        if let Some(fields) = &self.pit_fields {
            if let Some(obj) = doc.as_object_mut() {
                for (k, v) in fields {
                    obj.insert(k.clone(), v.clone());
                }
            }
        }
        doc
    }
}

/// Full v2 import from HTML text.
pub fn import_loop_doc(
    html: &str,
    num_samples: usize,
    num_corners: usize,
    start_finish: f32,
) -> anyhow::Result<ImportDoc> {
    let tid = parse_track_id_from_html(html);
    let scraped = Html::parse_document(html);
    let d_attr = resolve_active_path_d(&scraped)
        .ok_or_else(|| anyhow::anyhow!("Could not find active-config SVG path"))?;
    let raw = sample_best_subpath(&d_attr, num_samples)?;
    if raw.len() < 3 {
        anyhow::bail!("SVG path produced too few points");
    }

    let layers = extract_layers_from_html(html);
    let sf_svg = layers.start_finish.as_deref();
    let aligned = align_loop_from_sf(&raw, sf_svg);
    let (normalized, norm) = normalize_loop(&aligned);

    let labelled = layers
        .turns
        .as_deref()
        .map(|turns| parse_turn_numbers(turns, &normalized, norm, false))
        .filter(|c| !c.is_empty());
    let from_turns_layer = labelled.is_some();
    let mut corners = labelled.unwrap_or_else(|| {
        if num_corners > 0 {
            oval_corners(&normalized, num_corners)
        } else {
            vec![]
        }
    });

    let n_turns = if !corners.is_empty() {
        Some(corners.len() as i64)
    } else if num_corners > 0 {
        Some(num_corners as i64)
    } else {
        None
    };

    if let Some(n) = n_turns {
        // Synthesized oval corners keep the historical unconditional flip;
        // real labels are only flipped when they actually count down the lap.
        if n >= 2 && (!from_turns_layer || labels_run_backwards(&corners)) {
            corners = apply_iracing_oval_labels(corners, n);
        }
    }

    let name = tid
        .map(|t| format!("Track {t}"))
        .unwrap_or_else(|| "Imported track".into());

    let (pit, pit_fields) = match layers.pit.as_deref().and_then(extract_pit_polylines_svg) {
        Some(svg_pit) => {
            let (road, merge, entry) = normalize_pit_polylines(&svg_pit, norm);
            match build_manual_pit_lane_fields(&normalized, &entry, &road, &merge) {
                Some(fields) => {
                    let pit = pit_lane_from_fields(&fields);
                    (Some(pit), Some(fields))
                }
                None => (None, None),
            }
        }
        None => (None, None),
    };

    Ok(ImportDoc {
        track_id: tid,
        name,
        points: normalized,
        corners,
        num_turns: n_turns,
        start_finish,
        pit,
        pit_fields,
    })
}

fn pit_lane_from_fields(fields: &serde_json::Map<String, Value>) -> PitLane {
    let poly = |key: &str| -> Vec<(f32, f32)> {
        fields
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| {
                        let a = p.as_array()?;
                        Some((a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    let span = fields.get("pit_span").and_then(|v| {
        let a = v.as_array()?;
        Some((
            a.first()?.as_f64()? as f32,
            a.get(1)?.as_f64()? as f32,
        ))
    });
    PitLane {
        path: poly("pit_path"),
        entry: poly("pit_in"),
        exit: poly("pit_out"),
        in_pct: fields
            .get("pit_in_pct")
            .and_then(|v| v.as_f64())
            .map(|p| p as f32),
        out_pct: fields
            .get("pit_out_pct")
            .and_then(|v| v.as_f64())
            .map(|p| p as f32),
        span,
        speed_ms: None,
        lane_speed_pct: 1.0,
        source: Some("html".into()),
    }
}

/// Import from a `.html` / `.htm` / `.svg` file path.
pub fn import_track_source(
    path: &Path,
    num_samples: usize,
    num_corners: usize,
    start_finish: f32,
) -> anyhow::Result<ImportDoc> {
    let text = read_text(path)?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let html = if ext == "svg" {
        format!(r#"<div class="track-svg active-config"><svg>{text}</svg></div>"#)
    } else if ext == "html" || ext == "htm" || ext.is_empty() {
        text
    } else {
        anyhow::bail!("V2 import supports .html / .svg members exports, not .{ext}");
    };
    import_loop_doc(&html, num_samples, num_corners, start_finish)
}

fn read_text(path: &Path) -> anyhow::Result<String> {
    let raw = std::fs::read(path)?;
    for enc in ["utf-8-sig", "utf-8"] {
        let _ = enc;
    }
    // Try UTF-8 first, then lossy.
    match String::from_utf8(raw.clone()) {
        Ok(s) => Ok(s.trim_start_matches('\u{feff}').to_string()),
        Err(_) => Ok(String::from_utf8_lossy(&raw).into_owned()),
    }
}

fn resolve_active_path_d(doc: &Html) -> Option<String> {
    let sel = Selector::parse(".active-config").ok()?;
    for el in doc.select(&sel) {
        if el.value().name() == "path" {
            if let Some(d) = el.value().attr("d") {
                if !d.is_empty() {
                    return Some(d.to_string());
                }
            }
        }
        let mut best: Option<String> = None;
        for child in el.select(&Selector::parse("path").ok()?) {
            if let Some(d) = child.value().attr("d") {
                if best.as_ref().map(|b| d.len() > b.len()).unwrap_or(true) {
                    best = Some(d.to_string());
                }
            }
        }
        if best.is_some() {
            return best;
        }
    }
    let sel = Selector::parse("svg#inactive path.cls-1").ok()?;
    for el in doc.select(&sel) {
        if let Some(d) = el.value().attr("d") {
            if !d.is_empty() {
                return Some(d.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> String {
        let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // crate is crates/gridglance-overlay → repo root is ../..
        p.pop();
        p.pop();
        p.push("tests");
        p.push("fixtures");
        p.push(name);
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("fixture {}: {e}", p.display()))
    }

    #[test]
    fn compound_oval_loop_only() {
        let html = fixture("compound_oval.html");
        let doc = import_loop_doc(&html, 120, 4, 0.0).expect("import");
        assert_eq!(doc.points.len(), 120);
        assert!(doc.track_id.is_none());
        let xs: Vec<_> = doc.points.iter().map(|p| p.0).collect();
        let ys: Vec<_> = doc.points.iter().map(|p| p.1).collect();
        assert!(xs.iter().cloned().fold(f32::MAX, f32::min) >= -0.001);
        assert!(ys.iter().cloned().fold(f32::MAX, f32::min) >= -0.001);
        assert!(xs.iter().cloned().fold(f32::MIN, f32::max) <= 1.001);
        assert!(ys.iter().cloned().fold(f32::MIN, f32::max) <= 1.001);
        // Outer loop: no huge jumps between consecutive samples.
        let mut max_jump = 0.0f32;
        for w in doc.points.windows(2) {
            let d = ((w[0].0 - w[1].0).powi(2) + (w[0].1 - w[1].1).powi(2)).sqrt();
            max_jump = max_jump.max(d);
        }
        assert!(max_jump < 0.15, "max_jump={max_jump}");
    }

    #[test]
    fn rudskogen_track_id() {
        let html = fixture("rudskogen_pit.html");
        assert_eq!(parse_track_id_from_html(&html), Some(451));
        let doc = import_loop_doc(&html, 80, 4, 0.0).expect("import");
        assert_eq!(doc.track_id, Some(451));
        assert!(doc.points.len() >= 64);
    }

    /// Charlotte Roval draws S/F as `<rect>` + `<polygon>` and wraps turn
    /// labels in `<tspan>` — markup that used to be skipped entirely, leaving
    /// the loop rotated to its bottom-most point and running the wrong way.
    #[test]
    fn charlotte_roval_aligns_and_labels() {
        let html = fixture("charlotte_roval.html");
        let doc = import_loop_doc(&html, 400, 4, 0.0).expect("import");
        assert_eq!(doc.track_id, Some(554));
        assert_eq!(doc.num_turns, Some(15), "all 15 tspan labels must parse");

        // Loop starts on the S/F stripe (svg x≈963 → x_norm≈0.50), not at the
        // path's own first vertex (x_norm≈0.61).
        assert!(
            (doc.points[0].0 - 0.5).abs() < 0.01,
            "p0 must sit on the stripe, got {:?}",
            doc.points[0]
        );
        // Members arrow points +X, so lap % must advance that way.
        assert!(doc.points[1].0 > doc.points[0].0, "loop must follow arrow");

        let labels: Vec<i64> = doc
            .corners
            .iter()
            .filter_map(|c| c.get("label")?.as_str()?.parse().ok())
            .collect();
        let (up, down) = labels
            .windows(2)
            .fold((0, 0), |(u, d), w| match w[1].cmp(&w[0]) {
                std::cmp::Ordering::Greater => (u + 1, d),
                std::cmp::Ordering::Less => (u, d + 1),
                std::cmp::Ordering::Equal => (u, d),
            });
        assert!(
            up > down,
            "turn numbers must climb with lap %, got {labels:?}"
        );
    }

    /// Oran Park GP crosses itself at Yokohama Bridge, so the exporter breaks
    /// the stroke there. That leaves the config layer as a single out-and-back
    /// ribbon rather than an annulus, which used to lap the circuit twice.
    #[test]
    fn oran_park_bridge_is_folded_to_one_lap() {
        let html = fixture("oran_park_gp.html");
        let doc = import_loop_doc(&html, 400, 0, 0.0).expect("import");
        assert_eq!(doc.track_id, Some(202));
        let pts = &doc.points;
        let n = pts.len();

        // Direction comes from the members arrow, which points +X along the
        // bottom straight here — never from the path's own winding order.
        assert_eq!(
            crate::tracks::layers::sf_arrow_direction(
                crate::tracks::layers::extract_layers_from_html(&html)
                    .start_finish
                    .as_deref()
            )
            .map(|(x, _)| x > 0.9),
            Some(true),
            "fixture arrow must point +X"
        );
        assert!(
            pts[1].0 > pts[0].0 && (pts[1].1 - pts[0].1).abs() < 0.01,
            "lap must advance the way the arrow points, got {:?} -> {:?}",
            pts[0],
            pts[1]
        );

        // Turn names share the layer with the numbers; only the numbers are turns.
        let labels: Vec<&str> = doc
            .corners
            .iter()
            .filter_map(|c| c.get("label")?.as_str())
            .collect();
        assert_eq!(
            labels,
            ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"]
        );

        // A fold leaves almost every sample with a mirror partner one track
        // width away on the far side of the lap. A single traversal does not.
        let paired = (0..n)
            .filter(|&i| {
                (0..n).any(|j| {
                    let step = i.abs_diff(j);
                    if step.min(n - step) < n / 20 {
                        return false;
                    }
                    let d = ((pts[i].0 - pts[j].0).powi(2) + (pts[i].1 - pts[j].1).powi(2)).sqrt();
                    d < 0.02
                })
            })
            .count();
        assert!(
            paired < n / 4,
            "loop still folds back on itself: {paired}/{n} samples have a mirror partner"
        );

        // The bridge means the lap genuinely crosses over — that must survive.
        let loop_pts: Vec<(f32, f32)> = pts.clone();
        assert!(
            crate::track_path::loop_self_crossing(&loop_pts),
            "Yokohama Bridge crossing must be present in the loop"
        );

        // No sample-to-sample jump big enough to be a chord across the map.
        let max_jump = pts
            .windows(2)
            .map(|w| ((w[0].0 - w[1].0).powi(2) + (w[0].1 - w[1].1).powi(2)).sqrt())
            .fold(0.0f32, f32::max);
        assert!(max_jump < 0.1, "max_jump={max_jump}");
    }

    #[test]
    fn schema2_json_shape() {
        let html = fixture("compound_oval.html");
        let doc = import_loop_doc(&html, 80, 4, 0.0).unwrap();
        let j = doc.to_json();
        assert_eq!(j["schema"], 2);
        assert_eq!(j["import_version"], 2);
        assert!(j["points"].as_array().unwrap().len() == 80);
        // Compound oval fixture has a stitchable pit layer.
        assert_eq!(j["pit_source"], "html");
        assert!(
            j.get("pit_path")
                .and_then(|p| p.as_array())
                .map(|a| a.len() >= 8)
                .unwrap_or(false),
            "expected pit_path from HTML"
        );
        assert!(
            j.get("pit_out")
                .and_then(|p| p.as_array())
                .map(|a| a.len() >= 4)
                .unwrap_or(false),
            "expected pit_out from HTML"
        );
    }

    #[test]
    fn rudskogen_imports_pit_road() {
        let html = fixture("rudskogen_pit.html");
        let doc = import_loop_doc(&html, 80, 4, 0.0).expect("import");
        let pit = doc.pit.as_ref().expect("rudskogen should stitch a pit road");
        assert!(pit.path.len() >= 8, "pit_path len={}", pit.path.len());
        assert!(pit.exit.len() >= 4, "pit_out len={}", pit.exit.len());
        assert_eq!(doc.to_json()["pit_source"], "html");
    }

    #[test]
    fn iowa_oval_pit_stays_on_frontstretch() {
        let html = fixture("iowa_oval.html");
        let doc = import_loop_doc(&html, 120, 4, 0.0).expect("import");
        assert_eq!(doc.track_id, Some(559));
        let pit = doc.pit.as_ref().expect("iowa should import pit");
        assert!(pit.path.len() >= 8);
        assert!(pit.exit.len() >= 2);
        // No infield-spanning entry chord.
        assert!(
            pit.entry.len() < 2,
            "iowa should not invent a full-width pit_in, got {}",
            pit.entry.len()
        );
        let xs: Vec<f32> = pit.path.iter().map(|p| p.0).collect();
        let ys: Vec<f32> = pit.path.iter().map(|p| p.1).collect();
        let xspan = xs.iter().cloned().fold(f32::MIN, f32::max)
            - xs.iter().cloned().fold(f32::MAX, f32::min);
        let yspan = ys.iter().cloned().fold(f32::MIN, f32::max)
            - ys.iter().cloned().fold(f32::MAX, f32::min);
        assert!(
            xspan > yspan * 1.5,
            "pit road should run along the frontstretch, xspan={xspan} yspan={yspan}"
        );
        // No single segment should chord across the infield.
        let max_seg = pit
            .path
            .windows(2)
            .chain(pit.exit.windows(2))
            .map(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1))
            .fold(0.0f32, f32::max);
        assert!(max_seg < 0.35, "infield chord segment={max_seg}");
        assert_eq!(doc.to_json()["pit_source"], "html");
        assert!(
            !crate::track_path::pit_path_cuts_infield(&doc.points, &pit.path),
            "iowa pit must hug the frontstretch"
        );
    }

    #[test]
    fn iowa_oval_uses_annulus_centerline() {
        let html = fixture("iowa_oval.html");
        let scraped = Html::parse_document(&html);
        let d = resolve_active_path_d(&scraped).expect("active path");
        let subs = super::super::path_sample::split_subpaths(&d).expect("subs");
        assert!(
            super::super::ribbon::annulus_centerline(&subs).is_some(),
            "Iowa active config is an outer+inner annulus"
        );
        let outer = super::super::path_sample::pick_best_subpath(&subs).expect("outer");
        let mid = super::super::path_sample::sample_best_subpath(&d, 120).expect("mid");
        let inside = mid
            .iter()
            .filter(|&&p| iowa_point_in_ring(p, outer))
            .count();
        assert!(
            (inside as f32) > mid.len() as f32 * 0.85,
            "centreline must sit inside the outer kerb ({inside}/{})",
            mid.len()
        );
    }

    fn iowa_point_in_ring(p: (f32, f32), ring: &[(f32, f32)]) -> bool {
        let n = ring.len();
        if n < 3 {
            return false;
        }
        let mut inside = false;
        let mut j = n - 1;
        for i in 0..n {
            let (xi, yi) = ring[i];
            let (xj, yj) = ring[j];
            if (yi > p.1) != (yj > p.1) {
                let at_x = (xj - xi) * (p.1 - yi) / (yj - yi) + xi;
                if p.0 < at_x {
                    inside = !inside;
                }
            }
            j = i;
        }
        inside
    }

    #[test]
    fn lime_rock_gp_pit_exits_via_mergeline() {
        // Lime Rock puts `#Mergeline` on the left of the frontstretch. A tight
        // dash-jump cap used to drop it and synthesize an exit on the right
        // bend, baking a ~0.86 lap pit_span (long way around).
        let html = fixture("lime_rock_gp.html");
        let doc = import_loop_doc(&html, 120, 4, 0.0).expect("import");
        assert_eq!(doc.track_id, Some(353));
        let pit = doc.pit.as_ref().expect("lime rock should import pit");
        assert!(pit.path.len() >= 8);
        assert!(pit.exit.len() >= 2);

        let p0 = pit.path[0];
        let p1 = *pit.path.last().unwrap();
        let tip = *pit.exit.last().unwrap();
        // Exit/merge belongs on the left side of the pit road.
        assert!(
            tip.0 < 0.45 && tip.0 <= p0.0.max(p1.0),
            "merge tip should be on the left, tip={tip:?} path=({p0:?}->{p1:?})"
        );
        assert!(
            (p1.0 - tip.0).abs() + 1e-3 < (p0.0 - tip.0).abs()
                || (p1.0 - tip.0).hypot(p1.1 - tip.1) + 0.02
                    < (p0.0 - tip.0).hypot(p0.1 - tip.1),
            "exit should attach near path end, tip={tip:?} p0={p0:?} p1={p1:?}"
        );

        let (lo, hi) = pit.span.expect("span");
        let travel = (hi - lo).rem_euclid(1.0);
        assert!(
            travel < 0.35,
            "pit span should be the short frontstretch arc, travel={travel} span=({lo},{hi})"
        );
        assert_eq!(doc.to_json()["pit_source"], "html");
    }

    #[test]
    fn charlotte_imports_pit_road_without_mergeline() {
        // Charlotte draws merge ticks inside `#Pitroad` (no `#Mergeline`).
        let html = fixture("charlotte_roval.html");
        let doc = import_loop_doc(&html, 80, 4, 0.0).expect("import");
        assert!(doc.points.len() >= 40);
        let pit = doc.pit.as_ref().expect("charlotte should stitch pit from ticks");
        assert!(pit.path.len() >= 8, "pit_path len={}", pit.path.len());
        assert!(pit.exit.len() >= 4, "pit_out len={}", pit.exit.len());
        assert_eq!(doc.to_json()["pit_source"], "html");

        let p0 = pit.path[0];
        let p1 = *pit.path.last().unwrap();
        let e0 = pit.exit[0];
        let d_end = (p1.0 - e0.0).hypot(p1.1 - e0.1);
        let d_start = (p0.0 - e0.0).hypot(p0.1 - e0.1);
        // Exit blend should attach at the road end (travel direction).
        assert!(
            d_end < d_start + 0.05,
            "merge should meet road end, d_end={d_end} d_start={d_start}"
        );
        let (lo, hi) = pit.span.expect("span");
        let pct0 = super::super::geom::pct_on_loop(&doc.points, p0);
        let pct1 = super::super::geom::pct_on_loop(&doc.points, p1);
        assert!(
            (pct0 - lo).abs() < 0.05 || (pct0 - lo).rem_euclid(1.0) < 0.05,
            "span lo should match path start: lo={lo} pct0={pct0}"
        );
        assert!(
            (pct1 - hi).abs() < 0.05 || (pct1 - hi).rem_euclid(1.0) < 0.05,
            "span hi should match path end: hi={hi} pct1={pct1}"
        );
    }
}