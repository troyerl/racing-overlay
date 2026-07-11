"""Mid-race restart / garage-race context follow regressions."""

from __future__ import annotations

from overlay import config
from overlay.app import AdvancedSimHUD


def _hud(**kwargs) -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud.ir = kwargs.pop("ir", {})
    hud.demo = False
    hud._car_info = kwargs.pop("car_info", {})
    hud._pace_idxs = set()
    hud._profile_loading_depth = 0
    hud._profile_loading_dialog = None
    for k, v in kwargs.items():
        setattr(hud, k, v)
    return hud


def test_lap_est_falls_back_to_default_when_empty():
    hud = _hud(car_info={})
    assert hud._lap_est([]) == 90.0
    assert hud._lap_est(None) == 90.0
    assert hud._lap_est([0, 0, None]) == 90.0


def test_lap_est_uses_max_est_time():
    hud = _hud(car_info={})
    assert hud._lap_est([10.0, 45.5, 0]) == 45.5


def test_lap_est_prefers_driver_car_est_lap():
    hud = _hud(car_info={"est_lap": 62.3})
    assert hud._lap_est([10.0]) == 62.3


def test_prefer_grid_when_live_all_zeros():
    hud = _hud(ir={"SessionState": 4, "LapCompleted": 3})
    live = [0, 0, 0, 0]
    grid = [0, 2, 1, 3]
    assert hud._prefer_grid_positions(live, 1, grid) is True


def test_prefer_grid_false_when_live_populated():
    hud = _hud(ir={"SessionState": 4, "LapCompleted": 3})
    live = [3, 1, 2, 4]
    grid = [0, 2, 1, 3]
    assert hud._prefer_grid_positions(live, 1, grid) is False


def test_update_map_ensures_track_when_surface_missing(monkeypatch):
    hud = _hud()
    called = []

    def _ensure(player, lap_pct):
        called.append((player, lap_pct))

    monkeypatch.setattr(hud, "_ensure_track", _ensure)
    hud._update_map(None, None, None, {})
    assert called == [(None, None)]


def test_preview_pin_cleared_when_telemetry_disagrees():
    """Settings pin must not keep race CFG while IsInGarage is true."""
    config.set_preview_context(None)
    config.set_context("race", notify=False)
    config.set_preview_context("race")
    assert config.preview_context() == "race"
    assert config.effective_context() == "race"

    hud = _hud(ir={"IsInGarage": True})
    hud._maybe_auto_switch_preset = lambda: None  # type: ignore
    AdvancedSimHUD._update_context(hud)

    assert config.active_context() == "garage"
    assert config.preview_context() is None
    assert config.effective_context() == "garage"

    config.set_preview_context(None)
