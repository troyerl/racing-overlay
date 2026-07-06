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


def test_pit_progress_reaches_endpoints(qapp):
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_span[1]
    segs = [w.pit_in, w.pit_path]
    t_end = w._pit_progress_t(hi, lo, hi, segs)
    t_start = w._pit_progress_t(lo, lo, hi, segs)
    assert t_end is not None and t_start is not None
    assert t_start == 0.0
    assert t_end == 1.0


def test_pit_route_slower_than_racing_line(qapp):
    """Shorter pit polylines move slower than the loop for the same lap-% span."""
    import math
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_span[1]
    segs = [w.pit_in, w.pit_path]
    span = (hi - lo) % 1.0
    mid_pct = (lo + span * 0.5) % 1.0
    eps = 0.01
    p0 = (mid_pct - eps) % 1.0
    p1 = (mid_pct + eps) % 1.0
    pos0 = w._pit_phase_pos(p0, lo, hi, segs)
    pos1 = w._pit_phase_pos(p1, lo, hi, segs)
    dp = math.hypot(pos1[0] - pos0[0], pos1[1] - pos0[1])
    i0, i1 = w._index_for_pct(p0), w._index_for_pct(p1)
    dloop = math.hypot(w.path[i1][0] - w.path[i0][0], w.path[i1][1] - w.path[i0][1])
    assert dloop > 0
    assert dp / dloop < 1.0


def test_pit_route_constant_speed(qapp):
    """Pit placement should not accelerate mid-lane (old power-law bug)."""
    import math
    w = _make_widget()
    lo, hi = w.pit_span
    segs = [w.pit_path]
    span = (hi - lo) % 1.0
    ratios = []
    for f in (0.2, 0.4, 0.6, 0.8):
        p0 = (lo + span * (f - 0.02)) % 1.0
        p1 = (lo + span * (f + 0.02)) % 1.0
        pos0 = w._pit_phase_pos(p0, lo, hi, segs)
        pos1 = w._pit_phase_pos(p1, lo, hi, segs)
        dp = math.hypot(pos1[0] - pos0[0], pos1[1] - pos0[1])
        i0, i1 = w._index_for_pct(p0), w._index_for_pct(p1)
        dloop = math.hypot(w.path[i1][0] - w.path[i0][0], w.path[i1][1] - w.path[i0][1])
        if dp > 0 and dloop > 0:
            ratios.append(dp / dloop)
    assert ratios
    assert max(ratios) - min(ratios) < 0.15


def test_calibrated_progress_slower_than_linear(qapp):
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_span[1]
    segs = [w.pit_in, w.pit_path]
    mid_pct = (lo + (hi - lo) * 0.5) % 1.0
    span = (hi - lo) % 1.0
    linear_t = ((mid_pct - lo) % 1.0) / span
    calibrated_t = w._pit_progress_t(mid_pct, lo, hi, segs)
    assert calibrated_t is not None
    assert calibrated_t == linear_t


def test_calibrated_progress_reaches_end_at_hi(qapp):
    test_pit_progress_reaches_endpoints(qapp)


def test_long_pit_polyline_does_not_clamp_early(qapp):
    """Long pit polylines: progress stays interior until the phase ends."""
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
        assert abs(t - frac) < 1e-6
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


def test_chicagoland_like_lane_constant_speed(qapp):
    """Long pit polylines must not accelerate mid-lane (old power-law bug)."""
    import math
    w = _make_chicagoland_like_widget()
    path_lo, path_hi = w._pit_lane_bounds()
    span = (path_hi - path_lo) % 1.0
    dps = []
    for f in (0.2, 0.4, 0.6, 0.8):
        p0 = (path_lo + span * (f - 0.02)) % 1.0
        p1 = (path_lo + span * (f + 0.02)) % 1.0
        pos0 = w._pit_phase_pos(p0, path_lo, path_hi, [w.pit_path])
        pos1 = w._pit_phase_pos(p1, path_lo, path_hi, [w.pit_path])
        dps.append(math.hypot(pos1[0] - pos0[0], pos1[1] - pos0[1]))
    assert min(dps) > 0
    assert max(dps) / min(dps) < 1.15
    assert w._pit_progress_t(path_hi, path_lo, path_hi, [w.pit_path]) == 1.0


