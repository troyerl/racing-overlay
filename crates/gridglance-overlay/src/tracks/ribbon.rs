//! Fold a stroke-outline "ribbon" back onto its centreline.
//!
//! Members-site exports draw the track as a stroked line converted to a filled
//! outline. For a closed circuit that outline is an annulus — an outer kerb plus
//! an inner hole. Averaging those two rings recovers the asphalt centreline.
//! Using the outer ring alone rounds D-ovals (Richmond) into a stadium.
//!
//! Where a track passes under itself the exporter breaks the stroke so the
//! overpass reads visually (Oran Park GP leaves an 80-unit gap at Yokohama
//! Bridge). That break makes the circuit an open arc, so its outline is a single
//! ribbon that runs the whole lap out along one kerb and back along the other.
//! Used verbatim it laps the circuit twice, once in each direction.

use super::geom::resample_open;

/// Dense samples used to find the fold, independent of the caller's output size.
const FOLD_SAMPLES: usize = 1200;
/// A ribbon is a track width across; anything fatter is a real loop.
const MAX_WIDTH_FRAC: f32 = 0.06;
/// Annulus kerbs are a track width apart. Slightly looser than a fold so a
/// 60 ft NASCAR lane still pairs on a ~0.75 mi D-oval.
const ANNULUS_MAX_WIDTH_FRAC: f32 = 0.12;
const ANNULUS_MIN_WIDTH_FRAC: f32 = 0.004;
/// Inner kerb must be almost as long as the outer — rejects a fountain / lake.
const ANNULUS_MIN_LEN_RATIO: f32 = 0.55;
const ANNULUS_SAMPLES: usize = 400;
/// Share of samples that must have an opposite-edge partner.
const MIN_PAIRED_FRAC: f32 = 0.85;
/// Partners nearer than this (as a share of the outline) are same-edge neighbours.
const MIN_INDEX_SEP_FRAC: f32 = 0.04;
/// Area-derived and pairing-derived widths must agree within this ratio.
const WIDTH_AGREE_FRAC: f32 = 0.5;

