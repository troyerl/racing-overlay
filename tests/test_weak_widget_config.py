"""Defaults and customization wiring for newer card widgets."""

from __future__ import annotations

import copy

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtWidgets import QApplication

from overlay import config
from overlay.config import DEFAULTS, diff_from_defaults
from overlay.widgets.delta_bar import DeltaBarWidget
from overlay.widgets.leaderboard_strip import LeaderboardStripWidget
from overlay.widgets.sector_timing import SectorTimingWidget


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


_WEAK_WIDGET_KEYS = {
    "weather_panel": {"show_title", "title", "corner_radius_frac",
                      "row_height_px", "max_row_height_frac"},
    "pit_board": {"title", "show_pit_banner", "pit_banner_text",
                  "corner_radius_frac", "row_height_px", "max_row_height_frac"},
    "tire_panel": {"show_title", "title", "corner_radius_frac"},
    "leaderboard_strip": {"show_position", "show_lap", "show_mph", "show_gap",
                          "corner_radius_frac", "row_height_px", "max_row_height_frac"},
    "radio_tower": {"show_title", "title", "show_position", "show_car_number",
                    "show_name", "corner_radius_frac", "row_height_px",
                    "max_row_height_frac"},
    "ers_hybrid": {"show_title", "title", "label_battery", "label_lap",
                   "label_boost", "label_p2p", "empty_text", "corner_radius_frac"},
    "system_panel": {"show_title", "title", "show_icons", "show_cpu", "show_mem",
                     "show_gpu", "show_fps", "show_network", "corner_radius_frac",
                     "row_height_px", "max_row_height_frac"},
    "delta_bar": {"corner_radius_frac"},
    "sector_timing": {"corner_radius_frac", "row_height_px", "max_row_height_frac"},
}


@pytest.mark.parametrize("section,keys", list(_WEAK_WIDGET_KEYS.items()))
def test_weak_widget_defaults_include_customization_keys(section, keys):
    for key in keys:
        assert key in DEFAULTS[section], f"{section}.{key} missing from DEFAULTS"


def test_weather_panel_show_title_persists_in_diff():
    cfg = copy.deepcopy(DEFAULTS)
    cfg["weather_panel"]["show_title"] = False
    cfg["weather_panel"]["title"] = "SKY"
    diff = diff_from_defaults(cfg)
    assert diff["weather_panel"]["show_title"] is False
    assert diff["weather_panel"]["title"] == "SKY"


def test_leaderboard_strip_paints_without_gap(qapp, monkeypatch):
    monkeypatch.setitem(config.CFG["leaderboard_strip"], "show_gap", False)
    w = LeaderboardStripWidget()
    w.resize(320, 120)
    w.set_data({
        "edit": True,
        "rows": [
            {"position": 1, "car_number": "1", "name": "A", "gap": "+0.0",
             "class_color": "#888", "is_player": False},
        ],
    })
    w.repaint()


def test_sector_timing_and_delta_bar_edit_paint(qapp):
    st = SectorTimingWidget()
    st.resize(320, 160)
    st.set_data({
        "cur_lap": 92.1,
        "last_lap": 93.0,
        "best_lap": 91.5,
        "sectors": [{"time": 30.1, "status": "done", "active": False, "delta": None}],
    })
    st.repaint()

    db = DeltaBarWidget()
    db.resize(320, 100)
    db.set_data({"delta": -0.12})
    db.repaint()
