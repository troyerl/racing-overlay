//! Fold a stroke-outline "ribbon" back onto its centreline.
//!
//! Members-site exports draw the track as a stroked line converted to a filled
//! outline. For a closed circuit that outline is an annulus — an outer boundary
//! plus an inner hole — and `pick_best_subpath` takes the outer boundary, which
//! laps the circuit exactly once.
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
}
