//! Stitch members-HTML `#Pitroad` / `#Mergeline` dash segments into open polylines.

use regex::Regex;

use super::layers::{apply_norm, NormParams};
use super::path_sample::{paths_d_in_svg, split_subpaths};

/// Minimum dashes required for a usable stitched centerline.
const MIN_DASHES: usize = 4;
/// Merge lines are often shorter / sparser than the pit road.
const MIN_MERGE_DASHES: usize = 3;
/// Reject a chain whose total length is tiny vs the SVG bbox diagonal.
const MIN_CHAIN_FRAC: f32 = 0.04;

type Poly = Vec<(f32, f32)>;

#[derive(Debug, Clone)]
pub struct PitPolylinesSvg {
    pub road: Vec<(f32, f32)>,
    pub merge: Vec<(f32, f32)>,
    pub entry: Vec<(f32, f32)>,
}

/// Extract pit / merge / optional entry polylines from a pit-layer SVG fragment
/// (SVG coordinates, not yet normalized).
pub fn extract_pit_polylines_svg(pit_svg: &str) -> Option<PitPolylinesSvg> {
    let pitroad = group_inner(pit_svg, "Pitroad").unwrap_or_else(|| pit_svg.to_string());
    let mergeline = group_inner(pit_svg, "Mergeline");

    let road_subs = collect_subpaths(&pitroad);
    if road_subs.is_empty() {
        return None;
    }
    let (dashes, longs) = split_dashes_and_long(&road_subs);
    let dash_src = if dashes.len() >= MIN_DASHES {
        &dashes
    } else {
        &road_subs
    };
    let stitched = stitch_dash_midpoints(dash_src, MIN_DASHES)?;
    if stitched.len() < 2 {
        return None;
    }

    let (road, merge) = if let Some(m) = mergeline.as_deref() {
        let merge_subs = collect_subpaths(m);
        // Sparse Mergeline ticks (Lime Rock: 3 chevrons) can sit just outside
        // the default NN jump cap; fall back to axis-sorted midpoints before
        // synthesizing a stub from the road end (which picks the wrong exit).
        let merge = stitch_dash_midpoints(&merge_subs, MIN_MERGE_DASHES)
            .or_else(|| sort_midpoints_along_axis(&merge_subs, MIN_MERGE_DASHES))
            .or_else(|| {
                // Leftover paths in the pit SVG (not in Pitroad).
                let all = collect_subpaths(pit_svg);
                let rest: Vec<_> = all
                    .into_iter()
                    .filter(|s| !road_subs.iter().any(|r| approx_same_poly(r, s)))
                    .collect();
                stitch_dash_midpoints(&rest, MIN_MERGE_DASHES)
                    .or_else(|| sort_midpoints_along_axis(&rest, MIN_MERGE_DASHES))
            })
            .or_else(|| synthesize_merge_from_road(&stitched))?;
        (stitched, merge)
    } else {
        // Charlotte-style: merge ticks live inside `#Pitroad` (no Mergeline).
        // Prefer splitting an exit climb off the stitched centerline.
        if let Some((road, merge)) = split_exit_climb(&stitched) {
            (road, merge)
        } else {
            let merge = synthesize_merge_from_road(&stitched)?;
            (stitched, merge)
        }
    };
    if road.len() < 2 || merge.len() < 2 {
        return None;
    }

    // Optional entry: longest "long" path whose end is near the road start.
    let entry = pick_entry_blend(&longs, &road).unwrap_or_default();

    Some(PitPolylinesSvg { road, merge, entry })
}

/// Normalize SVG pit polylines with the racing-loop `NormParams`.
pub fn normalize_pit_polylines(pit: &PitPolylinesSvg, norm: NormParams) -> (Poly, Poly, Poly) {
    (
        apply_norm(&pit.road, norm),
        apply_norm(&pit.merge, norm),
        apply_norm(&pit.entry, norm),
    )
}

fn group_inner(svg: &str, id: &str) -> Option<String> {
    let re = Regex::new(&format!(
        r#"(?is)<g[^>]*\bid\s*=\s*["']{}["'][^>]*>([\s\S]*?)</g>"#,
        regex::escape(id)
    ))
    .ok()?;
    re.captures(svg)
        .and_then(|c| c.get(1).map(|m| m.as_str().to_string()))
}

