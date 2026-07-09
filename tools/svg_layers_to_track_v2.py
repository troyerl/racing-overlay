#!/usr/bin/env python3
"""V2 track map import — active-config loop only (manual pit authoring).

Reads the members-site ``active-config`` SVG path via BeautifulSoup + svgpathtools,
normalizes the racing loop to 0–1 coordinates, and leaves pit geometry for the
Track Scan manual pit editor (pit_source: manual).

Usage::

    python3 tools/svg_layers_to_track_v2.py page.html <TrackID> "Track Name" [out_dir] --force

Requires: ``pip install -r requirements-dev.txt`` (beautifulsoup4, svgpathtools)
"""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import sys
from typing import Any

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import paths, svgpath
from tools.schematic_to_track import (
    _ensure_ccw,
    _reorder_loop,
)
from tools.svg_layers_to_track import (
    _detect_sf_svg,
    _parse_turn_numbers,
    _sf_anchor_point,
    _sf_paths_sorted,
    _sf_stripe_centroid,
    _sf_stripe_crossing,
    extract_layers_from_html,
)

_SUBPATH_LENGTH_TIE_FRAC = 0.03
_TRACK_MAP_ID_RE = re.compile(r"^track-map-(\d+)$", re.I)
_TRACK_MAP_ID_ATTR_RE = re.compile(
    r"""id\s*=\s*["']track-map-(\d+)["']""", re.I)


def _read_text(path: str | None) -> str | None:
    if not path:
        return None
    try:
        with open(path, "rb") as fh:
            raw = fh.read()
    except OSError:
        return None
    for enc in ("utf-8-sig", "utf-16", "utf-16-le", "utf-16-be", "cp1252", "latin-1"):
        try:
            return raw.decode(enc)
        except (UnicodeDecodeError, LookupError):
            continue
    return raw.decode("utf-8", errors="replace")


def parse_track_id_from_html(
    *,
    html_path: str | None = None,
    html_text: str | None = None,
    regex_only: bool = False,
) -> int | None:
    """Read iRacing TrackID from a members page ``id=\"track-map-123\"`` wrapper.

    When ``regex_only`` is True, skip BeautifulSoup (fast path for file browse).
    """
    if html_text is None:
        if not html_path:
            return None
        html_text = _read_text(html_path) or ""

    m = _TRACK_MAP_ID_ATTR_RE.search(html_text)
    if m:
        return int(m.group(1))

    if regex_only:
        return None

    try:
        _require_v2_deps()
    except ImportError:
        return None

    from bs4 import BeautifulSoup  # type: ignore

    soup = BeautifulSoup(html_text, "html.parser")
    for tag in soup.find_all(id=True):
        mid = _TRACK_MAP_ID_RE.match(str(tag.get("id", "")).strip())
        if mid:
            return int(mid.group(1))
    return None


def _require_v2_deps():
    try:
        from bs4 import BeautifulSoup  # noqa: F401
        from svgpathtools import parse_path  # noqa: F401
    except ImportError as exc:
        raise ImportError(
            "V2 import requires beautifulsoup4 and svgpathtools — "
            "run: pip install beautifulsoup4 svgpathtools") from exc


def _sf_arrow_direction(sf_svg: str | None) -> tuple[float, float] | None:
    """Unit vector of the start-finish direction arrow (SVG coords, Y down)."""
    paths = _sf_paths_sorted(sf_svg)
    if len(paths) < 2:
        return None
    arrow_pts = paths[-1]
    stripe = _sf_stripe_centroid(sf_svg)
    if stripe is None:
        return None
    sfx, sfy = stripe
    best_near, best_far = 1e18, -1.0
    near_pt = far_pt = arrow_pts[0]
    for p in arrow_pts:
        d = math.hypot(p[0] - sfx, p[1] - sfy)
        if d < best_near:
            best_near, near_pt = d, p
        if d > best_far:
            best_far, far_pt = d, p
    dx, dy = far_pt[0] - near_pt[0], far_pt[1] - near_pt[1]
    ln = math.hypot(dx, dy)
    if ln < 1e-6:
        return None
    return dx / ln, dy / ln


