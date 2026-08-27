//! Turn recorded laps into a lap-% → loop-arc table.
//!
//! Placing a car at `point_at(loop, lap_pct)` assumes the drawn loop's arc length
//! grows in step with real track distance. Imported schematics get the overall
//! shape and scale right but not that distribution, so the dot runs ahead through
//! some parts of the lap and lags through others — measurably up to 40 m, which
//! reads as the dot outpacing the car on straights.
//!
//! The table here records where the car really was, as a fraction of the drawn
//! loop, at each lap %. Feeding lap % through it cancels the distortion without
//! touching the drawing.

use crate::track_path::{nearest_pct_on_loop, point_at};

/// One telemetry sample of the seated car.
#[derive(Clone, Copy, Debug)]
pub struct Sample {
    pub t: f64,
    pub pct: f32,
    pub vx: f32,
    pub vy: f32,
    pub yaw: f32,
}

/// Table knots. 256 resolves a ~14 m feature on a 3.6 km lap, well under the
/// error being corrected, and keeps the JSON small.
pub const KNOTS: usize = 256;
/// Working resolution for shape fitting.
const FIT_N: usize = 720;
/// Dense loop used for projection; finer than the error we are measuring.
const DENSE_N: usize = 4000;
/// Reject a lap whose shape does not match the drawing — a wrong track, a
/// spin, or an off means the projection would be meaningless.
const MAX_FIT_RMS: f32 = 0.05;
/// Fit noise nudges a knot behind its predecessor here and there; that is
/// smoothed away. Past this much total backtracking the walk has lost the
/// track rather than wobbled along it.
const MAX_BACKTRACK: f32 = 0.02;

/// How to fix a drawn loop that does not match the driven path's sense of travel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoopFix {
    None,
    /// Point order runs against LapDistPct. Reversing keeps the familiar drawing
    /// and is the usual Members-SVG oval failure mode.
    Reverse,
    /// Drawing is a left/right mirror of the circuit (rare; prefer [`LoopFix::Reverse`]
    /// when both fit, because ovals make the two look alike to Procrustes).
    Mirror,
}

/// Choose a geometry fix so lap % advances the right way around the drawn loop.
pub fn choose_loop_fix(samples: &[Sample], loop_pts: &[(f32, f32)]) -> LoopFix {
    let Some(lap) = complete_laps(samples).into_iter().next() else {
        return LoopFix::None;
    };
    let path = dead_reckon(lap);
    let shape = resample_by_arc(&path, FIT_N);
    let loop_fit: Vec<(f32, f32)> = (0..FIT_N)
        .map(|i| point_at(loop_pts, i as f32 / FIT_N as f32))
        .collect();
    let mut reversed = loop_pts.to_vec();
    reverse_loop_keep_sf(&mut reversed);
    let rev_fit: Vec<(f32, f32)> = (0..FIT_N)
        .map(|i| point_at(&reversed, i as f32 / FIT_N as f32))
        .collect();

    let plain = fit_rms_for_handedness(&shape, &loop_fit, false);
    let mirrored = fit_rms_for_handedness(&shape, &loop_fit, true);
    let reversed_rms = fit_rms_for_handedness(&shape, &rev_fit, false);

    let plain = plain.unwrap_or(f32::MAX);
    let mirrored = mirrored.unwrap_or(f32::MAX);
    let reversed_rms = reversed_rms.unwrap_or(f32::MAX);

    // Chirality authority: after fitting the dead-reckoned path onto the loop,
    // do projected arcs land near LapDistPct (up to a constant phase)?
    // Delta-of-arc vs delta-of-pct is not enough — a wrong-handed DR path still
    // advances along whichever loop it was fitted to. Absolute arc↔pct
    // alignment catches that. Shape-only reverse on Iowa flipped a correct
    // drawing and made live dots run backwards / sit behind the car.
    let plain_agree = lap_pct_arc_alignment(lap, &path, loop_pts).unwrap_or(0.0);
    let rev_agree = lap_pct_arc_alignment(lap, &path, &reversed).unwrap_or(0.0);

    // Already matches LapDistPct — never reverse (even if DR shape prefers it).
    if plain_agree > 0.7 {
        return LoopFix::None;
    }

    // Diagnostic only for synthetic/fixture tests. Live calibrate must not bake
    // this — see `probe::resolve_loop_fix` (Iowa reverse/mirror flip-flops).
    if rev_agree > plain_agree + 0.35 && rev_agree > 0.55 {
        return LoopFix::Reverse;
    }

    let reverse_ok = reversed_rms < MAX_FIT_RMS
        && reversed_rms * 3.0 < plain
        && rev_agree > plain_agree + 0.35
        && rev_agree > 0.55;
    let mirror_ok = mirrored < MAX_FIT_RMS && mirrored * 3.0 < plain && plain_agree < 0.25;
    match (reverse_ok, mirror_ok) {
        (true, true) if mirrored * 3.0 < reversed_rms => LoopFix::Mirror,
        (true, _) => LoopFix::Reverse,
        (false, true) => LoopFix::Mirror,
        _ => LoopFix::None,
    }
}

