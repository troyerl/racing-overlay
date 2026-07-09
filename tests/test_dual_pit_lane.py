"""Dual pit lane: car assignment and map placement."""

from __future__ import annotations

import math
from unittest.mock import MagicMock

import pytest
from PyQt6.QtCore import QRect
from PyQt6.QtGui import QPaintEvent
from PyQt6.QtWidgets import QApplication

from overlay.app import AdvancedSimHUD
from overlay.widgets.track_map import TrackMapWidget, is_schematic_pit_source


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _hud_with_lanes() -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    w = TrackMapWidget()
    loop = [(math.cos(t), math.sin(t)) for t in [i * 0.4 for i in range(16)]]
    w.set_track(loop, start_finish=0.0, corners=[])
    w.set_pit_source("manual")
    hud.map_widget = w
    hud._pit_path = [(0.2, 0.5), (0.5, 0.52), (0.8, 0.54)]
    hud._pit_path_2 = [(0.2, -0.5), (0.5, -0.52), (0.8, -0.54)]
    hud._pit_in_pct = 0.1
    hud._pit_out_pct = 0.3
    hud._pit_in_pct_2 = 0.55
    hud._pit_out_pct_2 = 0.75
    hud._pit_span = (0.12, 0.28)
    hud._pit_span_2 = (0.58, 0.72)
    w.set_pit_path(hud._pit_path)
    w.set_pit_route_pct(hud._pit_in_pct, hud._pit_out_pct)
    w.set_pit_span_2(hud._pit_span_2)
    w.set_pit_path_2(hud._pit_path_2)
    w.set_pit_route_pct_2(hud._pit_in_pct_2, hud._pit_out_pct_2)
    w.pit_source = "manual"
    return hud


def test_pit_lane_for_car_interval_match(qapp):
    hud = _hud_with_lanes()
    assert hud._pit_lane_for_car(0, 0.6, True) == 2
    assert hud._pit_lane_for_car(0, 0.15, True) == 1


def test_pit_lane_for_car_single_lane_when_no_lane2(qapp):
    hud = _hud_with_lanes()
    hud._pit_path_2 = None
    hud.map_widget.set_pit_path_2(None)
    assert hud._pit_lane_for_car(0, 0.6, True) == 1


def test_pit_lane_for_car_not_on_pit(qapp):
    hud = _hud_with_lanes()
    assert hud._pit_lane_for_car(0, 0.6, False) == 1


def test_schematic_placement_uses_lane2_route(qapp):
    hud = _hud_with_lanes()
    w = hud.map_widget
    w.pit_source = "schematic"
    car = (3, 0.6, "42", "#fff", False, True, True,
           False, False, None, False, False, 2)
    pt = w._resolve_car_point(
        lambda p: __import__("PyQt6.QtCore", fromlist=["QPointF"]).QPointF(p[0], p[1]),
        car, MagicMock(), 0.0, True,
    )
    assert pt is not None
    lane2_mid = w._pit_path_pos_for_route_pct(
        0.6, hud._pit_in_pct_2, hud._pit_out_pct_2, lane=2)
    assert lane2_mid is not None
    assert math.hypot(pt.x() - lane2_mid[0], pt.y() - lane2_mid[1]) < 0.05


def test_draw_cars_paints_13_tuple_without_error(qapp):
    """Regression: _draw_cars must unpack 13-field car tuples from _update_map."""
    hud = _hud_with_lanes()
    w = hud.map_widget
    w.resize(320, 240)
    w.set_cars([
        (0, 0.25, "1", "#4af", True, False, False,
         False, False, None, False, False, 1),
        (3, 0.6, "42", "#fff", False, True, True,
         False, False, None, False, False, 2),
    ])
    ev = QPaintEvent(QRect(0, 0, 320, 240))
    w.paintEvent(ev)
