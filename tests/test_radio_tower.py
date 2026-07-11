"""Radio tower widget tests."""

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtCore import QRect
from PyQt6.QtGui import QPaintEvent
from PyQt6.QtWidgets import QApplication

from overlay.widgets.radio_tower import RadioTowerWidget, _row_text


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_row_text_position_name_and_number():
    row = {"position": 5, "name": "Logan Troyer", "car_number": "12"}
    assert _row_text(row, show_position=True, show_name=True,
                     show_car_number=True) == "5 - Logan Troyer #12"


def test_row_text_no_position():
    row = {"position": 5, "name": "Logan Troyer", "car_number": "12"}
    assert _row_text(row, show_position=False, show_name=True,
                     show_car_number=True) == "Logan Troyer #12"


def test_row_text_position_and_number_only():
    row = {"position": 5, "name": "Logan Troyer", "car_number": "12"}
    assert _row_text(row, show_position=True, show_name=False,
                     show_car_number=True) == "5 - #12"


def test_row_text_position_only():
    row = {"position": 5, "name": "Logan Troyer", "car_number": "12"}
    assert _row_text(row, show_position=True, show_name=False,
                     show_car_number=False) == "5"


def test_row_text_empty_position_still_shows_name():
    row = {"position": "", "name": "Me", "car_number": "48"}
    assert _row_text(row, show_position=True, show_name=True,
                     show_car_number=True) == "Me #48"


def test_radio_tower_paint_active_row(qapp):
    w = RadioTowerWidget()
    w.resize(220, 56)
    w.set_data({
        "rows": [{
            "position": 3,
            "car_number": "48",
            "name": "Driver",
            "active": True,
            "is_player": True,
        }],
    })
    w.paintEvent(QPaintEvent(QRect(0, 0, 220, 56)))


def test_radio_tower_paint_pro_badge(qapp):
    w = RadioTowerWidget()
    w.resize(220, 56)
    w.set_data({
        "rows": [{
            "position": 1,
            "car_number": "33",
            "name": "Pro Driver",
            "active": True,
            "is_player": False,
            "is_pro": True,
            "group_icon": "league",
            "group_color": "#5bb8ff",
        }],
    })
    w.paintEvent(QPaintEvent(QRect(0, 0, 220, 56)))


def test_radio_tower_paint_group_badge(qapp):
    w = RadioTowerWidget()
    w.resize(220, 56)
    w.set_data({
        "rows": [{
            "position": "",
            "car_number": "11",
            "name": "League Mate",
            "active": True,
            "is_player": True,
            "is_pro": False,
            "group_icon": "flag",
            "group_color": "#46df7a",
        }],
    })
    w.paintEvent(QPaintEvent(QRect(0, 0, 220, 56)))


def test_radio_tower_paint_edit_preview(qapp):
    w = RadioTowerWidget()
    w.resize(220, 56)
    w.set_data({"edit": True, "rows": []})
    w.paintEvent(QPaintEvent(QRect(0, 0, 220, 56)))


def test_radio_tower_silent_paints_nothing(qapp):
    w = RadioTowerWidget()
    w.resize(220, 56)
    w.set_data({"rows": [], "edit": False})
    w.paintEvent(QPaintEvent(QRect(0, 0, 220, 56)))
