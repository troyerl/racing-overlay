"""Pit route latch: cars should ride entry blends before OnPitRoad."""

from __future__ import annotations

from overlay.app import AdvancedSimHUD


def test_schematic_player_on_route_during_entry_blend():
    hud = object.__new__(AdvancedSimHUD)
    hud._pit_source = "manual"
    hud._player_on_route = False
    hud._pit_route_latch = {}
    route = (0.90, 0.12)
    pct = 0.92
    on_pit = False
    assert hud._car_on_route(0, pct, on_pit, is_player=True, route=route)
    assert not hud._car_on_route(0, 0.50, on_pit, is_player=True, route=route)


def test_schematic_opponent_latches_on_entry_blend():
    hud = object.__new__(AdvancedSimHUD)
    hud._pit_source = "manual"
    hud._pit_route_latch = {}
    route = (0.90, 0.12)
    assert hud._car_on_route(3, 0.91, False, is_player=False, route=route)
    assert hud._pit_route_latch[3]
