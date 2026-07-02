"""Map competitor dot colors from lap distance."""

from __future__ import annotations

from overlay import config
from overlay.app import AdvancedSimHUD


def _hud_with_laps(player_lap, player_pct, other_lap, other_pct):
    hud = object.__new__(AdvancedSimHUD)
    hud._lap_pct = [other_pct, other_pct, other_pct, other_pct, player_pct]
    car_lap = [0, 0, 0, 0, player_lap]
    car_lap[0] = other_lap
    return hud, car_lap


def test_same_lap_uses_competitor_color():
    hud, car_lap = _hud_with_laps(5, 0.5, 5, 0.52)
    color = hud._map_car_color(0, 4, car_lap, hud._lap_pct)
    assert color == config.CFG["map"]["colors"]["competitor"]


def test_lap_down_uses_blue():
    hud, car_lap = _hud_with_laps(6, 0.5, 5, 0.48)
    color = hud._map_car_color(0, 4, car_lap, hud._lap_pct)
    assert color == config.CFG["map"]["colors"]["lapped"]


def test_lap_ahead_far_stays_competitor():
    hud, car_lap = _hud_with_laps(5, 0.5, 6, 0.05)
    color = hud._map_car_color(0, 4, car_lap, hud._lap_pct)
    assert color == config.CFG["map"]["colors"]["competitor"]


def test_lap_ahead_close_uses_red():
    hud, car_lap = _hud_with_laps(5, 0.5, 6, 0.52)
    color = hud._map_car_color(0, 4, car_lap, hud._lap_pct)
    assert color == config.CFG["map"]["colors"]["lapping"]


def test_full_lap_ahead_uses_red_when_far():
    hud, car_lap = _hud_with_laps(5, 0.5, 6, 0.05)
    car_lap[0] = 7
    color = hud._map_car_color(0, 4, car_lap, hud._lap_pct)
    assert color == config.CFG["map"]["colors"]["lapping"]
