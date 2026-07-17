//! Members-site SVG layer helpers (Python `svg_layers_to_track` S/F + turns subset).

use regex::Regex;
use serde_json::{json, Value};
use std::sync::OnceLock;

use super::geom::pct_on_loop;
use super::path_sample::{flatten_path_d, paths_d_in_svg};

/// Normalization params matching Python `_normalize_loop` (no pad).
#[derive(Debug, Clone, Copy)]
pub struct NormParams {
    pub min_x: f32,
    pub min_y: f32,
    pub scale: f32,
}

pub fn extract_layers_from_html(html: &str) -> Layers {
    Layers {
        turns: layer_svg(html, "turn-numbers"),
        start_finish: layer_svg(html, "start-finish"),
    }
}

#[derive(Debug, Default, Clone)]
pub struct Layers {
    pub turns: Option<String>,
    pub start_finish: Option<String>,
}

fn layer_svg(html: &str, class_token: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r#"(?is)<div[^>]*class="[^"]*\b{}\b[^"]*"[^>]*>\s*(<svg[\s\S]*?</svg>)"#,
        regex::escape(class_token)
    ))
    .ok()?;
    re.captures(html)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

/// Normalize SVG-space points to 0–1 (Python `_normalize_loop`, no pad).
pub fn normalize_loop(raw: &[(f32, f32)]) -> (Vec<(f32, f32)>, NormParams) {
    if raw.is_empty() {
        return (
            vec![],
            NormParams {
                min_x: 0.0,
                min_y: 0.0,
                scale: 1.0,
            },
        );
    }
    let min_x = raw.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = raw.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let min_y = raw.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_y = raw.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    let scale = (max_x - min_x).max(max_y - min_y).max(1e-6);
    let pts = raw
        .iter()
        .map(|(x, y)| ((x - min_x) / scale, (y - min_y) / scale))
        .collect();
    (
        pts,
        NormParams {
            min_x,
            min_y,
            scale,
        },
    )
}

pub fn reorder_loop(loop_pts: &[(f32, f32)], sf_idx: usize) -> Vec<(f32, f32)> {
    if loop_pts.is_empty() {
        return vec![];
    }
    let i = sf_idx % loop_pts.len();
    let mut out = Vec::with_capacity(loop_pts.len());
    out.extend_from_slice(&loop_pts[i..]);
    out.extend_from_slice(&loop_pts[..i]);
    out
}

fn path_bbox_area(pts: &[(f32, f32)]) -> f32 {
    if pts.is_empty() {
        return 0.0;
    }
    let min_x = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    (max_x - min_x) * (max_y - min_y)
}

