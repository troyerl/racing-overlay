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
import os
import re
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import svgpath
from tools.schematic_to_track import (
    _bbox_span,
    _chain_segments,
    _connect_blend_to_loop,
    _dist,
    _ensure_ccw,
    _nearest_index,
    _normalize_all,
    _orient_poly_from_anchor,
    _oval_corners,
    _pct_on_loop,
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


def _segments_from_group(svg_text: str, group_id: str) -> list[list[tuple[float, float]]]:
    out: list[list[tuple[float, float]]] = []
    for d in _paths_d_in_svg(svg_text, group_id=group_id):
        pts = svgpath.flatten_path(d)
        if len(pts) >= 2:
            out.append(pts)
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


def _classify_pitroad_segments(
    segments: list[list[tuple[float, float]]],
) -> tuple[list[list[tuple[float, float]]], list[list[tuple[float, float]]]]:
    """Split #Pitroad into straight dashes vs entry-blend curves (skip exit chevron)."""
    if not segments:
        return [], []
    small = [s for s in segments if _poly_len(s) < 100 and len(s) < 35]
    pool = small or segments
    cys = sorted(_segment_centroid(s)[1] for s in pool)
    cxs = sorted(_segment_centroid(s)[0] for s in pool)
    med_y = cys[len(cys) // 2]
    med_x = cxs[len(cxs) // 2]

    dashes: list[list[tuple[float, float]]] = []
    entry: list[list[tuple[float, float]]] = []
    for seg in segments:
        cx, cy = _segment_centroid(seg)
        span = _segment_span(seg)
        plen = _poly_len(seg)
        # Exit chevron / arrow at pit-out end (complex, right side).
        if plen > 120 or len(seg) > 45 or (span > 80 and cx > med_x + 80):
            continue
        # Entry blend: left of pit straight or curves up toward the loop.
        if cx < med_x - 60 or cy < med_y - 35 or (span > 40 and cy < med_y):
            entry.append(seg)
        elif abs(cy - med_y) < 40:
            dashes.append(seg)
        else:
            entry.append(seg)

    dashes.sort(key=lambda s: _segment_centroid(s)[0])
    entry.sort(key=lambda s: (_segment_centroid(s)[0], _segment_centroid(s)[1]))
    return dashes, entry


def _chain_sorted_dashes(dashes: list[list[tuple[float, float]]], *,
                         max_gap: float = 90.0) -> list[tuple[float, float]]:
    if not dashes:
        return []
    merged: list[tuple[float, float]] = []
    for seg in dashes:
        if not merged:
            merged.extend(seg)
            continue
        d0 = _dist(merged[-1], seg[0])
        d1 = _dist(merged[-1], seg[-1])
        part = list(reversed(seg)) if d1 < d0 else seg
        merged.extend(part[1:] if _dist(merged[-1], part[0]) < max_gap * 2 else part)
    if len(merged) >= 4:
        return merged
    return _chain_dashes(dashes, max_gap=max_gap)


def _chain_entry_blend(entry: list[list[tuple[float, float]]]) -> list[tuple[float, float]]:
    if not entry:
        return []
    if len(entry) == 1:
        return list(entry[0])
    return _chain_dashes(entry, max_gap=150.0)


def _trim_merge_at_rejoin(
    merge: list[tuple[float, float]], loop: list[tuple[float, float]],
) -> list[tuple[float, float]]:
    """Stop the merge polyline where it meets the loop (not past onto the backstretch)."""
    if len(merge) < 4:
        return merge
    cy = sum(p[1] for p in loop) / len(loop)
    best_i = 0
    best_d = 1e18
    for i, p in enumerate(merge):
        if p[1] > cy * 0.92:
            continue
        d = min(_dist(p, q) for q in loop)
        if d < best_d:
            best_d, best_i = d, i
    if best_i >= 3:
        return merge[: best_i + 1]
    return merge


def _oval_pit_geometry(
    pit_svg: str, loop: list[tuple[float, float]],
) -> tuple[list[tuple[float, float]], list[tuple[float, float]], list[tuple[float, float]]]:
    """Build pit_in, pit_path, pit_out from members #Pitroad / #Mergeline groups."""
    pit_segs = _segments_from_group(pit_svg, "Pitroad")
    if not pit_segs:
        raise ValueError("No #Pitroad paths found in pit SVG")

    dashes, entry = _classify_pitroad_segments(pit_segs)
    pit_path = _chain_sorted_dashes(dashes)
    if len(pit_path) < 4:
        pit_path = _chain_dashes(pit_segs, max_gap=90.0)
    if len(pit_path) < 4:
        raise ValueError("Pit road dashes did not chain — check pit SVG layer")

    # Oval pit road runs entry (low x / T4) → exit (high x / T1).
    if pit_path[-1][0] < pit_path[0][0]:
        pit_path = list(reversed(pit_path))

    entry_blend = _chain_entry_blend(entry)
    if entry_blend:
        pit_in = _connect_blend_to_loop(entry_blend, loop, attach_end=False)
    else:
        # Fallback: short stub from loop to pit-path start.
        pit_in = _connect_blend_to_loop(pit_path[: max(2, len(pit_path) // 10)],
                                        loop, attach_end=False)

    merge_segs = _segments_from_group(pit_svg, "Mergeline")
    if not merge_segs:
        raise ValueError("No #Mergeline paths found in pit SVG")
    merge_blue = _chain_dashes(merge_segs, max_gap=180.0)
    if len(merge_blue) < 4:
        raise ValueError("Safe-merge dashes did not chain — check pit SVG layer")
    merge_blue = _orient_poly_from_anchor(merge_blue, pit_path[-1])
    merge_blue = _trim_merge_at_rejoin(merge_blue, loop)
    merge_blue = _connect_blend_to_loop(merge_blue, loop, attach_end=True)

    return pit_in, pit_path, merge_blue


def _main_loop_path(svg_text: str) -> list[tuple[float, float]]:
    best: list[tuple[float, float]] = []
    for d in _paths_d_in_svg(svg_text):
        pts = svgpath.flatten_path(d)
        if len(pts) > len(best):
            best = pts
    if len(best) < 4:
        raise ValueError("No track outline path found in active-config SVG")
    return _resample_closed(best, 720)


def _detect_sf_svg(loop: list[tuple[float, float]], sf_svg: str | None) -> int:
    if sf_svg:
        centroids: list[tuple[float, float]] = []
        for d in _paths_d_in_svg(sf_svg):
            pts = svgpath.flatten_path(d)
            if not pts:
                continue
            cx = sum(p[0] for p in pts) / len(pts)
            cy = sum(p[1] for p in pts) / len(pts)
            centroids.append((cx, cy))
        if centroids:
            tx, ty = max(centroids, key=lambda t: t[1])
            return _nearest_index(loop, (tx, ty))
    return max(range(len(loop)), key=lambda i: loop[i][1])


def _parse_turn_numbers(svg_text: str, loop: list[tuple[float, float]],
                        norm: tuple[float, float, float]) -> list[dict]:
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
        corners.append({
            "pct": round(_pct_on_loop(loop, (nx, ny)), 5),
            "label": label,
        })
    return sorted(corners, key=lambda c: c["pct"])


def extract_layers_from_html(html: str) -> dict[str, str | None]:
    return {
        "config": _layer_svg(html, "active-config"),
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
    if html and not any((config_svg, pit_svg, turns_svg, sf_svg)):
        layers = extract_layers_from_html(html)
        config_svg = config_svg or layers.get("config")
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

    loop_raw = _main_loop_path(config_svg)
    sf_idx = _detect_sf_svg(loop_raw, sf_svg)
    loop = _ensure_ccw(_reorder_loop(loop_raw, sf_idx))

    pit_in, pit_path, merge_blue = _oval_pit_geometry(pit_svg, loop)

    pit_in = _smooth_poly(pit_in)
    pit_path = _smooth_poly(pit_path)
    merge_blue = _smooth_poly(merge_blue)

    segs_norm, norm = _normalize_all([loop, pit_in, pit_path, merge_blue])
    loop, pit_in, pit_path, merge_blue = segs_norm

    pit_in = _resample_open(pit_in, 8)
    pit_path = _resample_open(pit_path, 140)
    merge_blue = _resample_open(merge_blue, 40)

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
    return {
        "schema": 2,
        "pit_source": "schematic",
        "start_finish": sf,
        "points": [[round(x, 7), round(y, 7)] for x, y in loop],
        "pit_in": [[round(x, 7), round(y, 7)] for x, y in pit_in],
        "pit_path": [[round(x, 7), round(y, 7)] for x, y in pit_path],
        "pit_out": [[round(x, 7), round(y, 7)] for x, y in merge_blue],
        "pit_in_pct": round(_pct_on_loop(loop, pit_in[0]), 5),
        "pit_span": [
            round(_pct_on_loop(loop, pit_path[0]), 5),
            round(_pct_on_loop(loop, pit_path[-1]), 5),
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
        default=os.path.join(_ROOT, "tracks"),
        help="output directory (default: tracks/)",
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