def _align_loop_from_sf(
    raw_points: list[list[float]],
    sf_svg: str | None,
) -> list[tuple[float, float]]:
    """Rotate to S/F at index 0 and match members arrow driving direction."""
    loop = [(float(p[0]), float(p[1])) for p in raw_points]
    if len(loop) < 3:
        return loop
    sf_idx = _detect_sf_svg(loop, sf_svg)
    loop = _reorder_loop(loop, sf_idx)
    arrow = _sf_arrow_direction(sf_svg)
    if arrow and len(loop) >= 2:
        dx = loop[1][0] - loop[0][0]
        dy = loop[1][1] - loop[0][1]
        ln = math.hypot(dx, dy) or 1.0
        if (dx / ln) * arrow[0] + (dy / ln) * arrow[1] < 0:
            loop = [loop[0]] + list(reversed(loop[1:]))
    elif not arrow:
        loop = _ensure_ccw(loop)
    crossing = _sf_stripe_crossing(loop, sf_svg)
    if crossing is not None:
        _, pt = crossing
        loop[0] = pt
    return loop


def _find_class_element(soup, class_token: str):
    for tag in soup.find_all(True):
        classes = tag.get("class") or []
        if isinstance(classes, str):
            classes = classes.split()
        if class_token in classes:
            return tag
    return None


def _find_path_by_class(soup, class_token: str):
    return soup.find(
        lambda tag: tag.name == "path"
        and tag.has_attr("class")
        and any(class_token in c for c in tag.get("class", [])),
    )


def _resolve_active_path_element(soup):
    """Return the active-config layout path (members ``div.track-svg`` layer)."""
    active_path_element = _find_class_element(soup, "active-config")
    if active_path_element is not None and active_path_element.name != "path":
        subpaths = active_path_element.find_all("path")
        if subpaths:
            active_path_element = max(
                subpaths, key=lambda p: len(p.get("d") or ""))

    if active_path_element is None or not active_path_element.get("d"):
        active_path_element = _find_path_by_class(soup, "active-config")

    inactive_svg = soup.find("svg", id="inactive")
    if inactive_svg and (
        active_path_element is None or not active_path_element.get("d")
    ):
        active_path_element = inactive_svg.find("path", class_="cls-1")

    return active_path_element


def _subpath_bbox_area(segment) -> float:
    pts = [segment.point(i / 19) for i in range(20)]
    xs = [p.real for p in pts]
    ys = [p.imag for p in pts]
    return (max(xs) - min(xs)) * (max(ys) - min(ys))


def _pick_best_subpath(subpaths: list) -> Any:
    if not subpaths:
        raise ValueError("No track subpaths found")
    best_len = max(sp.length() for sp in subpaths)
    candidates = [
        sp for sp in subpaths
        if sp.length() >= best_len * (1.0 - _SUBPATH_LENGTH_TIE_FRAC)
    ]
    return max(candidates, key=lambda sp: (sp.length(), _subpath_bbox_area(sp)))


def _resolve_loop_segment(soup):
    from svgpathtools import parse_path  # type: ignore

    active_path_element = _resolve_active_path_element(soup)
    if not active_path_element or not active_path_element.get("d"):
        raise ValueError(
            "Could not isolate the reference 'active-config' path segment.")
    path_geometry = parse_path(active_path_element.get("d"))
    subpaths = path_geometry.continuous_subpaths()
    segment = _pick_best_subpath(subpaths) if subpaths else path_geometry
    if segment.length() < 1e-6:
        raise ValueError("Track path has zero length")
    return segment


def _sample_segment(segment, num_samples: int) -> list[list[float]]:
    if num_samples < 2:
        raise ValueError("num_samples must be >= 2")
    raw_points: list[list[float]] = []
    for i in range(num_samples):
        alpha = i / (num_samples - 1)
        pt = segment.point(alpha)
        raw_points.append([float(pt.real), float(pt.imag)])
    return raw_points