fn collect_subpaths(svg_or_fragment: &str) -> Vec<Vec<(f32, f32)>> {
    let mut out = Vec::new();
    for d in paths_d_in_svg(svg_or_fragment) {
        // Whole-path parse first; on failure, parse each Move chunk so one
        // broken cubic (common in members HTML) does not drop the rest.
        let chunks = match split_subpaths(&d) {
            Ok(subs) if !subs.is_empty() => {
                for s in subs {
                    push_sub(&mut out, s);
                }
                continue;
            }
            _ => split_d_on_moves(&d),
        };
        for chunk in chunks {
            if let Ok(subs) = split_subpaths(&chunk) {
                for s in subs {
                    push_sub(&mut out, s);
                }
            }
        }
    }
    out
}

fn push_sub(out: &mut Vec<Vec<(f32, f32)>>, s: Vec<(f32, f32)>) {
    if s.len() >= 2 && polyline_length(&s) > 1e-3 {
        out.push(s);
    }
}

/// Split a path `d` into Move-led chunks so bad segments can be skipped.
fn split_d_on_moves(d: &str) -> Vec<String> {
    let bytes = d.as_bytes();
    let mut starts = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'M' || b == b'm' {
            // Avoid matching inside numbers (not needed for command letters).
            let boundary = i == 0 || !bytes[i - 1].is_ascii_alphanumeric();
            if boundary {
                starts.push(i);
            }
        }
        i += 1;
    }
    if starts.is_empty() {
        return vec![d.to_string()];
    }
    let mut out = Vec::with_capacity(starts.len());
    for (k, &s) in starts.iter().enumerate() {
        let e = starts.get(k + 1).copied().unwrap_or(d.len());
        let chunk = d[s..e].trim();
        if !chunk.is_empty() {
            out.push(chunk.to_string());
        }
    }
    out
}

fn polyline_length(pts: &[(f32, f32)]) -> f32 {
    pts.windows(2)
        .map(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1))
        .sum()
}

