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
