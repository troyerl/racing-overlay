"""Auto-switch must wait for a real car before falling through to Default."""

from __future__ import annotations

from overlay import config
from overlay.app import AdvancedSimHUD


def _hud() -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud.demo = False
    hud._driver_cache = {0: {"CarPath": ""}}
    hud._last_car_path = None
    hud._last_league_id = None
    hud._league_id_cache = 0
    hud._session_info_counter = 1
    hud.ir = None
    hud.current_car = lambda: ("", "")
    hud._session_league_id = lambda: 0
    hud._drivers = lambda: hud._driver_cache
    return hud


def test_empty_car_does_not_force_default(monkeypatch):
    named = "__named_preset_test__"
    monkeypatch.setattr(config, "save_profiles", lambda path=None: {})
    monkeypatch.setattr(config, "auto_switch_enabled", lambda: True)
    monkeypatch.setattr(config, "AUTO_SWITCH_BY_LEAGUE", True)
    monkeypatch.setattr(config, "AUTO_SWITCH_BY_CAR", True)
    monkeypatch.setattr(config, "AUTO_SWITCH_TO_DEFAULT", True)
    monkeypatch.setattr(config, "default_preset", lambda: "Default")
    monkeypatch.setattr(config, "preset_for_league", lambda _lid: None)
    monkeypatch.setattr(config, "preset_for_car", lambda _car: None)

    prev = config.ACTIVE_PRESET
    config._PRESETS[named] = {
        "base": {}, "garage": {}, "layout": {}, "layout_garage": {},
        "cars": [], "leagues": [], "default": False,
    }
    if "Default" not in config._PRESETS:
        config._PRESETS["Default"] = {
            "base": {}, "garage": {}, "layout": {}, "layout_garage": {},
            "cars": [], "leagues": [], "default": True,
        }
    config.ACTIVE_PRESET = named

    hud = _hud()
    hud._maybe_auto_switch_preset()
    assert config.active_preset() == named
    assert hud._last_car_path is None

    # Once a real unbound car appears, Default fallback may run.
    hud.current_car = lambda: ("ferrari296gt3", "Ferrari 296 GT3")
    switched = []

    def _set(name, notify=True, persist=True):
        config.ACTIVE_PRESET = name
        switched.append(name)
        return config.CFG

    monkeypatch.setattr(config, "set_active_preset", _set)
    monkeypatch.setattr(config, "preset_for_session",
                        lambda lid, car: "Default")
    hud._maybe_auto_switch_preset()
    assert switched == ["Default"]
    assert hud._last_car_path == "ferrari296gt3"

    config.ACTIVE_PRESET = prev
    config._PRESETS.pop(named, None)