fn bbox_diag(subs: &[Vec<(f32, f32)>]) -> f32 {
    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    for s in subs {
        for &(x, y) in s {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    if !min_x.is_finite() {
        return 1.0;
    }
    (max_x - min_x).hypot(max_y - min_y).max(1.0)
}

fn split_dashes_and_long(subs: &[Poly]) -> (Vec<Poly>, Vec<Poly>) {
    if subs.is_empty() {
        return (vec![], vec![]);
    }
    let longest = subs
        .iter()
        .map(|s| polyline_length(s))
        .fold(0.0f32, f32::max)
        .max(1e-3);
    let mut dashes = Vec::new();
    let mut longs = Vec::new();
    for s in subs {
        let len = polyline_length(s);
        // Short tick / dash vs long continuous stroke (entry blend, chevrons).
        if len <= longest * 0.35 || len <= 120.0 {
            dashes.push(s.clone());
        } else {
            longs.push(s.clone());
        }
    }
    // If almost everything was classified long, treat all as dashes.
    if dashes.len() < MIN_DASHES && longs.len() >= MIN_DASHES {
        return (subs.to_vec(), vec![]);
    }
    (dashes, longs)
}

#[derive(Clone, Copy)]
struct Dash {
    mid: (f32, f32),
}

/// Centroid (works for closed tick shapes where first≈last).
fn dash_mid(s: &[(f32, f32)]) -> Option<(f32, f32)> {
    if s.is_empty() {
        return None;
    }
    let n = s.len() as f32;
    let (sx, sy) = s
        .iter()
        .fold((0.0f32, 0.0f32), |(ax, ay), p| (ax + p.0, ay + p.1));
    Some((sx / n, sy / n))
}

fn stitch_dash_midpoints(subs: &[Vec<(f32, f32)>], min_dashes: usize) -> Option<Vec<(f32, f32)>> {
    if subs.len() < min_dashes {
        // Few segments: if one is already a decent open polyline, use it.
        let best = subs.iter().max_by(|a, b| {
            polyline_length(a)
                .partial_cmp(&polyline_length(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;
        if best.len() >= 4 && polyline_length(best) > 1.0 {
            return Some(best.clone());
        }
        return None;
    }
    let dashes: Vec<Dash> = subs
        .iter()
        .filter_map(|s| Some(Dash { mid: dash_mid(s)? }))
        .collect();
    if dashes.len() < min_dashes {
        return None;
    }

    let diag = bbox_diag(subs);
    // Prefer local spacing over huge bbox fractions so exit-climb ticks do not
    // leap across the map; still allow ~3× median gap.
    let max_jump = adaptive_max_jump(&dashes, diag);

    // Start at the dash whose midpoint is farthest from the centroid (an end).
    let cx: f32 = dashes.iter().map(|d| d.mid.0).sum::<f32>() / dashes.len() as f32;
    let cy: f32 = dashes.iter().map(|d| d.mid.1).sum::<f32>() / dashes.len() as f32;
    let mut start_i = 0usize;
    let mut best_d = -1.0f32;
    for (i, d) in dashes.iter().enumerate() {
        let dist = (d.mid.0 - cx).hypot(d.mid.1 - cy);
        if dist > best_d {
            best_d = dist;
            start_i = i;
        }
    }

    let mut chain = grow_chain(&dashes, start_i, max_jump);

    // If coverage is weak, try the opposite extreme as a start.
    if chain.len() < dashes.len() / 2 {
        let mut alt_i = 0usize;
        let mut alt_d = -1.0f32;
        for (i, d) in dashes.iter().enumerate() {
            let dist = (d.mid.0 - dashes[start_i].mid.0).hypot(d.mid.1 - dashes[start_i].mid.1);
            if dist > alt_d {
                alt_d = dist;
                alt_i = i;
            }
        }
        let alt = grow_chain(&dashes, alt_i, max_jump);
        if alt.len() > chain.len() {
            chain = alt;
        }
    }

    if chain.len() < min_dashes {
        return None;
    }
    let chain_len = polyline_length(&chain);
    if chain_len < diag * MIN_CHAIN_FRAC {
        return None;
    }
    Some(chain)
}

/// Split a stitched pit centerline into road + exit climb when merge ticks were
/// drawn inside `#Pitroad` (no `#Mergeline` group).
fn split_exit_climb(chain: &[(f32, f32)]) -> Option<(Poly, Poly)> {
    if chain.len() < 8 {
        return None;
    }
    let head_end = (chain.len() * 3 / 5).max(3);
    let dx = chain[head_end].0 - chain[0].0;
    let dy = chain[head_end].1 - chain[0].1;
    let horiz = dx.abs() >= dy.abs();

    // Walk from the exit end while segments are off the main pit axis.
    let mut merge_start = None;
    for i in (1..chain.len()).rev() {
        let sx = chain[i].0 - chain[i - 1].0;
        let sy = chain[i].1 - chain[i - 1].1;
        let along_main = if horiz {
            sx.abs() >= sy.abs() * 0.85
        } else {
            sy.abs() >= sx.abs() * 0.85
        };
        if along_main {
            if chain.len() - i >= 2 {
                merge_start = Some(i);
            }
            break;
        }
    }
    let ms = merge_start?;
    if ms < MIN_DASHES {
        return None;
    }
    let road = chain[..=ms].to_vec();
    let merge = chain[ms..].to_vec();
    if road.len() < 2 || merge.len() < 2 {
        return None;
    }
    // Merge should be a meaningful climb/turn, not a tiny stub.
    if polyline_length(&merge) < polyline_length(&road) * 0.05 {
        return None;
    }
    Some((road, merge))
}

/// Short exit stub from the last road segment (loop blend fills the rest).
fn synthesize_merge_from_road(road: &[(f32, f32)]) -> Option<Vec<(f32, f32)>> {
    if road.len() < 2 {
        return None;
    }
    let a = road[road.len() - 2];
    let b = *road.last()?;
    let dx = b.0 - a.0;
    let dy = b.1 - a.1;
    let len = dx.hypot(dy).max(1.0);
    let ext = 48.0;
    let c = (b.0 + dx / len * ext, b.1 + dy / len * ext);
    Some(vec![b, c])
}

/// Max jump from local spacing (median NN × 3), capped by bbox diagonal.
fn adaptive_max_jump(dashes: &[Dash], diag: f32) -> f32 {
    let mut nn: Vec<f32> = Vec::with_capacity(dashes.len());
    for (i, d) in dashes.iter().enumerate() {
        let mut best = f32::MAX;
        for (j, o) in dashes.iter().enumerate() {
            if i == j {
                continue;
            }
            let dist = (d.mid.0 - o.mid.0).hypot(d.mid.1 - o.mid.1);
            if dist < best {
                best = dist;
            }
        }
        if best.is_finite() {
            nn.push(best);
        }
    }
    nn.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = if nn.is_empty() {
        diag * 0.1
    } else {
        nn[nn.len() / 2]
    };
    // Sparse merge ticks (e.g. Lime Rock Mergeline) often sit at ~0.35·diag
    // spacing; the old 0.35 cap rejected the next tick by a fraction of a
    // pixel and fell through to a synthesized stub on the wrong road end.
    let cap = if dashes.len() <= 6 {
        diag * 0.55
    } else {
        diag * 0.35
    };
    (median * 3.0).max(diag * 0.08).min(cap).max(1.0)
}

/// Sort dash midpoints along the cluster's long axis (fallback when NN stitch
/// fails on a short, evenly spaced Mergeline).
fn sort_midpoints_along_axis(
    subs: &[Vec<(f32, f32)>],
    min_dashes: usize,
) -> Option<Vec<(f32, f32)>> {
    let mut mids: Vec<(f32, f32)> = subs.iter().filter_map(|s| dash_mid(s)).collect();
    if mids.len() < min_dashes {
        return None;
    }
    let cx = mids.iter().map(|p| p.0).sum::<f32>() / mids.len() as f32;
    let cy = mids.iter().map(|p| p.1).sum::<f32>() / mids.len() as f32;
    let mut var_x = 0.0f32;
    let mut var_y = 0.0f32;
    for p in &mids {
        var_x += (p.0 - cx).powi(2);
        var_y += (p.1 - cy).powi(2);
    }
    if var_x >= var_y {
        mids.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        mids.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    }
    let chain_len = polyline_length(&mids);
    let diag = bbox_diag(subs);
    if chain_len < diag * MIN_CHAIN_FRAC {
        return None;
    }
    Some(mids)
}

/// Greedy nearest-neighbor chain grown forward, then backward from the start.
fn grow_chain(dashes: &[Dash], start_i: usize, max_jump: f32) -> Vec<(f32, f32)> {
    let mut used = vec![false; dashes.len()];
    used[start_i] = true;

    let mut forward = Vec::new();
    let mut cur = dashes[start_i].mid;
    while let Some(i) = nearest_unused(dashes, &used, cur, max_jump) {
        used[i] = true;
        cur = dashes[i].mid;
        forward.push(cur);
    }

    let mut backward = Vec::new();
    cur = dashes[start_i].mid;
    while let Some(i) = nearest_unused(dashes, &used, cur, max_jump) {
        used[i] = true;
        cur = dashes[i].mid;
        backward.push(cur);
    }

    backward.reverse();
    let mut chain = backward;
    chain.push(dashes[start_i].mid);
    chain.extend(forward);
    chain
}

fn nearest_unused(dashes: &[Dash], used: &[bool], cur: (f32, f32), max_jump: f32) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;
    for (i, d) in dashes.iter().enumerate() {
        if used[i] {
            continue;
        }
        let dist = (d.mid.0 - cur.0).hypot(d.mid.1 - cur.1);
        if dist > max_jump {
            continue;
        }
        if best.as_ref().is_some_and(|(_, bd)| *bd <= dist) {
            continue;
        }
        best = Some((i, dist));
    }
    best.map(|(i, _)| i)
}

fn pick_entry_blend(longs: &[Vec<(f32, f32)>], road: &[(f32, f32)]) -> Option<Vec<(f32, f32)>> {
    let road0 = *road.first()?;
    let road_len = polyline_length(road).max(1.0);
    let mut best: Option<(f32, Vec<(f32, f32)>)> = None;
    for s in longs {
        if s.len() < 2 {
            continue;
        }
        let a = *s.first()?;
        let b = *s.last()?;
        let da = (a.0 - road0.0).hypot(a.1 - road0.1);
        let db = (b.0 - road0.0).hypot(b.1 - road0.1);
        let (dist_join, oriented) = if da <= db {
            (da, s.clone())
        } else {
            let mut rev = s.clone();
            rev.reverse();
            (db, rev)
        };
        if dist_join > 80.0 {
            continue;
        }
        let len = polyline_length(&oriented);
        // Reject apron/decoration strokes that are as long as the pit itself
        // (Iowa HTML has closed ticks that flatten into "long" polylines).
        if len > road_len * 0.45 || len > 220.0 {
            continue;
        }
        let tip = *oriented.first()?;
        let span = (tip.0 - road0.0).hypot(tip.1 - road0.1);
        if span > road_len * 0.35 {
            continue;
        }
        let score = len / (dist_join + 1.0);
        if best.as_ref().is_some_and(|(bs, _)| *bs >= score) {
            continue;
        }
        best = Some((score, oriented));
    }
    best.map(|(_, p)| p)
}

fn approx_same_poly(a: &[(f32, f32)], b: &[(f32, f32)]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let n = a.len().min(3);
    (0..n).all(|i| (a[i].0 - b[i].0).abs() < 1e-3 && (a[i].1 - b[i].1).abs() < 1e-3)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stitches_horizontal_dashes() {
        let svg = r#"<svg>
<g id="Pitroad">
<path d="M0,0 L10,0"/><path d="M20,0 L30,0"/><path d="M40,0 L50,0"/>
<path d="M60,0 L70,0"/><path d="M80,0 L90,0"/><path d="M100,0 L110,0"/>
</g>
<g id="Mergeline">
<path d="M110,0 L110,10"/><path d="M110,20 L110,30"/><path d="M110,40 L110,50"/>
<path d="M110,60 L110,70"/><path d="M110,80 L110,90"/>
</g>
</svg>"#;
        let pit = extract_pit_polylines_svg(svg).expect("pit");
        assert!(pit.road.len() >= 4);
        assert!(pit.merge.len() >= 4);
        // Road should run roughly left→right (or reverse).
        let dx = (pit.road.last().unwrap().0 - pit.road[0].0).abs();
        assert!(dx > 40.0, "road dx={dx}");
    }

    #[test]
    fn skips_broken_cubic_in_mergeline() {
        // Members HTML sometimes emits incomplete cubics mid-path.
        let svg = r#"<svg>
<g id="Pitroad">
<path d="M0,0 L10,0 M20,0 L30,0 M40,0 L50,0 M60,0 L70,0 M80,0 L90,0"/>
</g>
<g id="Mergeline">
<path d="M100,0c-4.8,0 M100,20 l0,10 M100,40 l0,10 M100,60 l0,10 M100,80 l0,10"/>
</g>
</svg>"#;
        let pit = extract_pit_polylines_svg(svg).expect("pit despite broken cubic");
        assert!(pit.merge.len() >= 3);
    }

    #[test]
    fn stitches_sparse_mergeline_near_jump_cap() {
        // Three merge ticks spaced at ~0.35·diag — previously rejected by the
        // adaptive jump cap and replaced with a wrong-end synthesized stub.
        let svg = r#"<svg>
<g id="Pitroad">
<path d="M600,100 L610,100"/><path d="M660,100 L670,100"/><path d="M720,100 L730,100"/>
<path d="M780,100 L790,100"/><path d="M840,100 L850,100"/><path d="M900,100 L910,100"/>
<path d="M960,100 L970,100"/><path d="M1020,100 L1030,100"/>
</g>
<g id="Mergeline">
<path d="M450,100 L460,100"/><path d="M510,100 L520,100"/><path d="M570,100 L580,100"/>
</g>
</svg>"#;
        let pit = extract_pit_polylines_svg(svg).expect("pit");
        assert!(pit.merge.len() >= 3, "merge={}", pit.merge.len());
        let merge_x: f32 = pit.merge.iter().map(|p| p.0).sum::<f32>() / pit.merge.len() as f32;
        let road_x: f32 = pit.road.iter().map(|p| p.0).sum::<f32>() / pit.road.len() as f32;
        assert!(
            merge_x < road_x,
            "merge should sit left of pit road, merge_x={merge_x} road_x={road_x}"
        );
    }

    #[test]
    fn splits_exit_climb_without_mergeline() {
        // Horizontal pit dashes + vertical exit ticks, all under Pitroad.
        let svg = r#"<svg><g id="Pitroad">
<path d="M0,100 L10,100"/><path d="M20,100 L30,100"/><path d="M40,100 L50,100"/>
<path d="M60,100 L70,100"/><path d="M80,100 L90,100"/><path d="M100,100 L110,100"/>
<path d="M120,100 L130,100"/><path d="M140,100 L150,100"/><path d="M160,100 L170,100"/>
<path d="M180,100 L190,100"/><path d="M200,100 L210,100"/>
<path d="M210,80 L210,72"/><path d="M210,60 L210,52"/><path d="M210,40 L210,32"/>
<path d="M210,20 L210,12"/>
</g></svg>"#;
        let pit = extract_pit_polylines_svg(svg).expect("pit");
        assert!(pit.road.len() >= 8, "road={}", pit.road.len());
        assert!(pit.merge.len() >= 3, "merge={}", pit.merge.len());
        let road_dy = (pit.road.last().unwrap().1 - pit.road[0].1).abs();
        let merge_dy = (pit.merge.last().unwrap().1 - pit.merge[0].1).abs();
        assert!(road_dy < 30.0, "road should stay horizontal, dy={road_dy}");
        assert!(merge_dy > 20.0, "merge should climb, dy={merge_dy}");
    }
}