/// +1 when fitted DR positions sit at LapDistPct on `loop_pts` (phase-invariant).
///
/// Scores `cos(2π (arc − pct − φ))` at the best phase φ. A reversed drawing
/// scores near −1 / low here even when both pct and arc advance in time.
fn lap_pct_arc_alignment(
    lap: &[Sample],
    path: &[(f32, f32)],
    loop_pts: &[(f32, f32)],
) -> Option<f32> {
    if lap.len() < 32 || path.len() != lap.len() || loop_pts.len() < 8 {
        return None;
    }
    let shape = resample_by_arc(path, FIT_N);
    let loop_fit: Vec<(f32, f32)> = (0..FIT_N)
        .map(|i| point_at(loop_pts, i as f32 / FIT_N as f32))
        .collect();
    let (xf, rms) = best_fit_handedness(&shape, &loop_fit, false)?;
    if rms > MAX_FIT_RMS {
        return None;
    }
    let step = (lap.len() / 120).max(1);
    let mut deltas = Vec::new();
    for i in (0..lap.len()).step_by(step) {
        let p = xf.apply(path[i]);
        let arc = nearest_pct_on_loop(loop_pts, p.0, p.1);
        deltas.push((arc - lap[i].pct).rem_euclid(1.0));
    }
    if deltas.len() < 16 {
        return None;
    }
    // Best constant phase via circular mean of (arc − pct).
    let (mut sum_sin, mut sum_cos) = (0.0f32, 0.0f32);
    for d in &deltas {
        let (s, c) = (*d * std::f32::consts::TAU).sin_cos();
        sum_sin += s;
        sum_cos += c;
    }
    let phase = sum_sin.atan2(sum_cos) / std::f32::consts::TAU;
    let mut score = 0.0f32;
    for d in &deltas {
        let err = (*d - phase).rem_euclid(1.0);
        let err = if err > 0.5 { err - 1.0 } else { err };
        score += (err * std::f32::consts::TAU).cos();
    }
    Some(score / deltas.len() as f32)
}

/// Reverse winding but keep the S/F sample at index 0.
pub fn reverse_loop_keep_sf(pts: &mut Vec<(f32, f32)>) {
    if pts.len() < 3 {
        return;
    }
    let first = pts[0];
    let mut rest: Vec<(f32, f32)> = pts.drain(1..).collect();
    rest.reverse();
    pts.clear();
    pts.push(first);
    pts.extend(rest);
}

/// Reflect every point through the vertical centreline of the polyline set so
/// the drawing matches the driven path's handedness without leaving the bbox.
pub fn mirror_polyline(pts: &mut [(f32, f32)]) {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    for &(x, _) in pts.iter() {
        min_x = min_x.min(x);
        max_x = max_x.max(x);
    }
    if !min_x.is_finite() || !max_x.is_finite() || max_x - min_x < 1e-9 {
        return;
    }
    let cx = 0.5 * (min_x + max_x);
    for p in pts.iter_mut() {
        p.0 = 2.0 * cx - p.0;
    }
}