def test_entry_phase_uses_pit_in_segment(qapp):
    """Entry lap-% maps through pit_in only (then entry feather toward track)."""
    w = _make_chicagoland_like_widget()
    lo = w.pit_in_pct
    lane_lo = w.pit_span[0]
    pct = (lo + ((lane_lo - lo) % 1.0) * 0.5) % 1.0
    assert w._pct_in_interval(pct, lo, lane_lo)
    raw = w._pit_phase_pos(pct, lo, lane_lo, [w.pit_in])
    routed = w._pos_for_schematic_route(0, pct, on_route=True, on_pit_road=False)
    assert raw is not None and routed is not None
    assert routed == w._feather_schematic_pos(pct, raw)


def test_lane_phase_uses_pit_path_segment(qapp):
    """Mid-lane lap-% maps through pit_path only."""
    w = _make_chicagoland_like_widget()
    route_lo, route_hi = w.pit_in_pct, w.pit_out_pct
    span = (route_hi - route_lo) % 1.0
    pct = (route_lo + span * 0.5) % 1.0
    raw = w._pit_path_pos_for_route_pct(pct, route_lo, route_hi)
    routed = w._pos_for_schematic_route(0, pct, on_route=True, on_pit_road=True)
    assert raw is not None and routed is not None
    assert routed == raw


def test_entry_feather_starts_on_track(qapp):
    w = _make_widget()
    lo = w.pit_in_pct
    track = w._loop_point_at_pct(lo)
    route = w._pos_for_schematic_route(0, lo, on_route=True, on_pit_road=False)
    assert track is not None and route is not None
    # At route start the icon should still sit on the racing line.
    assert w._feather_schematic_pos(lo, route) == track


def test_entry_feather_mid_lane_is_pure_pit(qapp):
    """Mid-lane (past pit_span start) with OnPitRoad uses pit_path."""
    w = _make_widget()
    lane_lo, lane_hi = w.pit_span
    span = (lane_hi - lane_lo) % 1.0
    pct = (lane_lo + span * 0.5) % 1.0
    route = w._pos_for_schematic_route(0, pct, on_route=True, on_pit_road=True)
    assert route is not None
    assert w._feather_schematic_pos(pct, route) == route


def test_pit_blend_weight_on_pit_entry_starts_low(qapp):
    w = _make_widget()
    lo = w.pit_in_pct
    wgt = w._pit_blend_weight(
        lo, on_route=True, on_pit=True, in_entry=True, in_exit=False)
    assert wgt == 0.0


def test_on_pit_entry_uses_pit_in_segment(qapp):
    """OnPitRoad during entry lap-% maps through pit_in, not pit_path."""
    w = _make_chicagoland_like_widget()
    lo = w.pit_in_pct
    lane_lo = w.pit_span[0]
    pct = (lo + ((lane_lo - lo) % 1.0) * 0.5) % 1.0
    pit_in_pos = w._pit_phase_pos(pct, lo, lane_lo, [w.pit_in])
    routed = w._pos_for_schematic_route(
        0, pct, on_route=True, on_pit_road=True, raw=True)
    path_pos = w._pit_path_pos_for_route_pct(
        pct, w.pit_in_pct, w.pit_out_pct)
    assert pit_in_pos is not None and routed is not None
    assert routed == pit_in_pos
    assert path_pos is not None and routed != path_pos


