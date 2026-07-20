"""Settings wiring: hidden keys, runtime consumers, live apply, paint smoke."""

from __future__ import annotations

import copy
import os

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtWidgets import QApplication

from overlay import config
from overlay.config import DEFAULTS
from overlay.config_editor import (
    MAP_SETTINGS_SKIP,
    SECTION_SETTINGS_SKIP,
    SETTING_GROUPS,
)
from overlay.widgets.leaderboard_strip import LeaderboardStripWidget
from overlay.widgets.pit_board import PitBoardWidget
from overlay.widgets.relative import RelativeWidget
from overlay.widgets.standings import StandingsWidget
from overlay.widgets.weather_panel import WeatherPanelWidget

# Keys that must not appear in Settings for a section (schema-only or N/A).
_HIDDEN_KEYS: dict[str, frozenset[str]] = {
    **SECTION_SETTINGS_SKIP,
}

# Top-level keys grouped in SETTING_GROUPS must be wired or explicitly hidden.
_WIRED_VIA_SHARED = frozenset({
    "text_scale", "corner_radius_frac", "data_font_bold", "row_dividers",
    "colors", "column_order", "columns", "widths", "license_colors",
    "header", "footer", "header_icons", "footer_icons", "sizes", "palette",
    "show",
})

# Section -> primary runtime file(s) for scalar key references.
_SECTION_FILES: dict[str, list[str]] = {
    "relative": ["overlay/widgets/table.py", "overlay/app.py"],
    "standings": ["overlay/widgets/table.py", "overlay/app.py"],
    "laptime_log": ["overlay/widgets/laptime_log.py", "overlay/app.py"],
    "fuel_calc": ["overlay/widgets/fuel_calc.py", "overlay/pit_strategy.py", "overlay/app.py"],
    "radar": ["overlay/widgets/radar.py", "overlay/app.py"],
    "dash": ["overlay/widgets/dash.py", "overlay/app.py"],
    "inputs": ["overlay/widgets/inputs.py", "overlay/app.py"],
    "delta_bar": ["overlay/widgets/delta_bar.py", "overlay/app.py"],
    "flags": ["overlay/widgets/flags.py", "overlay/app.py"],
    "sector_timing": ["overlay/widgets/sector_timing.py", "overlay/app.py"],
    "lap_compare": ["overlay/widgets/lap_compare.py", "overlay/app.py"],
    "map": ["overlay/widgets/track_map.py", "overlay/app.py"],
    "tire_panel": ["overlay/widgets/tire_panel.py", "overlay/app.py"],
    "pit_board": ["overlay/widgets/pit_board.py", "overlay/app.py"],
    "weather_panel": ["overlay/widgets/weather_panel.py", "overlay/app.py"],
    "leaderboard_strip": ["overlay/widgets/leaderboard_strip.py", "overlay/app.py"],
    "radio_tower": ["overlay/widgets/radio_tower.py", "overlay/app.py"],
    "ers_hybrid": ["overlay/widgets/ers_hybrid.py", "overlay/app.py"],
    "system_panel": ["overlay/widgets/system_panel.py", "overlay/app.py"],
    "pit_advisor": ["overlay/widgets/pit_advisor.py", "overlay/pit_strategy.py",
                    "overlay/telemetry.py", "overlay/app.py"],
}

_SHARED_FILES = [
    "overlay/widgets/chrome.py",
    "overlay/widgets/fonts.py",
]


def _load_sources(paths: list[str]) -> str:
    root = os.path.dirname(os.path.dirname(__file__))
    chunks = []
    for rel in paths:
        p = os.path.join(root, rel)
        if os.path.isfile(p):
            chunks.append(open(p, encoding="utf-8").read())
    return "\n".join(chunks)


def _grouped_keys(section: str) -> set[str]:
    keys: set[str] = set()
    for _title, group_keys in SETTING_GROUPS.get(section, []):
        keys.update(group_keys)
    return keys


def _key_referenced(key: str, text: str) -> bool:
    return f'"{key}"' in text or f"'{key}'" in text


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_section_settings_skip_matches_map():
    assert SECTION_SETTINGS_SKIP["map"] == MAP_SETTINGS_SKIP


@pytest.mark.parametrize("section,hidden", [
    (sec, sorted(keys)) for sec, keys in _HIDDEN_KEYS.items()
])
def test_hidden_keys_not_in_setting_groups(section, hidden):
    grouped = _grouped_keys(section)
    for key in hidden:
        assert key not in grouped, (
            f"{section}.{key} is hidden but still listed in SETTING_GROUPS")


@pytest.mark.parametrize("section", list(SETTING_GROUPS.keys()))
def test_grouped_keys_wired_or_shared(section):
    hidden = _HIDDEN_KEYS.get(section, frozenset())
    files = _SECTION_FILES.get(section, []) + _SHARED_FILES
    text = _load_sources(files)
    for key in _grouped_keys(section):
        if key in hidden or key in _WIRED_VIA_SHARED:
            continue
        assert _key_referenced(key, text), (
            f"{section}.{key} is grouped in Settings but not referenced in "
            f"runtime code ({', '.join(files)})")


