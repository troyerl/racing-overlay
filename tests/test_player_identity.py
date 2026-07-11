"""Player DriverInfo identity: session invalidation + YAML reconciliation."""

from __future__ import annotations

from types import SimpleNamespace

from overlay.app import AdvancedSimHUD


def _harness() -> AdvancedSimHUD:
    """Minimal stand-in with only the identity helpers / cache fields."""
    h = SimpleNamespace()
    h._driver_cache = {}
    h._pace_idxs = set()
    h._driver_car_idx = None
    h._driver_user_id = None
    h._force_driver_refresh = False
    h._driver_refresh_counter = 0
    h._car_info = {}
    h._track_name = ""
    h.ir = None
    # Bind unbound methods from AdvancedSimHUD onto the harness.
    for name in (
        "_invalidate_driver_cache",
        "_int_or_none",
        "_driver_identity_mismatch",
        "_player_driver",
        "_driver_for_row",
        "_drivers",
    ):
        setattr(h, name, getattr(AdvancedSimHUD, name).__get__(h, type(h)))
    return h


def test_invalidate_driver_cache_clears_stale_map():
    h = _harness()
    h._driver_cache = {3: {"UserName": "Wrong Person", "UserID": 99, "CarIdx": 3}}
    h._driver_refresh_counter = 12
    h._driver_car_idx = 3
    h._driver_user_id = 99
    h._invalidate_driver_cache()
    assert h._driver_cache == {}
    assert h._driver_refresh_counter == 0
    assert h._driver_car_idx is None
    assert h._driver_user_id is None
    assert h._force_driver_refresh is True


def test_player_driver_prefers_driver_car_idx():
    h = _harness()
    h._driver_car_idx = 5
    h._driver_user_id = 1001
    h._driver_cache = {
        3: {"UserName": "Wrong Person", "UserID": 99, "CarIdx": 3},
        5: {"UserName": "Me", "UserID": 1001, "CarIdx": 5},
    }
    # Live PlayerCarIdx still 3 (stale slot) — name must come from DriverCarIdx.
    d = h._player_driver(3)
    assert d["UserName"] == "Me"
    row_d = h._driver_for_row(3, player=3, drivers=h._driver_cache)
    assert row_d["UserName"] == "Me"
    other = h._driver_for_row(3, player=5, drivers=h._driver_cache)
    assert other["UserName"] == "Wrong Person"


def test_player_driver_falls_back_to_user_id():
    h = _harness()
    h._driver_car_idx = None
    h._driver_user_id = 1001
    h._driver_cache = {
        2: {"UserName": "Other", "UserID": 50, "CarIdx": 2},
        7: {"UserName": "Me", "UserID": 1001, "CarIdx": 7},
    }
    assert h._player_driver(2)["UserName"] == "Me"


def test_identity_mismatch_detects_wrong_slot_user():
    h = _harness()
    h._driver_user_id = 1001
    h._driver_car_idx = 5
    h._driver_cache = {
        3: {"UserName": "Wrong Person", "UserID": 99, "CarIdx": 3},
        5: {"UserName": "Me", "UserID": 1001, "CarIdx": 5},
    }
    assert h._driver_identity_mismatch(3) is True
    assert h._driver_identity_mismatch(5) is False


def test_drivers_rebuilds_after_invalidate_and_int_keys():
    h = _harness()
    h._driver_cache = {3: {"UserName": "Stale", "UserID": 1, "CarIdx": 3}}
    h._invalidate_driver_cache()

    class _IR:
        def __getitem__(self, key):
            if key == "DriverInfo":
                return {
                    "DriverCarIdx": 5,
                    "DriverUserID": 1001,
                    "Drivers": [
                        {"CarIdx": "3", "UserName": "Wrong Person",
                         "UserID": 99},
                        {"CarIdx": "5", "UserName": "Me", "UserID": 1001},
                        {"CarIdx": 1, "UserName": "Pace", "CarIsPaceCar": 1},
                    ],
                    "DriverCarRedLine": 8000,
                    "DriverCarFuelMaxLtr": 0,
                    "DriverCarMaxFuelPct": 1,
                    "DriverCarEstLapTime": 0,
                }
            raise KeyError(key)

    h.ir = _IR()
    cache = h._drivers(player=3)
    assert set(cache) == {3, 5}  # pace excluded; keys are ints
    assert h._driver_car_idx == 5
    assert h._driver_user_id == 1001
    assert h._player_driver(3)["UserName"] == "Me"
    assert h._force_driver_refresh is False
