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


def test_garage_visible_without_in_garage():
    """Spectators: garage UI open but car physics not in garage."""
    config.set_preview_context(None)
    config.set_context("race", notify=False)
    hud = _hud(ir={"IsInGarage": False, "IsGarageVisible": True})
    hud._maybe_auto_switch_preset = lambda: None  # type: ignore
    AdvancedSimHUD._update_context(hud)
    assert config.active_context() == "garage"
    config.set_preview_context(None)


def test_race_when_neither_garage_flag():
    config.set_preview_context(None)
    config.set_context("garage", notify=False)
    hud = _hud(ir={"IsInGarage": False, "IsGarageVisible": False})
    hud._maybe_auto_switch_preset = lambda: None  # type: ignore
    AdvancedSimHUD._update_context(hud)
    assert config.active_context() == "race"
    config.set_preview_context(None)


def test_focus_car_idx_prefers_player_then_cam():
    hud = _hud(ir={"PlayerCarIdx": 2, "CamCarIdx": 5}, _driver_car_idx=7)
    assert hud._focus_car_idx() == 2
    hud.ir = {"PlayerCarIdx": -1, "CamCarIdx": 5}
    assert hud._focus_car_idx() == 5
    hud.ir = {"PlayerCarIdx": -1, "CamCarIdx": -1}
    hud._driver_car_idx = 7
    assert hud._focus_car_idx() == 7
    hud._driver_car_idx = None
    assert hud._focus_car_idx() is None


def test_car_idx_or_none_rejects_negative():
    hud = _hud()
    assert hud._car_idx_or_none(-1) is None
    assert hud._car_idx_or_none(0) == 0
    assert hud._car_idx_or_none(3) == 3


def test_leader_car_idx():
    hud = _hud()
    assert hud._leader_car_idx([0, 3, 1, 2]) == 2
    assert hud._leader_car_idx([0, 0, 0]) is None


def test_radio_tower_shows_speaker_without_player():
    hud = _hud()

    class _W:
        data = None

        def set_data(self, d):
            self.data = d

    hud.radio_tower_widget = _W()
    hud.edit_mode_enabled = lambda: False  # type: ignore
    hud._is_pro_driver_name = lambda n: False  # type: ignore
    hud._group_badge_fields = lambda n: ("", "")  # type: ignore
    hud._driver_for_row = lambda idx, player, drivers: {  # type: ignore
        "UserName": "Caller", "CarNumber": "9"}
    hud._update_radio_tower([1, 2, 3], {}, None, 1)
    assert hud.radio_tower_widget.data["rows"]
    assert hud.radio_tower_widget.data["rows"][0]["name"] == "Caller"
    assert hud.radio_tower_widget.data["rows"][0]["is_player"] is False


def test_update_map_continues_without_focus():
    """Spectating: no focus car still loads weather/zones (past the old early return)."""
    hud = _hud(ir={"WindDir": 0, "WindVel": 0, "CarIdxOnPitRoad": [False, False]})
    hud._ensure_track = lambda *a, **k: None  # type: ignore
    hud._pit_latch_seed_pending = False
    hud._track_zones = {}
    zones = []

    class _Map:
        def set_wind(self, *a):
            pass

        def set_weather(self, *a):
            pass

        def set_track_zones(self, **k):
            zones.append(True)
            raise RuntimeError("stop-after-zones")

    hud.map_widget = _Map()
    try:
        hud._update_map(None, [0.1, 0.2], [3, 3], {})
    except RuntimeError as exc:
        assert "stop-after-zones" in str(exc)
    assert zones == [True]
