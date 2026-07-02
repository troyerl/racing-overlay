"""Per-track pit lane speed % and pit_span-based placement."""

from __future__ import annotations

import math

import pytest
from PyQt6.QtWidgets import QApplication

from overlay.widgets.track_map import TrackMapWidget


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _make_widget():
    w = TrackMapWidget()
    loop = [(x, 0.5) for x in [i / 20 for i in range(21)]]
    loop += [(1.0, 0.5 - y) for y in [i / 20 for i in range(1, 21)]]
    loop += [(1.0 - x, 0.0) for x in [i / 20 for i in range(1, 21)]]
    loop += [(0.0, y) for y in [i / 20 for i in range(1, 20)]]
    w.set_track(loop, start_finish=0.0, corners=[])
    pit_path = [(x, 0.42) for x in [i / 14 for i in range(15)]]
    w.set_pit_source("manual")
    w.pit_in_pct = 0.05
    w.pit_out_pct = 0.75
    w.pit_span = (0.10, 0.70)
    w.pit_in = [(0.05, 0.48), pit_path[0]]
    w.pit_path = pit_path
    w.pit_out = [pit_path[-1], (0.75, 0.48)]
    return w


def test_pit_lane_speed_pct_halves_progress(qapp):
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_span[1]
    segs = [w.pit_in, w.pit_path]
    span = (hi - lo) % 1.0
    mid_pct = (lo + span * 0.5) % 1.0
    w.pit_lane_speed_pct = 1.0
    pos_full = w._pit_phase_pos(mid_pct, lo, hi, segs)
    w.pit_lane_speed_pct = 0.5
    pos_half = w._pit_phase_pos(mid_pct, lo, hi, segs)
    assert pos_full is not None and pos_half is not None
    start = w._pos_on_polyline_chain(segs, 0.0)
    d_full = math.hypot(pos_full[0] - start[0], pos_full[1] - start[1])
    d_half = math.hypot(pos_half[0] - start[0], pos_half[1] - start[1])
    assert d_half < d_full


def _make_oval_span_mismatch_widget():
    """pit_span on top straight; pit_path projects to a narrower loop interval."""
    w = TrackMapWidget()
    loop = [(i / 40, 0.5) for i in range(41)]
    loop += [(1.0, 0.5 - i / 20) for i in range(1, 21)]
    loop += [(1.0 - i / 40, 0.0) for i in range(1, 41)]
    loop += [(0.0, i / 20) for i in range(1, 20)]
    w.set_track(loop, start_finish=0.0, corners=[])
    pit_path = [(0.1 + i / 200, 0.42) for i in range(10)]
    w.set_pit_source("manual")
    w.pit_in_pct = 0.82
    w.pit_span = (0.87, 0.95)
    w.pit_out_pct = 0.39
    w.pit_in = [(0.82, 0.48), pit_path[0]]
    w.pit_path = pit_path
    w.pit_out = [pit_path[-1], (0.39, 0.48)]
    return w


def test_lane_uses_pit_span_not_narrow_path_bounds(qapp):
    """Car at lap-% inside pit_span but outside path projection stays on pit_path."""
    w = _make_oval_span_mismatch_widget()
    lane_lo, lane_hi = w.pit_span
    path_lo, path_hi = w._pit_lane_bounds()
    pct = 0.92
    assert w._pct_in_interval(pct, lane_lo, lane_hi)
    assert path_lo is not None and path_hi is not None
    assert not w._pct_in_interval(pct, path_lo, path_hi)
    routed = w._pos_for_schematic_route(0, pct, on_route=True, on_pit_road=True)
    assert routed is not None
    on_loop = w._loop_point_at_pct(pct)
    assert routed != on_loop


def _make_chicagoland_wide_span_widget():
    """Chicagoland-like oval: pit_span wraps most of the lap."""
    from tools.svg_layers_to_track import import_svg_layers

    v1 = import_svg_layers(
        html_path="tracks-html/Oval/Chicagoland.html", num_corners=4)
    w = TrackMapWidget()
    w.set_track([(p[0], p[1]) for p in v1["points"]], 0.0, v1["corners"])
    w.set_pit_source(v1["pit_source"])
    w.pit_in_pct = v1["pit_in_pct"]
    w.pit_out_pct = v1["pit_out_pct"]
    w.pit_span = tuple(v1["pit_span"])
    w.pit_in = [tuple(p) for p in v1["pit_in"]]
    w.pit_path = [tuple(p) for p in v1["pit_path"]]
    w.pit_out = [tuple(p) for p in v1["pit_out"]]
    return w


def test_mid_pit_stays_on_path_not_exit_blend(qapp):
    """OnPitRoad mid-lane must use pit_path, not pit_out exit feather."""
    w = _make_chicagoland_wide_span_widget()
    pct = 0.5
    lane_lo, lane_hi = w.pit_span
    exit_pct = lane_hi
    assert w._pct_in_interval(pct, lane_lo, lane_hi)
    assert w._pct_in_interval(pct, exit_pct, w.pit_out_pct)
    on_path = w._pit_phase_pos(
        pct, *w._pit_lane_mapping_interval(), [w.pit_path])
    routed = w._pos_for_schematic_route(
        0, pct, on_route=True, on_pit_road=True)
    exit_pos = w._pit_phase_pos(
        pct, exit_pct, w.pit_out_pct, [w.pit_out])
    assert routed is not None and on_path is not None and exit_pos is not None
    assert routed == on_path
    assert routed != exit_pos
    loop_pt = w._loop_point_at_pct(pct)
    assert routed != loop_pt


def test_wide_span_maps_without_early_clamp(qapp):
    """Wide pit_span uses path projection so some mid-lap pcts are not pinned at pit end."""
    w = _make_chicagoland_wide_span_widget()
    map_lo, map_hi = w._pit_lane_mapping_interval()
    assert (map_hi - map_lo) % 1.0 < 0.9
    pct = 0.3
    pos = w._pos_for_schematic_route(
        0, pct, on_route=True, on_pit_road=True)
    end = w._pos_on_polyline_chain([w.pit_path], 1.0)
    assert pos is not None
    assert math.hypot(pos[0] - end[0], pos[1] - end[1]) > 0.05