/// Solve a lap-% → loop-arc table from recorded laps.
pub fn solve_pct_map(samples: &[Sample], loop_pts: &[(f32, f32)]) -> Result<Vec<f32>, String> {
    if loop_pts.len() < 8 {
        return Err("track loop is too short".into());
    }
    let loop_fit: Vec<(f32, f32)> = (0..FIT_N)
        .map(|i| point_at(loop_pts, i as f32 / FIT_N as f32))
        .collect();
    let dense: Vec<(f32, f32)> = (0..DENSE_N)
        .map(|i| point_at(loop_pts, i as f32 / DENSE_N as f32))
        .collect();

    let laps = complete_laps(samples);
    if laps.is_empty() {
        return Err(format!(
            "need a full flying lap (got {} samples spanning {:.1}% of a lap)",
            samples.len(),
            samples
                .last()
                .zip(samples.first())
                .map(|(a, b)| (a.pct - b.pct).rem_euclid(1.0) * 100.0)
                .unwrap_or(0.0)
        ));
    }
    let mut tables = Vec::new();
    let mut rejects = Vec::new();
    for (i, lap) in laps.iter().enumerate() {
        match solve_one_lap(lap, &loop_fit, &dense) {
            Ok(t) => tables.push(t),
            Err(why) => rejects.push(format!("lap {i}: {why}")),
        }
    }
    if tables.is_empty() {
        return Err(format!(
            "no lap matched the drawn loop ({})",
            rejects.join("; ")
        ));
    }
    Ok(average_tables(&tables))
}

/// Slices spanning a full lap, split on the lap-% wrap.
fn complete_laps(samples: &[Sample]) -> Vec<&[Sample]> {
    let mut bounds = vec![0usize];
    for i in 1..samples.len() {
        if samples[i].pct - samples[i - 1].pct < -0.5 {
            bounds.push(i);
        }
    }
    bounds.push(samples.len());
    let mut out = Vec::new();
    for w in bounds.windows(2) {
        let lap = &samples[w[0]..w[1]];
        // A partial out-lap covers only part of the range and would extrapolate.
        if lap.len() < 200 || lap[lap.len() - 1].pct - lap[0].pct <= 0.97 {
            continue;
        }
        // Session restart / pause leaves a hole that dead reckoning cannot bridge.
        let dt_ok = lap.windows(2).all(|p| {
            let dt = p[1].t - p[0].t;
            (0.0..1.0).contains(&dt)
        });
        if dt_ok {
            out.push(lap);
        }
    }
    out
}

fn solve_one_lap(
    lap: &[Sample],
    loop_fit: &[(f32, f32)],
    dense: &[(f32, f32)],
) -> Result<Vec<f32>, String> {
    let path = dead_reckon(lap);
    let shape = resample_by_arc(&path, FIT_N);
    let (xf, rms) = best_fit(&shape, loop_fit).ok_or_else(|| "shape fit failed".to_string())?;
    if rms > MAX_FIT_RMS {
        return Err(format!(
            "driven path does not match drawing (rms {rms:.4} > {MAX_FIT_RMS})"
        ));
    }

    // Sample the driven path at even lap-% steps, then read off where each lands.
    let at_pct: Vec<(f32, f32)> = (0..KNOTS)
        .map(|i| xf.apply(interp_at_pct(lap, &path, i as f32 / KNOTS as f32)))
        .collect();
    let walk = project_monotone(&at_pct, dense).ok_or_else(|| "projection failed".to_string())?;

    // The drawing is wrong for this track if the walk could not follow it
    // round, and a table built on that would scatter the dot.
    if walk.backtrack > MAX_BACKTRACK || !(0.8..1.05).contains(&walk.advance) {
        return Err(format!(
            "projection lost the loop (advance={:.3}, backtrack={:.3})",
            walk.advance, walk.backtrack
        ));
    }
    Ok(walk.arcs)
}

