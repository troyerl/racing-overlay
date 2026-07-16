//! SVG path `d` attribute flattening and subpath selection.

use svgtypes::{PathParser, PathSegment};

use super::geom::resample_open;

const SUBPATH_LENGTH_TIE_FRAC: f32 = 0.03;

/// Extract all `d="..."` / `d='...'` attributes from an SVG fragment.
pub fn paths_d_in_svg(svg_text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let bytes = svg_text.as_bytes();
    let mut i = 0;
    while i + 3 < bytes.len() {
        if (bytes[i] == b'd' || bytes[i] == b'D') && bytes[i + 1] == b'=' {
            let quote = bytes[i + 2];
            if quote == b'"' || quote == b'\'' {
                let start = i + 3;
                let mut j = start;
                while j < bytes.len() && bytes[j] != quote {
                    j += 1;
                }
                if j < bytes.len() {
                    if let Ok(s) = std::str::from_utf8(&bytes[start..j]) {
                        if !s.is_empty() {
                            out.push(s.to_string());
                        }
                    }
                    i = j + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// Flatten a path `d` string to polyline points (no uniform resample).
pub fn flatten_path_d(d: &str) -> anyhow::Result<Vec<(f32, f32)>> {
    let mut pts = Vec::new();
    walk_path(d, |cx, cy, is_move| {
        if is_move && !pts.is_empty() {
            // keep continuous for single flatten — callers that need subpaths
            // should use `split_subpaths`.
        }
        pts.push((cx as f32, cy as f32));
    })?;
    Ok(pts)
}

/// Split `d` into continuous subpaths (each M/m starts a new one).
pub fn split_subpaths(d: &str) -> anyhow::Result<Vec<Vec<(f32, f32)>>> {
    let mut subs: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    walk_path(d, |cx, cy, is_move| {
        if is_move && !cur.is_empty() {
            if cur.len() >= 2 {
                subs.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
        cur.push((cx as f32, cy as f32));
    })?;
    if cur.len() >= 2 {
        subs.push(cur);
    }
    Ok(subs)
}

fn polyline_length(pts: &[(f32, f32)]) -> f32 {
    pts.windows(2)
        .map(|w| ((w[0].0 - w[1].0).powi(2) + (w[0].1 - w[1].1).powi(2)).sqrt())
        .sum()
}

fn bbox_area(pts: &[(f32, f32)]) -> f32 {
    if pts.is_empty() {
        return 0.0;
    }
    let min_x = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    (max_x - min_x) * (max_y - min_y)
}

/// Pick the best continuous subpath (longest, bbox-area tie-break).
pub fn pick_best_subpath(subs: &[Vec<(f32, f32)>]) -> Option<&[(f32, f32)]> {
    if subs.is_empty() {
        return None;
    }
    let best_len = subs
        .iter()
        .map(|s| polyline_length(s))
        .fold(0.0f32, f32::max);
    let candidates: Vec<&Vec<(f32, f32)>> = subs
        .iter()
        .filter(|s| polyline_length(s) >= best_len * (1.0 - SUBPATH_LENGTH_TIE_FRAC))
        .collect();
    candidates
        .into_iter()
        .max_by(|a, b| {
            let la = polyline_length(a);
            let lb = polyline_length(b);
            la.partial_cmp(&lb)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    bbox_area(a)
                        .partial_cmp(&bbox_area(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        })
        .map(|v| v.as_slice())
}

/// Sample the best subpath of `d` to approximately `n` points.
pub fn sample_best_subpath(d: &str, n: usize) -> anyhow::Result<Vec<(f32, f32)>> {
    let subs = split_subpaths(d)?;
    let best = if let Some(b) = pick_best_subpath(&subs) {
        b.to_vec()
    } else {
        flatten_path_d(d)?
    };
    if best.len() < 3 {
        anyhow::bail!("SVG path produced too few points");
    }
    Ok(resample_open(&best, n.max(64)))
}

fn walk_path(d: &str, mut on_pt: impl FnMut(f64, f64, bool)) -> anyhow::Result<()> {
    let mut cx = 0.0f64;
    let mut cy = 0.0f64;
    let mut start_x = 0.0f64;
    let mut start_y = 0.0f64;
    let mut last_cx = 0.0f64;
    let mut last_cy = 0.0f64;
    for seg in PathParser::from(d) {
        let seg = seg.map_err(|e| anyhow::anyhow!("SVG path: {e}"))?;
        match seg {
            PathSegment::MoveTo { abs, x, y } => {
                if abs {
                    cx = x;
                    cy = y;
                } else {
                    cx += x;
                    cy += y;
                }
                start_x = cx;
                start_y = cy;
                on_pt(cx, cy, true);
            }
            PathSegment::LineTo { abs, x, y } => {
                if abs {
                    cx = x;
                    cy = y;
                } else {
                    cx += x;
                    cy += y;
                }
                on_pt(cx, cy, false);
            }
            PathSegment::HorizontalLineTo { abs, x } => {
                if abs {
                    cx = x;
                } else {
                    cx += x;
                }
                on_pt(cx, cy, false);
            }
            PathSegment::VerticalLineTo { abs, y } => {
                if abs {
                    cy = y;
                } else {
                    cy += y;
                }
                on_pt(cx, cy, false);
            }
            PathSegment::CurveTo {
                abs,
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (x1, y1, x2, y2, x, y) = if abs {
                    (x1, y1, x2, y2, x, y)
                } else {
                    (cx + x1, cy + y1, cx + x2, cy + y2, cx + x, cy + y)
                };
                sample_cubic(cx, cy, x1, y1, x2, y2, x, y, 12, &mut on_pt);
                last_cx = x2;
                last_cy = y2;
                cx = x;
                cy = y;
            }
            PathSegment::SmoothCurveTo { abs, x2, y2, x, y } => {
                let x1 = 2.0 * cx - last_cx;
                let y1 = 2.0 * cy - last_cy;
                let (x2, y2, x, y) = if abs {
                    (x2, y2, x, y)
                } else {
                    (cx + x2, cy + y2, cx + x, cy + y)
                };
                sample_cubic(cx, cy, x1, y1, x2, y2, x, y, 12, &mut on_pt);
                last_cx = x2;
                last_cy = y2;
                cx = x;
                cy = y;
            }
            PathSegment::Quadratic { abs, x1, y1, x, y } => {
                let (x1, y1, x, y) = if abs {
                    (x1, y1, x, y)
                } else {
                    (cx + x1, cy + y1, cx + x, cy + y)
                };
                sample_quad(cx, cy, x1, y1, x, y, 10, &mut on_pt);
                last_cx = x1;
                last_cy = y1;
                cx = x;
                cy = y;
            }
            PathSegment::SmoothQuadratic { abs, x, y } => {
                let x1 = 2.0 * cx - last_cx;
                let y1 = 2.0 * cy - last_cy;
                let (x, y) = if abs { (x, y) } else { (cx + x, cy + y) };
                sample_quad(cx, cy, x1, y1, x, y, 10, &mut on_pt);
                last_cx = x1;
                last_cy = y1;
                cx = x;
                cy = y;
            }
            PathSegment::ClosePath { .. } => {
                cx = start_x;
                cy = start_y;
                on_pt(cx, cy, false);
            }
            PathSegment::EllipticalArc { abs, x, y, .. } => {
                if abs {
                    cx = x;
                    cy = y;
                } else {
                    cx += x;
                    cy += y;
                }
                on_pt(cx, cy, false);
            }
        }
    }
    Ok(())
}

fn sample_cubic(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    x3: f64,
    y3: f64,
    steps: usize,
    on_pt: &mut impl FnMut(f64, f64, bool),
) {
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let u = 1.0 - t;
        let x = u * u * u * x0 + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x3;
        let y = u * u * u * y0 + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3;
        on_pt(x, y, false);
    }
}

fn sample_quad(
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
    steps: usize,
    on_pt: &mut impl FnMut(f64, f64, bool),
) {
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let u = 1.0 - t;
        let x = u * u * x0 + 2.0 * u * t * x1 + t * t * x2;
        let y = u * u * y0 + 2.0 * u * t * y1 + t * t * y2;
        on_pt(x, y, false);
    }
}
