"""Tests for settings help text and default config values."""

from __future__ import annotations

import re

from overlay import config
from overlay.setting_help import COLOR_HELP, help_for


def _leaves(d, path=()):
    for k, v in d.items():
        p = path + (k,)
        if isinstance(v, dict):
            yield from _leaves(v, p)
        elif not isinstance(v, list) or k == "palette":
            yield list(p), v


GENERIC_HELP_PATTERNS = [
    re.compile(r"When on, shows show_"),
    re.compile(r"Chooses \w+ for the"),
    re.compile(r"Adjusts \w+ in the"),
    re.compile(r"Color used when painting"),
    re.compile(r"Selects \w+ for the"),
    re.compile(r"Controls \w+ in the"),
]


def _leaves(d, path=()):
    for k, v in d.items():
        p = path + (k,)
        if isinstance(v, dict):
            yield from _leaves(v, p)
        elif not isinstance(v, list) or k == "palette":
            yield list(p), v


def test_map_default_line_weights():
    assert config.DEFAULTS["map"]["asphalt_width"] == 12
    assert config.DEFAULTS["map"]["outline_width"] == 6


def test_table_corner_radius_minimum():
    assert config.DEFAULTS["relative"]["corner_radius_frac"] == 0.0
    assert config.DEFAULTS["standings"]["corner_radius_frac"] == 0.0


def test_widget_chrome_corner_radius_minimum():
    from overlay.config import _WIDGET_CHROME
    assert _WIDGET_CHROME["corner_radius_frac"] == 0.0


def test_help_for_map_asphalt_explicit():
    text = help_for(["map", "asphalt_width"], 12, "Asphalt width")
    assert "thickness" in text.lower() or "track" in text.lower()


def test_help_for_color_template():
    text = help_for(["relative", "colors", "player_row"], "#ff941658",
                    "Player row")
    assert "relative" in text.lower() or "row" in text.lower()


def test_help_for_bool_non_empty():
    text = help_for(["map", "show_pit"], True, "Show pit")
    assert "pit" in text.lower()
    assert len(text.strip()) > 10


def test_every_default_leaf_has_help():
    missing = []
    for path, val in _leaves(config.DEFAULTS):
        label = str(path[-1])
        if not help_for(path, val, label).strip():
            missing.append(".".join(path))
    assert not missing, f"empty help for: {missing[:5]}"


def test_no_generic_help_text():
    generic = []
    for path, val in _leaves(config.DEFAULTS):
        label = str(path[-1])
        text = help_for(path, val, label)
        if any(p.search(text) for p in GENERIC_HELP_PATTERNS):
            generic.append(".".join(path))
    assert not generic, f"generic help for: {generic[:5]}"


def test_color_keys_documented():
    missing = []
    for path, _val in _leaves(config.DEFAULTS):
        if len(path) >= 2 and path[-2] == "colors":
            key = str(path[-1])
            if key not in COLOR_HELP:
                missing.append(".".join(path))
    assert not missing, f"undocumented color keys: {missing[:5]}"