/// Integrate car-local velocity through heading, then spread the loop-closure
/// error over the lap so the ends meet.
fn dead_reckon(lap: &[Sample]) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(lap.len());
    let (mut x, mut y) = (0.0f32, 0.0f32);
    out.push((x, y));
    for w in lap.windows(2) {
        let dt = (w[1].t - w[0].t) as f32;
        if !(0.0..1.0).contains(&dt) {
            out.push((x, y));
            continue;
        }
        let (ax, ay) = world_vel(&w[0]);
        let (bx, by) = world_vel(&w[1]);
        x += 0.5 * (ax + bx) * dt;
        y += 0.5 * (ay + by) * dt;
        out.push((x, y));
    }
    let span = (lap[lap.len() - 1].t - lap[0].t) as f32;
    if span > 1.0 {
        let (dx, dy) = (x, y);
        for (i, p) in out.iter_mut().enumerate() {
            let f = (lap[i].t - lap[0].t) as f32 / span;
            p.0 -= dx * f;
            p.1 -= dy * f;
        }
    }
    out
}

fn world_vel(s: &Sample) -> (f32, f32) {
    let (sin, cos) = s.yaw.sin_cos();
    (s.vx * cos - s.vy * sin, s.vx * sin + s.vy * cos)
}

/// Position on the driven path at a given lap %, by interpolating on lap %.
fn interp_at_pct(lap: &[Sample], path: &[(f32, f32)], pct: f32) -> (f32, f32) {
    let lo = lap[0].pct;
    let hi = lap[lap.len() - 1].pct;
    // The recorded lap starts just after the wrap, so a knot below its first
    // sample belongs to the short span that closes back onto the start.
    let target = if pct < lo { pct + 1.0 } else { pct };
    if target >= hi {
        let t = (target - hi) / (lo + 1.0 - hi).max(1e-6);
        return lerp(path[path.len() - 1], path[0], t.clamp(0.0, 1.0));
    }
    let mut i = lap.partition_point(|s| s.pct < target);
    i = i.clamp(1, lap.len() - 1);
    let (a, b) = (lap[i - 1].pct, lap[i].pct);
    let t = if b > a { (target - a) / (b - a) } else { 0.0 };
    lerp(path[i - 1], path[i], t.clamp(0.0, 1.0))
}

fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn resample_by_arc(pts: &[(f32, f32)], n: usize) -> Vec<(f32, f32)> {
    let mut cum = Vec::with_capacity(pts.len());
    let mut total = 0.0f32;
    cum.push(0.0);
    for w in pts.windows(2) {
        total += dist(w[0], w[1]);
        cum.push(total);
    }
    if total <= 1e-6 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n);
    let mut j = 0usize;
    for i in 0..n {
        let target = total * (i as f32 / n as f32);
        while j + 2 < pts.len() && cum[j + 1] < target {
            j += 1;
        }
        let seg = (cum[j + 1] - cum[j]).max(1e-9);
        out.push(lerp(pts[j], pts[j + 1], (target - cum[j]) / seg));
    }
    out
}

/// Similarity transform (scale + rotation + translation, optionally mirrored).
#[derive(Clone, Copy)]
pub struct Fit {
    mirror: bool,
    ca: (f32, f32),
    cb: (f32, f32),
    s_cos: f32,
    s_sin: f32,
}

impl Fit {
    fn apply(&self, p: (f32, f32)) -> (f32, f32) {
        let x = if self.mirror { -p.0 } else { p.0 };
        let (dx, dy) = (x - self.ca.0, p.1 - self.ca.1);
        (
            self.cb.0 + dx * self.s_cos - dy * self.s_sin,
            self.cb.1 + dx * self.s_sin + dy * self.s_cos,
        )
    }
}

/// Best similarity for one handedness (cyclic start searched).
fn fit_rms_for_handedness(
    shape: &[(f32, f32)],
    loop_fit: &[(f32, f32)],
    mirror: bool,
) -> Option<f32> {
    best_fit_handedness(shape, loop_fit, mirror).map(|(_, e)| e)
}

