//! Members-site HTML/SVG → racing loop (Python `svg_layers_to_track_v2`).

use regex::Regex;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::OnceLock;

use super::layers::{
    align_loop_from_sf, apply_iracing_oval_labels, extract_layers_from_html, normalize_loop,
    oval_corners, parse_turn_numbers,
};
use super::path_sample::sample_best_subpath;

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

/// Full schema-2 import result (loop only; pit authored manually).
#[derive(Debug, Clone)]
pub struct ImportDoc {
    pub track_id: Option<i64>,
    pub name: String,
    pub points: Vec<(f32, f32)>,
    pub corners: Vec<Value>,
    pub num_turns: Option<i64>,
    pub start_finish: f32,
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
        let mut doc = json!({
            "schema": 2,
            "import_version": 2,
            "pit_source": "manual",
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

    let mut corners = if let Some(ref turns) = layers.turns {
        parse_turn_numbers(turns, &normalized, norm, false)
    } else if num_corners > 0 {
        oval_corners(&normalized, num_corners)
    } else {
        vec![]
    };

    let n_turns = if !corners.is_empty() {
        Some(corners.len() as i64)
    } else if num_corners > 0 {
        Some(num_corners as i64)
    } else {
        None
    };

    if let Some(n) = n_turns {
        if n >= 2 {
            corners = apply_iracing_oval_labels(corners, n);
        }
    }

    let name = tid
        .map(|t| format!("Track {t}"))
        .unwrap_or_else(|| "Imported track".into());

    Ok(ImportDoc {
        track_id: tid,
        name,
        points: normalized,
        corners,
        num_turns: n_turns,
        start_finish,
    })
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
        format!(
            r#"<div class="track-svg active-config"><svg>{text}</svg></div>"#
        )
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
        // crate is overlay-rs/crates/gridglance-overlay → repo root is ../../..
        p.pop();
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

    #[test]
    fn schema2_json_shape() {
        let html = fixture("compound_oval.html");
        let doc = import_loop_doc(&html, 80, 4, 0.0).unwrap();
        let j = doc.to_json();
        assert_eq!(j["schema"], 2);
        assert_eq!(j["import_version"], 2);
        assert_eq!(j["pit_source"], "manual");
        assert!(j.get("pit_path").is_none());
        assert!(j["points"].as_array().unwrap().len() == 80);
    }
}
