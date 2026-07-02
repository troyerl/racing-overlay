#!/usr/bin/env python3
"""Import iRacing members-site track map SVG layers into schema-2 track JSON.

The members.iracing.com track page embeds separate SVG layers (no API token):
  * active-config  — racing line
  * pit (#Pitroad + #Mergeline) — red pit road + blue safe merge
  * turn-numbers   — corner labels
  * start-finish   — S/F tick (optional, for loop alignment)

Save the track page HTML from DevTools (outer ``#track-map-*`` div or full page),
or save individual layer SVGs, then run:

    python3 tools/svg_layers_to_track.py page.html <TrackID> "Track Name"
    python3 tools/svg_layers_to_track.py --config loop.svg --pit pit.svg 123 "Name"

No OpenCV required — vector paths chain cleanly (unlike PNG screenshots).
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import paths, svgpath
from tools.schematic_to_track import (
    _bbox_span,
    _chain_segments,
    _connect_blend_to_loop,
    _dist,
    _ensure_ccw,
    _nearest_index,
    _orient_poly_from_anchor,
    _oval_corners,
    _pct_on_loop,
    _pit_span_on_loop,
    _poly_len,
    _reorder_loop,
    _resample_closed,
    _resample_open,
    _smooth_poly,
)

def _read_text(path: str | None) -> str | None:
    if not path:
        return None
    with open(path, encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _layer_svg(html: str, class_token: str) -> str | None:
    """Extract the inner <svg> from a track-map layer div."""
    m = re.search(
        rf'<div[^>]*class="[^"]*\b{re.escape(class_token)}\b[^"]*"[^>]*>\s*'
        rf"(<svg[\s\S]*?</svg>)",
        html,
        re.IGNORECASE,
    )
    return m.group(1) if m else None


def _paths_d_in_svg(svg_text: str, *, group_id: str | None = None) -> list[str]:
    chunk = svg_text
    if group_id:
        gm = re.search(
            rf'<g[^>]*\bid=["\']{re.escape(group_id)}["\'][^>]*>([\s\S]*?)</g>',
            svg_text,
            re.IGNORECASE,
        )
        if not gm:
            return []
        chunk = gm.group(1)
    ds = re.findall(r'\bd\s*=\s*"([^"]+)"', chunk, re.IGNORECASE)
    ds += re.findall(r"\bd\s*=\s*'([^']+)'", chunk, re.IGNORECASE)
    return ds


def _group_chunk(svg_text: str, group_id: str) -> str:
    gm = re.search(
        rf'<g[^>]*\bid=["\']{re.escape(group_id)}["\'][^>]*>([\s\S]*?)</g>',
        svg_text,
        re.IGNORECASE,
    )
    return gm.group(1) if gm else ""


def _parse_points_attr(raw: str) -> list[tuple[float, float]]:
    nums = re.findall(r"-?[\d.]+(?:e[-+]?\d+)?", raw, re.IGNORECASE)
    vals = [float(n) for n in nums]
    if len(vals) < 4 or len(vals) % 2:
        return []
    return [(vals[i], vals[i + 1]) for i in range(0, len(vals), 2)]


def _segments_from_group(svg_text: str, group_id: str) -> list[list[tuple[float, float]]]:
    """Flatten paths per dash subpath; include polygon/rect primitives."""
    chunk = _group_chunk(svg_text, group_id)
    if not chunk:
        return []
    out: list[list[tuple[float, float]]] = []
    for d in _paths_d_in_svg(svg_text, group_id=group_id):
        try:
            subs = svgpath.split_subpaths(d)
        except (ValueError, IndexError):
            subs = []
        if not subs:
            try:
                flat = svgpath.flatten_path(d)
                if len(flat) >= 2:
                    subs = [flat]
            except (ValueError, IndexError):
                subs = []
        for sub in subs:
            if len(sub) >= 2:
                out.append(sub)
    for raw in re.findall(
        r"<polygon[^>]*\bpoints\s*=\s*\"([^\"]+)\"",
        chunk,
        re.IGNORECASE,
    ):
        pts = _parse_points_attr(raw)
        if len(pts) >= 3:
            out.append(pts)
    for raw in re.findall(
        r"<polygon[^>]*\bpoints\s*=\s*'([^']+)'",
        chunk,
        re.IGNORECASE,
    ):
        pts = _parse_points_attr(raw)
        if len(pts) >= 3:
            out.append(pts)
    for m in re.finditer(
        r"<rect[^>]*\bx\s*=\s*\"([^\"]+)\"[^>]*\by\s*=\s*\"([^\"]+)\"[^>]*"
        r"\bwidth\s*=\s*\"([^\"]+)\"[^>]*\bheight\s*=\s*\"([^\"]+)\"",
        chunk,
        re.IGNORECASE,
    ):
        x, y, w, h = (float(m.group(i)) for i in range(1, 5))
        out.append([(x, y), (x + w, y), (x + w, y + h), (x, y + h)])
    return out


def _chain_dashes(segments: list[list[tuple[float, float]]], *,
                  max_gap: float = 120.0) -> list[tuple[float, float]]:
    if not segments:
        return []
    poly = _chain_segments(segments, max_gap=max_gap)
    if len(poly) >= 4:
        return poly
    ordered = sorted(
        segments,
        key=lambda s: (sum(p[0] for p in s) / len(s), sum(p[1] for p in s) / len(s)),
    )
    merged: list[tuple[float, float]] = []
    for seg in ordered:
        if not merged:
            merged.extend(seg)
            continue
        d0 = _dist(merged[-1], seg[0])
        d1 = _dist(merged[-1], seg[-1])
        if d1 < d0:
            seg = list(reversed(seg))
        if d0 > max_gap * 2 and d1 > max_gap * 2:
            merged.extend(seg)
        else:
            merged.extend(seg[1:])
    return merged


def _segment_centroid(seg: list[tuple[float, float]]) -> tuple[float, float]:
    return (sum(p[0] for p in seg) / len(seg), sum(p[1] for p in seg) / len(seg))


def _segment_span(seg: list[tuple[float, float]]) -> float:
    xs = [p[0] for p in seg]
    ys = [p[1] for p in seg]
    return max(max(xs) - min(xs), max(ys) - min(ys))


def _is_entry_polygon(
    seg: list[tuple[float, float]], med_x: float,
) -> bool:
    """Compact left-side entry arrow (polygon), not horizontal pit dashes."""
    if len(seg) < 3 or len(seg) > 6:
        return False
    if _segment_span(seg) >= 150:
        return False
    cx, _cy = _segment_centroid(seg)
    return cx < med_x - 60


def _is_exit_indicator(seg: list[tuple[float, float]], med_x: float) -> bool:
    cx, _cy = _segment_centroid(seg)
    span = _segment_span(seg)
    plen = _poly_len(seg)
    # Flattened bezier pit ticks stay compact; don't treat them as exit chevrons.
    if span < 100 and plen < 150:
        return False
    return cx > med_x + 60 and (plen > 120 or span > 80)


def _is_pit_dash_blob(seg: list[tuple[float, float]], med_y: float, med_x: float) -> bool:
    """Members-site pit ticks: bezier blobs or short line dashes on the pit straight."""
    if _is_entry_polygon(seg, med_x):
        return False
    cx, cy = _segment_centroid(seg)
    span = _segment_span(seg)
    plen = _poly_len(seg)
    if abs(cy - med_y) > 55:
        return False
    if span < 100 and plen < 150:
        return True
    return abs(cy - med_y) < 45 and plen < 120 and span < 90


def _classify_pitroad_segments(
    segments: list[list[tuple[float, float]]],
) -> tuple[list[list[tuple[float, float]]],
           list[list[tuple[float, float]]],
           list[tuple[float, float]] | None]:
    """Split #Pitroad into dashes, entry curves, and optional exit chevron."""
    if not segments:
        return [], [], None
    cxs = sorted(_segment_centroid(s)[0] for s in segments)
    med_x = cxs[len(cxs) // 2]
    pool = [s for s in segments if not _is_entry_polygon(s, med_x)] or segments
    cys = sorted(_segment_centroid(s)[1] for s in pool)
    med_y = cys[len(cys) // 2]

    dashes: list[list[tuple[float, float]]] = []
    entry: list[list[tuple[float, float]]] = []
    exit_candidates: list[list[tuple[float, float]]] = []
    for seg in segments:
        if _is_entry_polygon(seg, med_x):
            entry.append(seg)
            continue
        if _is_exit_indicator(seg, med_x):
            exit_candidates.append(seg)
            continue
        if _is_pit_dash_blob(seg, med_y, med_x):
            dashes.append(seg)
            continue
        cx, cy = _segment_centroid(seg)
        span = _segment_span(seg)
        if cx < med_x and (cy < med_y - 35 or (span > 40 and cy < med_y)):
            entry.append(seg)
        else:
            dashes.append(seg)

    dashes.sort(key=lambda s: _segment_centroid(s)[0])
    entry.sort(key=lambda s: (_segment_centroid(s)[0], _segment_centroid(s)[1]))
    exit_indicator = None
    if exit_candidates:
        exit_indicator = max(exit_candidates,
                               key=lambda s: _segment_centroid(s)[0])
    return dashes, entry, exit_indicator


def _main_pit_straight_dashes(
    dashes: list[list[tuple[float, float]]], *,
    max_x_gap: float = 120.0,
    max_y_step: float = 55.0,
) -> list[list[tuple[float, float]]]:
    """Keep contiguous pit-road straight dashes; drop merge/exit ticks at pit out."""
    if len(dashes) < 4:
        return dashes
    ordered = sorted(dashes, key=lambda s: _segment_centroid(s)[0])
    kept: list[list[tuple[float, float]]] = [ordered[0]]
    for seg in ordered[1:]:
        cx, cy = _segment_centroid(seg)
        px, py = _segment_centroid(kept[-1])
        if cx - px > max_x_gap or abs(cy - py) > max_y_step:
            break
        kept.append(seg)
    return kept if len(kept) >= 4 else dashes


def _pit_path_from_dashes(
    dashes: list[list[tuple[float, float]]], *, max_gap: float = 150.0,
) -> list[tuple[float, float]]:
    """Build pit lane polyline; use dash centroids for bezier blob SVGs."""
    if not dashes:
        return []
    blob_style = (
        len(dashes) >= 5
        and sum(1 for s in dashes if len(s) > 30) >= len(dashes) // 2)
    if blob_style:
        return _centroids_polyline(dashes)
    pit_path = _chain_dashes(dashes, max_gap=max_gap)
    if len(pit_path) < 4:
        pit_path = _chain_sorted_dashes(dashes, max_gap=max_gap)
    return pit_path


def _entry_merge_handoff(
    pit_path: list[tuple[float, float]],
    merge_centroids: list[tuple[float, float]],
) -> tuple[float, float]:
    """Pit-path point nearest the entry-side mergeline handoff."""
    if not pit_path:
        raise ValueError("pit_path required")
    if not merge_centroids:
        return pit_path[0]
    ref = min(merge_centroids, key=lambda c: _dist(c, pit_path[0]))
    return min(pit_path, key=lambda p: _dist(p, ref))


def _centroids_polyline(segments: list[list[tuple[float, float]]]) -> list[tuple[float, float]]:
    """Connect dash/segment centroids in list order (caller sorts first)."""
    if not segments:
        return []
    return [_segment_centroid(seg) for seg in segments]


def _straighten_colinear_runs(
    pts: list[tuple[float, float]],
    *,
    min_run: int = 4,
    y_eps_frac: float = 0.004,
    x_dominance: float = 2.5,
) -> list[tuple[float, float]]:
    """Flatten Y noise on nearly-horizontal runs; preserve curved sections."""
    if len(pts) < min_run:
        return list(pts)
    span = _bbox_span(pts) or 1.0
    y_eps = span * y_eps_frac
    out = list(pts)
    n = len(out)
    i = 0
    while i < n:
        j = i + 1
        while j < n:
            run = out[i:j + 1]
            if len(run) < min_run:
                j += 1
                continue
            xs = [p[0] for p in run]
            ys = [p[1] for p in run]
            x_sp = max(xs) - min(xs)
            y_sp = max(ys) - min(ys)
            if x_sp > x_dominance * max(y_sp, y_eps) or y_sp <= y_eps:
                j += 1
            else:
                break
        if j - i >= min_run:
            run = out[i:j]
            x0, y0 = run[0]
            x1, y1 = run[-1]
            if abs(x1 - x0) < 1e-9:
                ys = sorted(p[1] for p in run)
                mid = len(ys) // 2
                med_y = ys[mid] if len(ys) % 2 else (ys[mid - 1] + ys[mid]) / 2
                for k in range(i, j):
                    out[k] = (out[k][0], med_y)
            else:
                for k in range(i, j):
                    t = (out[k][0] - x0) / (x1 - x0)
                    out[k] = (out[k][0], y0 + t * (y1 - y0))
            i = j
        else:
            i += 1
    return out


def _straighten_in_x_band(
    pts: list[tuple[float, float]],
    x_lo: float,
    x_hi: float,
    **kwargs,
) -> list[tuple[float, float]]:
    """Run colinear straightening only on points inside an X band."""
    if not pts or x_hi <= x_lo:
        return list(pts)
    idx = [i for i, p in enumerate(pts) if x_lo <= p[0] <= x_hi]
    if len(idx) < 4:
        return list(pts)
    out = list(pts)
    band = [pts[i] for i in idx]
    fixed = _straighten_colinear_runs(band, **kwargs)
    for i, p in zip(idx, fixed):
        out[i] = p
    return out


def _pit_straight_x_band(
    pit_svg: str,
    norm: tuple[float, float, float],
) -> tuple[float, float] | None:
    """Normalized X span of the main pit straight dash run."""
    ox, oy, scale = norm
    dashes, _, _ = _classify_pitroad_segments(
        _segments_from_group(pit_svg, "Pitroad"))
    straight = _main_pit_straight_dashes(dashes)
    if len(straight) < 4:
        return None
    xs = [(_segment_centroid(seg)[0] - ox) / scale for seg in straight]
    margin = (max(xs) - min(xs)) * 0.02
    return min(xs) - margin, max(xs) + margin


def _straighten_pit_path(pts: list[tuple[float, float]]) -> list[tuple[float, float]]:
    """Collapse dash-centroid Y noise into a clean front-straight pit lane."""
    if len(pts) < 4:
        return list(pts)
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    x_span = max(xs) - min(xs)
    y_span = max(ys) - min(ys)
    if x_span <= y_span * 4:
        return list(pts)
    ys.sort()
    mid = len(ys) // 2
    med_y = ys[mid] if len(ys) % 2 else (ys[mid - 1] + ys[mid]) / 2
    return [(p[0], med_y) for p in pts]


def _chain_sorted_dashes(dashes: list[list[tuple[float, float]]], *,
                         max_gap: float = 90.0) -> list[tuple[float, float]]:
    if not dashes:
        return []
    poly = _centroids_polyline(dashes)
    if len(poly) >= 4:
        return poly
    return _chain_dashes(dashes, max_gap=max_gap)


def _chain_entry_blend(
    entry: list[list[tuple[float, float]]], *, med_x: float | None = None,
) -> list[tuple[float, float]]:
    if not entry:
        return []
    if med_x is None:
        cxs = sorted(_segment_centroid(s)[0] for s in entry)
        med_x = cxs[len(cxs) // 2]
    poly_entry = [s for s in entry if _is_entry_polygon(s, med_x)]
    if poly_entry:
        entry = poly_entry
    poly = _chain_dashes(entry, max_gap=150.0)
    if len(poly) >= 2:
        return poly
    if len(entry) == 1:
        return list(entry[0])
    return _centroids_polyline(entry)


def _pit_in_invalid(
    pit_in: list[tuple[float, float]], pit_path: list[tuple[float, float]],
) -> bool:
    """Reject entry blends that slash across the track (misclassified dashes)."""
    if len(pit_in) < 2 or len(pit_path) < 2:
        return False
    xs = [p[0] for p in pit_in]
    x_span = max(xs) - min(xs)
    pxs = [p[0] for p in pit_path]
    path_x_span = max(pxs) - min(pxs)
    if path_x_span < 1e-6:
        return _dist(pit_in[0], pit_path[0]) > 50.0
    if x_span > path_x_span * 0.4:
        return True
    return _dist(pit_in[0], pit_path[0]) > path_x_span * 0.25


def _collapse_degenerate_pit_in(
    pit_in: list[tuple[float, float]],
    pit_path: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    """Drop pit-path-prefix fallbacks that start at handoff and run along the pit straight."""
    if len(pit_in) < 2 or len(pit_path) < 2:
        return pit_in
    handoff = pit_path[0]
    path_x_span = max(p[0] for p in pit_path) - min(p[0] for p in pit_path)
    if path_x_span < 1e-9:
        return pit_in
    if _dist(pit_in[0], handoff) > path_x_span * 0.12:
        return pit_in
    in_x_span = max(p[0] for p in pit_in) - min(p[0] for p in pit_in)
    if in_x_span < path_x_span * 0.12:
        return pit_in
    return [handoff, handoff]


def _snap_end(poly: list[tuple[float, float]],
              anchor: tuple[float, float]) -> list[tuple[float, float]]:
    if not poly:
        return [anchor]
    out = list(poly)
    out[-1] = anchor
    return out


def _join_at_start(poly: list[tuple[float, float]],
                   anchor: tuple[float, float]) -> list[tuple[float, float]]:
    if not poly:
        return [anchor]
    if _dist(poly[0], anchor) < 1e-6:
        return [anchor] + poly[1:]
    return [anchor] + poly


def _snap_start(poly: list[tuple[float, float]],
                anchor: tuple[float, float]) -> list[tuple[float, float]]:
    if not poly:
        return [anchor]
    out = list(poly)
    out[0] = anchor
    return out




def _point_seg_dist(pt: tuple[float, float], a: tuple[float, float],
                    b: tuple[float, float]) -> float:
    ax, ay = a
    bx, by = b
    px, py = pt
    dx, dy = bx - ax, by - ay
    if dx == 0 and dy == 0:
        return _dist(pt, a)
    t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
    qx, qy = ax + t * dx, ay + t * dy
    return math.hypot(px - qx, py - qy)


def _distance_to_polyline(pt: tuple[float, float],
                          poly: list[tuple[float, float]]) -> float:
    if len(poly) < 2:
        return float("inf")
    return min(_point_seg_dist(pt, a, b) for a, b in zip(poly, poly[1:]))


def _lane_margin(pit_path: list[tuple[float, float]],
                 loop: list[tuple[float, float]]) -> float:
    xs = [p[0] for p in loop]
    ys = [p[1] for p in loop]
    span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
    return max(30.0, span * 0.03)


def _is_on_lane(pt: tuple[float, float], pit_path: list[tuple[float, float]],
                margin: float) -> bool:
    return _distance_to_polyline(pt, pit_path) <= margin


def _polyline_length(pts: list[tuple[float, float]]) -> float:
    if len(pts) < 2:
        return 0.0
    return sum(_dist(a, b) for a, b in zip(pts, pts[1:]))


def _nearest_point_on_loop(
    pt: tuple[float, float], loop: list[tuple[float, float]],
) -> tuple[tuple[float, float], float]:
    """Closest point on the closed loop polyline and its distance."""
    if not loop:
        return pt, 0.0
    best_q = loop[0]
    best_d = float("inf")
    n = len(loop)
    for i in range(n):
        a = loop[i]
        b = loop[(i + 1) % n]
        ax, ay = a
        bx, by = b
        dx, dy = bx - ax, by - ay
        if abs(dx) < 1e-12 and abs(dy) < 1e-12:
            qx, qy = ax, ay
        else:
            t = max(0.0, min(1.0, ((pt[0] - ax) * dx + (pt[1] - ay) * dy) / (dx * dx + dy * dy)))
            qx, qy = ax + t * dx, ay + t * dy
        d = math.hypot(pt[0] - qx, pt[1] - qy)
        if d < best_d:
            best_d = d
            best_q = (qx, qy)
    return best_q, best_d


def _nearest_loop_point(pt: tuple[float, float],
                        loop: list[tuple[float, float]]) -> tuple[float, float]:
    q, _d = _nearest_point_on_loop(pt, loop)
    return q


def _snap_to_loop(pt: tuple[float, float],
                  loop: list[tuple[float, float]]) -> tuple[float, float]:
    return _nearest_loop_point(pt, loop)


def _normalize_loop_and_pit(
    loop: list[tuple[float, float]],
    pit_in: list[tuple[float, float]],
    pit_path: list[tuple[float, float]],
    pit_out: list[tuple[float, float]],
    pad: float = 0.04,
) -> tuple[list[tuple[float, float]], list[tuple[float, float]],
           list[tuple[float, float]], list[tuple[float, float]],
           tuple[float, float, float]]:
    """Normalize using loop bbox only; apply same affine to pit layers."""
    xs = [p[0] for p in loop]
    ys = [p[1] for p in loop]
    minx, maxx = min(xs), max(xs)
    miny, maxy = min(ys), max(ys)
    span = max(maxx - minx, maxy - miny) or 1.0
    ox = minx - pad * span
    oy = miny - pad * span
    scale = span * (1 + 2 * pad)

    def xform(seg: list[tuple[float, float]]) -> list[tuple[float, float]]:
        return [((p[0] - ox) / scale, (p[1] - oy) / scale) for p in seg]

    return xform(loop), xform(pit_in), xform(pit_path), xform(pit_out), (ox, oy, scale)


def _expand_pit_from_loop(
    segs: list[list[tuple[float, float]]],
    loop: list[tuple[float, float]],
    *,
    min_gap_frac: float = 0.024,
) -> list[list[tuple[float, float]]]:
    """Push pit layers outward uniformly when they sit too close to the loop."""
    span = _bbox_span(loop) or 1.0
    min_gap = span * min_gap_frac
    out: list[list[tuple[float, float]]] = []
    for seg in segs:
        if not seg:
            out.append(seg)
            continue
        dists = [_dist(pt, _nearest_loop_point(pt, loop)) for pt in seg]
        dists.sort()
        mid = len(dists) // 2
        median = dists[mid] if len(dists) % 2 else (dists[mid - 1] + dists[mid]) / 2
        delta = max(0.0, min_gap - median)
        if delta < 1e-6:
            out.append(list(seg))
            continue
        pushed: list[tuple[float, float]] = []
        for pt in seg:
            q = _nearest_loop_point(pt, loop)
            dx, dy = pt[0] - q[0], pt[1] - q[1]
            dist = math.hypot(dx, dy)
            if dist < 1e-6:
                cx = sum(p[0] for p in loop) / len(loop)
                cy = sum(p[1] for p in loop) / len(loop)
                dx, dy = pt[0] - cx, pt[1] - cy
                dist = math.hypot(dx, dy) or 1.0
            pushed.append((pt[0] + dx / dist * delta, pt[1] + dy / dist * delta))
        out.append(pushed)
    return out


def _param_on_polyline(pt: tuple[float, float],
                       poly: list[tuple[float, float]]) -> float:
    """Return normalized arc length (0..1) of the closest point on poly."""
    if len(poly) < 2:
        return 0.0
    total = _polyline_length(poly)
    if total < 1e-9:
        return 0.0
    best_t = 0.0
    best_d = float("inf")
    acc = 0.0
    for a, b in zip(poly, poly[1:]):
        seg_len = _dist(a, b)
        ax, ay = a
        bx, by = b
        px, py = pt
        dx, dy = bx - ax, by - ay
        if seg_len < 1e-9:
            t = 0.0
            qx, qy = ax, ay
        else:
            t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy) / (dx * dx + dy * dy)))
            qx, qy = ax + t * dx, ay + t * dy
        d = math.hypot(px - qx, py - qy)
        if d < best_d:
            best_d = d
            best_t = (acc + t * seg_len) / total
        acc += seg_len
    return best_t


def _exclude_lane_tick(
    tick: tuple[float, float],
    anchor: tuple[float, float],
    pit_path: list[tuple[float, float]],
    lane_m: float,
    span: float,
    *,
    anchor_at_entry: bool,
) -> bool:
    """True when an on-lane tick should be dropped from the merge arc pool."""
    handoff_r = max(40.0, span * 0.03)
    if _dist(tick, anchor) <= handoff_r:
        return False
    if not _is_on_lane(tick, pit_path, lane_m):
        return False
    t_anchor = _param_on_polyline(anchor, pit_path)
    t_tick = _param_on_polyline(tick, pit_path)
    buf = 0.04
    if anchor_at_entry:
        return t_tick > t_anchor + buf
    return t_tick < t_anchor - buf


def _merge_anchor(pit_path: list[tuple[float, float]],
                  centroids: list[tuple[float, float]]) -> tuple[float, float]:
    if not pit_path:
        raise ValueError("pit_path required")
    if not centroids:
        return pit_path[-1]
    mx = sum(c[0] for c in centroids) / len(centroids)
    my = sum(c[1] for c in centroids) / len(centroids)
    mc = (mx, my)
    if _dist(pit_path[0], mc) <= _dist(pit_path[-1], mc):
        return pit_path[0]
    return pit_path[-1]


def _anchor_at_entry(anchor: tuple[float, float],
                     pit_path: list[tuple[float, float]]) -> bool:
    return _dist(anchor, pit_path[0]) <= _dist(anchor, pit_path[-1])


def _pit_lane_curved(pts: list[tuple[float, float]]) -> bool:
    if len(pts) < 4:
        return False
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    x_span = max(xs) - min(xs)
    y_span = max(ys) - min(ys)
    if x_span < 1e-6:
        return True
    return (y_span / x_span) > 0.035


def _trim_straight_run_prefix(
    merge: list[tuple[float, float]],
    anchor: tuple[float, float],
    loop: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    """For pit-exit merges, drop on-straight ticks and jump to the first turn tick."""
    if len(merge) < 2:
        return merge
    span = _bbox_span(loop) or 1.0
    y_cut = anchor[1] - max(25.0, span * 0.02)
    for i, p in enumerate(merge[1:], 1):
        if p[1] < y_cut:
            return [anchor] + merge[i:]
    return merge


def _drop_lane_colinear_prefix(
    merge: list[tuple[float, float]],
    anchor: tuple[float, float],
    pit_path: list[tuple[float, float]],
    loop: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    if len(merge) < 2:
        return merge
    margin = _lane_margin(pit_path, loop)
    out: list[tuple[float, float]] = [anchor]
    for p in merge[1:]:
        if len(out) == 1 and _is_on_lane(p, pit_path, margin):
            if _dist(p, anchor) > 1e-3:
                continue
        out.append(p)
    return out if len(out) >= 2 else merge


def _trim_entry_colinear_prefix(
    merge: list[tuple[float, float]],
    anchor: tuple[float, float],
    pit_path: list[tuple[float, float]],
    loop: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    """Drop backward colinear run along pit_path for entry-side merges."""
    if len(merge) < 2:
        return merge
    span = _bbox_span(loop) or 1.0
    y_eps = span * 0.008
    x_buf = span * 0.005
    for i, p in enumerate(merge[1:], 1):
        backward = p[0] < anchor[0] - x_buf
        colinear = abs(p[1] - anchor[1]) <= y_eps
        if backward and colinear:
            continue
        margin = _lane_margin(pit_path, loop)
        if colinear and _is_on_lane(p, pit_path, margin * 1.2):
            continue
        return [anchor] + merge[i:]
    return [anchor, merge[-1]] if len(merge) >= 2 else merge


def _loop_proximity(pt: tuple[float, float],
                    loop: list[tuple[float, float]]) -> float:
    _q, d = _nearest_point_on_loop(pt, loop)
    return d


def _near_loop(pt: tuple[float, float], loop: list[tuple[float, float]],
               *, frac: float = 0.035) -> bool:
    xs = [p[0] for p in loop]
    ys = [p[1] for p in loop]
    span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
    return _loop_proximity(pt, loop) < span * frac


def _merge_ticks_to_polyline(
    merge_segs: list[list[tuple[float, float]]],
    loop: list[tuple[float, float]],
    anchor: tuple[float, float],
    pit_path: list[tuple[float, float]],
    *,
    transition_y: float | None = None,
    anchor_at_entry: bool = False,
) -> list[tuple[float, float]]:
    """Build merge polyline from Mergeline tick centroids (arc only, no lane run)."""
    if not merge_segs:
        return []
    xs = [p[0] for p in loop]
    img_w = max(xs) - min(xs)
    img_h = max(p[1] for p in loop) - min(p[1] for p in loop)
    span = max(img_w, img_h) or 1.0
    straight_margin = max(25.0, span * 0.025)
    max_anchor_dist = span * 0.70
    max_step = max(150.0, img_w * 0.15)
    lane_m = _lane_margin(pit_path, loop)

    handoff_r = max(40.0, span * 0.03)
    centroids = [_segment_centroid(seg) for seg in merge_segs]
    arc_pool: list[tuple[float, float]] = []
    for c in centroids:
        if _dist(c, anchor) > max_anchor_dist:
            continue
        if _exclude_lane_tick(c, anchor, pit_path, lane_m, span,
                              anchor_at_entry=anchor_at_entry):
            continue
        if (transition_y is not None
                and c[1] >= transition_y - straight_margin
                and _dist(c, anchor) > handoff_r):
            continue
        if (not anchor_at_entry and c[0] < anchor[0] - straight_margin
                and _dist(c, anchor) > handoff_r):
            continue
        if (anchor_at_entry and c[0] > anchor[0] + straight_margin
                and _dist(c, anchor) > handoff_r):
            continue
        arc_pool.append(c)

    if len(arc_pool) < 1:
        handoff_r = max(40.0, span * 0.03)
        arc_pool = [
            c for c in centroids
            if _dist(c, anchor) <= max_anchor_dist
            and not _exclude_lane_tick(c, anchor, pit_path, lane_m * 1.5, span,
                                       anchor_at_entry=anchor_at_entry)
        ]
        arc_pool.sort(key=lambda c: (c[1], _dist(c, anchor)))

    ordered: list[tuple[float, float]] = [anchor]
    remaining = list(arc_pool)
    if remaining:
        start_i = min(range(len(remaining)), key=lambda i: _dist(remaining[i], anchor))
        first = remaining.pop(start_i)
        if _dist(first, anchor) > 1e-3:
            ordered.append(first)
    while remaining:
        last = ordered[-1]
        near = [p for p in remaining if _dist(last, p) <= max_step]
        candidates = near or remaining
        nxt = min(candidates, key=lambda p: (_dist(last, p), last[1] - p[1]))
        ordered.append(nxt)
        remaining.remove(nxt)

    return ordered


def _trim_merge_at_rejoin(
    merge: list[tuple[float, float]],
    loop: list[tuple[float, float]],
    transition: tuple[float, float],
) -> list[tuple[float, float]]:
    """Stop at loop rejoin or when arc length from anchor exceeds ~25% of loop."""
    if len(merge) < 3:
        return merge
    xs = [p[0] for p in loop]
    ys = [p[1] for p in loop]
    span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
    prox = span * 0.04
    straight_floor = transition[1] - max(25.0, span * 0.02)
    max_arc = _polyline_length(loop) * 0.25
    acc = 0.0
    for i, p in enumerate(merge):
        if i > 0:
            acc += _dist(merge[i - 1], p)
        if i < 3:
            continue
        if acc > max_arc:
            return merge[: i + 1]
        if p[1] >= straight_floor:
            continue
        if _loop_proximity(p, loop) <= prox:
            return merge[: i + 1]
    return merge


def _decouple_merge_from_loop(
    merge: list[tuple[float, float]],
    loop: list[tuple[float, float]],
    *,
    min_frac: float = 0.008,
    uniform: bool = True,
) -> list[tuple[float, float]]:
    """Push merge points away from the loop."""
    if len(merge) < 3:
        return merge
    span = _bbox_span(loop) or 1.0
    hug_thresh = span * 0.012
    min_gap = span * min_frac
    if uniform:
        dirs: list[tuple[float, float]] = []
        deficit = 0.0
        for p in merge[1:-1]:
            q, poly_dist = _nearest_point_on_loop(p, loop)
            vert_dist = min(_dist(p, v) for v in loop)
            dist = min(poly_dist, vert_dist)
            if dist >= hug_thresh:
                continue
            dx, dy = p[0] - q[0], p[1] - q[1]
            ln = math.hypot(dx, dy)
            if ln < 1e-9:
                cx = sum(x for x, _y in loop) / len(loop)
                cy = sum(y for _x, y in loop) / len(loop)
                dx, dy = p[0] - cx, p[1] - cy
                ln = math.hypot(dx, dy) or 1.0
            dirs.append((dx / ln, dy / ln))
            deficit = max(deficit, max(0.0, min_gap - dist))
        if deficit < 1e-9 or not dirs:
            return merge
        ux = sum(d[0] for d in dirs) / len(dirs)
        uy = sum(d[1] for d in dirs) / len(dirs)
        ln = math.hypot(ux, uy)
        if ln > 1e-9:
            ux, uy = ux / ln, uy / ln
        else:
            ux, uy = dirs[0]
        out: list[tuple[float, float]] = [merge[0]]
        for p in merge[1:-1]:
            q, poly_dist = _nearest_point_on_loop(p, loop)
            vert_dist = min(_dist(p, v) for v in loop)
            if min(poly_dist, vert_dist) < hug_thresh:
                out.append((p[0] + ux * deficit, p[1] + uy * deficit))
            else:
                out.append(p)
        out.append(merge[-1])
        return out
    if not any(_nearest_point_on_loop(p, loop)[1] < hug_thresh for p in merge[1:-1]):
        return merge
    out: list[tuple[float, float]] = [merge[0]]
    for p in merge[1:-1]:
        q, poly_dist = _nearest_point_on_loop(p, loop)
        vert_dist = min(_dist(p, v) for v in loop)
        dist = min(poly_dist, vert_dist)
        if dist >= min_gap:
            out.append(p)
            continue
        dx, dy = p[0] - q[0], p[1] - q[1]
        ln = math.hypot(dx, dy)
        if ln < 1e-9:
            cx = sum(x for x, _y in loop) / len(loop)
            cy = sum(y for _x, y in loop) / len(loop)
            dx, dy = p[0] - cx, p[1] - cy
            ln = math.hypot(dx, dy) or 1.0
        push = min_gap - dist if dist > 1e-9 else min_gap
        out.append((p[0] + dx / ln * push, p[1] + dy / ln * push))
    out.append(merge[-1])
    return out


def _pit_geometry(
    pit_svg: str, loop: list[tuple[float, float]],
    *,
    inactive_svg: str | None = None,
    active_svg: str | None = None,
) -> tuple[list[tuple[float, float]], list[tuple[float, float]],
           list[tuple[float, float]], tuple[float, float], int, bool, str]:
    """Build pit_in, pit_path, pit_out from members #Pitroad / #Mergeline groups."""
    pit_segs = _segments_from_group(pit_svg, "Pitroad")
    if not pit_segs:
        raise ValueError("No #Pitroad paths found in pit SVG")

    dashes, entry, _exit_indicator = _classify_pitroad_segments(pit_segs)
    cxs = sorted(_segment_centroid(s)[0] for s in pit_segs)
    med_x = cxs[len(cxs) // 2]

    pit_path_source = "dashes"
    inactive_path = None
    if inactive_svg and active_svg:
        inactive_path = _inactive_pit_subpath(
            inactive_svg, active_svg, pit_svg, loop)
    if inactive_path and len(inactive_path) >= 4:
        pit_path = inactive_path
        pit_path_source = "inactive"
    else:
        pit_path = _pit_path_from_dashes(dashes, max_gap=150.0)
        if len(pit_path) < 4:
            pit_path = _chain_dashes(
                [s for s in pit_segs if s not in entry], max_gap=150.0)
        if len(pit_path) < 4:
            raise ValueError("Pit road dashes did not chain — check pit SVG layer")

        if pit_path[-1][0] < pit_path[0][0]:
            pit_path = list(reversed(pit_path))
        if not _pit_lane_curved(pit_path):
            pit_path = _straighten_pit_path(pit_path)

    if pit_path_source == "inactive":
        pit_in = [pit_path[0], pit_path[0]]
    else:
        entry_blend = _chain_entry_blend(entry, med_x=med_x)
        if entry_blend:
            pit_in = list(entry_blend)
            if _dist(pit_in[-1], pit_path[0]) > 1e-3:
                pit_in.append(pit_path[0])
        else:
            pit_in = [pit_path[0], pit_path[0]]
        pit_in = _snap_end(pit_in, pit_path[0])
        if _pit_in_invalid(pit_in, pit_path):
            poly_entry = [s for s in entry if _is_entry_polygon(s, med_x)]
            entry_blend = _chain_entry_blend(poly_entry, med_x=med_x)
            if entry_blend:
                pit_in = list(entry_blend)
                if _dist(pit_in[-1], pit_path[0]) > 1e-3:
                    pit_in.append(pit_path[0])
            else:
                pit_in = [pit_path[0], pit_path[0]]
            pit_in = _snap_end(pit_in, pit_path[0])
        pit_in = _collapse_degenerate_pit_in(pit_in, pit_path)

    merge_segs = _segments_from_group(pit_svg, "Mergeline")
    merge_tick_count = len(merge_segs)
    if not merge_segs:
        raise ValueError("No #Mergeline paths found in pit SVG")
    merge_centroids = [_segment_centroid(seg) for seg in merge_segs]
    merge_anchor = _merge_anchor(pit_path, merge_centroids)
    at_entry = _anchor_at_entry(merge_anchor, pit_path)
    if at_entry:
        merge_anchor = _entry_merge_handoff(pit_path, merge_centroids)
        if pit_path_source == "inactive":
            pit_in = [merge_anchor, merge_anchor]

    merge_blue = _merge_ticks_to_polyline(
        merge_segs, loop, merge_anchor, pit_path,
        transition_y=None if at_entry else merge_anchor[1],
        anchor_at_entry=at_entry)
    if len(merge_blue) < 2:
        raise ValueError("Safe-merge dashes did not chain — check pit SVG layer")
    merge_blue = _orient_poly_from_anchor(merge_blue, merge_anchor)
    merge_blue = _snap_start(merge_blue, merge_anchor)
    merge_blue = _drop_lane_colinear_prefix(
        merge_blue, merge_anchor, pit_path, loop)
    if at_entry:
        merge_blue = _trim_entry_colinear_prefix(
            merge_blue, merge_anchor, pit_path, loop)
    if not at_entry:
        merge_blue = _trim_straight_run_prefix(merge_blue, merge_anchor, loop)
    merge_blue = _trim_merge_at_rejoin(merge_blue, loop, merge_anchor)
    if not _near_loop(merge_blue[-1], loop):
        merge_blue = list(merge_blue)
        merge_blue[-1] = _snap_to_loop(merge_blue[-1], loop)

    return pit_in, pit_path, merge_blue, merge_anchor, merge_tick_count, at_entry, pit_path_source


def _oval_pit_geometry(
    pit_svg: str, loop: list[tuple[float, float]],
) -> tuple[list[tuple[float, float]], list[tuple[float, float]], list[tuple[float, float]]]:
    pit_in, pit_path, merge_blue, _anchor, _ticks, _at, _src = _pit_geometry(pit_svg, loop)
    return pit_in, pit_path, merge_blue


def _svg_id_from_layer(svg_text: str) -> str:
    m = re.search(r'<svg[^>]*\bid=["\']([^"\']+)["\']', svg_text, re.IGNORECASE)
    return m.group(1) if m else ""


def _layout_prefix(svg_id: str) -> str:
    if "_-_" in svg_id:
        return svg_id.split("_-_", 1)[0]
    return ""


_MIN_INACTIVE_PIT_SUBPATH_LEN = 400.0
_INACTIVE_BRIDGE_MAX_PIT_DIST = 80.0
_INACTIVE_SUBTRACT_TOL = 18.0
_INACTIVE_CLIP_X_LO_MARGIN = 80.0
_INACTIVE_CLIP_X_HI_MARGIN = 120.0
_INACTIVE_ENTRY_CLIP_RADIUS = 100.0


def _inactive_continuous_subpaths(inactive_svg: str) -> list | None:
    """Parse inactive layer path into svgpathtools continuous subpaths."""
    try:
        from svgpathtools import parse_path  # type: ignore
    except ImportError:
        return None
    ds = _paths_d_in_svg(inactive_svg)
    if not ds:
        return None
    d = max(ds, key=len)
    return parse_path(d).continuous_subpaths()


def _sample_svgpath_segment(segment, num_samples: int = 280) -> list[tuple[float, float]]:
    if num_samples < 2:
        pt = segment.point(0)
        return [(float(pt.real), float(pt.imag))]
    out: list[tuple[float, float]] = []
    for i in range(num_samples):
        pt = segment.point(i / (num_samples - 1))
        out.append((float(pt.real), float(pt.imag)))
    return out


def _active_loop_poly(
    active_svg: str,
    pit_centroids: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    subpaths = _collect_loop_subpaths(active_svg)
    return _pick_loop_subpath(subpaths, pit_centroids)


def _subpath_is_loop_match(
    sampled: list[tuple[float, float]],
    loop_poly: list[tuple[float, float]],
    pit_centroids: list[tuple[float, float]],
) -> bool:
    """True when an inactive subpath traces the active racing loop."""
    if len(sampled) < 4 or len(loop_poly) < 4:
        return False
    sub_len = _polyline_length(sampled)
    loop_len = _polyline_length(loop_poly)
    if loop_len > 1e-6 and sub_len / loop_len > 0.85:
        step = max(1, len(sampled) // 60)
        dists = [
            _distance_to_polyline(sampled[i], loop_poly)
            for i in range(0, len(sampled), step)
        ]
        if sorted(dists)[len(dists) // 2] < 25.0:
            return True

    sub_stats = _parallel_straight_stats(sampled, pit_centroids)
    loop_stats = _parallel_straight_stats(loop_poly, pit_centroids)
    if sub_stats and loop_stats:
        _, _sub_off, abs_sub, _sub_std = sub_stats
        _, _loop_off, abs_loop, _loop_std = loop_stats
        if abs_loop > 8.0 and abs(abs_sub - abs_loop) <= max(12.0, abs_loop * 0.25):
            return True
    return False


def _is_inactive_bridge_subpath(
    sampled: list[tuple[float, float]],
    pit_centroids: list[tuple[float, float]],
) -> bool:
    """True for shortcut chords (e.g. Okayama inactive subpath 2), not pit lane."""
    if _mean_pit_distance(sampled, pit_centroids) > _INACTIVE_BRIDGE_MAX_PIT_DIST:
        return True
    band = _parallel_straight_band(pit_centroids)
    if not band:
        return False
    x_lo, x_hi, y_lo, y_hi = band
    in_band = sum(
        1 for p in sampled if x_lo <= p[0] <= x_hi and y_lo <= p[1] <= y_hi)
    return in_band < 3


def _subtract_active_loop_points(
    pts: list[tuple[float, float]],
    loop_poly: list[tuple[float, float]],
    *,
    tol: float = _INACTIVE_SUBTRACT_TOL,
) -> list[tuple[float, float]]:
    """Drop points that lie on the active-config racing loop."""
    if len(pts) < 4 or len(loop_poly) < 4:
        return list(pts)
    kept = [p for p in pts if _distance_to_polyline(p, loop_poly) > tol]
    return kept if len(kept) >= 4 else list(pts)


def _clip_to_pit_layer_extent(
    pts: list[tuple[float, float]],
    pit_svg: str,
    pit_centroids: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    """Keep pit straight, entry hook, and entry-arrow vicinity."""
    if len(pts) < 4 or not pit_centroids:
        return pts
    xs = [c[0] for c in pit_centroids]
    x_lo = min(xs) - _INACTIVE_CLIP_X_LO_MARGIN
    x_hi = max(xs) + _INACTIVE_CLIP_X_HI_MARGIN

    entry_pts: list[tuple[float, float]] = []
    pit_segs = _segments_from_group(pit_svg, "Pitroad")
    if pit_segs:
        _dashes, entry, _exit = _classify_pitroad_segments(pit_segs)
        for seg in entry:
            entry_pts.extend(seg)

    def _keep(p: tuple[float, float]) -> bool:
        if x_lo <= p[0] <= x_hi:
            return True
        if entry_pts:
            if min(_dist(p, ep) for ep in entry_pts) <= _INACTIVE_ENTRY_CLIP_RADIUS:
                return True
        return False

    clipped = [p for p in pts if _keep(p)]
    return clipped if len(clipped) >= 4 else pts


def _open_pit_arc_from_closed(
    sampled: list[tuple[float, float]],
    pit_centroids: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    """Cut a closed inactive subpath to the open pit-lane arc between dash ends."""
    if len(sampled) < 4 or _dist(sampled[0], sampled[-1]) > 1.0:
        return sampled
    n = len(sampled)
    cxs = sorted(pit_centroids, key=lambda c: c[0])
    c0, c1 = cxs[0], cxs[-1]
    i0 = min(range(n), key=lambda i: _dist(sampled[i], c0))
    i1 = min(range(n), key=lambda i: _dist(sampled[i], c1))
    if i0 == i1:
        return sampled
    if i0 <= i1:
        arc_a = sampled[i0:i1 + 1]
        arc_b = sampled[i1:] + sampled[:i0 + 1]
    else:
        arc_a = sampled[i0:] + sampled[:i1 + 1]
        arc_b = sampled[i1:i0 + 1]

    def _straight_y_err(arc: list[tuple[float, float]]) -> float:
        band = _parallel_straight_band(pit_centroids)
        if not band:
            return 0.0
        x_lo, x_hi, y_lo, y_hi = band
        ys = sorted(c[1] for c in pit_centroids)
        pit_med_y = ys[len(ys) // 2]
        back = [p for p in arc if x_lo <= p[0] <= x_hi]
        if len(back) < 3:
            return 1e9
        med_y = sorted(p[1] for p in back)[len(back) // 2]
        return abs(med_y - pit_med_y)

    return arc_a if _straight_y_err(arc_a) <= _straight_y_err(arc_b) else arc_b


def _trim_active_overlap(
    pit_pts: list[tuple[float, float]],
    loop_poly: list[tuple[float, float]],
    pit_centroids: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    """Drop straight-band points that sit on the active loop lane, not the pit lane."""
    band = _parallel_straight_band(pit_centroids)
    if not band or len(pit_pts) < 4:
        return pit_pts
    x_lo, x_hi, y_lo, y_hi = band
    loop_straight = [p for p in loop_poly if x_lo <= p[0] <= x_hi]
    if len(loop_straight) < 4:
        return pit_pts
    loop_med_y = sorted(p[1] for p in loop_straight)[len(loop_straight) // 2]
    y_tol = _INACTIVE_STRAIGHT_Y_TOL
    kept = [
        p for p in pit_pts
        if not (
            x_lo <= p[0] <= x_hi
            and y_lo <= p[1] <= y_hi
            and abs(p[1] - loop_med_y) <= y_tol
        )
    ]
    if len(kept) < 4:
        return pit_pts
    return kept


def _inactive_pit_subpath(
    inactive_svg: str | None,
    active_svg: str,
    pit_svg: str,
    loop: list[tuple[float, float]] | None = None,  # noqa: ARG001 — use active subpath
) -> list[tuple[float, float]] | None:
    """Extract pit lane: inactive minus active-config minus bridge chords."""
    if not inactive_svg:
        return None
    pit_centroids = _pit_dash_centroids(pit_svg)
    if not pit_centroids:
        return None
    subpaths = _inactive_continuous_subpaths(inactive_svg)
    if not subpaths:
        return None

    active_prefix = _layout_prefix(_svg_id_from_layer(active_svg))
    pit_prefix = _layout_prefix(_svg_id_from_layer(pit_svg))
    inactive_prefix = _layout_prefix(_svg_id_from_layer(inactive_svg))
    if (
        active_prefix
        and pit_prefix
        and active_prefix != pit_prefix
        and inactive_prefix
        and inactive_prefix != pit_prefix
    ):
        return None

    loop_ref = _active_loop_poly(active_svg, pit_centroids)
    if len(loop_ref) < 4:
        return None
    loop_span = _bbox_span(loop_ref) or 1.0

    candidates: list[tuple[list[tuple[float, float]], float]] = []
    for segment in subpaths:
        if segment.length() < _MIN_INACTIVE_PIT_SUBPATH_LEN:
            continue
        sampled = _sample_svgpath_segment(segment, 500)
        if _subpath_is_loop_match(sampled, loop_ref, pit_centroids):
            continue
        if _is_inactive_bridge_subpath(sampled, pit_centroids):
            continue
        subtracted = _subtract_active_loop_points(sampled, loop_ref)
        clipped = _clip_to_pit_layer_extent(subtracted, pit_svg, pit_centroids)
        if len(clipped) < 4:
            continue
        x_span = max(p[0] for p in clipped) - min(p[0] for p in clipped)
        if x_span < loop_span * 0.12:
            continue
        pit_dist = _mean_pit_distance(clipped, pit_centroids)
        candidates.append((clipped, pit_dist))

    if not candidates:
        return None

    best = min(candidates, key=lambda item: item[1])[0]
    best = _resample_open(best, 280)
    if len(best) < 4:
        return None

    dash_xs = [c[0] for c in pit_centroids]
    if max(dash_xs) - min(dash_xs) > 50 and best[-1][0] < best[0][0]:
        best = list(reversed(best))
    return best


def _pit_dash_centroids(pit_svg: str) -> list[tuple[float, float]]:
    """Classified #Pitroad road-dash centroids in SVG space (excludes entry)."""
    pit_segs = _segments_from_group(pit_svg, "Pitroad")
    if not pit_segs:
        return []
    dashes, _entry, _exit = _classify_pitroad_segments(pit_segs)
    dashes = _main_pit_straight_dashes(dashes)
    return [_segment_centroid(seg) for seg in dashes]


def _collect_loop_subpaths(svg_text: str) -> list[list[tuple[float, float]]]:
    out: list[list[tuple[float, float]]] = []
    for d in _paths_d_in_svg(svg_text):
        for sub in svgpath.split_subpaths(d):
            if len(sub) >= 4:
                out.append(sub)
    return out


def _mean_pit_distance(
    sub: list[tuple[float, float]],
    pit_centroids: list[tuple[float, float]],
    *,
    sample_n: int = 200,
) -> float:
    if not pit_centroids:
        return float("inf")
    step = max(1, len(sub) // sample_n)
    loop_pts = [sub[i] for i in range(0, len(sub), step)]
    total = 0.0
    for cx, cy in pit_centroids:
        best = min(_dist((cx, cy), lp) for lp in loop_pts)
        total += best
    return total / len(pit_centroids)


def _parallel_straight_band(
    pit_centroids: list[tuple[float, float]],
) -> tuple[float, float, float, float] | None:
    """X span and Y band around pit dashes for the parallel straight region."""
    if not pit_centroids:
        return None
    xs = [c[0] for c in pit_centroids]
    ys = [c[1] for c in pit_centroids]
    x_lo, x_hi = min(xs), max(xs)
    ys.sort()
    mid = len(ys) // 2
    pit_med_y = ys[mid] if len(ys) % 2 else (ys[mid - 1] + ys[mid]) / 2
    return x_lo, x_hi, pit_med_y - 80.0, pit_med_y + 40.0


def _parallel_straight_stats(
    sub: list[tuple[float, float]],
    pit_centroids: list[tuple[float, float]],
) -> tuple[int, float, float, float] | None:
    """Sample count, median offset, |median offset|, and offset std on parallel straight."""
    band = _parallel_straight_band(pit_centroids)
    if band is None:
        return None
    x_lo, x_hi, y_lo, y_hi = band
    ys = [c[1] for c in pit_centroids]
    ys.sort()
    mid = len(ys) // 2
    pit_med_y = ys[mid] if len(ys) % 2 else (ys[mid - 1] + ys[mid]) / 2
    pts = [p for p in sub if x_lo <= p[0] <= x_hi and y_lo <= p[1] <= y_hi]
    if len(pts) < 3:
        return None
    offsets = [p[1] - pit_med_y for p in pts]
    offsets.sort()
    o_mid = len(offsets) // 2
    median_offset = (
        offsets[o_mid] if len(offsets) % 2
        else (offsets[o_mid - 1] + offsets[o_mid]) / 2
    )
    if len(offsets) < 2:
        offset_std = 0.0
    else:
        mean = sum(offsets) / len(offsets)
        offset_std = math.sqrt(sum((o - mean) ** 2 for o in offsets) / len(offsets))
    return len(pts), median_offset, abs(median_offset), offset_std


def _is_dual_parallel_lane_pair(
    stats_list: list[tuple[int, float, float, float] | None],
    *,
    min_samples: int = 5,
    max_offset_std: float = 2.5,
) -> bool:
    stable = [
        st for st in stats_list
        if st is not None and st[0] >= min_samples and st[3] < max_offset_std
    ]
    return len(stable) >= 2


def _back_straight_median_y(
    sub: list[tuple[float, float]],
    pit_centroids: list[tuple[float, float]],
) -> float:
    """Median Y on the pit-straight X span (fallback tie-breaker only)."""
    if not pit_centroids:
        return float("inf")
    xs = [c[0] for c in pit_centroids]
    x_lo, x_hi = min(xs), max(xs)
    ys = [p[1] for p in sub if x_lo <= p[0] <= x_hi]
    if not ys:
        return float("inf")
    ys.sort()
    mid = len(ys) // 2
    return ys[mid] if len(ys) % 2 else (ys[mid - 1] + ys[mid]) / 2


def _pick_loop_subpath(
    subpaths: list[list[tuple[float, float]]],
    pit_centroids: list[tuple[float, float]] | None = None,
) -> list[tuple[float, float]]:
    """Pick the racing-line subpath using pit-dash overlay on the parallel straight."""
    if not subpaths:
        return []
    if not pit_centroids or len(subpaths) == 1:
        return max(subpaths, key=len)

    stats = [_parallel_straight_stats(sub, pit_centroids) for sub in subpaths]
    if _is_dual_parallel_lane_pair(stats):
        candidates = [
            (sub, st) for sub, st in zip(subpaths, stats)
            if st is not None and st[0] >= 5 and st[3] < 2.5
        ]
        return max(candidates, key=lambda item: item[1][2])[0]

    scored = [(sub, _mean_pit_distance(sub, pit_centroids)) for sub in subpaths]
    aligned = [(sub, d) for sub, d in scored if d < 100.0]
    if not aligned:
        return max(subpaths, key=len)
    min_dist = min(d for _, d in aligned)
    margin = max(5.0, min_dist * 0.5)
    near = [sub for sub, d in aligned if d <= min_dist + margin]
    return min(near, key=lambda sub: _back_straight_median_y(sub, pit_centroids))


def _main_loop_path(
    svg_text: str,
    *,
    pit_svg: str | None = None,
) -> list[tuple[float, float]]:
    pit_centroids = _pit_dash_centroids(pit_svg) if pit_svg else []
    subpaths = _collect_loop_subpaths(svg_text)
    best = _pick_loop_subpath(subpaths, pit_centroids)

    if len(best) < 4:
        raise ValueError("No track outline path found in active-config SVG")
    return _resample_closed(best, 720)


def _path_bbox_area(pts: list[tuple[float, float]]) -> float:
    if not pts:
        return 0.0
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return (max(xs) - min(xs)) * (max(ys) - min(ys))


def _sf_paths_sorted(sf_svg: str | None) -> list[list[tuple[float, float]]]:
    """Paths in the start-finish layer, smallest bbox first (stripe before arrow)."""
    paths: list[list[tuple[float, float]]] = []
    for d in _paths_d_in_svg(sf_svg or ""):
        pts = svgpath.flatten_path(d)
        if pts:
            paths.append(pts)
    paths.sort(key=_path_bbox_area)
    return paths


def _sf_stripe_centroid(sf_svg: str | None) -> tuple[float, float] | None:
    """Centroid of the S/F stripe (smallest path), not the direction arrow."""
    paths = _sf_paths_sorted(sf_svg)
    if not paths:
        return None
    pts = paths[0]
    return (sum(p[0] for p in pts) / len(pts), sum(p[1] for p in pts) / len(pts))


def _sf_arrow_tip(sf_svg: str | None) -> tuple[float, float] | None:
    """Moveto / tip of the direction arrow (largest path) in SVG coords."""
    paths = _sf_paths_sorted(sf_svg)
    if len(paths) < 2:
        return None
    return paths[-1][0]


def _sf_anchor_point(sf_svg: str | None) -> tuple[float, float] | None:
    """Point on the racing line for S/F alignment (start-finish layer stripe).

    Members maps stack layers in a shared viewBox; the smallest path in the
    start-finish layer is the vertical stripe crossing the track. Single-path
    layers (some road courses) use that path's centroid.
    """
    return _sf_stripe_centroid(sf_svg)


def _sf_stripe_crossing(
    loop: list[tuple[float, float]],
    sf_svg: str | None,
) -> tuple[int, tuple[float, float]] | None:
    """Loop index and exact point where the vertical stripe crosses the loop."""
    paths = _sf_paths_sorted(sf_svg)
    if not paths or len(loop) < 2:
        return None
    stripe = paths[0]
    stripe_x = sum(p[0] for p in stripe) / len(stripe)
    ys = [p[1] for p in stripe]
    y_mid = (min(ys) + max(ys)) / 2.0
    y_band = max(max(ys) - min(ys), 40.0) * 2.5

    best: tuple[float, int, tuple[float, float]] | None = None
    n = len(loop)
    for i in range(n):
        a, b = loop[i], loop[(i + 1) % n]
        xmin, xmax = min(a[0], b[0]), max(a[0], b[0])
        if stripe_x < xmin - 1e-6 or stripe_x > xmax + 1e-6:
            continue
        dx = b[0] - a[0]
        if abs(dx) < 1e-9:
            continue
        t = max(0.0, min(1.0, (stripe_x - a[0]) / dx))
        cy = a[1] + t * (b[1] - a[1])
        if abs(cy - y_mid) > y_band:
            continue
        score = abs(cy - y_mid)
        if best is None or score < best[0]:
            best = (score, i, (stripe_x, cy))

    if best is not None:
        return best[1], best[2]

    pt = _sf_stripe_centroid(sf_svg)
    if pt is None:
        return None
    pct = _pct_on_loop(loop, pt)
    return int(pct * n) % n, pt


def _detect_sf_svg(
    loop: list[tuple[float, float]],
    sf_svg: str | None,
) -> int:
    crossing = _sf_stripe_crossing(loop, sf_svg)
    if crossing is not None:
        return crossing[0]
    pt = _sf_anchor_point(sf_svg)
    if pt is not None and loop:
        pct = _pct_on_loop(loop, pt)
        return int(pct * len(loop)) % len(loop)
    return max(range(len(loop)), key=lambda i: loop[i][1])


def _parse_turn_numbers(svg_text: str, loop: list[tuple[float, float]],
                        norm: tuple[float, float, float],
                        *, flip_y: bool = False) -> list[dict]:
    """Turn labels from members SVG; positions normalized with the track loop."""
    ox, oy, scale = norm
    corners: list[dict] = []
    pat = re.compile(
        r'<text[^>]*transform="translate\(([^)]+)\)"[^>]*>([^<]+)</text>',
        re.IGNORECASE,
    )
    for m in pat.finditer(svg_text):
        parts = re.split(r"[\s,]+", m.group(1).strip())
        if len(parts) < 2:
            continue
        try:
            tx, ty = float(parts[0]), float(parts[1])
        except ValueError:
            continue
        label = m.group(2).strip()
        if not label:
            continue
        nx = (tx - ox) / scale
        ny = (ty - oy) / scale
        if flip_y:
            ny = 1.0 - ny
        corners.append({
            "pct": round(_pct_on_loop(loop, (nx, ny)), 5),
            "label": label,
        })
    return sorted(corners, key=lambda c: c["pct"])


def extract_layers_from_html(html: str) -> dict[str, str | None]:
    return {
        "config": _layer_svg(html, "active-config"),
        "inactive": _layer_svg(html, "inactive"),
        "pit": _layer_svg(html, "pit"),
        "turns": _layer_svg(html, "turn-numbers"),
        "start_finish": _layer_svg(html, "start-finish"),
    }


def import_svg_layers(
    *,
    html_path: str | None = None,
    html_text: str | None = None,
    config_svg: str | None = None,
    pit_svg: str | None = None,
    turns_svg: str | None = None,
    sf_svg: str | None = None,
    start_finish_override: float | None = None,
    num_corners: int = 4,
) -> dict:
    """Build a schema-2 schematic doc from iRacing members SVG layers."""
    html = html_text or _read_text(html_path)
    inactive_svg = None
    if html and not any((config_svg, pit_svg, turns_svg, sf_svg)):
        layers = extract_layers_from_html(html)
        config_svg = config_svg or layers.get("config")
        inactive_svg = layers.get("inactive")
        pit_svg = pit_svg or layers.get("pit")
        turns_svg = turns_svg or layers.get("turns")
        sf_svg = sf_svg or layers.get("start_finish")

    if not config_svg:
        raise ValueError(
            "No active-config SVG found — save the members track page HTML or "
            "pass --config with the racing-line layer.")
    if not pit_svg:
        raise ValueError(
            "No pit SVG found — ensure Pit Road is enabled on the members map "
            "or pass --pit with the pit layer SVG.")

    loop_raw = _main_loop_path(
        config_svg,
        pit_svg=pit_svg,
    )
    sf_idx = _detect_sf_svg(loop_raw, sf_svg)
    loop = _ensure_ccw(_reorder_loop(loop_raw, sf_idx))

    pit_in, pit_path, merge_blue, merge_anchor, merge_tick_count, _at_entry, pit_path_source = _pit_geometry(
        pit_svg, loop, inactive_svg=inactive_svg, active_svg=config_svg)

    pit_in = _smooth_poly(pit_in)
    pit_path = _smooth_poly(pit_path)

    loop, pit_in, pit_path, merge_blue, norm = _normalize_loop_and_pit(
        loop, pit_in, pit_path, merge_blue)
    ox, oy, scale = norm
    merge_anchor = ((merge_anchor[0] - ox) / scale, (merge_anchor[1] - oy) / scale)

    pit_in, pit_path, _ = _expand_pit_from_loop(
        [pit_in, pit_path, []], loop)

    straight_band = _pit_straight_x_band(pit_svg, norm) if pit_svg else None
    if pit_path and straight_band and pit_path_source == "inactive":
        pit_path = _straighten_in_x_band(pit_path, *straight_band)

    if _at_entry and pit_path and pit_svg:
        merge_centroids_norm = [
            ((c[0] - ox) / scale, (c[1] - oy) / scale)
            for c in (
                _segment_centroid(seg)
                for seg in _segments_from_group(pit_svg, "Mergeline")
            )
        ]
        if merge_centroids_norm:
            merge_anchor = _entry_merge_handoff(pit_path, merge_centroids_norm)

    handoff = merge_anchor if (_at_entry and merge_anchor is not None) else (
        pit_path[0] if pit_path else None)
    if pit_in and handoff is not None:
        pit_in = _snap_end(pit_in, handoff)
    if merge_blue and pit_path and not _at_entry:
        near_end = (pit_path[0] if _dist(pit_path[0], merge_anchor)
                    <= _dist(pit_path[-1], merge_anchor) else pit_path[-1])
        merge_blue = _snap_start(merge_blue, near_end)

    if pit_in and pit_path:
        pit_in = _collapse_degenerate_pit_in(pit_in, pit_path)
    merge_blue = _straighten_colinear_runs(merge_blue, min_run=3)
    if pit_in and handoff is not None:
        pit_in = _snap_end(pit_in, handoff)
    if merge_blue and _at_entry and handoff is not None and pit_path:
        merge_blue = _snap_start(merge_blue, handoff)
        if pit_path_source != "inactive":
            merge_blue = _trim_entry_colinear_prefix(
                merge_blue, handoff, pit_path, loop)

    merge_blue = _decouple_merge_from_loop(merge_blue, loop, min_frac=0.018, uniform=True)

    pit_in = _resample_open(pit_in, 24)
    pit_path = _resample_open(pit_path, 140)
    if merge_tick_count <= 8 and len(merge_blue) <= 3:
        merge_target = min(40, max(8, len(merge_blue)))
    else:
        merge_target = 40
    if _at_entry and pit_path and pit_svg:
        merge_centroids_norm = [
            ((c[0] - ox) / scale, (c[1] - oy) / scale)
            for c in (
                _segment_centroid(seg)
                for seg in _segments_from_group(pit_svg, "Mergeline")
            )
        ]
        if merge_centroids_norm:
            merge_anchor = _entry_merge_handoff(pit_path, merge_centroids_norm)
    handoff = merge_anchor if (_at_entry and merge_anchor is not None) else (
        pit_path[0] if pit_path else None)
    if _at_entry and len(merge_blue) >= 2 and handoff is not None:
        merge_blue = _snap_start(merge_blue, handoff)
        ticks = merge_blue[1:]
        entry_target = max(merge_target, 40)
        if len(ticks) >= 2:
            body = _resample_open(ticks, entry_target)
            merge_blue = [handoff] + body
        else:
            merge_blue = [handoff] + list(ticks)
    else:
        merge_blue = _resample_open(merge_blue, merge_target)
    merge_blue = _decouple_merge_from_loop(merge_blue, loop, min_frac=0.019, uniform=True)
    if _at_entry and handoff and merge_blue:
        if len(merge_blue) > 2:
            merge_blue = [handoff] + _smooth_poly(merge_blue[1:])
        merge_blue = _snap_start(merge_blue, handoff)
    else:
        merge_blue = _smooth_poly(merge_blue)

    if pit_path and merge_blue and not _at_entry:
        near_end = (pit_path[0] if _dist(pit_path[0], merge_anchor)
                    <= _dist(pit_path[-1], merge_anchor) else pit_path[-1])
        merge_blue = _snap_start(merge_blue, near_end)

    if _bbox_span(merge_blue) < _bbox_span(pit_path) * 0.12:
        raise ValueError(
            "Blue merge trace is too short after SVG import — ensure the pit "
            "layer includes #Mergeline through turns 1–2.")

    if turns_svg:
        corners = _parse_turn_numbers(turns_svg, loop, norm)
    elif num_corners:
        corners = _oval_corners(loop, num_corners)
    else:
        corners = []

    sf = float(start_finish_override) if start_finish_override is not None else 0.0
    lane_lo, lane_hi = _pit_span_on_loop(loop, pit_path)
    return {
        "schema": 2,
        "pit_source": pit_path_source,
        "start_finish": sf,
        "points": [[round(x, 7), round(y, 7)] for x, y in loop],
        "pit_in": [[round(x, 7), round(y, 7)] for x, y in pit_in],
        "pit_path": [[round(x, 7), round(y, 7)] for x, y in pit_path],
        "pit_out": [[round(x, 7), round(y, 7)] for x, y in merge_blue],
        "pit_in_pct": round(lane_lo, 5),
        "pit_span": [
            round(lane_lo, 5),
            round(lane_hi, 5),
        ],
        "pit_out_pct": round(_pct_on_loop(loop, merge_blue[-1]), 5),
        "num_turns": len(corners) if corners else (num_corners or None),
        "corners": corners,
    }


def import_track_source(path: str, *, num_corners: int = 4,
                        start_finish_override: float | None = None) -> dict:
    """Route PNG / HTML / SVG members exports to the right importer."""
    ext = os.path.splitext(path)[1].lower()
    if ext in (".html", ".htm"):
        return import_svg_layers(
            html_path=path,
            num_corners=num_corners,
            start_finish_override=start_finish_override,
        )
    if ext == ".svg":
        text = _read_text(path) or ""
        if "Pitroad" in text or "Mergeline" in text:
            raise ValueError(
                "Pit SVG alone is not enough — save the full members page HTML "
                "or pass both --config and --pit.")
        return import_svg_layers(
            config_svg=text,
            pit_svg=None,
            num_corners=num_corners,
            start_finish_override=start_finish_override,
        )
    from tools.schematic_to_track import import_schematic
    return import_schematic(
        path,
        num_corners=num_corners,
        start_finish_override=start_finish_override,
    )


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument(
        "source",
        nargs="?",
        help="members page .html (preferred) or legacy schematic .png",
    )
    ap.add_argument("track_id", nargs="?", help="iRacing TrackID")
    ap.add_argument("name", nargs="?", help="track display name")
    ap.add_argument(
        "out_dir",
        nargs="?",
        default=paths.tracks_dir(),
        help="output directory (default: GridGlance tracks dir, e.g. App Support)",
    )
    ap.add_argument("--html", help="members track page HTML (same as positional source)")
    ap.add_argument("--config", help="active-config layer SVG file")
    ap.add_argument("--pit", help="pit layer SVG (#Pitroad + #Mergeline)")
    ap.add_argument("--turns", help="turn-numbers layer SVG")
    ap.add_argument("--start-finish", dest="sf", help="start-finish layer SVG")
    ap.add_argument(
        "--start-finish-pct",
        type=float,
        default=None,
        help="override start_finish lap fraction (default 0.0)",
    )
    ap.add_argument("--corners", type=int, default=4,
                    help="auto corner count when turn-numbers layer missing (0=skip)")
    ap.add_argument("--force", action="store_true", help="overwrite existing file")
    args = ap.parse_args(argv)

    html_path = args.html or (
        args.source if args.source and args.source.lower().endswith((".html", ".htm"))
        else None
    )
    png_path = (
        args.source
        if args.source and args.source.lower().endswith((".png", ".jpg", ".jpeg"))
        else None
    )

    if not html_path and not png_path and not args.config:
        ap.print_help()
        return 2
    if not args.track_id or not args.name:
        print("track_id and name are required")
        return 2

    tid = (int(args.track_id) if str(args.track_id).lstrip("-").isdigit()
           else args.track_id)
    os.makedirs(args.out_dir, exist_ok=True)
    out_path = os.path.join(args.out_dir, f"{tid}.json")
    if os.path.exists(out_path) and not args.force:
        print(f"Refusing to overwrite {out_path} (use --force)")
        return 1

    if png_path:
        from tools.schematic_to_track import import_schematic
        data = import_schematic(
            png_path,
            start_finish_override=args.start_finish_pct,
            num_corners=args.corners,
        )
    else:
        data = import_svg_layers(
            html_path=html_path,
            config_svg=_read_text(args.config),
            pit_svg=_read_text(args.pit),
            turns_svg=_read_text(args.turns),
            sf_svg=_read_text(args.sf),
            start_finish_override=args.start_finish_pct,
            num_corners=args.corners,
        )

    doc = {k: v for k, v in data.items() if not str(k).startswith("_")}
    doc["track_id"] = tid
    doc["name"] = args.name
    if doc.get("num_turns") is None:
        doc.pop("num_turns", None)

    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, indent=2)

    print(f"Wrote {out_path}")
    print(f"  loop={len(doc['points'])} pts  pit_in={len(doc['pit_in'])}  "
          f"pit_path={len(doc['pit_path'])}  pit_out={len(doc['pit_out'])}")
    print(f"  pit_in_pct={doc['pit_in_pct']}  pit_span={doc['pit_span']}  "
          f"pit_out_pct={doc['pit_out_pct']}")
    if doc.get("corners"):
        print(f"  corners={len(doc['corners'])}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