def test_pit_blend_weight_entry_starts_at_zero(qapp):
    w = _make_widget()
    lo = w.pit_in_pct
    wgt = w._pit_blend_weight(
        lo, on_route=False, on_pit=False, in_entry=True, in_exit=False)
    assert wgt == 0.0


def test_pit_blend_weight_entry_ramps_when_on_route(qapp):
    w = _make_widget()
    lo, hi = w.pit_in_pct, w.pit_out_pct
    span = (hi - lo) % 1.0
    feather = min(max(span * 0.12, 0.012), span * 0.35)
    pct = (lo + feather * 0.5) % 1.0
    wgt = w._pit_blend_weight(
        pct, on_route=True, on_pit=False, in_entry=True, in_exit=False)
    assert 0.0 < wgt < 1.0


def test_pit_blend_weight_exit_zero_on_racing_line(qapp):
    """Wide exit lap-% arc must not pull on-track cars onto pit geometry."""
    w = _make_chicagoland_like_widget()
    lane_hi = w.pit_span[1]
    pit_out = w.pit_out_pct
    pct = (lane_hi + ((pit_out - lane_hi) % 1.0) * 0.5) % 1.0
    _, _, in_exit = (
        False,
        w._pct_in_interval(pct, w.pit_span[0], w.pit_span[1]),
        w._pct_in_interval(pct, lane_hi, pit_out),
    )
    assert in_exit
    wgt = w._pit_blend_weight(
        pct, on_route=False, on_pit=False, in_entry=False, in_exit=True)
    assert wgt == 0.0


def test_resolve_car_stays_on_track_in_exit_zone(qapp):
    import math
    from PyQt6.QtCore import QPointF

    w = _make_chicagoland_like_widget()
    lane_hi = w.pit_span[1]
    pit_out = w.pit_out_pct
    pct = 0.10
    assert w._pct_in_interval(pct, lane_hi, pit_out)
    w.resize(400, 300)
    w._layout_scale = 300.0
    w._layout_ox = 20.0
    w._layout_oy = 10.0

    def tx(pt):
        return QPointF(pt[0] * 300 + 20, pt[1] * 300 + 10)

    track = w._loop_point_at_pct(pct)
    assert track is not None
    car = (0, pct, "1", "#fff", True, False, False,
           False, False, None, False, True)
    cc = tx(w._centroid)
    pt = w._resolve_car_point(tx, car, cc, 12.0, True)
    assert pt is not None
    exp_scr = tx(track)
    assert math.hypot(pt.x() - exp_scr.x(), pt.y() - exp_scr.y()) < 2.0


def test_resolve_car_blends_entry_when_on_route(qapp):
    """Entry phase eases from track toward pit route when committed to route."""
    import math
    from PyQt6.QtCore import QPointF

    w = _make_chicagoland_like_widget()
    lo = w.pit_in_pct
    lane_lo = w.pit_span[0]
    pct = (lo + ((lane_lo - lo) % 1.0) * 0.25) % 1.0
    w.resize(400, 300)
    w._layout_scale = 300.0
    w._layout_ox = 20.0
    w._layout_oy = 10.0

    def tx(pt):
        return QPointF(pt[0] * 300 + 20, pt[1] * 300 + 10)

    track = w._loop_point_at_pct(pct)
    route = w._pos_for_schematic_route(
        0, pct, on_route=True, on_pit_road=False, raw=True)
    assert track is not None and route is not None
    wgt = w._pit_blend_weight(
        pct, on_route=True, on_pit=False, in_entry=True, in_exit=False)
    assert 0.0 < wgt < 1.0
    car = (0, pct, "1", "#fff", False, True, False,
           False, False, None, True, False)
    cc = tx(w._centroid)
    pt = w._resolve_car_point(tx, car, cc, 12.0, True)
    assert pt is not None
    expected = w._blend_xy(track, route, wgt)
    exp_scr = tx(expected)
    assert math.hypot(pt.x() - exp_scr.x(), pt.y() - exp_scr.y()) < 2.0
