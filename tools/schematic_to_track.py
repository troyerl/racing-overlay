#!/usr/bin/env python3
"""Convert an iRacing-style schematic PNG into a schema-2 track JSON file.

Extracts the white racing loop, red pit road/entry, and blue safe-merge exit
from the legend colors, aligns start/finish, and writes tracks/<track_id>.json
with pit_source "schematic".

Requires tool-only deps (not needed at runtime):
    pip install opencv-python-headless numpy

Usage:
    python3 tools/schematic_to_track.py map.png 123 "Track Name"
    python3 tools/schematic_to_track.py map.png 123 "Track Name" --preview
    python3 tools/schematic_to_track.py map.png 123 "Track Name" --start-finish 0.0
"""

from __future__ import annotations

import argparse
import json
import math
import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

try:
    import cv2
    import numpy as np
except ImportError:
    cv2 = None  # type: ignore[assignment,misc]
    np = None  # type: ignore[assignment,misc]

_CV2_HINT = (
    "schematic_to_track requires opencv-python-headless and numpy:\n"
    "  pip install opencv-python-headless numpy"
)


def _require_cv2():
    """Import guard so GUI code can catch ImportError instead of SystemExit."""
    if cv2 is None or np is None:
        raise ImportError(_CV2_HINT)


def _signed_area(pts: list[tuple[float, float]]) -> float:
    a = 0.0
    n = len(pts)
    for i in range(n):
        x0, y0 = pts[i]
        x1, y1 = pts[(i + 1) % n]
        a += x0 * y1 - x1 * y0
    return a * 0.5


def _arc_length(pts: list[tuple[float, float]], closed: bool = False) -> float:
    total = 0.0
    lim = len(pts) if closed else len(pts) - 1
    for i in range(lim):
        a, b = pts[i], pts[(i + 1) % len(pts)]
        total += math.hypot(b[0] - a[0], b[1] - a[1])
    return total


