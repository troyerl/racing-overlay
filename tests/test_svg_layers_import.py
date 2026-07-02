"""Regression tests for members-site SVG layer import."""

from __future__ import annotations

import os
import sys

_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)

from overlay import svgpath
from tools.schematic_to_track import _bbox_span, _dist
from tools.svg_layers_to_track import import_svg_layers, _distance_to_polyline

_FIXTURES = os.path.join(os.path.dirname(__file__), "fixtures")
_TRACKS_HTML = os.path.join(_ROOT, "tracks-html")


def _html_path(*parts: str) -> str:
    return os.path.join(_TRACKS_HTML, *parts)


def _fixture(name: str) -> str:
    path = os.path.join(_FIXTURES, name)
    with open(path, encoding="utf-8") as fh:
        return fh.read()


def _max_segment_span(pts: list[list[float]]) -> float:
    if len(pts) < 2:
        return 0.0
    return max(_dist((a[0], a[1]), (b[0], b[1])) for a, b in zip(pts, pts[1:]))


def _max_jump_span(pts: list[list[float]]) -> float:
    return _max_segment_span(pts)


def test_split_subpaths_isolates_compound_path():
    d = (
        "M960,120 C1500,120 1820,400 1820,540 C1820,820 1500,960 960,960 Z "
        "M960,220 C1380,220 1680,420 1680,540 C1680,760 1380,880 960,880 Z "
        "M960,300 C1280,300 1520,440 1520,540 C1520,700 1280,820 960,820 Z"
    )
    subs = svgpath.split_subpaths(d)
    assert len(subs) == 3
    flat = svgpath.flatten_path(d)
    assert len(subs[0]) >= len(subs[1]) >= len(subs[2])
    # Flattening the whole string ties subpaths together (extra points / chords).
    assert len(flat) > len(subs[0])


def _pit_path_y_span(pit_path: list[list[float]]) -> float:
    ys = [p[1] for p in pit_path]
    return max(ys) - min(ys)


def _pit_straight_y_std(
    doc: dict,
    x_min: float = 0.65,
    x_max: float = 1.0,
) -> float:
    """Std dev of pit_path Y inside an X band (flat straight noise metric)."""
    pit_path = doc.get("pit_path") or []
    ys = [p[1] for p in pit_path if x_min <= p[0] <= x_max]
    if len(ys) < 3:
        return float("inf")
    mean = sum(ys) / len(ys)
    return (sum((y - mean) ** 2 for y in ys) / len(ys)) ** 0.5


def _median_pit_loop_gap(pit_path: list[list[float]], loop: list[list[float]]) -> float:
    loop_pts = [(p[0], p[1]) for p in loop]
    dists = [
        min(_dist((x, y), q) for q in loop_pts)
        for x, y in ((p[0], p[1]) for p in pit_path)
    ]
    if not dists:
        return 0.0
    dists.sort()
    mid = len(dists) // 2
    return dists[mid] if len(dists) % 2 else (dists[mid - 1] + dists[mid]) / 2


def _loop_near_count(pit_out: list[list[float]], loop: list[list[float]],
                     thresh: float = 0.015, *, skip_head: int = 4,
                     skip_tail: int = 5) -> int:
    loop_pts = [(p[0], p[1]) for p in loop]
    end = max(skip_head, len(pit_out) - skip_tail)
    body = pit_out[skip_head:end]
    return sum(
        1 for p in body
        if min(_dist((p[0], p[1]), q) for q in loop_pts) <= thresh
    )


def _assert_pit_spacing(doc: dict) -> None:
    loop = doc["points"]
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    assert _median_pit_loop_gap(pit_path, loop) >= 0.022
    span = _bbox_span([(p[0], p[1]) for p in loop])
    assert _max_segment_span(pit_out) < span * 0.10
    assert _loop_near_count(pit_out, loop) <= 2


