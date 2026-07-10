"""Fuel calc unit display, burn ordering, and pit-advisor history sharing."""

from __future__ import annotations

from overlay import config
from overlay.app import AdvancedSimHUD
from overlay.pit_strategy import CautionTracker, build_fuel_snapshot, update_caution_tracker
from overlay.widgets import fuel_calc as fc_mod


def _fake_ir(**overrides):
    base = {
        "FuelLevel": 45.0,
        "Lap": 10,
        "SessionLapsRemainEx": 20,
        "SessionTimeRemain": 900.0,
        "FuelUsePerHour": 180.0,
        "FuelLevelPct": 0.56,
        "OnPitRoad": False,
    }
    base.update(overrides)

    class FakeIR:
        def __getitem__(self, key):
            return base[key]

    return FakeIR()


def test_build_fuel_snapshot_usage_and_laps_ordering():
    snap = build_fuel_snapshot(
        _fake_ir(),
        car_info={"est_lap": 32.0, "fuel_max": 80.0},
        fc_use=[1.5, 1.0, 1.2],
        ll_laps=[],
        cfg={},
    )
    rows = snap["rows"]
    assert rows["max"]["usage"] > rows["min"]["usage"]
    assert rows["max"]["laps"] < rows["min"]["laps"]


def test_fmt_fuel_imperial(monkeypatch):
    monkeypatch.setitem(config.CFG, "units", "imperial")
    assert fc_mod._fmt_fuel(45.0) == "11.9"
    assert fc_mod._stat_headers()["usage"] == "USAGE"
    assert fc_mod._stat_headers()["refuel"] == "REFUEL"
    assert fc_mod._STAT_ROW_LABELS["max"] == "MAX"
    assert fc_mod._STAT_ROW_LABELS["min"] == "MIN"


def test_fmt_fuel_metric(monkeypatch):
    monkeypatch.setitem(config.CFG, "units", "metric")
    assert fc_mod._fmt_fuel(45.0) == "45.0"
    assert fc_mod._stat_headers()["usage"] == "USAGE"


def test_needs_fuel_lap_tracking_includes_pit_advisor():
    en = {"fuel_calc": False, "pit_advisor": True, "laptime_log": False}
    assert AdvancedSimHUD._needs_fuel_lap_tracking(en) is True


def test_pit_advisor_caution_reset_preserves_fc_use(monkeypatch):
    hud = object.__new__(AdvancedSimHUD)
    hud._fc_use = [1.5, 1.2, 1.0]
    hud._caution_tracker = CautionTracker()
    hud._caution_tracker.was_yellow = True
    hud._ll_laps = []
    hud._car_info = {"est_lap": 32.0, "fuel_max": 80.0}
    hud._pit = {}
    hud._pace_idxs = set()
    hud._pit_advisor_closing_state = {}
    hud._drivers = lambda: {}
    hud.ir = _fake_ir()
    hud.pit_advisor_widget = type(
        "W", (), {"data": None, "set_data": lambda s, d: setattr(s, "data", d)}
    )()
    hud._session_flag_bundle = lambda *a, **k: (None, {})
    hud.edit_mode_enabled = lambda: False

    monkeypatch.setattr(
        "overlay.telemetry.read_pit_advisor_telemetry",
        lambda ir, car_info: {
            "player": 0,
            "est_lap": 32.0,
            "est_time": [0.0],
            "positions": [1],
            "car_lap": [10],
            "on_pit_road": [False],
            "surface": [0],
            "lap_pcts": [0.5],
            "f2_time": [0.0],
            "car_last": [32.0],
            "car_flags": [0],
            "lap": 10,
            "session_time": 1000.0,
            "session_laps_remain_ex": 20,
            "session_flags": 0,
        },
    )
    monkeypatch.setitem(config.CFG.setdefault("pit_advisor", {}), "show", True)

    update_caution_tracker(
        hud._caution_tracker, yellow=False, lap=10, session_time=1000.0)
    assert hud._caution_tracker.fuel_ema_reset is True

    hud._update_pit_advisor()
    assert hud._fc_use == [1.5, 1.2, 1.0]
