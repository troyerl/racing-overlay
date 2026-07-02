"""Tests for v2 BeautifulSoup + svgpathtools track import."""

from __future__ import annotations

import os
import sys

import pytest

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

bs4 = pytest.importorskip("bs4")
svgpathtools = pytest.importorskip("svgpathtools")

from tools.schematic_to_track import _bbox_span, _dist
from tools.svg_layers_to_track_v2 import (
    extract_track_and_pit_json,
    import_svg_html_v2,
    import_track_source_v2,
)

_TRACKS_HTML = os.path.join(_ROOT, "tracks-html")


def _html_path(*parts: str) -> str:
    return os.path.join(_TRACKS_HTML, *parts)


def _require_html(*parts: str) -> str:
    path = _html_path(*parts)
    if not os.path.isfile(path):
        pytest.skip(f"tracks-html fixture missing: {path}")
    return path


def test_v2_normalized_points_in_unit_square():
    doc = import_svg_html_v2(
        html_path=_require_html("Road", "Rudskogen Motorsenter.html"),
        num_samples=300,
    )
    pts = doc["points"]
    assert doc.get("import_version") == 2
    assert len(pts) == 300
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    assert min(xs) >= -0.001
    assert min(ys) >= -0.001
    assert max(xs) <= 1.001
    assert max(ys) <= 1.001
    assert max(xs) - min(xs) > 0.5 or max(ys) - min(ys) > 0.5


def test_v2_includes_pit_lane_from_pit_layer():
    doc = extract_track_and_pit_json(
        html_file=_require_html("Road", "Motorsport Arena Oschersleben.html"),
        num_samples=400,
    )
    assert len(doc["points"]) == 400
    assert len(doc.get("pit_lane_points") or []) >= 2
    assert doc.get("pit_path") == doc.get("pit_lane_points")
    pit_span = doc.get("pit_span")
    assert isinstance(pit_span, list) and len(pit_span) == 2
    assert all(isinstance(v, (int, float)) and 0.0 <= float(v) <= 1.0 for v in pit_span)
    assert doc.get("pit_in_pct") == pit_span[0]
    assert doc.get("pit_out_pct") == pit_span[1]
    assert doc.get("pit_speed") == 22.0
    assert doc.get("layout_align") == "pit"


def _min_dist_to_loop(pt, loop):
    return min(_dist(pt, q) for q in loop)


def test_v2_oschersleben_pit_near_track():
    doc = extract_track_and_pit_json(
        html_file=_require_html("Road", "Motorsport Arena Oschersleben.html"),
        num_samples=400,
    )
    loop = doc["points"]
    pit = doc.get("pit_path") or []
    assert pit
    dists = [_min_dist_to_loop(p, loop) for p in pit]
    assert max(dists) < 0.01
    assert sum(dists) / len(dists) < 0.008
    pit_xs = [p[0] for p in pit]
    assert min(pit_xs) > 0.45
    assert all(pit_xs[i] <= pit_xs[i + 1] for i in range(len(pit_xs) - 1))


def test_load_track_maps_pit_lane_points_to_pit_path(tmp_path):
    from overlay.widgets.track_map import load_track

    track = {
        "points": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        "pit_lane_points": [[0.1, 0.1], [0.2, 0.2]],
        "pit_source": "schematic",
    }
    path = tmp_path / "test.json"
    path.write_text(__import__("json").dumps(track), encoding="utf-8")
    _, _, _, _, meta = load_track(str(path), n=4)
    assert meta.get("pit_path") == [(0.1, 0.1), (0.2, 0.2)]
    assert "pit_path" not in track  # source file unchanged


def test_v2_y_axis_flipped():
    from bs4 import BeautifulSoup
    from svgpathtools import parse_path
    from tools.svg_layers_to_track_v2 import _resolve_active_path_element

    html = open(_require_html("Road", "Rudskogen Motorsenter.html")).read()
    soup = BeautifulSoup(html, "html.parser")
    path_el = _resolve_active_path_element(soup)
    geom = parse_path(path_el.get("d"))
    subs = geom.continuous_subpaths()
    segment = max(subs, key=lambda sp: sp.length()) if subs else geom
    raw_y = [segment.point(i / 99).imag for i in range(100)]
    doc = import_svg_html_v2(html_text=html, num_samples=100)
    out_y = [p[1] for p in doc["points"]]
    if raw_y[0] != raw_y[-1]:
        assert (raw_y[0] > raw_y[50]) == (out_y[0] < out_y[50])


def test_v2_import_track_source_html():
    doc = import_track_source_v2(
        _require_html("Road", "Circuito de Navarra.html"),
        num_samples=200,
    )
    assert len(doc["points"]) == 200


def test_v2_oschersleben_track_no_long_chords():
    doc = extract_track_and_pit_json(
        html_file=_require_html("Road", "Motorsport Arena Oschersleben.html"),
        num_samples=400,
    )
    span = _bbox_span(doc["points"])
    assert span > 0.4
    jumps = [
        _dist(doc["points"][i], doc["points"][i + 1])
        for i in range(len(doc["points"]) - 1)
    ]
    assert max(jumps) < 0.08
