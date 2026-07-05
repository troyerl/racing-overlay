"""Unit tests for overlay/traffic.py gap and status helpers."""

from __future__ import annotations

import pytest

from overlay import common as oc
from overlay import traffic as tr


def test_wrap_est_delta_wraps():
    lap = 90.0
    assert tr.wrap_est_delta(85.0, 10.0, lap) == pytest.approx(-15.0, abs=1e-6)
    assert tr.wrap_est_delta(10.0, 85.0, lap) == pytest.approx(15.0, abs=1e-6)
    assert tr.wrap_est_delta(50.0, 48.0, lap) == pytest.approx(2.0, abs=1e-6)


def test_position_ahead_idx():
    positions = [0, 1, 3, 2, 0]
    assert tr.position_ahead_idx(positions, 2) == 3  # P3 -> P2 at idx 3
    assert tr.position_ahead_idx(positions, 3) == 1  # P2 -> P1 at idx 1
    assert tr.position_ahead_idx(positions, 1) is None  # leader


def test_est_and_f2_interval():
    est = [100.0, 102.0, 98.0]
    assert tr.est_interval(est, 0, 1, 90.0) == pytest.approx(2.0, abs=1e-6)
    f2 = [0.0, 1.5, 3.0]
    assert tr.f2_interval(f2, 1, 2) == pytest.approx(1.5, abs=1e-6)


def test_fmt_leader_gap():
    assert tr.fmt_leader_gap(0.0, 1, 90.0) == "\u2014"
    assert tr.fmt_leader_gap(1.2, 2, 90.0) == "+1.2"
    assert tr.fmt_leader_gap(95.0, 2, 90.0) == "-1L"


def test_nearest_ahead_behind():
    est = [100.0, 101.0, 99.0, 200.0]
    ahead, behind = tr.nearest_ahead_behind(est, 0, 90.0, pace_idxs=set())
    assert ahead == pytest.approx(1.0, abs=1e-6)
    assert behind == pytest.approx(1.0, abs=1e-6)


def test_closing_rate():
    state: dict = {}
    t0 = 1000.0
    assert tr.closing_rate(state, 1, 2.0, t0) is None
    rate = tr.closing_rate(state, 1, 1.0, t0 + 1.0)
    assert rate == pytest.approx(1.0, abs=1e-6)


def test_car_status_and_flag():
    assert tr.car_status_text(oc.TRK_ON_TRACK) == "OUT"
    assert tr.car_status_text(oc.TRK_IN_PIT_STALL) == "PIT"
    assert tr.car_status_text(oc.TRK_ON_TRACK, on_pit=True) == "PIT"
    assert tr.car_flag_text(tr.FLAG_REPAIR) == "MEAT"
    assert tr.car_flag_kind(tr.FLAG_BLACK) == "black"


def test_engine_warning_text():
    assert tr.engine_warning_text(tr.ENGINE_LIM | tr.ENGINE_WATER) == "LIM H2O"
    assert tr.engine_warning_text(0) == "\u2014"


def test_is_multiclass():
    assert tr.is_multiclass([1, 2, 1], [1, 2, 3]) is True
    assert tr.is_multiclass([1, 2, 3], [1, 2, 3]) is False


def test_map_car_status_kind():
    assert tr.map_car_status_kind(oc.TRK_ON_TRACK) is None
    assert tr.map_car_status_kind(oc.TRK_OFF_TRACK) == "off"
    assert tr.map_car_status_kind(oc.TRK_ON_TRACK, on_pit=True) == "pit"
    assert tr.map_car_status_kind(oc.TRK_ON_TRACK, car_flag=tr.FLAG_BLACK) == "black"


def test_nearest_alongside_and_candidates():
    lap_pct = [0.50, 0.503, 0.60]
    cands = tr.alongside_candidates(lap_pct, 0, alongside_zone=0.01)
    assert [i for i, _ in cands] == [1]
    idx, delta = tr.pick_alongside_car(cands)
    assert idx == 1
    assert delta == pytest.approx(0.003, abs=1e-6)
    idx2, lap_d, est_d = tr.nearest_alongside(
        lap_pct, 0, None, 0.0, alongside_zone=0.01)
    assert idx2 == 1
    assert lap_d == pytest.approx(0.003, abs=1e-6)
    assert est_d is None
