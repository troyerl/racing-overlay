"""Pit route latch: phased membership gated by pit telemetry."""

from __future__ import annotations

from overlay.app import AdvancedSimHUD, PIT_COMMIT_HOLD


def _chicagoland_hud():
    hud = object.__new__(AdvancedSimHUD)
    hud._pit_source = "manual"
    hud._player_on_route = False
    hud._pit_route_latch = {}
    hud.demo = False
    hud._pit_in_pct = 0.82745
    hud._pit_out_pct = 0.38967
    hud._pit_span = (0.86696, 0.95335)
    return hud


def test_chicagoland_on_track_not_on_route():
    hud = _chicagoland_hud()
    route = (hud._pit_in_pct, hud._pit_out_pct)
    assert not hud._car_on_route(
        0, 0.90, False, is_player=True, route=route, approaching=False)


def test_chicagoland_on_pit_is_on_route():
    hud = _chicagoland_hud()
    route = (hud._pit_in_pct, hud._pit_out_pct)
    assert hud._car_on_route(
        0, 0.90, True, is_player=True, route=route, approaching=False)


def test_player_entry_when_approaching_pits():
    hud = _chicagoland_hud()
    route = (hud._pit_in_pct, hud._pit_out_pct)
    pct = 0.85
    assert hud._car_on_route(
        0, pct, False, is_player=True, route=route, approaching=True)


def test_player_entry_without_approaching():
    hud = _chicagoland_hud()
    route = (hud._pit_in_pct, hud._pit_out_pct)
    pct = 0.85
    assert not hud._car_on_route(
        0, pct, False, is_player=True, route=route, approaching=False)


def test_opponent_exit_hold_after_pit():
    hud = _chicagoland_hud()
    route = (hud._pit_in_pct, hud._pit_out_pct)
    hud._pit_route_latch[3] = True
    assert hud._car_on_route(
        3, 0.02, False, is_player=False, route=route, approaching=False)


def test_opponent_exit_hold_until_past_pit_out():
    """Latch stays until past pit_out, not only during the exit-blend phase."""
    hud = _chicagoland_hud()
    route = (hud._pit_in_pct, hud._pit_out_pct)
    hud._pit_route_latch[3] = True
    # Still inside pit_in..pit_out (wrapping), after lane hi, before pit_out.
    assert hud._car_on_route(
        3, 0.20, False, is_player=False, route=route, approaching=False)
    assert hud._pit_route_latch[3]
    # Past pit_out on the racing line — clear latch.
    assert not hud._car_on_route(
        3, 0.50, False, is_player=False, route=route, approaching=False)
    assert not hud._pit_route_latch.get(3)


def test_seed_pit_latches_exit_hold():
    hud = _chicagoland_hud()
    hud._pit_source = "manual"
    route = (hud._pit_in_pct, hud._pit_out_pct)
    lap_pct = [None] * 4
    lap_pct[2] = 0.02
    on_pit = [False, False, False, False]
    hud._seed_pit_latches(lap_pct, on_pit, player=0)
    assert hud._pit_route_latch[2]
    assert hud._car_on_route(
        2, 0.02, False, is_player=False, route=route, approaching=False)


def test_seed_pit_latches_player_on_pit():
    hud = _chicagoland_hud()
    hud._pit_source = "manual"
    lap_pct = [0.90] + [None] * 3
    on_pit = [True, False, False, False]
    hud._seed_pit_latches(lap_pct, on_pit, player=0)
    assert hud._player_on_route
    assert hud._player_route_ticks == PIT_COMMIT_HOLD


def test_demo_pit_car_entry_blend():
    hud = object.__new__(AdvancedSimHUD)
    hud._pit_source = "manual"
    hud._pit_route_latch = {}
    hud.demo = True
    hud._pit_in_pct = 0.90
    hud._pit_out_pct = 0.12
    hud._pit_span = (0.95, 0.06)
    hud.ir = type("IR", (), {"_pit_cars": {2}})()
    route = (0.90, 0.12)
    assert hud._car_on_route(2, 0.92, False, is_player=False, route=route)
    assert hud._pit_route_latch[2]
