//! Pit / loop geometry helpers (Python `schematic_to_track` subset).

pub fn resample_open(pts: &[(f32, f32)], n: usize) -> Vec<(f32, f32)> {
    if pts.len() < 2 || n < 2 {
        return pts.to_vec();
    }
    let mut cum = vec![0.0f32];
    for w in pts.windows(2) {
        cum.push(cum.last().unwrap() + dist(w[0], w[1]));
    }
    let total = *cum.last().unwrap();
    if total <= 0.0 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    let step = total / (n - 1) as f32;
    let mut j = 0usize;
    for k in 0..n {
        let target = k as f32 * step;
        while j < cum.len() - 2 && cum[j + 1] < target {
            j += 1;
        }
        let seg = cum[j + 1] - cum[j];
        let t = if seg > 0.0 {
            (target - cum[j]) / seg
        } else {
            0.0
        };
        let a = pts[j];
        let b = pts[j + 1];
        out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
    }
    out
}

pub fn pct_on_loop(loop_pts: &[(f32, f32)], pt: (f32, f32)) -> f32 {
    let n = loop_pts.len();
    if n < 2 {
        return 0.0;
    }
    let mut best_d = f32::MAX;
    let mut best_i = 0usize;
    let mut best_t = 0.0f32;
    for i in 0..n {
        let a = loop_pts[i];
        let b = loop_pts[(i + 1) % n];
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let ln2 = dx * dx + dy * dy;
        let t = if ln2 < 1e-12 {
            0.0
        } else {
            (((pt.0 - a.0) * dx + (pt.1 - a.1) * dy) / ln2).clamp(0.0, 1.0)
        };
        let px = a.0 + dx * t;
        let py = a.1 + dy * t;
        let d = (px - pt.0).powi(2) + (py - pt.1).powi(2);
        if d < best_d {
            best_d = d;
            best_i = i;
            best_t = t;
        }
    }
    let mut cum = vec![0.0f32];
    for i in 0..n {
        cum.push(cum[i] + dist(loop_pts[i], loop_pts[(i + 1) % n]));
    }
    let total = *cum.last().unwrap();
    if total <= 0.0 {
        return 0.0;
    }
    let pos = cum[best_i] + best_t * (cum[best_i + 1] - cum[best_i]);
    pos / total
}

fn pit_lane_straight_points(pit_path: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if pit_path.len() < 4 {
        return pit_path.to_vec();
    }
    let ys: Vec<f32> = pit_path.iter().map(|p| p.1).collect();
    let y_span =
        ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min);
    if y_span < 0.015 {
        return pit_path.to_vec();
    }
    let xs: Vec<f32> = pit_path.iter().map(|p| p.0).collect();
    let mid_x = (xs.iter().cloned().fold(f32::MAX, f32::min)
        + xs.iter().cloned().fold(f32::MIN, f32::max))
        * 0.5;
    let hi: Vec<_> = pit_path.iter().copied().filter(|p| p.0 >= mid_x).collect();
    if hi.len() < 3 {
        return pit_path.to_vec();
    }
    let mut hy: Vec<f32> = hi.iter().map(|p| p.1).collect();
    hy.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let med_y = hy[hy.len() / 2];
    let tol = 0.006f32.max(y_span * 0.12);
    let straight: Vec<_> = hi
        .iter()
        .copied()
        .filter(|p| (p.1 - med_y).abs() <= tol)
        .collect();
    if straight.len() >= 3 {
        straight
    } else {
        hi
    }
}

pub fn pit_span_on_loop(loop_pts: &[(f32, f32)], pit_path: &[(f32, f32)]) -> (f32, f32) {
    let pts = pit_lane_straight_points(pit_path);
    let pcts: Vec<f32> = pts.iter().map(|p| pct_on_loop(loop_pts, *p)).collect();
    let mut lo = pcts.iter().cloned().fold(f32::MAX, f32::min);
    let mut hi = pcts.iter().cloned().fold(f32::MIN, f32::max);
    if (hi - lo).rem_euclid(1.0) < 0.02 {
        let p0 = pct_on_loop(loop_pts, pit_path[0]);
        let p1 = pct_on_loop(loop_pts, *pit_path.last().unwrap());
        lo = p0.min(p1);
        hi = p0.max(p1);
    }
    (lo, hi)
}

pub fn connect_blend_to_loop(
    blend: &[(f32, f32)],
    loop_pts: &[(f32, f32)],
    attach_end: bool,
    n_loop: usize,
    max_pts: Option<usize>,
    _pit_path: Option<&[(f32, f32)]>,
) -> Vec<(f32, f32)> {
    if blend.len() < 2 || loop_pts.is_empty() {
        return blend.to_vec();
    }
    let cap = max_pts.unwrap_or_else(|| 56.max(blend.len() + n_loop));
    let xs: Vec<f32> = loop_pts.iter().map(|p| p.0).collect();
    let ys: Vec<f32> = loop_pts.iter().map(|p| p.1).collect();
    let span_x =
        xs.iter().cloned().fold(f32::MIN, f32::max) - xs.iter().cloned().fold(f32::MAX, f32::min);
    let span_y =
        ys.iter().cloned().fold(f32::MIN, f32::max) - ys.iter().cloned().fold(f32::MAX, f32::min);
    let prox = span_x.max(span_y) * 0.035;
    let n = loop_pts.len();
    let merged = if attach_end {
        let anchor = *blend.last().unwrap();
        if loop_pts.iter().any(|p| dist(*p, anchor) < prox) {
            return resample_open(blend, blend.len().min(cap));
        }
        let li = (0..n)
            .min_by(|i, j| {
                dist(loop_pts[*i], anchor)
                    .partial_cmp(&dist(loop_pts[*j], anchor))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        let ext: Vec<_> = (0..n_loop).map(|k| loop_pts[(li + k) % n]).collect();
        let mut m = blend.to_vec();
        m.extend_from_slice(&ext[1..]);
        m
    } else {
        let anchor = blend[0];
        if loop_pts.iter().any(|p| dist(*p, anchor) < prox) {
            return resample_open(blend, blend.len().min(cap));
        }
        let li = (0..n)
            .min_by(|i, j| {
                dist(loop_pts[*i], anchor)
                    .partial_cmp(&dist(loop_pts[*j], anchor))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or(0);
        let mut ext: Vec<_> = (0..n_loop).map(|k| loop_pts[(li + n - k) % n]).collect();
        ext.reverse();
        let mut m = ext;
        m.extend_from_slice(&blend[1..]);
        m
    };
    resample_open(&merged, 16.max(merged.len().min(cap)))
}

pub fn ensure_ccw(loop_pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if signed_area(loop_pts) < 0.0 {
        let mut r = loop_pts.to_vec();
        r.reverse();
        r
    } else {
        loop_pts.to_vec()
    }
}

fn signed_area(loop_pts: &[(f32, f32)]) -> f32 {
    let n = loop_pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0f32;
    for i in 0..n {
        let (x0, y0) = loop_pts[i];
        let (x1, y1) = loop_pts[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a * 0.5
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
