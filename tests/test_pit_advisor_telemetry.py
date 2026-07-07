"""Telemetry contract tests for pit advisor SDK reads."""

from __future__ import annotations

from overlay import telemetry as tele


class _FakeIR(dict):
    pass


def test_read_pit_advisor_telemetry_minimal():
    ir = _FakeIR({
        "PlayerCarIdx": 2,
        "Lap": 10,
        "FuelLevel": 42.0,
        "LFwearM": 0.55,
    })
    out = tele.read_pit_advisor_telemetry(ir, {"fuel_max": 100.0, "est_lap": 90.0})
    assert out["player"] == 2
    assert out["lap"] == 10
    assert out["fuel_level"] == 42.0
    assert out["fuel_max"] == 100.0
    assert out["tire_corners"]["lf"]["wear"] == 0.55
    assert out["positions"] is None


def test_resolve_tire_inventory_sdk_available():
    telemetry = {"tire_sets_available": 2, "tire_sets_used": 1}
    inv = tele.resolve_tire_inventory(telemetry, {})
    assert inv["sets_limited"] is True
    assert inv["sets_remaining"] == 2
    assert inv["sets_total"] == 3
    assert inv["current_set"] == 2
    assert inv["inventory_source"] == "sdk"


def test_resolve_tire_inventory_manual_fallback():
    telemetry = {}
    cfg = {"race_tire_sets_total": 4, "tire_sets_reserve": 1}
    inv = tele.resolve_tire_inventory(telemetry, cfg, pit_stops_count=1)
    assert inv["sets_limited"] is True
    assert inv["sets_total"] == 4
    assert inv["sets_remaining"] == 3
    assert inv["inventory_source"] == "manual"
    assert inv["current_set"] == 2


def test_resolve_tire_inventory_unlimited():
    telemetry = {"tire_sets_available": 255}
    inv = tele.resolve_tire_inventory(telemetry, {})
    assert inv["sets_limited"] is False
    assert inv["inventory_source"] == "unlimited"


def test_resolve_tire_inventory_exhausted_blocks_window():
    telemetry = {"tire_sets_available": 0, "tire_sets_used": 4}
    inv = tele.resolve_tire_inventory(telemetry, {})
    assert inv["tire_inventory_exhausted"] is True
    assert inv["inventory_blocks_window"] is True


def test_read_pit_menu_decode():
    menu = tele.read_pit_menu({"pit_sv_flags": 0x0010 | 0x0001, "pit_sv_fuel": 40.0})
    assert menu["fuel_queued"] is True
    assert menu["tires_queued"] is True
    assert menu["pit_sv_fuel"] == 40.0
