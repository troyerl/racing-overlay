"""Tests for table column formatting helpers in overlay/traffic.py."""

from __future__ import annotations

from overlay import common as oc
from overlay import traffic as tr


def test_car_status_text():
    assert tr.car_status_text(oc.TRK_ON_TRACK) == "OUT"
    assert tr.car_status_text(oc.TRK_IN_PIT_STALL) == "PIT"
    assert tr.car_status_text(oc.TRK_NOT_IN_WORLD) == "GARAGE"


def test_car_flag_text_and_kind():
    assert tr.car_flag_text(tr.FLAG_BLACK) == "BLK"
    assert tr.car_flag_kind(tr.FLAG_REPAIR) == "meatball"
    assert tr.car_flag_text(0) == "\u2014"


def test_fmt_leader_gap_lap_down():
    assert tr.fmt_leader_gap(95.0, 3, 90.0) == "-1L"
    assert tr.fmt_leader_gap(1.5, 3, 90.0) == "+1.5"
    assert tr.fmt_leader_gap(0.0, 1, 90.0) == "\u2014"