def _normalize_loop(raw_points: list[list[float]]) -> tuple[list[list[float]],
                                                           tuple[float, float, float]]:
    all_x = [p[0] for p in raw_points]
    all_y = [p[1] for p in raw_points]
    min_x, max_x = min(all_x), max(all_x)
    min_y, max_y = min(all_y), max(all_y)
    max_range = max(max_x - min_x, max_y - min_y) or 1.0
    normalized = []
    for x, y in raw_points:
        norm_x = (x - min_x) / max_range
        norm_y = (y - min_y) / max_range
        normalized.append([round(norm_x, 7), round(norm_y, 7)])
    return normalized, (min_x, min_y, max_range)


def import_loop_from_html(
    *,
    html_path: str | None = None,
    html_text: str | None = None,
    num_samples: int = 400,
    num_corners: int = 4,
    start_finish: float = 0.0,
) -> dict:
    """Extract racing loop from members HTML; no pit geometry."""
    _require_v2_deps()
    from bs4 import BeautifulSoup  # type: ignore

    if html_text is not None:
        html = html_text
        soup = BeautifulSoup(html_text, "html.parser")
    elif html_path is not None:
        html = _read_text(html_path) or ""
        soup = BeautifulSoup(html, "html.parser")
    else:
        raise ValueError("No HTML provided")

    segment = _resolve_loop_segment(soup)
    raw_points = _sample_segment(segment, num_samples)

    layers = extract_layers_from_html(html) if html else {}
    sf_svg = layers.get("start_finish")
    turns_svg = layers.get("turns")

    aligned = _align_loop_from_sf(raw_points, sf_svg)
    normalized, norm = _normalize_loop(
        [[p[0], p[1]] for p in aligned])
    loop = [(p[0], p[1]) for p in normalized]

    corners: list[dict] = []
    if turns_svg:
        corners = _parse_turn_numbers(turns_svg, loop, norm, flip_y=False)
    elif num_corners:
        from tools.schematic_to_track import _oval_corners
        corners = _oval_corners(loop, num_corners)

    n_turns = len(corners) if corners else (num_corners or None)
    if corners and n_turns and n_turns >= 2:
        from overlay.widgets.track_map import TrackMapWidget
        corners = [
            {**c, "label": TrackMapWidget._iracing_oval_label(c["label"], n_turns)}
            for c in corners
            if c.get("label") is not None
        ]

    return {
        "schema": 2,
        "import_version": 2,
        "pit_source": "manual",
        "start_finish": 0.0,
        "points": normalized,
        "corners": corners,
        "num_turns": n_turns,
    }


def import_svg_html_v2(
    *,
    html_path: str | None = None,
    html_text: str | None = None,
    num_samples: int = 400,
    num_corners: int = 4,
    start_finish: float = 0.0,
    **_,
) -> dict:
    return import_loop_from_html(
        html_path=html_path,
        html_text=html_text,
        num_samples=num_samples,
        num_corners=num_corners,
        start_finish=start_finish,
    )


def import_track_source_v2(
    path: str,
    *,
    num_samples: int = 400,
    num_corners: int = 4,
    start_finish_override: float | None = None,
    **_,
) -> dict:
    ext = os.path.splitext(path)[1].lower()
    sf = float(start_finish_override) if start_finish_override is not None else 0.0
    if ext in (".html", ".htm"):
        return import_loop_from_html(
            html_path=path,
            num_samples=num_samples,
            num_corners=num_corners,
            start_finish=sf,
        )
    if ext == ".svg":
        text = _read_text(path) or ""
        wrapped = (
            '<div class="track-svg active-config">'
            f"<svg>{text}</svg></div>"
        )
        return import_loop_from_html(
            html_text=wrapped,
            num_samples=num_samples,
            num_corners=num_corners,
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
    ap.add_argument("--samples", type=int, default=400,
                    help="points along the loop (default: 400)")
    ap.add_argument("--corners", type=int, default=4,
                    help="auto corners when turn-numbers missing (0=skip)")
    ap.add_argument("--start-finish-pct", type=float, default=0.0)
    ap.add_argument("--force", action="store_true")
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
        num_corners=args.corners,
        start_finish_override=args.start_finish_pct,
    )
    doc["track_id"] = tid
    doc["name"] = args.name

    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(doc, fh, indent=2)
        fh.write("\n")

    n = len(doc.get("points") or [])
    print(f"Wrote {out_path} — {n} loop pts (pit: draw manually in Track Scan)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
