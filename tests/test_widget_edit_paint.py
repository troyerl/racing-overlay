"""Paint smoke tests for widget edit-mode previews."""

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtWidgets import QApplication

from overlay import config
from overlay.widgets.delta_bar import DeltaBarWidget
from overlay.widgets.ers_hybrid import ErsHybridWidget
from overlay.widgets.leaderboard_strip import LeaderboardStripWidget
from overlay.widgets.pit_advisor import PitAdvisorWidget
from overlay.widgets.pit_board import PitBoardWidget
from overlay.widgets.radio_tower import RadioTowerWidget
from overlay.widgets.sector_timing import SectorTimingWidget
from overlay.widgets.tire_panel import TirePanelWidget
from overlay.widgets.system_panel import SystemPanelWidget
from overlay.widgets.weather_panel import WeatherPanelWidget


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


@pytest.mark.parametrize("widget_cls", [
    WeatherPanelWidget,
    SystemPanelWidget,
    PitBoardWidget,
    PitAdvisorWidget,
    RadioTowerWidget,
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


def test_system_panel_paint_with_icons(qapp, monkeypatch):
    monkeypatch.setitem(config.CFG["system_panel"], "show_icons", True)
    w = SystemPanelWidget()
    w.resize(320, 220)
    w.set_data({
        "edit": True,
        "cpu": "42%",
        "mem": "61%",
        "gpu": "28%",
        "cpu_pct": 42.0,
        "mem_pct": 61.0,
        "gpu_pct": 28.0,
        "fps": 144,
        "chan_quality": 97.0,
        "chan_latency": 28.0,
    })
    w.repaint()
