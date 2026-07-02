"""Tests for v2 loop-only HTML import (manual pit authoring)."""

from __future__ import annotations

import os
import sys

import pytest

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

pytest.importorskip("bs4")
pytest.importorskip("svgpathtools")

from tools.schematic_to_track import _dist
from tools.svg_layers_to_track_v2 import (
    import_loop_from_html,
    import_svg_html_v2,
    import_track_source_v2,
)

_FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures")
_TRACKS_HTML = os.path.join(_ROOT, "tracks-html")


def _html_path(*parts: str) -> str:
    return os.path.join(_TRACKS_HTML, *parts)


def _fixture(name: str) -> str:
    path = os.path.join(_FIXTURES, name)
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def _require_html(*parts: str) -> str:
    path = _html_path(*parts)
    if not os.path.isfile(path):
        pytest.skip(f"tracks-html fixture missing: {path}")
    return path


def test_v2_loop_only_no_pit_path():
    doc = import_loop_from_html(
        html_text=_fixture("compound_oval.html"),
        num_samples=120,
        num_corners=4,
    )
    assert doc.get("schema") == 2
    assert doc.get("import_version") == 2
    assert doc.get("pit_source") == "manual"
    assert "pit_path" not in doc
    assert "pit_in" not in doc
    assert "pit_out" not in doc
    assert len(doc["points"]) == 120


def test_v2_normalized_points_in_unit_square():
    doc = import_svg_html_v2(
        html_path=_require_html("Road", "Rudskogen Motorsenter.html"),
        num_samples=300,
    )
    pts = doc["points"]
    assert doc.get("import_version") == 2
    assert doc.get("pit_source") == "manual"
    assert "pit_path" not in doc
    assert len(pts) == 300
    xs = [p[0] for p in pts]
    ys = [p[1] for p in pts]
    assert min(xs) >= -0.001
    assert min(ys) >= -0.001
    assert max(xs) <= 1.001
    assert max(ys) <= 1.001
    assert max(xs) - min(xs) > 0.5 or max(ys) - min(ys) > 0.5


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
    assert doc.get("pit_source") == "manual"
    assert "pit_path" not in doc


def test_v2_compound_oval_picks_outer_loop():
    doc = import_loop_from_html(
        html_text=_fixture("compound_oval.html"),
        num_samples=80,
    )
    pts = doc["points"]
    jumps = [_dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1)]
    assert max(jumps) < 0.15


def test_v2_parse_track_id_from_html():
    from tools.svg_layers_to_track_v2 import parse_track_id_from_html

    assert parse_track_id_from_html(html_text=_fixture("rudskogen_pit.html")) == 451
    assert parse_track_id_from_html(
        html_path=_require_html("Oval", "Chicagoland.html")) == 123
    assert parse_track_id_from_html(html_text=_fixture("compound_oval.html")) is None


def test_v2_chicagoland_sf_direction_and_turn_labels():
    from bs4 import BeautifulSoup

    from tools.schematic_to_track import _pct_on_loop
    from tools.svg_layers_to_track import (
        _sf_anchor_point,
        _sf_stripe_centroid,
        extract_layers_from_html,
    )
    from tools.svg_layers_to_track_v2 import (
        _align_loop_from_sf,
        _normalize_loop,
        _resolve_loop_segment,
        _sample_segment,
    )

    path = _require_html("Oval", "Chicagoland.html")
    html = open(path, encoding="utf-8").read()
    soup = BeautifulSoup(html, "html.parser")
    segment = _resolve_loop_segment(soup)
    raw = _sample_segment(segment, 400)
    layers = extract_layers_from_html(html)
    aligned = _align_loop_from_sf(raw, layers.get("start_finish"))
    normalized, norm = _normalize_loop([[p[0], p[1]] for p in aligned])
    loop = [(p[0], p[1]) for p in normalized]

    doc = import_loop_from_html(html_path=path, num_samples=400)

    assert doc["start_finish"] == 0.0
    assert len(doc["corners"]) == 4
    assert {c["label"] for c in doc["corners"]} == {"1", "2", "3", "4"}
    # Members ovals: turn 4 sits just after S/F when following the arrow (same as v1).
    assert [c["label"] for c in doc["corners"]] == ["4", "3", "2", "1"]

    sf_svg = layers["start_finish"]
    anchor = _sf_anchor_point(sf_svg)
    stripe = _sf_stripe_centroid(sf_svg)
    assert anchor is not None and stripe is not None and anchor == stripe
    min_x, min_y, span = norm
    anchor_norm = (
        (anchor[0] - min_x) / span,
        1.0 - ((anchor[1] - min_y) / span),
    )
    sf_pct = _pct_on_loop(loop, anchor_norm)
    assert sf_pct < 0.05 or sf_pct > 0.95

    pcts = [c["pct"] for c in doc["corners"]]
    assert pcts == sorted(pcts)
    assert all(pcts[i] < pcts[i + 1] for i in range(len(pcts) - 1))


def test_v2_indianapolis_sf_stripe_anchor():
    from bs4 import BeautifulSoup

    from tools.schematic_to_track import _dist, _pct_on_loop
    from tools.svg_layers_to_track import (
        _sf_anchor_point,
        _sf_arrow_tip,
        _sf_stripe_centroid,
        extract_layers_from_html,
    )
    from tools.svg_layers_to_track_v2 import (
        _align_loop_from_sf,
        _normalize_loop,
        _resolve_loop_segment,
        _sample_segment,
    )

    path = _require_html("Oval", "Indianapolis Motor Speedway.html")
    html = open(path, encoding="utf-8").read()
    soup = BeautifulSoup(html, "html.parser")
    segment = _resolve_loop_segment(soup)
    raw = _sample_segment(segment, 400)
    layers = extract_layers_from_html(html)
    sf_svg = layers["start_finish"]
    aligned = _align_loop_from_sf(raw, sf_svg)
    normalized, norm = _normalize_loop([[p[0], p[1]] for p in aligned])
    loop = [(p[0], p[1]) for p in normalized]

    doc = import_loop_from_html(html_path=path, num_samples=400)
    assert doc["start_finish"] == 0.0

    stripe = _sf_stripe_centroid(sf_svg)
    tip = _sf_arrow_tip(sf_svg)
    anchor = _sf_anchor_point(sf_svg)
    assert stripe is not None and tip is not None and anchor == stripe
    min_x, min_y, span = norm
    loop_pt = loop[0]

    def _to_norm(pt):
        return (
            (pt[0] - min_x) / span,
            1.0 - ((pt[1] - min_y) / span),
        )

    stripe_norm = _to_norm(stripe)
    tip_norm = _to_norm(tip)
    assert loop_pt[0] < tip_norm[0]  # stripe is left of direction arrow
    assert abs(loop_pt[0] - stripe_norm[0]) < 0.02
    assert _dist(loop_pt, stripe_norm) < 0.06

    sf_pct = _pct_on_loop(loop, stripe_norm)
    assert sf_pct < 0.05 or sf_pct > 0.95
    assert tip[0] > stripe[0]
