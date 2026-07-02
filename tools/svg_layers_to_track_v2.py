#!/usr/bin/env python3
"""V2 track map import — layer-targeted BeautifulSoup + svgpathtools extraction.

Reads members-site HTML by semantic layer instead of guessing from ``#inactive``
path length:

* ``active-config`` — main racing line loop → ``points``
* ``pit`` — ``#Pitroad`` geometry → ``pit_lane_points`` / ``pit_path``

Normalization uses a shared bounding box (track + pit) with Y inverted to 0–1
image coordinates.

Usage::

    python3 tools/svg_layers_to_track_v2.py "tracks-html/Road/Motorsport Arena Oschersleben.html" 449 "Oschersleben" tracks --force

Requires: ``pip install -r requirements-dev.txt`` (beautifulsoup4, svgpathtools)
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import paths
from tools.schematic_to_track import _pct_on_loop
from tools.svg_layers_to_track import (
    _classify_pitroad_segments,
    _main_pit_straight_dashes,
    _pit_lane_curved,
    _pit_path_from_dashes,
    _segment_centroid,
    _segments_from_group,
    _straighten_pit_path,
)

_LAYER_TOKENS = ("active-config", "inactive", "pit", "start-finish", "turn-numbers")
_MIN_INACTIVE_SUBPATH_LEN = 1500.0
_SUBPATH_LENGTH_TIE_FRAC = 0.03


def _require_v2_deps():
    try:
        from bs4 import BeautifulSoup  # noqa: F401
        from svgpathtools import parse_path  # noqa: F401
    except ImportError as exc:
        raise ImportError(
            "V2 import requires beautifulsoup4 and svgpathtools — "
            "run: pip install beautifulsoup4 svgpathtools") from exc


def _read_text(path: str | None) -> str | None:
    if not path:
        return None
    with open(path, encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _find_class_element(soup, class_token: str):
    """Find first tag whose class list contains *class_token*."""
    for tag in soup.find_all(True):
        classes = tag.get("class") or []
        if isinstance(classes, str):
            classes = classes.split()
        if class_token in classes:
            return tag
    return None


def _find_path_by_class(soup, class_token: str):
    """Locate a ``<path>`` tagged with *class_token* (substring match)."""
    el = soup.find(
        lambda tag: tag.name == "path"
        and tag.has_attr("class")
        and any(class_token in c for c in tag.get("class", [])),
    )
    return el


def _resolve_active_path_element(soup):
    """Return the active-config layout path (members ``div.track-svg`` layer)."""
    active_path_element = _find_class_element(soup, "active-config")
    if active_path_element is not None and active_path_element.name != "path":
        paths = active_path_element.find_all("path")
        if paths:
            active_path_element = max(
                paths, key=lambda p: len(p.get("d") or ""))

    if active_path_element is None or not active_path_element.get("d"):
        active_path_element = _find_path_by_class(soup, "active-config")

    inactive_svg = soup.find("svg", id="inactive")
    if inactive_svg and (
        active_path_element is None or not active_path_element.get("d")
    ):
        active_path_element = inactive_svg.find("path", class_="cls-1")

    return active_path_element


def _resolve_pit_path_element(soup):
    """Return pit layer container (``div.track-svg.pit``) for dash extraction."""
    pit_path_element = _find_class_element(soup, "pit")
    if pit_path_element is None:
        pit_path_element = _find_path_by_class(soup, "pit")
    return pit_path_element


def _svg_id_from_layer(layer_element) -> str:
    if layer_element is None:
        return ""
    svg = layer_element.find("svg")
    if svg is not None and svg.get("id"):
        return str(svg.get("id"))
    return ""


def _layout_prefix(svg_id: str) -> str:
    """Parse members layout prefix from ids like ``GP_-_pitroad`` or ``Moto_-_active``."""
    if "_-_" in svg_id:
        return svg_id.split("_-_", 1)[0]
    return ""


def _subpath_bbox_area(segment) -> float:
    pts = [segment.point(i / 19) for i in range(20)]
    xs = [p.real for p in pts]
    ys = [p.imag for p in pts]
    return (max(xs) - min(xs)) * (max(ys) - min(ys))


def _pick_best_subpath(subpaths: list) -> Any:
    """Prefer longest arc length; tie-break by largest bbox area (outer loop)."""
    if not subpaths:
        raise ValueError("No track subpaths found")
    best_len = max(sp.length() for sp in subpaths)
    candidates = [
        sp for sp in subpaths
        if sp.length() >= best_len * (1.0 - _SUBPATH_LENGTH_TIE_FRAC)
    ]
    return max(candidates, key=lambda sp: (sp.length(), _subpath_bbox_area(sp)))


def _mean_pit_distance(segment, pit_centroids: list[tuple[float, float]],
                       *, sample_n: int = 200) -> float:
    if not pit_centroids:
        return float("inf")
    loop_pts = [
        (float(segment.point(i / (sample_n - 1)).real),
         float(segment.point(i / (sample_n - 1)).imag))
        for i in range(sample_n)
    ]

    def _dist2(a, b):
        dx = a[0] - b[0]
        dy = a[1] - b[1]
        return dx * dx + dy * dy

    total = 0.0
    for cx, cy in pit_centroids:
        best = min(_dist2((cx, cy), lp) for lp in loop_pts)
        total += best ** 0.5
    return total / len(pit_centroids)


def _pit_dash_centroids(pit_element) -> list[tuple[float, float]]:
    """Classified #Pitroad road-dash centroids in SVG space (excludes entry)."""
    if pit_element is None:
        return []
    pit_svg = str(pit_element.find("svg") or pit_element)
    segments = _segments_from_group(pit_svg, "Pitroad")
    if not segments:
        return []
    dashes, _entry, _exit = _classify_pitroad_segments(segments)
    dashes = _main_pit_straight_dashes(dashes)
    return [_segment_centroid(seg) for seg in dashes]