fn best_fit_handedness(
    shape: &[(f32, f32)],
    loop_fit: &[(f32, f32)],
    mirror: bool,
) -> Option<(Fit, f32)> {
    let n = shape.len();
    if n == 0 || n != loop_fit.len() {
        return None;
    }
    let cb = centroid(loop_fit);
    let b: Vec<(f32, f32)> = loop_fit.iter().map(|p| (p.0 - cb.0, p.1 - cb.1)).collect();
    let sum_b: f32 = b.iter().map(|p| p.0 * p.0 + p.1 * p.1).sum();
    let m: Vec<(f32, f32)> = shape
        .iter()
        .map(|p| (if mirror { -p.0 } else { p.0 }, p.1))
        .collect();
    let ca = centroid(&m);
    let a: Vec<(f32, f32)> = m.iter().map(|p| (p.0 - ca.0, p.1 - ca.1)).collect();
    let sum_a: f32 = a.iter().map(|p| p.0 * p.0 + p.1 * p.1).sum();
    if sum_a <= 1e-12 {
        return None;
    }
    let mut best: Option<(Fit, f32)> = None;
    for k in 0..n {
        let (mut sxx, mut sxy) = (0.0f32, 0.0f32);
        for i in 0..n {
            let p = a[(i + k) % n];
            let q = b[i];
            sxx += p.0 * q.0 + p.1 * q.1;
            sxy += p.0 * q.1 - p.1 * q.0;
        }
        let mag = sxx.hypot(sxy);
        let err = ((sum_b - mag * mag / sum_a).max(0.0) / n as f32).sqrt();
        if best.as_ref().is_some_and(|(_, e)| *e <= err) {
            continue;
        }
        best = Some((
            Fit {
                mirror,
                ca,
                cb,
                s_cos: sxx / sum_a,
                s_sin: sxy / sum_a,
            },
            err,
        ));
    }
    best
}

/// Best similarity taking the driven shape onto the drawn loop, searching both
/// handednesses and every cyclic start offset.
fn best_fit(shape: &[(f32, f32)], loop_fit: &[(f32, f32)]) -> Option<(Fit, f32)> {
    let mut best: Option<(Fit, f32)> = None;
    for mirror in [false, true] {
        let Some(cand) = best_fit_handedness(shape, loop_fit, mirror) else {
            continue;
        };
        if best.as_ref().is_some_and(|(_, e)| *e <= cand.1) {
            continue;
        }
        best = Some(cand);
    }
    best
}

fn centroid(p: &[(f32, f32)]) -> (f32, f32) {
    let n = p.len().max(1) as f32;
    let (mut x, mut y) = (0.0, 0.0);
    for q in p {
        x += q.0;
        y += q.1;
    }
    (x / n, y / n)
}

/// Result of walking the driven points around the drawn loop.
struct Walk {
    /// Arc fraction of each point, non-decreasing around the loop.
    arcs: Vec<f32>,
    /// Total ground the raw walk gave up going backwards, as a lap fraction.
    backtrack: f32,
    /// Ground covered from first point to last, as a lap fraction.
    advance: f32,
}

