"""Paint smoke tests for widget edit-mode previews."""

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtWidgets import QApplication

from overlay.widgets.delta_bar import DeltaBarWidget
from overlay.widgets.ers_hybrid import ErsHybridWidget
from overlay.widgets.leaderboard_strip import LeaderboardStripWidget
from overlay.widgets.pit_board import PitBoardWidget
from overlay.widgets.sector_timing import SectorTimingWidget
from overlay.widgets.tire_panel import TirePanelWidget
from overlay.widgets.weather_panel import WeatherPanelWidget


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.mark.parametrize("widget_cls", [
    WeatherPanelWidget,
    PitBoardWidget,
    LeaderboardStripWidget,
    ErsHybridWidget,
    TirePanelWidget,
    SectorTimingWidget,
    DeltaBarWidget,
])
def test_edit_mode_paint_smoke(qapp, widget_cls):
    w = widget_cls()
    w.resize(320, 200)
    w.set_data({"edit": True})
    w.repaint()