def _resolve_track_segment(soup, pit_centroids: list[tuple[float, float]]):
    """Return (svgpathtools segment, layout_align key or None, warning or None)."""
    from svgpathtools import parse_path  # type: ignore

    active_layer = _find_class_element(soup, "active-config")
    pit_layer = _resolve_pit_path_element(soup)
    active_prefix = _layout_prefix(_svg_id_from_layer(active_layer))
    pit_prefix = _layout_prefix(_svg_id_from_layer(pit_layer))

    mismatch = (
        bool(active_prefix)
        and bool(pit_prefix)
        and active_prefix != pit_prefix
        and bool(pit_centroids)
    )

    if mismatch:
        inactive_svg = soup.find("svg", id="inactive")
        inactive_path = inactive_svg.find("path") if inactive_svg else None
        if inactive_path and inactive_path.get("d"):
            subpaths = parse_path(inactive_path.get("d")).continuous_subpaths()
            candidates = [
                sp for sp in subpaths
                if sp.length() >= _MIN_INACTIVE_SUBPATH_LEN
            ]
            if candidates:
                # Prefer the longest subpath among pit-near candidates.  Picking
                # only by minimum mean pit distance can select a short straight
                # beside the pit lane (Oschersleben inactive has several Zm
                # subpaths); the full outer loop is also pit-near but much longer.
                scored = [
                    (sp, _mean_pit_distance(sp, pit_centroids))
                    for sp in candidates
                ]
                min_dist = min(d for _, d in scored)
                margin = max(5.0, min_dist * 0.5)
                near = [sp for sp, d in scored if d <= min_dist + margin]
                segment = max(near, key=lambda sp: sp.length())
                warn = (
                    f"Layout mismatch {active_prefix} active / {pit_prefix} pit "
                    f"— using inactive subpath aligned to pit")
                return segment, "pit", warn

    active_path_element = _resolve_active_path_element(soup)
    if not active_path_element or not active_path_element.get("d"):
        raise ValueError(
            "Could not cleanly isolate the reference 'active-config' path segment.")
    path_geometry = parse_path(active_path_element.get("d"))
    subpaths = path_geometry.continuous_subpaths()
    if subpaths:
        segment = _pick_best_subpath(subpaths)
    else:
        segment = path_geometry
    if segment.length() < 1e-6:
        raise ValueError("Track path has zero length")
    return segment, None, None


def _sample_segment(segment, num_samples: int) -> list[list[float]]:
    if num_samples < 2:
        raise ValueError("num_samples must be >= 2")
    raw_points: list[list[float]] = []
    for i in range(num_samples):
        alpha = i / (num_samples - 1)
        pt = segment.point(alpha)
        raw_points.append([float(pt.real), float(pt.imag)])
    return raw_points