/// Arc fraction of each point, walking forward around the loop.
///
/// The search window is deliberately narrow. Points arrive one even lap-% step
/// apart, so the next one lies about one step further round, and a window of a
/// few steps covers the distortion being measured. A wider one lets a hairpin's
/// return leg win — a few metres away in space but a hundred ahead in arc —
/// which teleports the table forward and then strands it.
fn project_monotone(pts: &[(f32, f32)], dense: &[(f32, f32)]) -> Option<Walk> {
    let m = dense.len();
    if pts.is_empty() || m < 8 {
        return None;
    }
    let step = m as f32 / pts.len() as f32;
    let back = (step * 1.5).ceil() as isize;
    let ahead = (step * 3.0).ceil() as isize;

    let mut cur = nearest(pts[0], dense, 0, m as isize) as isize;
    // Unwrapped position of the raw walk, and the monotone table it feeds.
    let mut pos = 0.0f32;
    let mut top = 0.0f32;
    let mut backtrack = 0.0f32;
    let start = cur as f32 / m as f32;
    let mut arcs = Vec::with_capacity(pts.len());
    arcs.push(start);
    for p in &pts[1..] {
        let next = nearest(*p, dense, cur - back, ahead + back) as isize;
        let d = signed_wrap(next - cur, m as isize) as f32 / m as f32;
        cur = next;
        pos += d;
        if pos < top {
            backtrack += top - pos;
        }
        top = top.max(pos);
        arcs.push((start + top).rem_euclid(1.0));
    }
    Some(Walk {
        arcs,
        backtrack,
        advance: top,
    })
}

/// Signed step around a loop of `m` indices, in `(-m/2, m/2]`.
fn signed_wrap(d: isize, m: isize) -> isize {
    let d = d.rem_euclid(m);
    if d > m / 2 {
        d - m
    } else {
        d
    }
}

fn nearest(p: (f32, f32), dense: &[(f32, f32)], from: isize, count: isize) -> usize {
    let m = dense.len() as isize;
    let mut best = (f32::MAX, 0usize);
    for k in 0..count {
        let i = (from + k).rem_euclid(m) as usize;
        let d = dist2(p, dense[i]);
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1
}

/// Forward distance from `prev` to `cur` around a unit loop.
fn wrap_delta(cur: f32, prev: f32) -> f32 {
    (cur - prev).rem_euclid(1.0)
}

/// Average tables that may sit either side of the wrap.
fn average_tables(tables: &[Vec<f32>]) -> Vec<f32> {
    let n = tables[0].len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let base = tables[0][i];
        let mut acc = 0.0f64;
        for t in tables {
            let mut d = (t[i] - base) as f64;
            if d > 0.5 {
                d -= 1.0;
            } else if d < -0.5 {
                d += 1.0;
            }
            acc += d;
        }
        let v = base as f64 + acc / tables.len() as f64;
        out.push(v.rem_euclid(1.0) as f32);
    }
    out
}

/// Loop arc fraction for a lap %, reading the calibration table.
pub fn arc_for_pct(table: &[f32], pct: f32) -> f32 {
    let n = table.len();
    if n < 2 {
        return pct.rem_euclid(1.0);
    }
    let x = pct.rem_euclid(1.0) * n as f32;
    let i = (x.floor() as usize).min(n - 1);
    let t = x - i as f32;
    let a = table[i];
    let b = if i + 1 < n { table[i + 1] } else { table[0] };
    (a + wrap_delta(b, a) * t).rem_euclid(1.0)
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    dist2(a, b).sqrt()
}