def _pit_out_colinear_overlap_count(doc: dict) -> int:
    pit_path = [(p[0], p[1]) for p in doc["pit_path"]]
    pit_out = [(p[0], p[1]) for p in doc["pit_out"]]
    loop = [(p[0], p[1]) for p in doc["points"]]
    if len(pit_out) < 1:
        return 0
    span = _bbox_span(loop) or 1.0
    y_eps = span * 0.008
    x_buf = span * 0.005
    handoff = pit_path[0]
    lane_tol = span * 0.01
    return sum(
        1 for p in pit_out
        if (p[0] < handoff[0] - x_buf
            and abs(p[1] - handoff[1]) <= y_eps
            and _distance_to_polyline(p, pit_path) < lane_tol)
    )


def _assert_road_course_merge(doc: dict) -> None:
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    path_len = sum(
        _dist((a[0], a[1]), (b[0], b[1]))
        for a, b in zip(pit_path, pit_path[1:])
    )
    max_seg = _max_segment_span(pit_out)
    assert max_seg < path_len * 0.35


def _assert_pit_joins(doc: dict) -> None:
    pit_in = doc.get("pit_in") or []
    pit_path = doc.get("pit_path") or []
    pit_out = doc.get("pit_out") or []
    if len(pit_in) >= 2 and len(pit_path) >= 2:
        if doc.get("pit_source") == "inactive":
            join_pt = min(
                pit_path,
                key=lambda p: _dist((pit_in[-1][0], pit_in[-1][1]), (p[0], p[1])),
            )
            assert _dist((pit_in[-1][0], pit_in[-1][1]),
                         (join_pt[0], join_pt[1])) < 0.02
        else:
            assert _dist((pit_in[-1][0], pit_in[-1][1]),
                         (pit_path[0][0], pit_path[0][1])) < 0.02
    if len(pit_path) >= 2 and len(pit_out) >= 2:
        if doc.get("pit_source") == "inactive":
            join_pt = min(
                pit_path,
                key=lambda p: _dist((p[0], p[1]), (pit_out[0][0], pit_out[0][1])),
            )
            assert _dist((join_pt[0], join_pt[1]),
                         (pit_out[0][0], pit_out[0][1])) < 0.02
        else:
            d0 = _dist((pit_path[0][0], pit_path[0][1]),
                       (pit_out[0][0], pit_out[0][1]))
            d1 = _dist((pit_path[-1][0], pit_path[-1][1]),
                       (pit_out[0][0], pit_out[0][1]))
            x_span = max(p[0] for p in pit_path) - min(p[0] for p in pit_path)
            join_tol = 0.15 if x_span > 0.30 else 0.02
            assert min(d0, d1) < join_tol


def _pit_out_y_span(pit_out: list[list[float]]) -> float:
    ys = [p[1] for p in pit_out]
    return max(ys) - min(ys)


def test_compound_oval_exit_chevron_transition():
    doc = import_svg_layers(html_text=_fixture("compound_oval.html"), num_corners=4)
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    loop = doc["points"]
    loop_span = _bbox_span([(p[0], p[1]) for p in loop])
    # Red pit road ends at the right end of the straight (last dash ~1460), not at chevron (~1370).
    assert pit_path[-1][0] > 0.68
    _assert_pit_joins(doc)
    _assert_pit_spacing(doc)
    assert _pit_out_y_span(pit_out) > loop_span * 0.20
    assert len(pit_out) == 40


def test_members_oval_import():
    doc = import_svg_layers(html_text=_fixture("members_oval.html"), num_corners=4)
    loop = doc["points"]
    pit_out = doc["pit_out"]
    pit_path = doc["pit_path"]
    span = _bbox_span([(p[0], p[1]) for p in loop])
    assert _max_segment_span(loop) < span * 0.15
    assert len(pit_out) == 40
    assert _bbox_span([(p[0], p[1]) for p in pit_out]) >= _bbox_span(
        [(p[0], p[1]) for p in pit_path]) * 0.12
    assert _max_jump_span(pit_out) < span * 0.20
    assert _pit_path_y_span(pit_path) < 0.011
    _assert_pit_joins(doc)
    _assert_pit_spacing(doc)


