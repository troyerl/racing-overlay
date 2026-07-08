"""Dual race/garage widget layouts per preset."""

from __future__ import annotations

import copy

import pytest

from overlay import config


@pytest.fixture
def layout_preset(monkeypatch):
    """Isolate layout mutations to a disposable in-memory preset."""
    monkeypatch.setattr(config, "save_profiles", lambda path=None: {})
    name = "__dual_layout_test__"
    race_layout = {"standings": [10, 20, 100, 200], "map": [300, 400, 500, 600]}
    config._PRESETS[name] = {
        "base": copy.deepcopy(config.DEFAULTS),
        "garage": {},
        "layout": copy.deepcopy(race_layout),
        "layout_garage": {},
        "cars": [],
        "leagues": [],
        "default": False,
    }
    prev = config.ACTIVE_PRESET
    config.ACTIVE_PRESET = name
    yield name, race_layout
    config.ACTIVE_PRESET = prev
    config._PRESETS.pop(name, None)


def test_save_race_updates_layout_only(layout_preset):
    _name, race_layout = layout_preset
    updated = {"standings": [11, 22, 100, 200], "map": [300, 400, 500, 600]}
    config.save_active_layout(updated, ctx="race")
    preset = config._PRESETS[config.ACTIVE_PRESET]
    assert preset["layout"] == updated
    assert preset["layout_garage"] == {}
    assert config.active_layout("race") == updated
    # Original race snapshot should differ after save.
    assert race_layout != updated


def test_save_garage_sparse_keeps_race(layout_preset):
    _name, race_layout = layout_preset
    moved = {
        "standings": [99, 88, 100, 200],  # differs from race
        "map": list(race_layout["map"]),  # same as race → omitted from sparse
    }
    config.save_active_layout(moved, ctx="garage")
    preset = config._PRESETS[config.ACTIVE_PRESET]
    assert preset["layout"] == race_layout
    assert preset["layout_garage"] == {"standings": [99, 88, 100, 200]}


def test_active_layout_garage_falls_back_to_race(layout_preset):
    _name, race_layout = layout_preset
    config._PRESETS[config.ACTIVE_PRESET]["layout_garage"] = {
        "standings": [1, 2, 3, 4],
    }
    garage = config.active_layout("garage")
    assert garage["standings"] == [1, 2, 3, 4]
    assert garage["map"] == race_layout["map"]
    assert config.active_layout("race") == race_layout


def test_duplicate_preset_copies_both_layouts(layout_preset, monkeypatch):
    _name, race_layout = layout_preset
    garage_layout = {"standings": [7, 8, 9, 10]}
    config._PRESETS[config.ACTIVE_PRESET]["layout_garage"] = copy.deepcopy(
        garage_layout)
    src = config.ACTIVE_PRESET
    dest = "__dual_layout_copy__"
    monkeypatch.setattr(config, "set_active_preset",
                        lambda *a, **k: config.CFG)
    assert config.create_preset(dest, copy_from=src, activate=False)
    copied = config._PRESETS[dest]
    assert copied["layout"] == race_layout
    assert copied["layout_garage"] == garage_layout
    config._PRESETS.pop(dest, None)
