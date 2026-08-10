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
        pit: layer_svg(html, "pit"),
    }
}

#[derive(Debug, Default, Clone)]
pub struct Layers {
    pub turns: Option<String>,
    pub start_finish: Option<String>,
    /// Members `track-svg pit` layer (`#Pitroad` / `#Mergeline`).
    pub pit: Option<String>,
}

/// Apply loop normalization to SVG-space points (same bbox as the racing line).
pub fn apply_norm(pts: &[(f32, f32)], norm: NormParams) -> Vec<(f32, f32)> {
    if norm.scale <= 1e-12 {
        return pts.to_vec();
    }
    pts.iter()
        .map(|(x, y)| ((x - norm.min_x) / norm.scale, (y - norm.min_y) / norm.scale))
        .collect()
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

/// Vertices of every `<rect>` in an SVG fragment, as closed corner loops.
fn rects_in_svg(svg_text: &str) -> Vec<Vec<(f32, f32)>> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let pat = PAT.get_or_init(|| Regex::new(r"(?is)<rect\b([^>]*)>").expect("rect regex"));
    let attr = |tag: &str, name: &str| -> Option<f32> {
        Regex::new(&format!(r#"(?i)\b{name}\s*=\s*["']([^"']+)["']"#))
            .ok()?
            .captures(tag)?
            .get(1)?
            .as_str()
            .trim()
            .parse()
            .ok()
    };
    pat.captures_iter(svg_text)
        .filter_map(|c| {
            let tag = c.get(1)?.as_str();
            let x = attr(tag, "x").unwrap_or(0.0);
            let y = attr(tag, "y").unwrap_or(0.0);
            let w = attr(tag, "width")?;
            let h = attr(tag, "height")?;
            // Four corners, not five — a repeated closing vertex would bias
            // the centroid that locates the stripe.
            Some(vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
        })
        .collect()
}

/// Vertices of every `<polygon>` / `<polyline>` in an SVG fragment.
fn polys_in_svg(svg_text: &str) -> Vec<Vec<(f32, f32)>> {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let pat = PAT.get_or_init(|| {
        Regex::new(r#"(?is)<poly(?:gon|line)\b[^>]*\bpoints\s*=\s*["']([^"']+)["']"#)
            .expect("poly regex")
    });
    pat.captures_iter(svg_text)
        .filter_map(|c| {
            let nums: Vec<f32> = c
                .get(1)?
                .as_str()
                .split(|ch: char| ch.is_whitespace() || ch == ',')
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect();
            let pts: Vec<(f32, f32)> = nums.chunks_exact(2).map(|p| (p[0], p[1])).collect();
            (pts.len() >= 2).then_some(pts)
        })
        .collect()
}

/// Shapes in the start-finish layer, smallest bbox first (stripe before arrow).
///
/// Members exports are not consistent about markup: some draw the stripe and
/// direction arrow as `<path d>`, others as `<rect>` + `<polygon>`. Missing the
/// latter used to silently drop both, leaving the loop rotated to whatever
/// point happened to be bottom-most.
pub fn sf_paths_sorted(sf_svg: Option<&str>) -> Vec<Vec<(f32, f32)>> {
    let svg = sf_svg.unwrap_or("");
    let mut paths = Vec::new();
    for d in paths_d_in_svg(svg) {
        if let Ok(pts) = flatten_path_d(&d) {
            if !pts.is_empty() {
                paths.push(pts);
            }
        }
    }
    paths.extend(rects_in_svg(svg));
    paths.extend(polys_in_svg(svg));
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
///
/// The arrow is a dart: a single tip vertex opposite a cluster of tail/barb
/// vertices. That cluster drags the vertex mean away from the tip, so the
/// vertex farthest from the mean *is* the tip and mean → tip is the heading.
/// (Ranking distance from the stripe instead picks a barb whenever the arrow
/// sits beside the stripe rather than along it.)
pub fn sf_arrow_direction(sf_svg: Option<&str>) -> Option<(f32, f32)> {
    let paths = sf_paths_sorted(sf_svg);
    if paths.len() < 2 {
        return None;
    }
    let arrow_pts = paths.last()?;
    // Closed shapes repeat the first vertex; it would double-weight the mean.
    let verts: &[(f32, f32)] = match arrow_pts.split_last() {
        Some((last, head)) if !head.is_empty() && *last == head[0] => head,
        _ => arrow_pts,
    };
    let n = verts.len() as f32;
    let cx = verts.iter().map(|p| p.0).sum::<f32>() / n;
    let cy = verts.iter().map(|p| p.1).sum::<f32>() / n;
    let tip = verts.iter().copied().max_by(|a, b| {
        let da = (a.0 - cx).powi(2) + (a.1 - cy).powi(2);
        let db = (b.0 - cx).powi(2) + (b.1 - cy).powi(2);
        da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
    })?;
    let dx = tip.0 - cx;
    let dy = tip.1 - cy;
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

/// True for a turn designation — `7`, `12`, `6a`, `7/8` — and false for the
/// corner *names* some exports put in the same layer ("Coca-Cola Corner",
/// "Yokohama Bridge"), which would otherwise import as extra turns and inflate
/// the turn count.
fn is_turn_label(label: &str) -> bool {
    static PAT: OnceLock<Regex> = OnceLock::new();
    let pat = PAT.get_or_init(|| {
        Regex::new(r"(?i)^\d{1,2}[a-c]?([/&-]\d{1,2}[a-c]?)*$").expect("turn label regex")
    });
    pat.is_match(label)
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
        Regex::new(r#"(?is)<text[^>]*transform="translate\(([^)]+)\)"[^>]*>(.*?)</text>"#)
            .expect("turn regex")
    });
    static TAG: OnceLock<Regex> = OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r"(?s)<[^>]*>").expect("tag regex"));
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
        // Labels may be split across `<tspan>`s ("11" is often two of them),
        // so strip the markup and rejoin rather than reading bare text.
        let raw = m.get(2).map(|g| g.as_str()).unwrap_or("");
        let label: String = tag
            .replace_all(raw, "")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect();
        if !is_turn_label(&label) {
            continue;
        }
        let label = label.as_str();
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

/// True when numeric corner labels count *down* as lap % increases.
///
/// Members oval exports number turns against the direction of travel, so they
/// need flipping to match iRacing. Road-course exports already agree. Decide
/// from the parsed labels rather than from turn count, which cannot tell a
/// 4-turn oval apart from a 4-turn road course.
pub fn labels_run_backwards(corners: &[Value]) -> bool {
    let nums: Vec<i64> = corners
        .iter()
        .filter_map(|c| c.get("label")?.as_str()?.parse().ok())
        .collect();
    if nums.len() < 3 {
        return false;
    }
    let (mut up, mut down) = (0, 0);
    for w in nums.windows(2) {
        match w[1].cmp(&w[0]) {
            std::cmp::Ordering::Greater => up += 1,
            std::cmp::Ordering::Less => down += 1,
            std::cmp::Ordering::Equal => {}
        }
    }
    down > up
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
                obj.insert("label".into(), json!(iracing_oval_label(label, num_turns)));
            }
            Some(out)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Charlotte Roval 2025 markup: stripe as `<rect>`, arrow as `<polygon>`.
    const SF_RECT_POLY: &str = r#"<svg viewBox="0 0 1920 1080"><g>
        <polygon points="1021.03677 891.65369 921.03677 871.65369 944.16819 891.65369 921.03677 911.65369 1021.03677 891.65369"/>
        <rect x="957.99797" y="920.42944" width="10" height="70"/>
        </g></svg>"#;

    /// Older markup: both shapes as `<path d>`, arrow pointing -X.
    const SF_PATHS: &str = r#"<svg viewBox="0 0 1920 1080">
        <path d="M955,900 L965,900 L965,960 L955,960 Z"/>
        <path d="M1130.3,904.6l-74.3,-17.2l74.3,-17.2l-11.5,17.2Z"/>
        </svg>"#;

    #[test]
    fn parses_rect_and_polygon_shapes() {
        let shapes = sf_paths_sorted(Some(SF_RECT_POLY));
        assert_eq!(shapes.len(), 2, "rect + polygon must both parse");
        // Smallest bbox first: the 10x70 stripe, not the 100x40 arrow.
        let (sx, sy) = sf_stripe_centroid(Some(SF_RECT_POLY)).expect("stripe");
        assert!((sx - 962.998).abs() < 0.5, "stripe x {sx}");
        assert!((sy - 955.43).abs() < 0.5, "stripe y {sy}");
    }

    #[test]
    fn arrow_tip_gives_heading_for_both_markups() {
        let (dx, dy) = sf_arrow_direction(Some(SF_RECT_POLY)).expect("polygon arrow");
        assert!(dx > 0.99, "Charlotte arrow points +X, got ({dx},{dy})");

        let (dx, dy) = sf_arrow_direction(Some(SF_PATHS)).expect("path arrow");
        assert!(dx < -0.99, "path arrow points -X, got ({dx},{dy})");
    }

    #[test]
    fn loop_is_rotated_onto_the_stripe() {
        // Rectangular loop; the stripe at x≈963 crosses the y=966 bottom edge.
        let mut raw = Vec::new();
        for i in 0..100 {
            raw.push((20.0 + i as f32 * 18.0, 966.0));
        }
        for i in 0..100 {
            raw.push((1820.0, 966.0 - i as f32 * 9.0));
        }
        for i in 0..100 {
            raw.push((1820.0 - i as f32 * 18.0, 66.0));
        }
        for i in 0..100 {
            raw.push((20.0, 66.0 + i as f32 * 9.0));
        }
        let out = align_loop_from_sf(&raw, Some(SF_RECT_POLY));
        assert!(
            (out[0].0 - 962.998).abs() < 1.0 && (out[0].1 - 966.0).abs() < 1.0,
            "loop must start on the stripe, got {:?}",
            out[0]
        );
        // Arrow points +X, so the loop must run that way from the stripe.
        assert!(out[1].0 > out[0].0, "loop must follow the arrow (+X)");
    }
}