def test_compound_oval_no_slash():
    doc = import_svg_layers(html_text=_fixture("compound_oval.html"), num_corners=4)
    loop = doc["points"]
    pit_out = doc["pit_out"]
    pit_path = doc["pit_path"]
    span = _bbox_span([(p[0], p[1]) for p in loop])
    assert len(loop) >= 100
    assert _max_segment_span(loop) < span * 0.15
    assert len(pit_out) == 40
    assert _bbox_span([(p[0], p[1]) for p in pit_out]) >= _bbox_span(
        [(p[0], p[1]) for p in pit_path]) * 0.12
    assert _max_jump_span(pit_out) < span * 0.20
    assert _pit_path_y_span(pit_path) < 0.01
    _assert_pit_joins(doc)
    _assert_pit_spacing(doc)
    pit_in = doc["pit_in"]
    assert _pit_path_y_span(pit_in) > 0.02


def test_merge_ticks_use_centroids_not_scribble():
    doc = import_svg_layers(html_text=_fixture("compound_oval.html"), num_corners=4)
    pit_out = doc["pit_out"]
    # Merge should trace a mostly monotonic arc (y decreases from pit exit to T2).
    ys = [p[1] for p in pit_out]
    drops = sum(1 for a, b in zip(ys, ys[1:]) if b < a - 0.001)
    assert drops >= len(ys) * 0.35


def test_merge_straight_ticks_short_blue():
    doc = import_svg_layers(html_text=_fixture("merge_straight_ticks.html"), num_corners=4)
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    handoff_x = pit_path[-1][0]
    assert pit_out[0][0] >= handoff_x - 0.02
    assert min(p[0] for p in pit_out) >= handoff_x - 0.02
    on_straight = sum(1 for p in pit_out if abs(p[1] - pit_path[-1][1]) < 0.02)
    assert on_straight <= 5
    assert _pit_out_y_span(pit_out) > 0.06
    _assert_pit_joins(doc)
    _assert_pit_spacing(doc)


def test_rudskogen_road_course_pit():
    doc = import_svg_layers(html_text=_fixture("rudskogen_pit.html"), num_corners=14)
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    assert len(pit_path) >= 20
    assert _pit_path_y_span(pit_path) > 0.008
    assert pit_path[-1][0] > pit_path[0][0]
    assert _dist((pit_path[0][0], pit_path[0][1]),
                 (pit_out[0][0], pit_out[0][1])) < 0.02
    assert _dist((pit_path[-1][0], pit_path[-1][1]),
                 (pit_out[0][0], pit_out[0][1])) > 0.05
    assert _pit_out_y_span(pit_out) > 0.035
    assert _max_segment_span(pit_out[3:]) < 0.02
    assert _pit_out_colinear_overlap_count(doc) <= 2
    _assert_pit_joins(doc)
    _assert_road_course_merge(doc)


def test_chicagoland_real_html():
    doc = import_svg_layers(
        html_path=_html_path("Oval", "Chicagoland.html"), num_corners=4)
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    assert len(pit_out) == 40
    assert _dist((pit_path[-1][0], pit_path[-1][1]),
                 (pit_out[0][0], pit_out[0][1])) < 0.02
    _assert_pit_joins(doc)
    _assert_pit_spacing(doc)


