"""Smoke tests for laptime log painting."""

from __future__ import annotations

import pytest
from PyQt6.QtWidgets import QApplication

from overlay.widgets.laptime_log import LaptimeLogWidget


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_laptime_log_paints_with_temp_icon(qapp):
    w = LaptimeLogWidget()
    w.resize(380, 320)
    w.set_data({
        "columns": ["lap", "time", "delta", "temp"],
        "rows": [{
            "lap": "3",
            "time": "1:23.456",
            "delta": -0.12,
            "temp": "72°F",
        }],
    })
    w.show()
    qapp.processEvents()
    w.repaint()
    qapp.processEvents()
