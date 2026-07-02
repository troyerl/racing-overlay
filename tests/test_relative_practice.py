"""Qualifying standings / session-type behavior."""

from overlay import common as oc
from overlay.app import AdvancedSimHUD


def _hud(*, practice: bool) -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud.demo = False
    hud._demo_active = False
    hud._practice_cache = practice
    hud._qualifying_cache = False
    hud._session_type_cache = "practice" if practice else "race"
    hud._session_info_counter = 1
    hud._pace_idxs = set()
    return hud


def test_is_qualifying_session():
    class FakeIR:
        def __getitem__(self, key):
            if key == "SessionNum":
                return 0
            if key == "SessionInfo":
                return {"Sessions": [{"SessionType": "Qualify"}]}
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.demo = False
    hud._demo_active = False
    hud._practice_cache = None
    hud._qualifying_cache = None
    hud._session_info_counter = 0
    hud.ir = FakeIR()
    assert hud._is_qualifying_session() is True


def test_resolve_positions_uses_live_qual_results():
    class FakeIR:
        def __getitem__(self, key):
            if key == "SessionNum":
                return 0
            if key == "SessionInfo":
                return {"Sessions": [{"SessionType": "Qualify"}]}
            if key == "SessionUniqueID":
                return 99
            if key == "QualifyResultsInfo":
                return {"Results": [
                    {"CarIdx": 0, "Position": 2, "ClassPosition": 2},
                    {"CarIdx": 1, "Position": 1, "ClassPosition": 1},
                ]}
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.demo = False
    hud._demo_active = False
    hud._practice_cache = None
    hud._qualifying_cache = None
    hud._session_info_counter = 0
    hud._pace_idxs = set()
    hud._grid_session_uid = None
    hud._grid_positions_cache = None
    hud.ir = FakeIR()
    live = [0, 0]
    pos, cls = hud._resolve_positions(live, 0)
    assert pos[1] == 1
    assert pos[0] == 2
    assert cls[1] == 1


def test_qual_results_refresh_when_live():
    calls = {"n": 0}

    class FakeIR:
        def __getitem__(self, key):
            if key == "SessionUniqueID":
                return 1
            if key == "QualifyResultsInfo":
                calls["n"] += 1
                return {"Results": [{"CarIdx": 0, "Position": 1}]}
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud._pace_idxs = set()
    hud.ir = FakeIR()
    hud._qualify_grid_positions(live=True)
    hud._qualify_grid_positions(live=True)
    assert calls["n"] == 2


def test_practice_shows_on_track_only():
    hud = _hud(practice=True)
    surface = [oc.TRK_NOT_IN_WORLD, oc.TRK_ON_TRACK, oc.TRK_OFF_TRACK]
    positions = [1, 2, 3]
    player = 2
    assert hud._relative_include(0, surface, positions, player) is False
    assert hud._relative_include(1, surface, positions, player) is True


def test_positions_from_best_lap():
    class FakeIR:
        def __getitem__(self, key):
            if key == "CarIdxBestLapTime":
                return [40.0, 38.5, 0.0]
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud._pace_idxs = set()
    hud.ir = FakeIR()
    pos = hud._positions_from_best_lap([0, 0, 0])
    assert pos[1] == 1
    assert pos[0] == 2