def test_oschersleben_real_html():
    doc = import_svg_layers(
        html_path=_html_path("Road", "Motorsport Arena Oschersleben.html"),
        num_corners=14,
    )
    loop = doc["points"]
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    for pts in (loop, pit_path, pit_out):
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        assert min(xs) >= -0.05 and max(xs) <= 1.05
        assert min(ys) >= -0.05 and max(ys) <= 1.05
    assert max(p[0] for p in pit_path) - min(p[0] for p in pit_path) > 0.15
    assert _median_pit_loop_gap(pit_path, loop) >= 0.016
    back_loop = [p for p in loop if p[0] > 0.55]
    back_pit = [p for p in pit_path if p[0] > 0.55]
    assert back_loop and back_pit
    loop_med_y = sorted(p[1] for p in back_loop)[len(back_loop) // 2]
    pit_med_y = sorted(p[1] for p in back_pit)[len(back_pit) // 2]
    # v1 normalize keeps SVG Y (increases downward): inner lane is above pit → lower norm-y.
    assert loop_med_y < pit_med_y - 0.01
    assert max(p[1] for p in back_loop) < max(p[1] for p in back_pit) + 0.04
    assert doc["pit_source"] == "inactive"
    _assert_pit_joins(doc)


def test_okayama_real_html():
    doc = import_svg_layers(
        html_path=_html_path("Road", "Okayama International Circuit.html"),
        num_corners=14,
    )
    loop = doc["points"]
    pit_path = doc["pit_path"]
    pit_out = doc["pit_out"]
    for pts in (loop, pit_path, pit_out):
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        assert min(xs) >= -0.05 and max(xs) <= 1.05
        assert min(ys) >= -0.05 and max(ys) <= 1.05
    assert max(p[0] for p in pit_path) - min(p[0] for p in pit_path) > 0.15
    assert _median_pit_loop_gap(pit_path, loop) >= 0.015
    # High-x straight only; closed loop crosses the same X at multiple Y values.
    back_pit = [p for p in pit_path if p[0] > 0.80]
    assert back_pit
    pit_med_y = sorted(p[1] for p in back_pit)[len(back_pit) // 2]
    back_loop = [p for p in loop if p[0] > 0.80 and p[1] > pit_med_y]
    assert len(back_loop) >= 10
    loop_med_y = sorted(p[1] for p in back_loop)[len(back_loop) // 2]
    # Pit is the inner lane here: track runs outside (below) pit on the back straight.
    assert loop_med_y > pit_med_y + 0.01
    assert min(p[1] for p in back_loop) > min(p[1] for p in back_pit) - 0.02
    assert _pit_straight_y_std(doc, x_min=0.75, x_max=0.85) < 0.002
    assert _pit_path_y_span(pit_path) > 0.02
    assert max(p[0] for p in pit_path) > 0.80
    hook_pts = [p for p in pit_path if p[1] < pit_med_y - 0.005]
    assert hook_pts, "entry hook should rise above back straight median Y"
    pit_in = doc["pit_in"]
    assert max(p[1] for p in pit_in) - min(p[1] for p in pit_in) < 0.04
    assert doc["pit_source"] == "inactive"
    _assert_pit_joins(doc)


def test_compound_oval_falls_back_to_dashes_without_inactive():
    doc = import_svg_layers(html_text=_fixture("compound_oval.html"), num_corners=4)
    assert doc["pit_source"] == "dashes"


def test_rudskogen_real_html():
    doc = import_svg_layers(
        html_path=_html_path("Road", "Rudskogen Motorsenter.html"), num_corners=14)
    pit_path = doc["pit_path"]
    pit_in = doc["pit_in"]
    pit_out = doc["pit_out"]
    loop = doc["points"]
    assert max(p[0] for p in pit_in) - min(p[0] for p in pit_in) < 0.15
    assert _dist((pit_in[0][0], pit_in[0][1]),
                 (pit_path[0][0], pit_path[0][1])) < 0.08
    assert len(pit_out) >= 20
    assert _pit_path_y_span(pit_path) > 0.008 or (
        max(p[0] for p in pit_path) - min(p[0] for p in pit_path) > 0.15)
    assert max(p[0] for p in pit_path) - min(p[0] for p in pit_path) > 0.15
    assert _pit_out_y_span(pit_out) > 0.035
    assert _dist((pit_path[0][0], pit_path[0][1]),
                 (pit_out[0][0], pit_out[0][1])) < 0.02
    assert _dist((pit_out[0][0], pit_out[0][1]),
                 (pit_out[1][0], pit_out[1][1])) < 0.15
    assert _max_segment_span(pit_out[1:]) < 0.02
    assert _dist((pit_path[-1][0], pit_path[-1][1]),
                 (pit_out[0][0], pit_out[0][1])) > 0.05
    path_len = sum(
        _dist((a[0], a[1]), (b[0], b[1]))
        for a, b in zip(pit_path, pit_path[1:])
    )
    assert _max_segment_span(pit_out) < path_len * 0.35
    assert _median_pit_loop_gap(pit_path, loop) >= 0.020
    near_loop = sum(
        1 for p in pit_out[:-1]
        if min(_dist((p[0], p[1]), (q[0], q[1])) for q in loop) <= 0.005
    )
    assert near_loop <= 2
    assert _pit_out_colinear_overlap_count(doc) <= 2
    _assert_pit_joins(doc)

