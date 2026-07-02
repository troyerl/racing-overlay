"""Pit speed limit and pit lane speed are manual-only (not learned from telemetry)."""

from overlay.app import AdvancedSimHUD


def test_pit_speed_not_learned_from_driving():
    assert not hasattr(AdvancedSimHUD, "_learn_pit_speed")