def _resample_open(pts: list[tuple[float, float]], n: int) -> list[tuple[float, float]]:
    if len(pts) < 2 or n < 2:
        return list(pts)
    cum = [0.0]
    for a, b in zip(pts, pts[1:]):
        cum.append(cum[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
    total = cum[-1]
    if total <= 0:
        return list(pts)
    out = []
    step = total / (n - 1)
    j = 0
    for k in range(n):
        target = k * step
        while j < len(cum) - 2 and cum[j + 1] < target:
            j += 1
        seg = cum[j + 1] - cum[j]
        t = (target - cum[j]) / seg if seg else 0.0
        a, b = pts[j], pts[j + 1]
        out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    return out


def _resample_closed(pts: list[tuple[float, float]], n: int) -> list[tuple[float, float]]:
    if len(pts) < 3:
        return list(pts)
    closed = list(pts) + [pts[0]]
    cum = [0.0]
    for a, b in zip(closed, closed[1:]):
        cum.append(cum[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
    total = cum[-1]
    if total <= 0:
        return list(pts)
    out = []
    step = total / n
    j = 0
    for k in range(n):
        target = k * step
        while j < len(cum) - 2 and cum[j + 1] < target:
            j += 1
        seg = cum[j + 1] - cum[j]
        t = (target - cum[j]) / seg if seg else 0.0
        a, b = closed[j], closed[j + 1]
        out.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
    return out


def _nearest_index(pts: list[tuple[float, float]], pt: tuple[float, float]) -> int:
    best, bi = 1e18, 0
    for i, p in enumerate(pts):
        d = (p[0] - pt[0]) ** 2 + (p[1] - pt[1]) ** 2
        if d < best:
            best, bi = d, i
    return bi


def _pct_on_loop(loop: list[tuple[float, float]], pt: tuple[float, float]) -> float:
    """Arc-length lap fraction of the nearest point on a closed loop."""
    n = len(loop)
    if n < 2:
        return 0.0
    best_d, best_i, best_t = 1e18, 0, 0.0
    for i in range(n):
        a, b = loop[i], loop[(i + 1) % n]
        dx, dy = b[0] - a[0], b[1] - a[1]
        ln2 = dx * dx + dy * dy
        t = 0.0 if ln2 < 1e-12 else max(0.0, min(1.0, (
            (pt[0] - a[0]) * dx + (pt[1] - a[1]) * dy) / ln2))
        px, py = a[0] + dx * t, a[1] + dy * t
        d = (px - pt[0]) ** 2 + (py - pt[1]) ** 2
        if d < best_d:
            best_d, best_i, best_t = d, i, t
    cum = [0.0]
    for i in range(n):
        a, b = loop[i], loop[(i + 1) % n]
        cum.append(cum[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
    total = cum[-1]
    if total <= 0:
        return 0.0
    pos = cum[best_i] + best_t * (cum[best_i + 1] - cum[best_i])
    return pos / total


def _reorder_loop(loop: list[tuple[float, float]], sf_idx: int) -> list[tuple[float, float]]:
    if not loop:
        return loop
    return loop[sf_idx:] + loop[:sf_idx]


def _ensure_ccw(loop: list[tuple[float, float]]) -> list[tuple[float, float]]:
    if _signed_area(loop) < 0:
        return list(reversed(loop))
    return loop


def _normalize_all(segs: list[list[tuple[float, float]]], pad: float = 0.04
                   ) -> tuple[list[list[tuple[float, float]]], tuple[float, float, float, float]]:
    flat = [p for seg in segs for p in seg]
    xs = [p[0] for p in flat]
    ys = [p[1] for p in flat]
    minx, maxx = min(xs), max(xs)
    miny, maxy = min(ys), max(ys)
    span = max(maxx - minx, maxy - miny) or 1.0
    ox = minx - pad * span
    oy = miny - pad * span
    scale = span * (1 + 2 * pad)
    out = []
    for seg in segs:
        out.append([((p[0] - ox) / scale, (p[1] - oy) / scale) for p in seg])
    return out, (ox, oy, scale)


def _mask_white(bgr: np.ndarray) -> np.ndarray:
    hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV)
    # Light desaturated pixels (track outline)
    m1 = cv2.inRange(hsv, (0, 0, 180), (180, 60, 255))
    gray = cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY)
    m2 = cv2.threshold(gray, 200, 255, cv2.THRESH_BINARY)[1]
    return cv2.bitwise_and(m1, m2)


def _mask_red(bgr: np.ndarray) -> np.ndarray:
    """iRacing pit road / entry dashes — R-dominant (HSV mis-tags bright blue)."""
    b, g, r = cv2.split(bgr)
    m = (r > 55) & (r > g + 15) & (r > b + 15) & (g < 120)
    return (m.astype(np.uint8) * 255)


def _mask_blue(bgr: np.ndarray) -> np.ndarray:
    """iRacing safe-merge exit dashes — B-dominant."""
    b, g, r = cv2.split(bgr)
    m = (b > 80) & (b > g + 10) & (b > r + 10)
    return (m.astype(np.uint8) * 255)


def _centerline_mask(mask: np.ndarray) -> np.ndarray:
    """Thin fat hand-drawn strokes; leave iRacing dash masks unchanged."""
    if cv2.countNonZero(mask) == 0:
        return mask
    dist = cv2.distanceTransform(mask, cv2.DIST_L2, 5)
    max_d = float(dist.max())
    # iRacing schematic dashes are ~1–2 px wide — eroding destroys the chain.
    if max_d < 3.0:
        return mask
    thresh = max(1.0, max_d * 0.55)
    ridge = (dist >= thresh).astype(np.uint8) * 255
    ridge = cv2.bitwise_and(ridge, mask)
    if cv2.countNonZero(ridge) >= 25:
        return ridge
    k = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (3, 3))
    iters = max(1, min(4, int(max_d * 0.45)))
    thin = cv2.erode(mask, k, iterations=iters)
    return thin if cv2.countNonZero(thin) >= 20 else mask


def _strip_red_decorations(red_m: np.ndarray) -> np.ndarray:
    """Mask out SF tick / direction arrow; keep all pit-lane dash pixels."""
    h, w = red_m.shape[:2]
    out = red_m.copy()
    # SF crosshair tick (compact vertical blob on the front straight).
    cv2.rectangle(out, (w // 2 - 12, int(h * 0.88)), (w // 2 + 12, h - 2), 0, -1)
    # Large direction arrow under the track.
    cv2.rectangle(out, (w // 2 - 55, int(h * 0.82)), (w // 2 + 55, h - 2), 0, -1)
    if cv2.countNonZero(out) < 80:
        return red_m
    return out


def _smooth_poly(pts: list[tuple[float, float]], window: int = 5
                 ) -> list[tuple[float, float]]:
    if len(pts) < window:
        return list(pts)
    half = window // 2
    out: list[tuple[float, float]] = []
    for i in range(len(pts)):
        chunk = pts[max(0, i - half):min(len(pts), i + half + 1)]
        out.append((sum(p[0] for p in chunk) / len(chunk),
                    sum(p[1] for p in chunk) / len(chunk)))
    return out


def _bridge(mask: np.ndarray, k: int = 5) -> np.ndarray:
    kernel = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (k, k))
    closed = cv2.morphologyEx(mask, cv2.MORPH_CLOSE, kernel, iterations=2)
    return cv2.dilate(closed, kernel, iterations=1)


def _largest_contour(mask: np.ndarray, min_area: float = 500):
    cnts, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    best, ba = None, 0.0
    for c in cnts:
        a = cv2.contourArea(c)
        if a > ba and a >= min_area:
            ba, best = a, c
    return best


def _contour_to_pts(contour, step: int = 2) -> list[tuple[float, float]]:
    pts = [(float(p[0][0]), float(p[0][1])) for p in contour[::step]]
    if len(pts) < 3:
        pts = [(float(p[0][0]), float(p[0][1])) for p in contour]
    return pts


def _dist(a: tuple[float, float], b: tuple[float, float]) -> float:
    return math.hypot(a[0] - b[0], a[1] - b[1])


def _filter_lane_contours(cnts, img_h: int, *, min_area: float = 8.0) -> list:
    """Drop noise and the bottom-centre SF arrow blob."""
    keep = []
    for c in cnts:
        area = cv2.contourArea(c)
        if area < min_area:
            continue
        x, y, w, h = cv2.boundingRect(c)
        # Large filled decoration under the start/finish tick — not pit geometry.
        if area > 1200 and y > img_h * 0.78 and w < img_h * 0.25:
            continue
        keep.append(c)
    return keep


def _all_chains_from_mask(mask: np.ndarray, max_gap: float = 35.0
                          ) -> list[list[tuple[float, float]]]:
    """Return separate polylines for each dashed-lane cluster in a mask."""
    kernel = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (5, 5))
    dilated = cv2.dilate(mask, kernel, iterations=1)
    cnts, _ = cv2.findContours(dilated, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    cnts = _filter_lane_contours(cnts, mask.shape[0])
    segments = []
    for c in cnts:
        if len(c) < 3:
            continue
        step = max(1, len(c) // 60)
        seg = _contour_to_pts(c, step=step)
        if len(seg) >= 2:
            segments.append(seg)
    if not segments:
        return []
    chains = [list(seg) for seg in segments]
    merged = True
    while merged:
        merged = False
        best = None
        best_d = max_gap
        for i, si in enumerate(chains):
            for j, sj in enumerate(chains):
                if i >= j:
                    continue
                for rev_i in (False, True):
                    for rev_j in (False, True):
                        a = list(reversed(si)) if rev_i else si
                        b = list(reversed(sj)) if rev_j else sj
                        d = _dist(a[-1], b[0])
                        if d < best_d:
                            best_d = d
                            best = (i, j, rev_i, rev_j)
        if best is None:
            break
        i, j, rev_i, rev_j = best
        a = list(reversed(chains[i])) if rev_i else chains[i]
        b = list(reversed(chains[j])) if rev_j else chains[j]
        chains[i] = a + b[1:]
        del chains[j]
        merged = True
    return [c for c in chains if len(c) >= 2]


def _polyline_from_mask(mask: np.ndarray, min_len: int = 40,
                        *, max_gap: float = 45.0) -> list[tuple[float, float]]:
    """Extract the longest polyline from dashed lane markings in a mask."""
    segments = []
    kernel = cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (5, 5))
    dilated = cv2.dilate(mask, kernel, iterations=1)
    cnts, _ = cv2.findContours(dilated, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)
    cnts = _filter_lane_contours(cnts, mask.shape[0])
    for c in cnts:
        if len(c) < 3:
            continue
        step = max(1, len(c) // 60)
        seg = _contour_to_pts(c, step=step)
        if len(seg) >= 2:
            segments.append(seg)
    poly = _chain_segments(segments, max_gap=max_gap)
    if len(poly) < max(4, min_len // 8):
        return []
    return poly


def _orient_poly_from_anchor(poly: list[tuple[float, float]],
                             anchor: tuple[float, float]
                             ) -> list[tuple[float, float]]:
    """Return the longest arm of a polyline starting from the pit-road anchor."""
    if len(poly) < 2:
        return poly
    si = min(range(len(poly)), key=lambda i: _dist(poly[i], anchor))
    forward = poly[si:]
    backward = list(reversed(poly[: si + 1]))
    return forward if _poly_len(forward) >= _poly_len(backward) else backward


def _merge_blue_poly(blue_m: np.ndarray,
                     pit_path: list[tuple[float, float]],
                     loop: list[tuple[float, float]]) -> list[tuple[float, float]]:
    """Build the safe-merge polyline from pit-road end through T1–T2 back to loop."""
    img_h, img_w = blue_m.shape[:2]
    chains = _all_chains_from_mask(blue_m, max_gap=35.0)
    if not chains:
        return []
    anchor = pit_path[-1] if len(pit_path) >= 2 else chains[0][0]

    def _loop_prox(chain: list[tuple[float, float]]) -> float:
        return sum(min(_dist(p, q) for q in loop) for p in chain) / len(chain)

    near = [ch for ch in chains
            if min(_dist(p, anchor) for p in ch) < 75.0
            and _loop_prox(ch) < 35.0]
    arc_candidates = [ch for ch in chains
                      if max(p[0] for p in ch) > img_w * 0.82
                      and min(p[1] for p in ch) < anchor[1]
                      and _loop_prox(ch) < 20.0]
    arc = max(arc_candidates, key=len) if arc_candidates else None
    selected = list(near)
    if arc is not None and arc not in selected:
        selected.append(arc)
    if not selected:
        scored = sorted(
            (min(_dist(p, anchor) for p in ch), ch) for ch in chains)
        selected = [scored[0][1]]

    merge = _chain_segments(selected, max_gap=280.0)
    if len(merge) < 4:
        merge = max(selected, key=len)
    merge = _orient_poly_from_anchor(merge, anchor)
    if _poly_len(merge) < _poly_len(pit_path) * 0.08:
        merge = _orient_poly_from_anchor(max(chains, key=len), anchor)
    return _resample_open(merge, 40)


def _poly_len(pts: list[tuple[float, float]]) -> float:
    if len(pts) < 2:
        return 0.0
    return sum(_dist(a, b) for a, b in zip(pts, pts[1:]))


def _bbox_span(pts: list[tuple[float, float]]) -> float:
    if not pts:
        return 0.0
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    return max(max(xs) - min(xs), max(ys) - min(ys))


def _chain_segments(segments: list[list[tuple[float, float]]],
                    max_gap: float = 40.0) -> list[tuple[float, float]]:
    """Connect dashed-line fragments into one polyline by nearest endpoints."""
    if not segments:
        return []
    chains = [list(seg) for seg in segments if len(seg) >= 2]
    if not chains:
        return []
    merged = True
    while merged:
        merged = False
        best = None
        best_d = max_gap
        for i, si in enumerate(chains):
            for j, sj in enumerate(chains):
                if i >= j:
                    continue
                for rev_i in (False, True):
                    for rev_j in (False, True):
                        a = list(reversed(si)) if rev_i else si
                        b = list(reversed(sj)) if rev_j else sj
                        d = _dist(a[-1], b[0])
                        if d < best_d:
                            best_d = d
                            best = (i, j, rev_i, rev_j)
        if best is None:
            break
        i, j, rev_i, rev_j = best
        a = list(reversed(chains[i])) if rev_i else chains[i]
        b = list(reversed(chains[j])) if rev_j else chains[j]
        chains[i] = a + b[1:]
        del chains[j]
        merged = True
    return max(chains, key=len)


def _split_red_polyline(red: list[tuple[float, float]],
                        loop: list[tuple[float, float]],
                        centroid: tuple[float, float]
                        ) -> tuple[list[tuple[float, float]], list[tuple[float, float]]]:
    """Split red trace into pit_in (short entry blend) and pit_path (pit road)."""
    if len(red) < 4:
        return red, red
    # Junction: red point closest to the main loop (pit entry at the loop edge).
    loop_dists = [min(math.hypot(p[0] - q[0], p[1] - q[1]) for q in loop) for p in red]
    j_idx = min(range(len(loop_dists)), key=lambda i: loop_dists[i])
    forward = red[j_idx:]
    backward = list(reversed(red[: j_idx + 1]))
    if len(forward) >= len(backward):
        pit_path, pit_in = forward, backward
    else:
        pit_path, pit_in = backward, forward
    if len(pit_in) < 2:
        pit_in = pit_path[: max(2, len(pit_path) // 8)]
    # Bottom straight runs T4 (low x) -> pit exit (high x) on standard ovals.
    if len(pit_path) >= 2 and pit_path[-1][0] < pit_path[0][0]:
        pit_path = list(reversed(pit_path))
    pit_in = _resample_open(pit_in, 8)
    pit_path = _resample_open(pit_path, 140)
    return pit_in, pit_path


def _connect_blend_to_loop(blend: list[tuple[float, float]],
                           loop: list[tuple[float, float]], *,
                           attach_end: bool = False,
                           n_loop: int = 20,
                           max_pts: int | None = None) -> list[tuple[float, float]]:
    """Extend entry/exit blends so they visibly meet the racing loop."""
    if not blend or len(blend) < 2 or not loop:
        return blend
    cap = max_pts if max_pts is not None else max(56, len(blend) + n_loop)
    xs = [p[0] for p in loop]
    ys = [p[1] for p in loop]
    prox = max(max(xs) - min(xs), max(ys) - min(ys)) * 0.035
    n = len(loop)
    if attach_end:
        anchor = blend[-1]
        if min(_dist(anchor, p) for p in loop) < prox:
            return _resample_open(blend, min(len(blend), cap))
        li = min(range(n), key=lambda i: _dist(loop[i], anchor))
        ext = [loop[(li + k) % n] for k in range(n_loop)]
        merged = list(blend) + ext[1:]
    else:
        anchor = blend[0]
        if min(_dist(anchor, p) for p in loop) < prox:
            return _resample_open(blend, min(len(blend), cap))
        li = min(range(n), key=lambda i: _dist(loop[i], anchor))
        ext = [loop[(li - k) % n] for k in range(n_loop)]
        ext.reverse()
        merged = ext + blend[1:]
    return _resample_open(merged, max(16, min(len(merged), cap)))


def _detect_sf(loop_raw: list[tuple[float, float]],
               red_mask: np.ndarray) -> int:
    """Pick SF index: bottom-most loop point (front straight on standard maps)."""
    if not loop_raw:
        return 0
    # Prefer red SF tick: small red blob on the loop bottom edge
    cnts, _ = cv2.findContours(red_mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
    ticks = []
    for c in cnts:
        a = cv2.contourArea(c)
        if 20 < a < 800:
            m = cv2.moments(c)
            if m["m00"]:
                ticks.append((float(m["m10"] / m["m00"]), float(m["m01"] / m["m00"])))
    if ticks:
        # SF tick sits on the bottom straight — highest Y in image coords
        tx, ty = max(ticks, key=lambda t: t[1])
        return _nearest_index(loop_raw, (tx, ty))
    return max(range(len(loop_raw)), key=lambda i: loop_raw[i][1])


def _oval_corners(loop: list[tuple[float, float]], n: int = 4) -> list[dict]:
    """Place corner labels at quadrant extrema of the resampled loop."""
    if len(loop) < 8:
        return []
    cx = sum(p[0] for p in loop) / len(loop)
    cy = sum(p[1] for p in loop) / len(loop)
    quads = [(0, "1"), (1, "2"), (2, "3"), (3, "4")][:n]
    corners = []
    for qi, label in quads:
        angle_lo = qi * math.pi / 2
        angle_hi = (qi + 1) * math.pi / 2
        best_i, best_score = 0, -1e18
        for i, p in enumerate(loop):
            ang = math.atan2(p[1] - cy, p[0] - cx)
            if ang < 0:
                ang += 2 * math.pi
            if angle_lo <= ang < angle_hi:
                r = math.hypot(p[0] - cx, p[1] - cy)
                if r > best_score:
                    best_score, best_i = r, i
        corners.append({"pct": round(_pct_on_loop(loop, loop[best_i]), 5),
                        "label": label})
    return corners


def import_schematic(path: str, *, start_finish_override: float | None = None,
                     num_corners: int = 4) -> dict:
    _require_cv2()
    bgr = cv2.imread(path)
    if bgr is None:
        raise ValueError(f"Could not read image: {path}")

    white_m = _mask_white(bgr)
    red_m = _strip_red_decorations(_mask_red(bgr))
    blue_m = _mask_blue(bgr)
    # iRacing dashes are already thin — do not centerline.

    # Remove red/blue from white mask so pit lines don't distort the loop
    white_m = cv2.bitwise_and(white_m, cv2.bitwise_not(cv2.bitwise_or(red_m, blue_m)))
    white_m = _bridge(white_m, 9)

    loop_cnt = _largest_contour(white_m)
    if loop_cnt is None:
        raise ValueError("No white track loop found in image")
    loop_raw = _contour_to_pts(loop_cnt, step=max(1, len(loop_cnt) // 600))
    loop_raw = _resample_closed(loop_raw, 720)

    sf_idx = _detect_sf(loop_raw, red_m)
    loop = _reorder_loop(loop_raw, sf_idx)
    loop = _ensure_ccw(loop)

    cx = sum(p[0] for p in loop) / len(loop)
    cy = sum(p[1] for p in loop) / len(loop)

    red_poly = _polyline_from_mask(red_m, max_gap=55.0)
    if len(red_poly) < 4:
        raise ValueError("No red pit road trace found in image")

    pit_in, pit_path = _split_red_polyline(red_poly, loop, (cx, cy))
    pit_out = _merge_blue_poly(blue_m, pit_path, loop)
    if len(pit_out) < 4:
        raise ValueError("No blue safe-merge trace found in image")

    pit_in = _smooth_poly(pit_in)
    pit_path = _smooth_poly(pit_path)
    pit_out = _smooth_poly(pit_out)

    segs_norm, _ = _normalize_all([loop, pit_in, pit_path, pit_out])
    loop, pit_in, pit_path, pit_out = segs_norm

    # Traced schematic lanes already meet the loop — only resample to targets.
    pit_in = _resample_open(pit_in, 8)
    pit_path = _resample_open(pit_path, 140)
    pit_out = _resample_open(pit_out, 40)

    if _bbox_span(pit_out) < _bbox_span(pit_path) * 0.12:
        raise ValueError(
            "Blue merge trace is too short — ensure the schematic shows "
            "dashed blue from pit exit through turns 1–2.")

    pit_in_pct = round(_pct_on_loop(loop, pit_in[0]), 5)
    lane_lo = round(_pct_on_loop(loop, pit_path[0]), 5)
    lane_hi = round(_pct_on_loop(loop, pit_path[-1]), 5)
    pit_out_pct = round(_pct_on_loop(loop, pit_out[-1]), 5)

    sf = float(start_finish_override) if start_finish_override is not None else 0.0
    corners = _oval_corners(loop, num_corners) if num_corners else []

    return {
        "schema": 2,
        "pit_source": "schematic",
        "start_finish": sf,
        "points": [[round(x, 7), round(y, 7)] for x, y in loop],
        "pit_in": [[round(x, 7), round(y, 7)] for x, y in pit_in],
        "pit_path": [[round(x, 7), round(y, 7)] for x, y in pit_path],
        "pit_out": [[round(x, 7), round(y, 7)] for x, y in pit_out],
        "pit_in_pct": pit_in_pct,
        "pit_span": [lane_lo, lane_hi],
        "pit_out_pct": pit_out_pct,
        "num_turns": num_corners if num_corners else None,
        "corners": corners,
        "_raw_loop": loop_raw,
        "_sf_idx": sf_idx,
    }


def write_preview(src_path: str, doc: dict, out_path: str,
                  raw_loop: list[tuple[float, float]] | None) -> None:
    _require_cv2()
    bgr = cv2.imread(src_path)
    if bgr is None or not raw_loop:
        return
    overlay = bgr.copy()
    xs = [p[0] for p in raw_loop]
    ys = [p[1] for p in raw_loop]
    minx, maxx = min(xs), max(xs)
    miny, maxy = min(ys), max(ys)
    span = max(maxx - minx, maxy - miny) or 1.0
    pad = 0.04 * span
    ox, oy = minx - pad, miny - pad
    sc = span * (1 + 2 * 0.04)

    def denorm(seg):
        return [(p[0] * sc + ox, p[1] * sc + oy) for p in seg]

    colors = {"points": (255, 255, 255), "pit_in": (0, 255, 255),
              "pit_path": (0, 0, 255), "pit_out": (255, 128, 0)}
    for key, col in colors.items():
        seg = doc.get(key)
        if not seg or len(seg) < 2:
            continue
        pts = denorm(seg)
        for i in range(len(pts) - 1):
            p0 = (int(pts[i][0]), int(pts[i][1]))
            p1 = (int(pts[i + 1][0]), int(pts[i + 1][1]))
            cv2.line(overlay, p0, p1, col, 2, cv2.LINE_AA)
    cv2.imwrite(out_path, overlay)
    print(f"  preview -> {out_path}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("png", help="iRacing schematic PNG")
    ap.add_argument("track_id", help="iRacing TrackID (numeric or string)")
    ap.add_argument("name", help='Track display name')
    ap.add_argument("out_dir", nargs="?", default=os.path.join(_ROOT, "tracks"),
                    help="output directory (default: tracks/)")
    ap.add_argument("--preview", action="store_true",
                    help="write a debug overlay PNG next to the output JSON")
    ap.add_argument("--start-finish", type=float, default=None,
                    help="override start_finish lap fraction (default 0.0)")
    ap.add_argument("--corners", type=int, default=4,
                    help="number of auto corner labels (0 to skip)")
    ap.add_argument("--force", action="store_true", help="overwrite existing file")
    args = ap.parse_args(argv)

    tid = int(args.track_id) if str(args.track_id).lstrip("-").isdigit() else args.track_id
    os.makedirs(args.out_dir, exist_ok=True)
    out_path = os.path.join(args.out_dir, f"{tid}.json")
    if os.path.exists(out_path) and not args.force:
        print(f"Refusing to overwrite {out_path} (use --force)")
        return 1

    data = import_schematic(args.png, start_finish_override=args.start_finish,
                            num_corners=args.corners)
    doc = {k: v for k, v in data.items() if not k.startswith("_")}
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

    if args.preview:
        prev = os.path.splitext(out_path)[0] + "_preview.png"
        write_preview(args.png, doc, prev, data.get("_raw_loop"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
