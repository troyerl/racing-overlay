"""Panel geometry clamping tests."""

from __future__ import annotations

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtCore import QRect
from PyQt6.QtWidgets import QApplication, QLabel

from overlay import layout_store
from overlay.panel import PanelWindow


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _avail(w=1440, h=900, left=0, top=25) -> QRect:
    return QRect(left, top, w, h)


def test_clamp_leaves_on_screen_geometry_unchanged():
    avail = _avail()
    x, y, w, h = layout_store.clamp_panel_geometry(
        40, 120, 560, 360, avail=avail)
    assert (x, y, w, h) == (40, 120, 560, 360)


def test_clamp_moves_panel_below_screen_up():
    avail = _avail(h=900)
    x, y, w, h = layout_store.clamp_panel_geometry(
        320, 840, 108, 300, avail=avail)
    assert y + h <= avail.bottom()
    assert y >= avail.top()
    assert layout_store.geometry_mostly_off_screen(x, y, w, h, avail) is False


def test_clamp_moves_panel_above_screen_down():
    avail = _avail()
    x, y, w, h = layout_store.clamp_panel_geometry(
        100, -200, 200, 120, avail=avail)
    assert y >= avail.top()
    assert layout_store.geometry_mostly_off_screen(x, y, w, h, avail) is False


def test_clamp_moves_panel_right_of_screen_left():
    avail = _avail()
    x, y, w, h = layout_store.clamp_panel_geometry(
        2000, 200, 200, 120, avail=avail)
    assert x + w <= avail.right()
    assert layout_store.geometry_mostly_off_screen(x, y, w, h, avail) is False


def test_panel_window_clamps_on_init(qapp):
    layout = {"test_panel": [320, 840, 108, 300]}
    win = PanelWindow(
        "test_panel",
        QLabel("test"),
        (40, 120, 200, 100),
        layout,
        click_through=False,
    )
    g = win.geometry()
    assert g.y() + g.height() <= qapp.primaryScreen().availableGeometry().bottom()
    win.close()