def _sample_path_d(path_d: str, num_samples: int) -> list[list[float]]:
    """Sample *num_samples* points along parsed SVG path *d*."""
    from svgpathtools import parse_path  # type: ignore

    path_geometry = parse_path(path_d)
    subpaths = path_geometry.continuous_subpaths()
    if subpaths:
        segment = _pick_best_subpath(subpaths)
    else:
        segment = path_geometry
    if segment.length() < 1e-6:
        raise ValueError("Track path has zero length")
    return _sample_segment(segment, num_samples)


def _resample_polyline(poly: list[tuple[float, float]],
                       n_samples: int) -> list[list[float]]:
    if len(poly) < 2 or n_samples < 2:
        return [[p[0], p[1]] for p in poly]
    lengths = [0.0]
    for a, b in zip(poly, poly[1:]):
        dx = b[0] - a[0]
        dy = b[1] - a[1]
        lengths.append(lengths[-1] + (dx * dx + dy * dy) ** 0.5)
    total = lengths[-1] or 1.0
    out: list[list[float]] = []
    for i in range(n_samples):
        target = (i / (n_samples - 1)) * total
        j = 1
        while j < len(lengths) and lengths[j] < target:
            j += 1
        j = min(j, len(poly) - 1)
        seg_len = lengths[j] - lengths[j - 1] or 1.0
        t = (target - lengths[j - 1]) / seg_len
        x = poly[j - 1][0] + t * (poly[j][0] - poly[j - 1][0])
        y = poly[j - 1][1] + t * (poly[j][1] - poly[j - 1][1])
        out.append([x, y])
    return out


def _pit_lane_raw_points(pit_element, pit_samples: int) -> list[list[float]]:
    """Build pit polyline from classified ``#Pitroad`` road dashes."""
    if pit_element is None or pit_samples < 2:
        return []

    pit_svg = str(pit_element.find("svg") or pit_element)
    segments = _segments_from_group(pit_svg, "Pitroad")
    if not segments:
        paths = pit_element.find_all("path")
        if paths:
            longest = max(paths, key=lambda p: len(p.get("d") or ""))
            d = longest.get("d")
            if d:
                return _sample_path_d(d, pit_samples)
        return []

    dashes, _entry, _exit = _classify_pitroad_segments(segments)
    dashes = _main_pit_straight_dashes(dashes)
    if not dashes:
        return []

    poly = _pit_path_from_dashes(dashes)
    if len(poly) < 2:
        return []
    if not _pit_lane_curved(poly):
        poly = _straighten_pit_path(poly)

    return _resample_polyline(poly, pit_samples)


def extract_track_and_pit_json(
    html_file: str | None = None,
    *,
    html_text: str | None = None,
    output_json: str | None = None,
    num_samples: int = 400,
    start_finish: float = 0.0,
) -> dict:
    """Extract main loop + pit lane by layer class (v2 spec)."""
    _require_v2_deps()
    from bs4 import BeautifulSoup  # type: ignore

    if html_text is not None:
        soup = BeautifulSoup(html_text, "html.parser")
    elif html_file is not None:
        with open(html_file, encoding="utf-8") as f:
            soup = BeautifulSoup(f.read(), "html.parser")
    else:
        raise ValueError("No HTML provided")

    pit_path_element = _resolve_pit_path_element(soup)
    pit_centroids = _pit_dash_centroids(pit_path_element)

    track_segment, layout_align, layout_warn = _resolve_track_segment(
        soup, pit_centroids)
    if layout_warn:
        print(f"Warning: {layout_warn}")

    track_points = _sample_segment(track_segment, num_samples)

    all_x = [p[0] for p in track_points]
    all_y = [p[1] for p in track_points]

    pit_samples = max(2, int(num_samples / 4))
    pit_points = _pit_lane_raw_points(pit_path_element, pit_samples)
    if pit_points:
        all_x += [p[0] for p in pit_points]
        all_y += [p[1] for p in pit_points]

    min_x, max_x = min(all_x), max(all_x)
    min_y, max_y = min(all_y), max(all_y)
    max_range = max(max_x - min_x, max_y - min_y) or 1.0

    normalized_track = []
    for x, y in track_points:
        norm_x = (x - min_x) / max_range
        norm_y = 1.0 - ((y - min_y) / max_range)
        normalized_track.append([round(norm_x, 7), round(norm_y, 7)])

    normalized_pit = []
    for x, y in pit_points:
        norm_x = (x - min_x) / max_range
        norm_y = 1.0 - ((y - min_y) / max_range)
        normalized_pit.append([round(norm_x, 7), round(norm_y, 7)])

    output_data: dict[str, Any] = {
        "schema": 2,
        "import_version": 2,
        "pit_source": "schematic",
        "start_finish": float(start_finish),
        "points": normalized_track,
    }
    if layout_align:
        output_data["layout_align"] = layout_align

    if normalized_pit:
        output_data["pit_lane_points"] = normalized_pit
        output_data["pit_path"] = normalized_pit
        loop = [(p[0], p[1]) for p in normalized_track]
        p0 = (normalized_pit[0][0], normalized_pit[0][1])
        p1 = (normalized_pit[-1][0], normalized_pit[-1][1])
        lane_lo = round(_pct_on_loop(loop, p0), 5)
        lane_hi = round(_pct_on_loop(loop, p1), 5)
        output_data["pit_span"] = [lane_lo, lane_hi]
        output_data["pit_in_pct"] = lane_lo
        output_data["pit_out_pct"] = lane_hi
        output_data["pit_speed"] = 22.0

    if output_json is not None:
        with open(output_json, "w", encoding="utf-8") as out_f:
            json.dump(output_data, out_f, indent=2)
            out_f.write("\n")
        print(f"Clean track map created successfully at: {output_json}")

    return output_data


