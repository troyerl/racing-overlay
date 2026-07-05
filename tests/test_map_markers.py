"""Map pace car, sector boundaries, and traffic marker hold logic."""

from __future__ import annotations

import math

import pytest
from PyQt6.QtCore import QPointF
from PyQt6.QtWidgets import QApplication

from overlay import common as oc
from overlay.map_markers import (
    MARKER_SLOTS,
    apply_marker_hold,
    fresh_hold_states,
    marker_car_valid,
    resolve_traffic_markers,
    select_marker_candidates,
    wrap_lap_delta,
)
from overlay.widgets.track_map import TrackMapWidget


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_wrap_lap_delta():
    assert wrap_lap_delta(0.1, 0.9) == pytest.approx(0.2, abs=1e-6)
    assert wrap_lap_delta(0.9, 0.1) == pytest.approx(-0.2, abs=1e-6)


def test_select_marker_candidates_ahead_behind_leader():
    lap_pct = [0.5, 0.55, 0.48, 0.2]
    surface = [oc.TRK_ON_TRACK] * 4
    positions = [2, 1, 4, 3]
    out = select_marker_candidates(
        0, lap_pct, surface, positions,
        pace_idxs=set(), on_pit_arr=[False] * 4,
    )
    assert out["ahead"] == 1
    assert out["behind"] == 2
    assert out["leader"] == 1


def test_select_marker_candidates_excludes_pace():
    lap_pct = [0.5, 0.6]
    surface = [oc.TRK_ON_TRACK, oc.TRK_ON_TRACK]
    positions = [2, 1]
    out = select_marker_candidates(
        0, lap_pct, surface, positions,
        pace_idxs={1}, on_pit_arr=[False, False],
    )
    assert out["ahead"] is None
    assert out["leader"] is None


def test_marker_hold_requires_three_seconds():
    state = {"locked": 1, "pending": None, "pending_since": None}
    idx = apply_marker_hold(state, 2, now=10.0, hold_sec=3.0, locked_valid=True)
    assert idx == 1
    idx = apply_marker_hold(state, 2, now=12.0, hold_sec=3.0, locked_valid=True)
    assert idx == 1
    idx = apply_marker_hold(state, 2, now=13.0, hold_sec=3.0, locked_valid=True)
    assert idx == 2


def test_marker_hold_alternation_keeps_locked():
    state = {"locked": 1, "pending": None, "pending_since": None}
    apply_marker_hold(state, 2, now=10.0, hold_sec=3.0, locked_valid=True)
    apply_marker_hold(state, 1, now=10.5, hold_sec=3.0, locked_valid=True)
    apply_marker_hold(state, 2, now=11.0, hold_sec=3.0, locked_valid=True)
    assert state["locked"] == 1


def test_marker_hold_clears_invalid_locked():
    state = {"locked": 5, "pending": None, "pending_since": None}
    idx = apply_marker_hold(state, 2, now=10.0, hold_sec=3.0, locked_valid=False)
    assert idx is None
    assert state["locked"] is None


def test_resolve_traffic_markers_returns_pct(qapp):
    lap_pct = [0.5, 0.55, 0.48, 0.2]
    surface = [oc.TRK_ON_TRACK] * 4
    on_pit = [False] * 4
    positions = [2, 1, 4, 3]
    holds = fresh_hold_states()
    for t in range(40):
        cands = select_marker_candidates(
            0, lap_pct, surface, positions,
            pace_idxs=set(), on_pit_arr=on_pit,
        )
        out = resolve_traffic_markers(
            holds, cands, lap_pct,
            now=float(t) * 0.1, hold_sec=3.0,
            surface=surface, on_pit_arr=on_pit, pace_idxs=set(),
        )
    assert out["leader"]["idx"] == 1
    assert out["leader"]["pct"] == pytest.approx(0.55, abs=1e-6)

def test_resolve_traffic_markers_includes_idx():
    holds = fresh_hold_states()
    lap_pct = [0.5, 0.55, 0.48]
    surface = [oc.TRK_ON_TRACK] * 3
    cands = {"ahead": 1, "behind": 2, "leader": 1}
    for t in range(35):
        out = resolve_traffic_markers(
            holds, cands, lap_pct,
            now=float(t) * 0.1, hold_sec=3.0,
            surface=surface, on_pit_arr=[False] * 3, pace_idxs=set(),
        )
    assert out["ahead"]["idx"] == 1
    assert out["ahead"]["pct"] == pytest.approx(0.55, abs=1e-6)


def test_layout_pad_includes_traffic_marker_margin(qapp):
    w = TrackMapWidget()
    mc = {"asphalt_width": 11, "show_traffic_markers": True,
          "show_corners": False, "show_sector_boundaries": False}
    assert w._layout_pad(mc) > 26.0


def test_outward_point_farther_than_track(qapp):
    w = TrackMapWidget()
    loop = [(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8), (0.2, 0.2)]
    w.set_track(loop, start_finish=0.0, corners=[])
    w.resize(400, 300)
    w.show()
    qapp.processEvents()
    w._layout_scale = 200.0
    w._layout_ox = 50.0
    w._layout_oy = 40.0

    def tx(pt):
        return QPointF(pt[0] * 200 + 50, pt[1] * 200 + 40)

    cc = tx(w._centroid)
    track = w._track_point(tx, 0.25)
    out = w._outward_point(tx, 0.25, 30.0)
    assert math.hypot(out.x() - cc.x(), out.y() - cc.y()) > math.hypot(
        track.x() - cc.x(), track.y() - cc.y())


def test_car_screen_points_ease_toward_target(qapp):
    w = TrackMapWidget()
    loop = [(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8), (0.2, 0.2)]
    w.set_track(loop, start_finish=0.0, corners=[])
    w.resize(400, 300)
    w.show()
    qapp.processEvents()
    w._layout_scale = 200.0
    w._layout_ox = 50.0
    w._layout_oy = 40.0

    def tx(pt):
        return QPointF(pt[0] * 200 + 50, pt[1] * 200 + 40)

    mc = {"asphalt_width": 11}
    car = (1, 0.30, "7", "#3aa0ff", False, False, False)
    w.cars = [car]
    targets = w._build_car_screen_points(tx, mc)
    pts1, anim1 = w._build_smooth_car_screen_points(tx, mc)
    assert anim1 is False
    assert pts1[1] == targets[1]

    w.cars = [(1, 0.32, "7", "#3aa0ff", False, False, False)]
    targets2 = w._build_car_screen_points(tx, mc)
    w._last_ms = w._clock.elapsed() - 16
    pts2, anim2 = w._build_smooth_car_screen_points(tx, mc)
    assert anim2 is True
    assert pts2[1] != targets2[1]

    for _ in range(80):
        w._last_ms = w._clock.elapsed() - 16
        pts2, anim2 = w._build_smooth_car_screen_points(tx, mc)
    assert anim2 is False
    assert math.hypot(pts2[1].x() - targets2[1].x(),
                      pts2[1].y() - targets2[1].y()) < 1.0
