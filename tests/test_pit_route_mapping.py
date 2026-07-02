"""Pit car placement: length-calibrated progress along schematic pit polylines."""

from __future__ import annotations

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
    # Long horizontal loop (model space).
    loop = [(x, 0.5) for x in [i / 20 for i in range(21)]]
    loop += [(1.0, 0.5 - y) for y in [i / 20 for i in range(1, 21)]]
    loop += [(1.0 - x, 0.0) for x in [i / 20 for i in range(1, 21)]]
    loop += [(0.0, y) for y in [i / 20 for i in range(1, 20)]]
    w.set_track(loop, start_finish=0.0, corners=[])
    # Shorter parallel pit straight below the top straight.
    pit_path = [(x, 0.42) for x in [i / 14 for i in range(15)]]
    w.set_pit_source("manual")
    w.pit_in_pct = 0.05
    w.pit_out_pct = 0.75
    w.pit_span = (0.10, 0.70)
    w.pit_in = [(0.05, 0.48), pit_path[0]]
    w.pit_path = pit_path
    w.pit_out = [pit_path[-1], (0.75, 0.48)]
    return w


def test_calibrated_progress_slower_than_linear(qapp):
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_span[1]
    segs = [w.pit_in, w.pit_path]
    mid_pct = (lo + (hi - lo) * 0.5) % 1.0
    span = (hi - lo) % 1.0
    linear_t = ((mid_pct - lo) % 1.0) / span
    calibrated_t = w._pit_progress_t(mid_pct, lo, hi, segs)
    assert calibrated_t is not None
    assert calibrated_t < linear_t


def test_calibrated_progress_reaches_end_at_hi(qapp):
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_span[1]
    segs = [w.pit_in, w.pit_path]
    t_end = w._pit_progress_t(hi, lo, hi, segs)
    t_start = w._pit_progress_t(lo, lo, hi, segs)
    assert t_end is not None and t_start is not None
    assert t_start == 0.0
    assert t_end == 1.0
    assert t_end > t_start


def test_long_pit_polyline_does_not_clamp_early(qapp):
    """Pit longer than loop arc: eased progress, no early clamp at polyline end."""
    w = TrackMapWidget()
    loop = [(x, 0.5) for x in [i / 20 for i in range(21)]]
    w.set_track(loop, start_finish=0.0, corners=[])
    pit_path = [(x, 0.1) for x in [i / 200 for i in range(201)]]
    w.pit_in = [(0.05, 0.48), pit_path[0]]
    w.pit_path = pit_path
    w.pit_out = [pit_path[-1], (0.15, 0.48)]
    w.pit_in_pct = 0.05
    w.pit_out_pct = 0.15
    w.pit_span = (0.05, 0.14)
    lo, hi = w.pit_in_pct, w.pit_span[1]
    segs = [w.pit_in, w.pit_path]
    span = (hi - lo) % 1.0
    for frac in (0.2, 0.5, 0.8):
        pct = (lo + span * frac) % 1.0
        t = w._pit_progress_t(pct, lo, hi, segs)
        assert t is not None
        assert t < 0.99, f"early clamp at lap fraction {frac}"
        assert t < frac, f"should be slower than linear at {frac}"
    assert w._pit_progress_t(hi, lo, hi, segs) == 1.0


def _make_chicagoland_like_widget():
    """Oval-like geometry where pit polyline is longer than the loop arc span."""
    w = TrackMapWidget()
    loop = [(x, 0.5) for x in [i / 30 for i in range(31)]]
    loop += [(1.0, 0.5 - y) for y in [i / 15 for i in range(1, 16)]]
    loop += [(1.0 - x, 0.0) for x in [i / 30 for i in range(1, 31)]]
    loop += [(0.0, y) for y in [i / 15 for i in range(1, 15)]]
    w.set_track(loop, start_finish=0.0, corners=[])
    pit_path = [(x, 0.38) for x in [i / 50 for i in range(51)]]
    w.set_pit_source("manual")
    w.pit_in_pct = 0.82
    w.pit_span = (0.87, 0.95)
    w.pit_out_pct = 0.39
    w.pit_in = [(0.82, 0.48), (0.85, 0.44), pit_path[0]]
    w.pit_path = pit_path
    w.pit_out = [pit_path[-1], (0.95, 0.48), (0.39, 0.48)]
    return w


def test_chicagoland_like_lane_slower_than_linear(qapp):
    w = _make_chicagoland_like_widget()
    lane_lo, lane_hi = w.pit_span
    segs = [w.pit_path]
    mid_pct = (lane_lo + (lane_hi - lane_lo) * 0.5) % 1.0
    span = (lane_hi - lane_lo) % 1.0
    linear_t = ((mid_pct - lane_lo) % 1.0) / span
    calibrated_t = w._pit_progress_t(mid_pct, lane_lo, lane_hi, segs)
    pit_arc = w._pit_arc_length(segs)
    loop_arc = w._loop_arc_between(lane_lo, lane_hi)
    assert loop_arc / pit_arc < 1.0
    assert calibrated_t is not None
    assert calibrated_t < linear_t
    assert w._pit_progress_t(lane_hi, lane_lo, lane_hi, segs) == 1.0


def test_entry_phase_uses_pit_in_segment(qapp):
    """Entry lap-% maps through pit_in only (then entry feather toward track)."""
    w = _make_chicagoland_like_widget()
    lo, lane_lo = w.pit_in_pct, w.pit_span[0]
    pct = (lo + (lane_lo - lo) * 0.5) % 1.0
    assert w._pct_in_interval(pct, lo, lane_lo)
    assert not w._pct_in_interval(pct, lane_lo, w.pit_span[1])
    t = w._pit_progress_t(pct, lo, lane_lo, [w.pit_in])
    raw = w._pos_on_polyline(w.pit_in, t)
    routed = w._pos_for_schematic_route(0, pct, on_route=True, on_pit_road=False)
    assert t is not None and raw is not None and routed is not None
    assert routed == w._feather_schematic_pos(pct, raw)


def test_lane_phase_uses_pit_path_segment(qapp):
    """Mid-lane lap-% maps through pit_path only."""
    w = _make_chicagoland_like_widget()
    lane_lo, lane_hi = w.pit_span
    pct = (lane_lo + (lane_hi - lane_lo) * 0.5) % 1.0
    t = w._pit_progress_t(pct, lane_lo, lane_hi, [w.pit_path])
    raw = w._pos_on_polyline(w.pit_path, t)
    routed = w._pos_for_schematic_route(0, pct, on_route=True, on_pit_road=True)
    assert t is not None and raw is not None and routed is not None
    assert routed == raw


def test_entry_feather_starts_on_track(qapp):
    w = _make_widget()
    lo = w.pit_in_pct
    track = w._loop_point_at_pct(lo)
    route = w._pos_for_schematic_route(0, lo, on_route=True, on_pit_road=False)
    assert track is not None and route is not None
    # At route start the icon should still sit on the racing line.
    assert w._feather_schematic_pos(lo, route) == track


def test_entry_feather_mid_route_is_pure_pit(qapp):
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_span[1]
    span = (hi - lo) % 1.0
    pct = (lo + span * 0.5) % 1.0
    route = w._pos_for_schematic_route(0, pct, on_route=True, on_pit_road=True)
    assert route is not None
    assert w._feather_schematic_pos(pct, route) == route