def import_svg_html_v2(
    *,
    html_path: str | None = None,
    html_text: str | None = None,
    layer: str = "active-config",  # noqa: ARG001 — track always from active-config
    num_samples: int = 400,
    start_finish: float = 0.0,
    pad: float = 0.0,  # noqa: ARG001
) -> dict:
    return extract_track_and_pit_json(
        html_file=html_path,
        html_text=html_text,
        num_samples=num_samples,
        start_finish=start_finish,
    )


def import_track_source_v2(
    path: str,
    *,
    layer: str = "active-config",  # noqa: ARG001
    num_samples: int = 400,
    start_finish_override: float | None = None,
) -> dict:
    ext = os.path.splitext(path)[1].lower()
    sf = float(start_finish_override) if start_finish_override is not None else 0.0
    if ext in (".html", ".htm"):
        return extract_track_and_pit_json(
            html_file=path,
            num_samples=num_samples,
            start_finish=sf,
        )
    if ext == ".svg":
        text = _read_text(path) or ""
        wrapped = (
            '<div class="track-svg active-config">'
            f"<svg>{text}</svg></div>"
        )
        return extract_track_and_pit_json(
            html_text=wrapped,
            num_samples=num_samples,
            start_finish=sf,
        )
    raise ValueError(
        f"V2 import supports .html / .svg members exports, not {ext!r}")


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    ap.add_argument("source", help="members page .html or layer .svg")
    ap.add_argument("track_id", help="iRacing TrackID")
    ap.add_argument("name", help="track display name")
    ap.add_argument(
        "out_dir",
        nargs="?",
        default=paths.tracks_dir(),
        help="output directory (default: GridGlance tracks dir)",
    )
    ap.add_argument(
        "--samples",
        type=int,
        default=400,
        help="number of points along the track path (default: 400)",
    )
    ap.add_argument(
        "--start-finish-pct",
        type=float,
        default=0.0,
        help="start_finish lap fraction (default: 0.0)",
    )
    ap.add_argument("--force", action="store_true", help="overwrite existing file")
    args = ap.parse_args(argv)

    tid = (int(args.track_id) if str(args.track_id).lstrip("-").isdigit()
           else args.track_id)
    os.makedirs(args.out_dir, exist_ok=True)
    out_path = os.path.join(args.out_dir, f"{tid}.json")
    if os.path.exists(out_path) and not args.force:
        print(f"Refusing to overwrite {out_path} (use --force)")
        return 1

    doc = import_track_source_v2(
        args.source,
        num_samples=args.samples,
        start_finish_override=args.start_finish_pct,
    )
    doc["track_id"] = tid
    doc["name"] = args.name

    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, indent=2)
        fh.write("\n")

    n = len(doc.get("points") or [])
    n_pit = len(doc.get("pit_lane_points") or [])
    print(
        f"Clean track map created successfully at: {out_path} "
        f"({n} track pts, {n_pit} pit pts)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
