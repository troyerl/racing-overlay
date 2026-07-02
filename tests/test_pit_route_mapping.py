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
    w.pit_span = (0.05, 0.70)
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
    assert 0.0 < t_end <= 1.0
    assert t_end > t_start
