"""Demo-mode telemetry + paint smoke tests for all overlay widgets."""

from __future__ import annotations

import os

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtCore import QRect
from PyQt6.QtGui import QPaintEvent
from PyQt6.QtWidgets import QApplication

from overlay import config, paths
from overlay.app import AdvancedSimHUD, _WIDGET_KEYS


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _paint_widget(widget, w: int, h: int) -> None:
    widget.resize(w, h)
    widget.show()
    widget.repaint()
    ev = QPaintEvent(QRect(0, 0, w, h))
    widget.paintEvent(ev)


def test_demo_all_widgets_paint_after_ticks(qapp, monkeypatch):
    """Run demo telemetry then paint every widget (default sizes)."""
    for key in _WIDGET_KEYS:
        monkeypatch.setitem(config.CFG[key], "show", True)

    hud = AdvancedSimHUD(demo=True, click_through=False)
    hud.start_overlay()
    for _ in range(30):
        hud.process_telemetry_tick()

    sizes = {
        "leaderboard_strip": (96, 373),
        "map": (480, 320),
        "lap_compare": (963, 444),
        "pit_advisor": (328, 127),
    }
    for key in _WIDGET_KEYS:
        widget = hud._widget_by_key()[key]
        w, h = sizes.get(key, (400, 300))
        _paint_widget(widget, w, h)


def test_demo_map_paint_partial_colors(qapp, monkeypatch):
    """Map paint must not KeyError when map.colors is a partial override."""
    colors = dict(config.CFG["map"].get("colors", {}))
    for drop in ("pit_car", "corner_text", "wind", "pit"):
        colors.pop(drop, None)
    monkeypatch.setitem(config.CFG["map"], "colors", colors)
    monkeypatch.setitem(config.CFG["map"], "show", True)

    hud = AdvancedSimHUD(demo=True, click_through=False)
    hud.start_overlay()
    path = os.path.join(paths.tracks_dir(), "123.json")
    if os.path.exists(path):
        hud._apply_track_from_path(path, "123")
    for _ in range(10):
        hud.process_telemetry_tick()
    _paint_widget(hud.map_widget, 480, 320)


def test_track_sync_skips_inflight_duplicate(monkeypatch):
    from overlay import track_store

    sync = track_store.TrackSync()
    monkeypatch.setattr(track_store, "read_available", lambda: True)
    sync._fetch_inflight.add("99")
    started: list = []

    class _FakeThread:
        def __init__(self, target=None, args=(), daemon=None):
            started.append(args[0] if args else None)

        def start(self) -> None:
            pass

    monkeypatch.setattr(track_store.threading, "Thread", _FakeThread)
    sync.fetch_async(99)
    sync.fetch_async(100)
    assert started == [100]