/// Collapse an out-and-back stroke outline to its centreline, closed across the
/// break the exporter drew. `None` when `outline` is not a fold, which is the
/// normal case — leave those callers on the outline they already had.
pub fn collapse_to_centerline(outline: &[(f32, f32)]) -> Option<Vec<(f32, f32)>> {
    if outline.len() < 32 {
        return None;
    }
    let span = bbox_span(outline);
    if span <= 0.0 || !span.is_finite() {
        return None;
    }

    // Uniform ring with no duplicate closing vertex, so index arithmetic wraps.
    let mut ring = resample_open(outline, FOLD_SAMPLES + 1);
    ring.pop();
    let m = ring.len();
    if m < 32 {
        return None;
    }
    let perimeter = perimeter_closed(&ring);
    if perimeter <= 0.0 {
        return None;
    }

    // A fold encloses no area of its own: |A| = width * centreline length, and
    // the centreline is half the outline. A real loop encloses far more.
    let width_from_area = 2.0 * signed_area(&ring).abs() / perimeter;
    if width_from_area <= 0.0 || width_from_area > span * MAX_WIDTH_FRAC {
        return None;
    }

    let sep = ((m as f32) * MIN_INDEX_SEP_FRAC).ceil().max(1.0) as usize;
    let partners: Vec<(usize, f32)> = (0..m).map(|i| nearest_far(&ring, i, sep)).collect();

    let mut widths: Vec<f32> = partners.iter().map(|(_, d)| *d).collect();
    widths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let width = widths[m / 2];
    if width <= 0.0 {
        return None;
    }
    // Both estimates must agree, else this is a genuine loop that merely runs
    // close to itself somewhere (a hairpin, or two straights side by side).
    if (width - width_from_area).abs() > width.max(width_from_area) * WIDTH_AGREE_FRAC {
        return None;
    }
    let paired = partners.iter().filter(|(_, d)| *d <= width * 2.0).count();
    if (paired as f32) < (m as f32) * MIN_PAIRED_FRAC {
        return None;
    }

    // Under a fold i <-> j the sum i + j is constant, so the involution's two
    // fixed points are the caps: at sum/2 and sum/2 + m/2.
    let cap_a = fold_axis(&partners, m)?;
    let cap_b = cap_a + m / 2;

    // Step past the cap semicircles, which have no opposite edge to pair with.
    let per_sample = perimeter / m as f32;
    let guard = (((width * 0.5) / per_sample).ceil() as usize + 2).min(m / 8);
    let side_a = walk(
        &ring,
        cap_a as isize + guard as isize,
        cap_b as isize - guard as isize,
    );
    let side_b = walk(
        &ring,
        cap_b as isize + guard as isize,
        cap_a as isize - guard as isize,
    );
    if side_a.len() < 16 || side_b.len() < 16 {
        return None;
    }

    // Project onto the opposite polyline rather than pairing vertices: uniform
    // arc-length sampling drifts by a few samples through corners, because the
    // outer edge of a bend is longer than the inner one.
    let mut mid = Vec::with_capacity(side_a.len());
    for &p in &side_a {
        let (q, d) = closest_on_polyline(&side_b, p);
        if d > width * 3.0 {
            continue;
        }
        mid.push(((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5));
    }
    if mid.len() < 16 {
        return None;
    }
    // Close the ring across the break, and keep the duplicate closing vertex so
    // the result has the same shape as an annulus subpath.
    mid.push(mid[0]);
    Some(mid)
}

/// Average the outer kerb and inner hole of a filled stroke outline.
///
/// `None` when the two longest subpaths are not a nested track-width pair
/// (decorative holes, a single outline, or an Oran-Park-style ribbon).
pub fn annulus_centerline(subs: &[Vec<(f32, f32)>]) -> Option<Vec<(f32, f32)>> {
    let mut rings: Vec<Vec<(f32, f32)>> = subs
        .iter()
        .map(|s| strip_closing_dup(s))
        .filter(|s| s.len() >= 32)
        .collect();
    if rings.len() < 2 {
        return None;
    }
    rings.sort_by(|a, b| {
        polyline_len(b)
            .partial_cmp(&polyline_len(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let a = &rings[0];
    let b = &rings[1];
    let (outer, inner) = if ring_contains(a, b) {
        (a, b)
    } else if ring_contains(b, a) {
        (b, a)
    } else {
        return None;
    };
    let outer_len = polyline_len(outer);
    let inner_len = polyline_len(inner);
    if outer_len <= 0.0 || inner_len / outer_len < ANNULUS_MIN_LEN_RATIO {
        return None;
    }
    let span = bbox_span(outer);
    if span <= 0.0 || !span.is_finite() {
        return None;
    }

    let mut outer_r = resample_open(outer, ANNULUS_SAMPLES + 1);
    outer_r.pop();
    let mut inner_r = resample_open(inner, ANNULUS_SAMPLES + 1);
    inner_r.pop();
    if outer_r.len() < 32 || inner_r.len() < 32 {
        return None;
    }

    let mut widths: Vec<f32> = outer_r
        .iter()
        .map(|&p| closest_on_ring(&inner_r, p).1)
        .collect();
    widths.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let width = widths[widths.len() / 2];
    if width < span * ANNULUS_MIN_WIDTH_FRAC || width > span * ANNULUS_MAX_WIDTH_FRAC {
        return None;
    }

    // Pair along the inward normal. Global closest-point snaps an outer
    // corner onto the inner backstretch of a D-oval and chamfers T3.
    let outer_ccw = signed_area(&outer_r) > 0.0;
    let mut mid = Vec::with_capacity(outer_r.len() + 1);
    let mut hint = 0usize;
    for (i, &p) in outer_r.iter().enumerate() {
        let (q, d, j) = pair_inner(&outer_r, i, &inner_r, hint, outer_ccw, width);
        hint = j;
        if d > width * 3.0 {
            continue;
        }
        mid.push(((p.0 + q.0) * 0.5, (p.1 + q.1) * 0.5));
    }
    if mid.len() < 32 {
        return None;
    }
    mid.push(mid[0]);
    Some(mid)
}

/// Inner kerb opposite `outer[i]`: inward ray, then a local closest-point window.
fn pair_inner(
    outer: &[(f32, f32)],
    i: usize,
    inner: &[(f32, f32)],
    hint: usize,
    outer_ccw: bool,
    width: f32,
) -> ((f32, f32), f32, usize) {
    let p = outer[i];
    let n = inward_normal(outer, i, outer_ccw);
    if let Some((q, d)) = ray_hit_ring(p, n, inner) {
        if d <= width * 3.0 {
            return (q, d, nearest_vertex(inner, q));
        }
    }
    let half = (inner.len() / 8).max(12);
    closest_on_ring_window(inner, p, hint, half)
}

fn inward_normal(ring: &[(f32, f32)], i: usize, ccw: bool) -> (f32, f32) {
    let n = ring.len();
    let a = ring[(i + n - 1) % n];
    let c = ring[(i + 1) % n];
    let dx = c.0 - a.0;
    let dy = c.1 - a.1;
    let len = dx.hypot(dy).max(1e-6);
    // CCW: interior is left of the tangent → (-dy, dx).
    if ccw {
        (-dy / len, dx / len)
    } else {
        (dy / len, -dx / len)
    }
}

fn ray_hit_ring(
    origin: (f32, f32),
    dir: (f32, f32),
    ring: &[(f32, f32)],
) -> Option<((f32, f32), f32)> {
    let n = ring.len();
    let mut best: Option<((f32, f32), f32)> = None;
    for i in 0..n {
        let a = ring[i];
        let b = ring[(i + 1) % n];
        let Some((q, t)) = ray_seg(origin, dir, a, b) else {
            continue;
        };
        if t > 1e-4 && best.map_or(true, |(_, bt)| t < bt) {
            best = Some((q, t));
        }
    }
    best
}

fn ray_seg(
    p: (f32, f32),
    d: (f32, f32),
    a: (f32, f32),
    b: (f32, f32),
) -> Option<((f32, f32), f32)> {
    let ex = b.0 - a.0;
    let ey = b.1 - a.1;
    let den = d.0 * ey - d.1 * ex;
    if den.abs() < 1e-12 {
        return None;
    }
    let apx = a.0 - p.0;
    let apy = a.1 - p.1;
    let t = (apx * ey - apy * ex) / den;
    let u = (apx * d.1 - apy * d.0) / den;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    Some(((p.0 + d.0 * t, p.1 + d.1 * t), t))
}

fn closest_on_ring(ring: &[(f32, f32)], p: (f32, f32)) -> ((f32, f32), f32) {
    let (q, d, _) = closest_on_ring_window(ring, p, 0, ring.len());
    (q, d)
}

fn closest_on_ring_window(
    ring: &[(f32, f32)],
    p: (f32, f32),
    hint: usize,
    half: usize,
) -> ((f32, f32), f32, usize) {
    let n = ring.len();
    let mut best = (ring[hint % n], dist(ring[hint % n], p), hint % n);
    let span = half.min(n);
    for k in 0..span {
        let deltas: [isize; 2] = [k as isize, -(k as isize)];
        for delta in deltas {
            if k == 0 && delta < 0 {
                continue;
            }
            let i = (hint as isize + delta).rem_euclid(n as isize) as usize;
            let a = ring[i];
            let b = ring[(i + 1) % n];
            let (q, d) = closest_on_seg(a, b, p);
            if d < best.1 {
                best = (q, d, i);
            }
        }
    }
    best
}

fn closest_on_seg(a: (f32, f32), b: (f32, f32), p: (f32, f32)) -> ((f32, f32), f32) {
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-12 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let q = (a.0 + dx * t, a.1 + dy * t);
    (q, dist(q, p))
}

fn nearest_vertex(ring: &[(f32, f32)], p: (f32, f32)) -> usize {
    let mut best = (0usize, f32::MAX);
    for (i, &q) in ring.iter().enumerate() {
        let d = dist(q, p);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

fn strip_closing_dup(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut out = pts.to_vec();
    if out.len() >= 2 {
        let a = out[0];
        let b = *out.last().unwrap();
        if dist(a, b) < 1e-3 {
            out.pop();
        }
    }
    out
}

fn polyline_len(pts: &[(f32, f32)]) -> f32 {
    if pts.len() < 2 {
        return 0.0;
    }
    pts.windows(2).map(|w| dist(w[0], w[1])).sum::<f32>() + dist(*pts.last().unwrap(), pts[0])
}

/// True when most sample points of `inner` sit inside `outer`.
fn ring_contains(outer: &[(f32, f32)], inner: &[(f32, f32)]) -> bool {
    if inner.len() < 8 || outer.len() < 8 {
        return false;
    }
    let step = (inner.len() / 24).max(1);
    let mut n = 0usize;
    let mut hit = 0usize;
    let mut i = 0usize;
    while i < inner.len() {
        n += 1;
        if point_in_poly(inner[i], outer) {
            hit += 1;
        }
        i += step;
    }
    n > 0 && (hit as f32) >= (n as f32) * 0.9
}

fn point_in_poly(p: (f32, f32), ring: &[(f32, f32)]) -> bool {
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

/// Index of one cap, from the circular mean of `i + partner(i)`.
fn fold_axis(partners: &[(usize, f32)], m: usize) -> Option<usize> {
    let mut sx = 0.0f64;
    let mut sy = 0.0f64;
    for (i, (j, _)) in partners.iter().enumerate() {
        let a = ((i + *j) % m) as f64 / m as f64 * std::f64::consts::TAU;
        sx += a.cos();
        sy += a.sin();
    }
    if sx.hypot(sy) < 1e-9 {
        return None;
    }
    let ang = sy.atan2(sx).rem_euclid(std::f64::consts::TAU);
    let sum = (ang / std::f64::consts::TAU * m as f64).round() as usize % m;
    Some((sum / 2) % m)
}

/// Nearest vertex at least `sep` indices away around the ring.
fn nearest_far(ring: &[(f32, f32)], i: usize, sep: usize) -> (usize, f32) {
    let m = ring.len();
    let mut best = (i, f32::MAX);
    for j in 0..m {
        let step = i.abs_diff(j);
        if step.min(m - step) < sep {
            continue;
        }
        let d = dist(ring[i], ring[j]);
        if d < best.1 {
            best = (j, d);
        }
    }
    best
}

/// Forward walk `from`..=`to` around the ring, wrapping.
fn walk(ring: &[(f32, f32)], from: isize, to: isize) -> Vec<(f32, f32)> {
    let m = ring.len();
    let wrap = |v: isize| -> usize { v.rem_euclid(m as isize) as usize };
    let end = wrap(to);
    let mut i = wrap(from);
    let mut out = Vec::new();
    loop {
        out.push(ring[i]);
        if i == end || out.len() > m {
            break;
        }
        i = (i + 1) % m;
    }
    out
}

fn closest_on_polyline(poly: &[(f32, f32)], p: (f32, f32)) -> ((f32, f32), f32) {
    let mut best = (poly[0], dist(poly[0], p));
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        let dx = b.0 - a.0;
        let dy = b.1 - a.1;
        let len2 = dx * dx + dy * dy;
        let t = if len2 < 1e-12 {
            0.0
        } else {
            (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
        };
        let q = (a.0 + dx * t, a.1 + dy * t);
        let d = dist(q, p);
        if d < best.1 {
            best = (q, d);
        }
    }
    best
}

fn bbox_span(pts: &[(f32, f32)]) -> f32 {
    let min_x = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
    let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
    let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
    let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
    (max_x - min_x).max(max_y - min_y)
}

fn perimeter_closed(pts: &[(f32, f32)]) -> f32 {
    let n = pts.len();
    (0..n).map(|i| dist(pts[i], pts[(i + 1) % n])).sum()
}

fn signed_area(pts: &[(f32, f32)]) -> f32 {
    let n = pts.len();
    let mut a = 0.0f32;
    for i in 0..n {
        let (x0, y0) = pts[i];
        let (x1, y1) = pts[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a * 0.5
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stroke outline of an open arc: out along one side, cap, back along the other.
    fn ribbon_of(centre: &[(f32, f32)], width: f32) -> Vec<(f32, f32)> {
        let h = width * 0.5;
        let normal = |i: usize| -> (f32, f32) {
            let a = centre[i.saturating_sub(1)];
            let b = centre[(i + 1).min(centre.len() - 1)];
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let l = (dx * dx + dy * dy).sqrt().max(1e-6);
            (-dy / l, dx / l)
        };
        let mut out: Vec<(f32, f32)> = Vec::new();
        for (i, p) in centre.iter().enumerate() {
            let n = normal(i);
            out.push((p.0 + n.0 * h, p.1 + n.1 * h));
        }
        for i in (0..centre.len()).rev() {
            let n = normal(i);
            out.push((centre[i].0 - n.0 * h, centre[i].1 - n.1 * h));
        }
        out.push(out[0]);
        out
    }

    fn circle_arc(n: usize, gap: f32) -> Vec<(f32, f32)> {
        (0..n)
            .map(|i| {
                let t = gap * 0.5 + (1.0 - gap) * i as f32 / (n - 1) as f32;
                let a = t * std::f32::consts::TAU;
                (500.0 + 400.0 * a.cos(), 500.0 + 400.0 * a.sin())
            })
            .collect()
    }

    #[test]
    fn folds_a_ribbon_back_onto_its_centreline() {
        let centre = circle_arc(300, 0.03);
        let outline = ribbon_of(&centre, 20.0);
        let mid = collapse_to_centerline(&outline).expect("ribbon must be detected");
        // Every recovered point must sit on the original centreline.
        for &p in &mid {
            let (_, d) = closest_on_polyline(&centre, p);
            assert!(
                d < 3.0,
                "recovered point {p:?} is {d:.2} off the centreline"
            );
        }
        // And it must span the arc once, not twice.
        let len: f32 = mid.windows(2).map(|w| dist(w[0], w[1])).sum();
        let cl: f32 = centre.windows(2).map(|w| dist(w[0], w[1])).sum();
        let closing = dist(*mid.last().unwrap(), mid[0]);
        assert!(
            ((len - closing) - cl).abs() < cl * 0.1,
            "recovered length {len:.0} vs centreline {cl:.0}"
        );
    }

    #[test]
    fn leaves_a_real_loop_alone() {
        let loop_pts: Vec<(f32, f32)> = (0..400)
            .map(|i| {
                let a = i as f32 / 400.0 * std::f32::consts::TAU;
                (500.0 + 400.0 * a.cos(), 500.0 + 250.0 * a.sin())
            })
            .collect();
        assert!(collapse_to_centerline(&loop_pts).is_none());
    }

    /// Two straights a track width apart must not read as a fold.
    #[test]
    fn leaves_a_pinched_loop_alone() {
        let mut pts: Vec<(f32, f32)> = Vec::new();
        for i in 0..200 {
            pts.push((100.0 + 800.0 * i as f32 / 199.0, 500.0));
        }
        for i in 0..40 {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 39.0;
            pts.push((900.0 + 300.0 * a.cos(), 490.0 + 300.0 * a.sin() + 300.0));
        }
        for i in 0..200 {
            pts.push((900.0 - 800.0 * i as f32 / 199.0, 480.0));
        }
        for i in 0..40 {
            let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / 39.0;
            pts.push((100.0 + 300.0 * a.cos(), 490.0 + 300.0 * a.sin() - 300.0));
        }
        assert!(
            collapse_to_centerline(&pts).is_none(),
            "a loop with a narrow neck is not a ribbon"
        );
    }

    fn ellipse(n: usize, cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<(f32, f32)> {
        (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                (cx + rx * a.cos(), cy + ry * a.sin())
            })
            .collect()
    }

    fn ring_bbox(pts: &[(f32, f32)]) -> (f32, f32) {
        let min_x = pts.iter().map(|p| p.0).fold(f32::MAX, f32::min);
        let max_x = pts.iter().map(|p| p.0).fold(f32::MIN, f32::max);
        let min_y = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min);
        let max_y = pts.iter().map(|p| p.1).fold(f32::MIN, f32::max);
        (max_x - min_x, max_y - min_y)
    }

    #[test]
    fn annulus_averages_concentric_circles() {
        let outer = ellipse(180, 500.0, 500.0, 400.0, 400.0);
        let inner = ellipse(180, 500.0, 500.0, 360.0, 360.0);
        let mid = annulus_centerline(&[outer, inner]).expect("annulus");
        for &p in &mid {
            let r = ((p.0 - 500.0).hypot(p.1 - 500.0) - 380.0).abs();
            assert!(r < 4.0, "centreline radius off by {r:.2} at {p:?}");
        }
    }

    /// Outer kerb is rounder; inner kerb is the D. Centreline must keep more of
    /// the D than the outer ring would (Richmond).
    #[test]
    fn annulus_keeps_inner_d_aspect() {
        let outer = ellipse(200, 500.0, 500.0, 400.0, 240.0);
        let inner = ellipse(200, 500.0, 500.0, 360.0, 160.0);
        let mid = annulus_centerline(&[outer.clone(), inner.clone()]).expect("annulus");
        let (ow, oh) = ring_bbox(&outer);
        let (mw, mh) = ring_bbox(&mid);
        let outer_aspect = ow / oh;
        let mid_aspect = mw / mh;
        assert!(
            mid_aspect > outer_aspect * 1.08,
            "centreline aspect {mid_aspect:.3} should be D-er than outer {outer_aspect:.3}"
        );
        assert!(annulus_centerline(&[outer]).is_none());
    }

    #[test]
    fn annulus_rejects_a_tiny_infield_hole() {
        let outer = ellipse(180, 500.0, 500.0, 400.0, 250.0);
        let hole = ellipse(80, 500.0, 500.0, 40.0, 25.0);
        assert!(annulus_centerline(&[outer, hole]).is_none());
    }

    /// CCW stadium: top straight, right cap, bottom straight, left cap.
    fn stadium(n: usize, cx: f32, cy: f32, straight: f32, r: f32) -> Vec<(f32, f32)> {
        let n_cap = (n / 4).max(16);
        let n_str = (n / 4).max(8);
        let left = cx - straight * 0.5;
        let right = cx + straight * 0.5;
        let mut pts = Vec::new();
        for i in 0..n_str {
            let t = i as f32 / n_str as f32;
            pts.push((left + straight * t, cy - r));
        }
        for i in 0..n_cap {
            let a = -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / n_cap as f32;
            pts.push((right + r * a.cos(), cy + r * a.sin()));
        }
        for i in 0..n_str {
            let t = i as f32 / n_str as f32;
            pts.push((right - straight * t, cy + r));
        }
        for i in 0..n_cap {
            let a = std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * i as f32 / n_cap as f32;
            pts.push((left + r * a.cos(), cy + r * a.sin()));
        }
        pts
    }

    fn max_turn_deg(pts: &[(f32, f32)]) -> f32 {
        let n = pts.len();
        if n < 3 {
            return 0.0;
        }
        let mut m = 0.0f32;
        for i in 0..n {
            let a = pts[(i + n - 1) % n];
            let b = pts[i];
            let c = pts[(i + 1) % n];
            let v1 = (a.0 - b.0, a.1 - b.1);
            let v2 = (c.0 - b.0, c.1 - b.1);
            let n1 = v1.0.hypot(v1.1);
            let n2 = v2.0.hypot(v2.1);
            if n1 < 1e-6 || n2 < 1e-6 {
                continue;
            }
            let d = ((v1.0 * v2.0 + v1.1 * v2.1) / (n1 * n2)).clamp(-1.0, 1.0);
            let turn = 180.0 - d.acos().to_degrees();
            if turn < 170.0 {
                m = m.max(turn);
            }
        }
        m
    }

    /// Outer apron is a big-radius stadium; inner line has tighter corners and
    /// longer straights (Richmond). Closest-point pairing used to chamfer T3.
    #[test]
    fn annulus_keeps_stadium_corners_round() {
        let outer = stadium(240, 500.0, 400.0, 400.0, 200.0);
        let inner = stadium(240, 500.0, 400.0, 460.0, 150.0);
        let mid = annulus_centerline(&[outer, inner]).expect("annulus");
        let left_cx = 500.0 - 215.0;
        let cy = 400.0;
        let cap: Vec<(f32, f32)> = mid
            .iter()
            .copied()
            .filter(|p| p.0 < left_cx + 8.0)
            .collect();
        assert!(cap.len() > 16, "left cap samples {}", cap.len());
        let radii: Vec<f32> = cap
            .iter()
            .map(|p| (p.0 - left_cx).hypot(p.1 - cy))
            .collect();
        let mean = radii.iter().sum::<f32>() / radii.len() as f32;
        let max_dev = radii.iter().map(|r| (r - mean).abs()).fold(0.0f32, f32::max);
        assert!(
            max_dev < mean * 0.14,
            "left cap is chamfered: radius spread {max_dev:.1} around {mean:.1}"
        );
        assert!(
            max_turn_deg(&mid) < 12.0,
            "kink {} deg on centreline",
            max_turn_deg(&mid)
        );
    }
}