/// Paths in the start-finish layer, smallest bbox first (stripe before arrow).
pub fn sf_paths_sorted(sf_svg: Option<&str>) -> Vec<Vec<(f32, f32)>> {
    let mut paths = Vec::new();
    for d in paths_d_in_svg(sf_svg.unwrap_or("")) {
        if let Ok(pts) = flatten_path_d(&d) {
            if !pts.is_empty() {
                paths.push(pts);
            }
        }
    }
    paths.sort_by(|a, b| {
        path_bbox_area(a)
            .partial_cmp(&path_bbox_area(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    paths
}

pub fn sf_stripe_centroid(sf_svg: Option<&str>) -> Option<(f32, f32)> {
    let paths = sf_paths_sorted(sf_svg);
    let pts = paths.first()?;
    let n = pts.len() as f32;
    Some((
        pts.iter().map(|p| p.0).sum::<f32>() / n,
        pts.iter().map(|p| p.1).sum::<f32>() / n,
    ))
}

pub fn sf_anchor_point(sf_svg: Option<&str>) -> Option<(f32, f32)> {
    sf_stripe_centroid(sf_svg)
}

/// Unit vector of the start-finish direction arrow (SVG coords, Y down).
pub fn sf_arrow_direction(sf_svg: Option<&str>) -> Option<(f32, f32)> {
    let paths = sf_paths_sorted(sf_svg);
    if paths.len() < 2 {
        return None;
    }
    let arrow_pts = paths.last()?;
    let (sfx, sfy) = sf_stripe_centroid(sf_svg)?;
    let mut best_near = f32::MAX;
    let mut best_far = -1.0f32;
    let mut near_pt = arrow_pts[0];
    let mut far_pt = arrow_pts[0];
    for &p in arrow_pts {
        let d = ((p.0 - sfx).powi(2) + (p.1 - sfy).powi(2)).sqrt();
        if d < best_near {
            best_near = d;
            near_pt = p;
        }
        if d > best_far {
            best_far = d;
            far_pt = p;
        }
    }
    let dx = far_pt.0 - near_pt.0;
    let dy = far_pt.1 - near_pt.1;
    let ln = (dx * dx + dy * dy).sqrt();
    if ln < 1e-6 {
        return None;
    }
    Some((dx / ln, dy / ln))
}

/// Loop index and exact point where the vertical stripe crosses the loop.
pub fn sf_stripe_crossing(
    loop_pts: &[(f32, f32)],
    sf_svg: Option<&str>,
) -> Option<(usize, (f32, f32))> {
    let paths = sf_paths_sorted(sf_svg);
    if paths.is_empty() || loop_pts.len() < 2 {
        return None;
    }
    let stripe = &paths[0];
    let stripe_x = stripe.iter().map(|p| p.0).sum::<f32>() / stripe.len() as f32;
    let ys: Vec<f32> = stripe.iter().map(|p| p.1).collect();
    let y_min = ys.iter().cloned().fold(f32::MAX, f32::min);
    let y_max = ys.iter().cloned().fold(f32::MIN, f32::max);
    let y_mid = (y_min + y_max) * 0.5;
    let y_band = (y_max - y_min).max(40.0) * 2.5;

    let n = loop_pts.len();
    let mut best: Option<(f32, usize, (f32, f32))> = None;
    for i in 0..n {
        let a = loop_pts[i];
        let b = loop_pts[(i + 1) % n];
        let xmin = a.0.min(b.0);
        let xmax = a.0.max(b.0);
        if stripe_x < xmin - 1e-6 || stripe_x > xmax + 1e-6 {
            continue;
        }
        let dx = b.0 - a.0;
        if dx.abs() < 1e-9 {
            continue;
        }
        let t = ((stripe_x - a.0) / dx).clamp(0.0, 1.0);
        let cy = a.1 + t * (b.1 - a.1);
        if (cy - y_mid).abs() > y_band {
            continue;
        }
        let score = (cy - y_mid).abs();
        if best.map(|(s, _, _)| score < s).unwrap_or(true) {
            best = Some((score, i, (stripe_x, cy)));
        }
    }
    if let Some((_, i, pt)) = best {
        return Some((i, pt));
    }
    let pt = sf_stripe_centroid(sf_svg)?;
    let pct = pct_on_loop(loop_pts, pt);
    Some((((pct * n as f32) as usize) % n, pt))
}

pub fn detect_sf_svg(loop_pts: &[(f32, f32)], sf_svg: Option<&str>) -> usize {
    if let Some((i, _)) = sf_stripe_crossing(loop_pts, sf_svg) {
        return i;
    }
    if let Some(pt) = sf_anchor_point(sf_svg) {
        if !loop_pts.is_empty() {
            let pct = pct_on_loop(loop_pts, pt);
            return ((pct * loop_pts.len() as f32) as usize) % loop_pts.len();
        }
    }
    loop_pts
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Rotate to S/F at index 0 and match members arrow driving direction.
pub fn align_loop_from_sf(raw: &[(f32, f32)], sf_svg: Option<&str>) -> Vec<(f32, f32)> {
    if raw.len() < 3 {
        return raw.to_vec();
    }
    let sf_idx = detect_sf_svg(raw, sf_svg);
    let mut loop_pts = reorder_loop(raw, sf_idx);
    if let Some(arrow) = sf_arrow_direction(sf_svg) {
        if loop_pts.len() >= 2 {
            let dx = loop_pts[1].0 - loop_pts[0].0;
            let dy = loop_pts[1].1 - loop_pts[0].1;
            let ln = (dx * dx + dy * dy).sqrt().max(1e-6);
            if (dx / ln) * arrow.0 + (dy / ln) * arrow.1 < 0.0 {
                let first = loop_pts[0];
                let mut rest: Vec<_> = loop_pts[1..].to_vec();
                rest.reverse();
                loop_pts = std::iter::once(first).chain(rest).collect();
            }
        }
    } else {
        loop_pts = super::geom::ensure_ccw(&loop_pts);
    }
    if let Some((_, pt)) = sf_stripe_crossing(&loop_pts, sf_svg) {
        if !loop_pts.is_empty() {
            loop_pts[0] = pt;
        }
    }
    loop_pts
}

/// Turn labels from members SVG; positions normalized with the track loop.
pub fn parse_turn_numbers(
    svg_text: &str,
    loop_pts: &[(f32, f32)],
    norm: NormParams,
    flip_y: bool,
) -> Vec<Value> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let pat = PAT.get_or_init(|| {
        Regex::new(r#"(?i)<text[^>]*transform="translate\(([^)]+)\)"[^>]*>([^<]+)</text>"#)
            .expect("turn regex")
    });
    let mut corners = Vec::new();
    for m in pat.captures_iter(svg_text) {
        let parts: Vec<&str> = m
            .get(1)
            .map(|g| g.as_str().trim())
            .unwrap_or("")
            .split(|c: char| c.is_whitespace() || c == ',')
            .filter(|s| !s.is_empty())
            .collect();
        if parts.len() < 2 {
            continue;
        }
        let Ok(tx) = parts[0].parse::<f32>() else {
            continue;
        };
        let Ok(ty) = parts[1].parse::<f32>() else {
            continue;
        };
        let label = m.get(2).map(|g| g.as_str().trim()).unwrap_or("");
        if label.is_empty() {
            continue;
        }
        let nx = (tx - norm.min_x) / norm.scale;
        let mut ny = (ty - norm.min_y) / norm.scale;
        if flip_y {
            ny = 1.0 - ny;
        }
        corners.push(json!({
            "pct": (pct_on_loop(loop_pts, (nx, ny)) as f64 * 1e5).round() / 1e5,
            "label": label,
        }));
    }
    corners.sort_by(|a, b| {
        let pa = a.get("pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let pb = b.get("pct").and_then(|v| v.as_f64()).unwrap_or(0.0);
        pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal)
    });
    corners
}

/// Place corner labels at quadrant extrema of the resampled loop.
pub fn oval_corners(loop_pts: &[(f32, f32)], n: usize) -> Vec<Value> {
    if loop_pts.len() < 8 || n == 0 {
        return vec![];
    }
    let cx = loop_pts.iter().map(|p| p.0).sum::<f32>() / loop_pts.len() as f32;
    let cy = loop_pts.iter().map(|p| p.1).sum::<f32>() / loop_pts.len() as f32;
    let labels = ["1", "2", "3", "4"];
    let mut corners = Vec::new();
    for (qi, label) in labels.iter().enumerate().take(n.min(4)) {
        let angle_lo = qi as f32 * std::f32::consts::FRAC_PI_2;
        let angle_hi = (qi as f32 + 1.0) * std::f32::consts::FRAC_PI_2;
        let mut best_i = 0usize;
        let mut best_score = f32::NEG_INFINITY;
        for (i, p) in loop_pts.iter().enumerate() {
            let mut ang = (p.1 - cy).atan2(p.0 - cx);
            if ang < 0.0 {
                ang += std::f32::consts::TAU;
            }
            if ang >= angle_lo && ang < angle_hi {
                let r = ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt();
                if r > best_score {
                    best_score = r;
                    best_i = i;
                }
            }
        }
        corners.push(json!({
            "pct": (pct_on_loop(loop_pts, loop_pts[best_i]) as f64 * 1e5).round() / 1e5,
            "label": *label,
        }));
    }
    corners
}

/// Members SVG ovals label 4,3,2,1 along lap %; iRacing uses 1,2,3,4.
pub fn iracing_oval_label(label: &str, num_turns: i64) -> String {
    if let Ok(n) = label.parse::<i64>() {
        if n >= 1 && n <= num_turns {
            return (num_turns + 1 - n).to_string();
        }
    }
    label.to_string()
}

pub fn apply_iracing_oval_labels(corners: Vec<Value>, num_turns: i64) -> Vec<Value> {
    if num_turns < 2 {
        return corners;
    }
    corners
        .into_iter()
        .filter_map(|c| {
            let label = c.get("label").and_then(|v| v.as_str())?;
            let mut out = c.clone();
            if let Some(obj) = out.as_object_mut() {
                obj.insert(
                    "label".into(),
                    json!(iracing_oval_label(label, num_turns)),
                );
            }
            Some(out)
        })
        .collect()
}