def test_fade_ease_tau_wired_in_table():
    text = _load_sources(["overlay/widgets/table.py"])
    assert "fade_ease_tau" in text


def test_row_dividers_wired_in_list_widgets():
    for section, path in (
        ("pit_board", "overlay/widgets/pit_board.py"),
        ("weather_panel", "overlay/widgets/weather_panel.py"),
    ):
        text = _load_sources([path])
        assert "row_dividers" in text, f"{section} must gate row_dividers"


def test_apply_edits_notifies_listener():
    seen: list[dict] = []

    def _cb(cfg):
        seen.append(copy.deepcopy(cfg))

    config.on_change(_cb)
    try:
        full = config.editor_full("race")
        full["relative"]["rows_ahead"] = 2
        config.apply_edits("race", full, notify=True)
        assert seen, "apply_edits with notify=True should fire on_change"
        assert seen[-1]["relative"]["rows_ahead"] == 2
    finally:
        config._listeners.remove(_cb)  # noqa: SLF001


def test_leaderboard_strip_row_dividers_toggle(qapp, monkeypatch):
    monkeypatch.setitem(config.CFG["leaderboard_strip"], "row_dividers", False)
    w = LeaderboardStripWidget()
    w.resize(320, 120)
    w.set_data({"edit": True})
    w.repaint()


def test_pit_board_row_dividers_toggle(qapp, monkeypatch):
    monkeypatch.setitem(config.CFG["pit_board"], "row_dividers", False)
    w = PitBoardWidget()
    w.resize(320, 160)
    w.set_data({"edit": True, "pit_active": True})
    w.repaint()


def test_weather_panel_row_dividers_toggle(qapp, monkeypatch):
    monkeypatch.setitem(config.CFG["weather_panel"], "row_dividers", False)
    w = WeatherPanelWidget()
    w.resize(320, 160)
    w.set_data({"edit": True})
    w.repaint()


def test_table_fade_ease_tau_paint(qapp, monkeypatch):
    monkeypatch.setitem(config.CFG["relative"], "fade_ease_tau", 0.05)
    w = RelativeWidget()
    w.resize(400, 240)
    w.set_data({
        "rows": [
            {"key": 1, "position": 1, "name": "A", "gap": "+0.0",
             "is_player": True, "class_color": "#888"},
            {"key": 2, "position": 2, "name": "B", "gap": "+1.0",
             "class_color": "#888"},
        ],
        "slots": {},
    })
    w.repaint()


def _table_rows(a_first: bool):
    if a_first:
        return [
            {"key": 1, "position": 1, "name": "A", "gap": "+0.0",
             "is_player": True, "class_color": "#888"},
            {"key": 2, "position": 2, "name": "B", "gap": "+1.0",
             "class_color": "#888"},
        ]
    return [
        {"key": 2, "position": 1, "name": "B", "gap": "+0.0",
         "class_color": "#888"},
        {"key": 1, "position": 2, "name": "A", "gap": "+1.0",
         "is_player": True, "class_color": "#888"},
    ]


def test_table_stable_reorder_eases(qapp, monkeypatch):
    """A single reorder after populate eases toward the new slot."""
    from PyQt6.QtGui import QPaintEvent

    monkeypatch.setitem(config.CFG["relative"], "row_ease_tau", 0.5)
    w = RelativeWidget()
    w.resize(400, 240)
    w.set_data({"rows": _table_rows(True), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))
    assert w._anim[1]["idx"] == 0.0
    assert w._anim[2]["idx"] == 1.0

    # Simulate a frame of elapsed time so easing advances.
    w._last_ms = w._clock.elapsed() - 50
    w.set_data({"rows": _table_rows(False), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))
    # Mid-flight with slow tau — not snapped to final slots.
    assert 0.02 < w._anim[1]["idx"] < 0.98
    assert 0.02 < w._anim[2]["idx"] < 0.98


def test_table_rapid_reorder_snaps(qapp, monkeypatch):
    """Standings: a second reorder within the stability window snaps."""
    from PyQt6.QtGui import QPaintEvent

    monkeypatch.setitem(config.CFG["standings"], "row_ease_tau", 0.5)
    w = StandingsWidget()
    w.resize(400, 240)
    w.set_data({"rows": _table_rows(True), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))

    # First reorder starts a slide (and opens the stability window).
    w._last_ms = w._clock.elapsed() - 50
    w.set_data({"rows": _table_rows(False), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))
    assert w._order_stable_after_ms > 0
    assert abs(w._anim[1]["idx"] - 1.0) > 0.02  # not snapped yet

    # Immediate second reorder must snap, not stack mid-flight.
    w.set_data({"rows": _table_rows(True), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))
    assert w._anim[1]["idx"] == 0.0
    assert w._anim[2]["idx"] == 1.0


