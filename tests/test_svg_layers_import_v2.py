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
