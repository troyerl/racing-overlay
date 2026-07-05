"""Tests for radar helper logic in overlay/traffic.py."""

from __future__ import annotations

import pytest

from overlay import traffic as tr


def test_closing_rate_tint():
    assert tr.closing_rate_tint(None, 1.5) is None
    assert tr.closing_rate_tint(0.0, 1.5) == 0.0
    assert tr.closing_rate_tint(0.75, 1.5) == pytest.approx(0.5, abs=1e-6)
    assert tr.closing_rate_tint(3.0, 1.5) == 1.0


def test_radar_clear_seconds():
    assert tr.radar_clear_seconds(None, 100.0) is None
    assert tr.radar_clear_seconds(90.0, 100.0) == pytest.approx(10.0, abs=1e-6)
    assert tr.radar_clear_seconds(100.0, 100.0) == 0.0


def test_nearest_alongside_prefers_est():
    lap_pct = [0.50, 0.502, 0.498]
    est = [100.0, 100.5, 99.7]
    idx, lap_d, est_d = tr.nearest_alongside(
        lap_pct, 0, est, 90.0, alongside_zone=0.01)
    assert idx == 2
    assert lap_d == pytest.approx(-0.002, abs=1e-6)
    assert est_d == pytest.approx(-0.3, abs=1e-6)
