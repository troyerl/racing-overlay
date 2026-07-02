"""Oval corner label renumbering (members 4,3,2,1 -> iRacing 1,2,3,4)."""

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


def test_iracing_oval_label_remap():
    assert TrackMapWidget._iracing_oval_label("4", 4) == "1"
    assert TrackMapWidget._iracing_oval_label("1", 4) == "4"
    assert TrackMapWidget._iracing_oval_label("A", 4) == "A"


def test_display_corners_renumbers_on_oval(qapp):
    w = TrackMapWidget()
    w.set_track([(0, 0), (1, 0), (1, 1), (0, 1)], corners=[
        {"pct": 0.1, "label": "4"},
        {"pct": 0.4, "label": "3"},
        {"pct": 0.6, "label": "2"},
        {"pct": 0.8, "label": "1"},
    ])
    w.set_num_turns(4)
    w.set_track_is_oval(True)
    labels = [l for _, l, _, _ in w.display_corners()]
    assert labels == ["1", "2", "3", "4"]
