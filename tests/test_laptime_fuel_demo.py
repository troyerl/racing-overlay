"""Lap log + fuel calc telemetry integration (demo and helpers)."""

import time

from overlay import demo_data


def test_demo_lap_last_lap_time_tracks_pace():
    ir = demo_data.FakeIRSDK()
    t1 = ir["LapLastLapTime"]
    time.sleep(0.05)
    # Advance far enough to cross a lap boundary in total_laps.
    ir._start -= ir.lap_time * 1.2
    t2 = ir["LapLastLapTime"]
    assert 30.0 < t1 < 35.0
    assert 30.0 < t2 < 35.0


def test_demo_fuel_burn_matches_use_per_hour():
    ir = demo_data.FakeIRSDK()
    per_hr = ir["FuelUsePerHour"]
    burn = ir._fuel_burn_per_sec()
    per_lap = burn * ir.lap_time
    assert abs(per_hr * (ir.lap_time / 3600.0) - per_lap) < 0.01
    level0 = ir["FuelLevel"]
    ir._start -= ir.lap_time
    level1 = ir["FuelLevel"]
    assert abs((level0 - level1) - per_lap) < 0.15


def test_player_last_lap_time_fallback(monkeypatch):
    from overlay.app import AdvancedSimHUD

    class FakeIR:
        def __getitem__(self, key):
            if key == "LapLastLapTime":
                return 0.0
            if key == "PlayerCarIdx":
                return 2
            if key == "CarIdxLastLapTime":
                return [0.0, 0.0, 41.512, 0.0]
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.ir = FakeIR()
    assert hud._player_last_lap_time() == 41.512


def test_fuel_lap_secs_prefers_est_when_log_mismatch():
    from overlay.app import AdvancedSimHUD

    hud = object.__new__(AdvancedSimHUD)
    hud._car_info = {"est_lap": 32.0}
    hud._ll_laps = [{"lap": 1, "secs": 136.0, "temp_c": 20.0}]
    assert hud._fuel_lap_secs() == 32.0


def test_fuel_lap_secs_uses_log_when_close_to_est():
    from overlay.app import AdvancedSimHUD

    hud = object.__new__(AdvancedSimHUD)
    hud._car_info = {"est_lap": 32.0}
    hud._ll_laps = [{"lap": 1, "secs": 32.4, "temp_c": 20.0}]
    assert abs(hud._fuel_lap_secs() - 32.4) < 0.01


def test_build_laptime_rows_personal_best_delta(monkeypatch):
    from overlay import config
    from overlay.app import AdvancedSimHUD

    monkeypatch.setitem(config.CFG.setdefault("laptime_log", {}),
                        "delta_mode", "personal_best")
    monkeypatch.setitem(config.CFG["laptime_log"], "rows", 5)
    hud = object.__new__(AdvancedSimHUD)
    hud._ll_laps = [
        {"lap": 2, "secs": 32.5, "temp_c": 25.0, "personal_best": 32.0},
    ]
    rows = hud._build_laptime_rows()
    assert rows["rows"][0]["delta"] == 0.5


def test_build_laptime_rows_optional_columns(monkeypatch):
    from overlay import config
    from overlay.app import AdvancedSimHUD

    monkeypatch.setitem(
        config.CFG.setdefault("laptime_log", {}),
        "column_order",
        ["lap", "time", "sectors", "fuel", "tag"],
    )
    hud = object.__new__(AdvancedSimHUD)
    hud._ll_laps = [{
        "lap": 1, "secs": 33.0, "temp_c": 20.0,
        "sectors": [11.0, 11.5, 10.5],
        "fuel_l": 2.3,
        "tag": "OUT",
    }]
    row = hud._build_laptime_rows()["rows"][0]
    assert "11.0" in row["sectors"]
    assert row["tag"] == "OUT"


def test_fuel_strip_now_index_from_race_progress(monkeypatch):
    from overlay.app import AdvancedSimHUD

    hud = object.__new__(AdvancedSimHUD)
    hud.ir = type("IR", (), {
        "__getitem__": lambda s, k: {
            "FuelLevel": 40.0, "Lap": 26, "SessionLapsTotal": 50,
            "SessionLapsRemainEx": 24, "SessionTimeRemain": 900.0,
            "FuelUsePerHour": 180.0, "FuelLevelPct": 55.0,
        }[k],
    })()
    hud._fc_use = [2.0]
    hud._fc_prev_lap = 26
    hud._fc_lap_start_fuel = 42.0
    hud._car_info = {"est_lap": 32.0}
    hud._ll_laps = []
    hud.fuel_widget = type("W", (), {"set_data": lambda s, d: setattr(s, "d", d)})()
    monkeypatch.setattr(hud, "_fuel_capacity", lambda fuel: 80.0)
    hud._update_fuel_calc()
    strip = hud.fuel_widget.d.get("strip") or {}
    assert strip.get("now") is not None
    assert strip["now"] == 12


def test_demo_handbrake_and_torque_available():
    ir = demo_data.FakeIRSDK()
    assert isinstance(ir["HandbrakeRaw"], (int, float))
    assert isinstance(ir["SteeringWheelPctTorque"], (int, float))


def test_needs_sector_timer_lap_log_sectors(monkeypatch):
    from overlay import config
    from overlay.app import AdvancedSimHUD

    monkeypatch.setitem(config.CFG.setdefault("sector_timing", {}), "show", False)
    monkeypatch.setitem(config.CFG.setdefault("laptime_log", {}), "show", True)
    monkeypatch.setitem(
        config.CFG["laptime_log"], "column_order",
        ["lap", "time", "sectors"],
    )
    en = {"sector_timing": False, "laptime_log": True, "map": False}
    assert AdvancedSimHUD._needs_sector_timer(en)


def test_needs_lap_engine_dash_lap_corners(monkeypatch):
    from overlay import config
    from overlay.app import AdvancedSimHUD

    monkeypatch.setitem(config.CFG.setdefault("lap_compare", {}), "show", False)
    monkeypatch.setitem(config.CFG.setdefault("dash", {}), "show", True)
    monkeypatch.setitem(config.CFG["dash"], "strip_right", "lap_corners")
    en = {"lap_compare": False, "dash": True}
    assert AdvancedSimHUD._needs_lap_engine(en)


def test_track_fuel_per_lap_without_fuel_widget(monkeypatch):
    from overlay import config
    from overlay.app import AdvancedSimHUD

    monkeypatch.setitem(config.CFG.setdefault("fuel_calc", {}), "show", False)
    monkeypatch.setitem(config.CFG.setdefault("laptime_log", {}), "show", True)
    monkeypatch.setitem(
        config.CFG["laptime_log"], "column_order",
        ["lap", "time", "fuel"],
    )

    class FakeIR:
        def __init__(self):
            self._lap = 5
            self._fuel = 50.0

        def __getitem__(self, key):
            if key == "FuelLevel":
                return self._fuel
            if key == "Lap":
                return self._lap
            if key == "FuelLevelPct":
                return 0.5
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.ir = FakeIR()
    hud._fc_prev_lap = 5
    hud._fc_lap_start_fuel = 52.0
    hud._fc_use = []
    monkeypatch.setattr(hud, "_fuel_capacity", lambda fuel: 100.0)
    hud.ir._lap = 6
    hud.ir._fuel = 49.8
    hud._track_fuel_per_lap()
    assert len(hud._fc_use) == 1
    assert abs(hud._fc_use[0] - 2.2) < 0.01