fn dist2(a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (a.0 - b.0, a.1 - b.1);
    dx * dx + dy * dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    /// Drive a circle of `radius` at constant speed, `n` samples per lap.
    fn circle_lap(n: usize, laps: usize) -> Vec<Sample> {
        let dt = 0.02f64;
        let speed = 40.0f32;
        let mut out = Vec::new();
        for i in 0..(n * laps + 1) {
            let lap_pos = i as f32 / n as f32;
            out.push(Sample {
                t: i as f64 * dt,
                pct: lap_pos.fract(),
                vx: speed,
                vy: 0.0,
                yaw: lap_pos.fract() * TAU + TAU / 4.0,
            });
        }
        out
    }

    fn circle_loop(n: usize) -> Vec<(f32, f32)> {
        (0..n)
            .map(|i| {
                let a = i as f32 / n as f32 * TAU;
                (0.5 + 0.4 * a.cos(), 0.5 + 0.4 * a.sin())
            })
            .collect()
    }

    #[test]
    fn an_undistorted_lap_maps_pct_straight_through() {
        let samples = circle_lap(3000, 2);
        let table = solve_pct_map(&samples, &circle_loop(400)).expect("solved");
        assert_eq!(table.len(), KNOTS);
        // Table is only defined up to where the loop's own origin sits, so
        // compare advance from the first knot rather than absolute values.
        for i in 0..KNOTS {
            let want = i as f32 / KNOTS as f32;
            let got = wrap_delta(table[i], table[0]);
            assert!(
                (got - want).abs() < 0.02,
                "knot {i}: expected {want:.3}, got {got:.3}"
            );
        }
    }

    #[test]
    fn table_lookup_interpolates_and_wraps() {
        let table: Vec<f32> = (0..KNOTS).map(|i| i as f32 / KNOTS as f32).collect();
        assert!((arc_for_pct(&table, 0.25) - 0.25).abs() < 1e-4);
        // Halfway between the last knot and the wrap back to zero.
        let mid = arc_for_pct(&table, 1.0 - 0.5 / KNOTS as f32);
        assert!(!(0.003..=0.997).contains(&mid), "got {mid}");
        assert!((arc_for_pct(&table, 1.25) - arc_for_pct(&table, 0.25)).abs() < 1e-4);
    }

    #[test]
    fn a_partial_lap_yields_nothing() {
        let mut samples = circle_lap(3000, 1);
        samples.truncate(900);
        assert!(solve_pct_map(&samples, &circle_loop(400)).is_err());
    }

    /// A recorded-lap fixture: `[t, pct, vx, vy, yaw]` rows plus the loop the
    /// map draws for that track.
    fn fixture(name: &str) -> (Vec<Sample>, Vec<(f32, f32)>) {
        let mut p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        p.pop();
        p.pop();
        let raw = std::fs::read_to_string(p.join("tests/fixtures").join(name)).expect("fixture");
        let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let samples = doc["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| Sample {
                t: r[0].as_f64().unwrap(),
                pct: r[1].as_f64().unwrap() as f32,
                vx: r[2].as_f64().unwrap() as f32,
                vy: r[3].as_f64().unwrap() as f32,
                yaw: r[4].as_f64().unwrap() as f32,
            })
            .collect();
        let loop_pts = doc["loop"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| (p[0].as_f64().unwrap() as f32, p[1].as_f64().unwrap() as f32))
            .collect();
        (samples, loop_pts)
    }

    #[test]
    fn a_recorded_lap_recovers_the_measured_distortion() {
        let (samples, loop_pts) = fixture("trackpath_charlotte_roval.json");
        let table = solve_pct_map(&samples, &loop_pts).expect("solved from recorded lap");
        let resid: Vec<f32> = (0..KNOTS)
            .map(|i| {
                let want = i as f32 / KNOTS as f32;
                let got = wrap_delta(table[i], table[0]);
                let d = got - want;
                if d > 0.5 {
                    d - 1.0
                } else if d < -0.5 {
                    d + 1.0
                } else {
                    d
                }
            })
            .collect();
        let lead = resid.iter().cloned().fold(f32::MIN, f32::max);
        let lag = resid.iter().cloned().fold(f32::MAX, f32::min);
        // Applying the table must cancel what it measured: feeding each knot's
        // lap % back through the lookup has to land on that knot's arc.
        for i in 0..KNOTS {
            let got = arc_for_pct(&table, i as f32 / KNOTS as f32);
            let err = wrap_delta(got, table[i]).min(wrap_delta(table[i], got));
            assert!(err < 1e-3, "knot {i} round-trips to {err:.5} off");
        }
        // Offline analysis of this lap put the drawn loop between 1.2% behind
        // and 0.5% ahead of the car; the solver must see the same thing, and in
        // particular must not flatten it into the identity mapping.
        assert!(
            (0.003..0.010).contains(&lead),
            "expected a small forward error, got {lead:.4}"
        );
        assert!(
            (-0.020..-0.008).contains(&lag),
            "expected the ~1.2% lag, got {lag:.4}"
        );
    }

    /// Charlotte's infield hairpin folds back within a few metres of itself, so
    /// a projection window reaching far enough ahead finds the return leg
    /// closer than the leg the car is actually on. The walk jumped the gap,
    /// doubled back once past it, and the whole recording was thrown away.
    #[test]
    fn a_hairpin_does_not_throw_the_walk_onto_the_return_leg() {
        let (samples, loop_pts) = fixture("trackpath_charlotte_roval_hairpin.json");
        let table = solve_pct_map(&samples, &loop_pts).expect("solved despite the hairpin");

        let mut worst = 0.0f32;
        for i in 1..table.len() {
            let step = wrap_delta(table[i], table[i - 1]);
            worst = worst.max(step);
        }
        // Knots sit one 256th of a lap apart, so even the fastest stretch
        // advances only a small multiple of that; a snap crosses several.
        assert!(
            worst < 4.0 / KNOTS as f32,
            "a knot advanced {worst:.4} of a lap in one step"
        );
        // The table has to keep describing a real distortion, not collapse to
        // the identity that the tight window is there to preserve.
        let lead = (0..KNOTS)
            .map(|i| wrap_delta(table[i], table[0]) - i as f32 / KNOTS as f32)
            .fold(f32::MIN, f32::max);
        assert!(lead > 0.002, "distortion was flattened away: {lead:.4}");
    }

    /// Elongated circuit so a left/right flip is not a free rotation of a circle.
    fn oval_lap(n: usize) -> (Vec<Sample>, Vec<(f32, f32)>) {
        let dt = 0.02f64;
        let mut samples = Vec::new();
        let mut loop_pts = Vec::with_capacity(n);
        for i in 0..n {
            let a = i as f32 / n as f32 * TAU;
            // Stadium-ish: stretch X so mirror cannot be absorbed into rotation.
            let (x, y) = (0.5 + 0.45 * a.cos(), 0.5 + 0.22 * a.sin());
            loop_pts.push((x, y));
        }
        // Constant-speed drive around the same oval.
        let mut dist = 0.0f32;
        let mut cum = vec![0.0f32];
        for w in loop_pts.windows(2) {
            dist += dist2(w[0], w[1]).sqrt();
            cum.push(dist);
        }
        dist += dist2(loop_pts[loop_pts.len() - 1], loop_pts[0]).sqrt();
        let speed = dist / (n as f32 * dt as f32);
        for i in 0..(n + 1) {
            let frac = (i % n) as f32 / n as f32;
            let a = frac * TAU;
            // Tangent heading for the parametric oval.
            let tx = -0.45 * a.sin();
            let ty = 0.22 * a.cos();
            let yaw = ty.atan2(tx);
            samples.push(Sample {
                t: i as f64 * dt,
                pct: frac,
                vx: speed,
                vy: 0.0,
                yaw,
            });
        }
        // Two laps so complete_laps keeps a full flying one.
        let mut two = samples.clone();
        let t0 = samples.last().unwrap().t;
        for s in &samples[1..] {
            two.push(Sample {
                t: t0 + s.t,
                pct: s.pct,
                vx: s.vx,
                vy: s.vy,
                yaw: s.yaw,
            });
        }
        (two, loop_pts)
    }

    #[test]
    fn reverse_winding_is_preferred_over_mirror_on_an_oval() {
        let (samples, loop_pts) = oval_lap(400);
        assert_eq!(choose_loop_fix(&samples, &loop_pts), LoopFix::None);

        let mut reversed = loop_pts.clone();
        reverse_loop_keep_sf(&mut reversed);
        assert_eq!(choose_loop_fix(&samples, &reversed), LoopFix::Reverse);

        // Left/right flip of a near-symmetric oval looks like reverse to
        // Procrustes — we still prefer reverse so the familiar outline is kept.
        let mut mirrored = loop_pts.clone();
        for p in &mut mirrored {
            p.0 = 1.0 - p.0;
        }
        let fix = choose_loop_fix(&samples, &mirrored);
        assert!(
            matches!(fix, LoopFix::Reverse | LoopFix::Mirror),
            "opposite chirality should be detected, got {fix:?}"
        );
    }
}