def test_relative_rapid_reorder_still_eases(qapp, monkeypatch):
    """Relative does not force-snap on rapid reorders — slides keep easing."""
    from PyQt6.QtGui import QPaintEvent

    monkeypatch.setitem(config.CFG["relative"], "row_ease_tau", 0.5)
    w = RelativeWidget()
    w.resize(400, 240)
    w.set_data({"rows": _table_rows(True), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))

    w._last_ms = w._clock.elapsed() - 50
    w.set_data({"rows": _table_rows(False), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))
    mid = w._anim[1]["idx"]
    assert 0.02 < mid < 0.98

    # Immediate second reorder — Relative keeps easing (no force-snap).
    w._last_ms = w._clock.elapsed() - 30
    w.set_data({"rows": _table_rows(True), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))
    # Target is 0 again; should be mid-flight toward 0, not snapped to 0.
    assert 0.02 < w._anim[1]["idx"] < 0.98
    assert w._anim[1]["idx"] != mid  # eased further from the prior mid-flight idx


def test_relative_pass_across_player_eases(qapp, monkeypatch):
    """A car crossing the player (|Δidx|≈2) eases — does not distance-snap."""
    from PyQt6.QtGui import QPaintEvent

    monkeypatch.setitem(config.CFG["relative"], "row_ease_tau", 0.5)
    w = RelativeWidget()
    w.resize(400, 280)
    # ahead / player / behind
    w.set_data({
        "rows": [
            {"key": 2, "position": 1, "name": "Ahead", "gap": "+0.5",
             "class_color": "#888"},
            {"key": 1, "position": 2, "name": "Me", "gap": "0.0",
             "is_player": True, "class_color": "#888"},
            {"key": 3, "position": 3, "name": "Behind", "gap": "+0.4",
             "class_color": "#888"},
        ],
        "slots": {},
    })
    w.paintEvent(QPaintEvent(w.rect()))
    assert w._anim[2]["idx"] == 0.0

    # Car 2 passes: now behind the player (index 0 → 2).
    w._last_ms = w._clock.elapsed() - 50
    w.set_data({
        "rows": [
            {"key": 3, "position": 1, "name": "Behind", "gap": "+0.4",
             "class_color": "#888"},
            {"key": 1, "position": 2, "name": "Me", "gap": "0.0",
             "is_player": True, "class_color": "#888"},
            {"key": 2, "position": 3, "name": "Ahead", "gap": "+0.5",
             "class_color": "#888"},
        ],
        "slots": {},
    })
    w.paintEvent(QPaintEvent(w.rect()))
    # With the old 1.25-slot snap this would teleport to 2.0 instantly.
    assert 0.02 < w._anim[2]["idx"] < 1.98


def test_table_slide_progresses_monotonically(qapp, monkeypatch):
    """Repeated paints with elapsed wall time move idx steadily toward the
    target — never backward."""
    from PyQt6.QtGui import QPaintEvent

    monkeypatch.setitem(config.CFG["relative"], "row_ease_tau", 0.3)
    w = RelativeWidget()
    w.resize(400, 240)
    w.set_data({"rows": _table_rows(True), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))

    w._last_ms = w._clock.elapsed() - 30
    w.set_data({"rows": _table_rows(False), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))

    # Row 1 slides 0 → 1; each subsequent frame must move it closer.
    prev = w._anim[1]["idx"]
    assert 0.0 < prev < 1.0
    for _ in range(5):
        w._last_ms = w._clock.elapsed() - 30  # simulate ~30ms frame gap
        w.paintEvent(QPaintEvent(w.rect()))
        cur = w._anim[1]["idx"]
        assert cur >= prev  # monotonic toward target 1.0
        prev = cur
    assert prev > 0.4  # converging after ~180ms of simulated time (tau 0.3)


def test_table_animating_schedules_timer_repaint(qapp, monkeypatch):
    """Mid-slide paints go through the fixed-rate anim timer path."""
    from PyQt6.QtGui import QPaintEvent

    monkeypatch.setitem(config.CFG["relative"], "row_ease_tau", 0.5)
    w = RelativeWidget()
    w.resize(400, 240)
    w.set_data({"rows": _table_rows(True), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))

    w._last_ms = w._clock.elapsed() - 30
    w.set_data({"rows": _table_rows(False), "slots": {}})
    w.paintEvent(QPaintEvent(w.rect()))
    assert w._animating
    # A paint inside the 33ms window must arm the timer instead of requesting
    # an immediate repaint (fixed cadence, no uneven chained repaints).
    w._anim_timer.stop()
    w._last_anim_sched_ms = w._clock.elapsed()
    w.paintEvent(QPaintEvent(w.rect()))
    assert w._anim_timer.isActive()


def test_hidden_row_dividers_still_in_defaults_for_compat():
    for section in SECTION_SETTINGS_SKIP:
        if "row_dividers" in SECTION_SETTINGS_SKIP[section]:
            assert "row_dividers" in DEFAULTS.get(section, {}), section
