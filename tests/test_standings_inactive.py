"""Standings inactive row detection and row payload."""

from overlay import common as oc
from overlay import traffic as tr
from overlay.app import AdvancedSimHUD


def test_is_standings_inactive_garage():
    assert tr.is_standings_inactive(oc.TRK_NOT_IN_WORLD) is True


def test_is_standings_inactive_on_track():
    assert tr.is_standings_inactive(oc.TRK_ON_TRACK) is False


def test_is_standings_inactive_negative_lap_pct():
    assert tr.is_standings_inactive(oc.TRK_ON_TRACK, lap_pct_val=-0.1) is True


def test_build_standings_row_inactive_in_garage(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("standings", {}), "columns", {
        "position": True, "name": True,
    })
    hud = object.__new__(AdvancedSimHUD)
    hud._pace_idxs = set()
    hud._lap_pct = [0.5, 0.6, 0.3]
    hud._car_last = None
    hud._car_best = None
    hud._irating_deltas = {}
    hud._is_qualifying_session = lambda: False
    hud._lap_tint = lambda *a, **k: (False, False)
    hud._table_extra_fields = lambda *a, **k: {}
    surface = [oc.TRK_NOT_IN_WORLD, oc.TRK_ON_TRACK, oc.TRK_ON_TRACK]
    positions = [1, 2, 3]
    drivers = {
        0: {"UserName": "Garage", "CarNumber": "1", "IRating": 1500},
        1: {"UserName": "On Track", "CarNumber": "2", "IRating": 1400},
    }
    cols = hud._visible_cols("standings")
    row = hud._build_standings_row(
        0, drivers, positions, surface, None, 1, 90.0, cols,
        None, None, "laps_since")
    assert row["inactive"] is True
    row_on = hud._build_standings_row(
        1, drivers, positions, surface, None, 1, 90.0, cols,
        None, None, "laps_since")
    assert row_on["inactive"] is False
