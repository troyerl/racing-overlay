"""Leaderboard strip row builder tests."""

from overlay import traffic as tr
from overlay.app import AdvancedSimHUD


def _hud(**kwargs) -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud._pace_idxs = set()
    for k, v in kwargs.items():
        setattr(hud, k, v)
    return hud


def test_leaderboard_strip_top_three_order():
    hud = _hud(
        ir=type("IR", (), {"__getitem__": lambda s, k: None})(),
    )
    positions = [2, 1, 3, 4, 5]
    drivers = {
        0: {"CarNumber": "48", "UserName": "Player", "CarClassColor": "#3aa0ff"},
        1: {"CarNumber": "11", "UserName": "Leader", "CarClassColor": "#ff5bac"},
        2: {"CarNumber": "24", "UserName": "Third", "CarClassColor": "#b06bff"},
    }
    car_f2 = [2.1, 0.0, 1.5, 3.0, 4.0]
    hud.leaderboard_strip_widget = type(
        "W", (), {"set_data": lambda s, d: setattr(s, "d", d)})()
    hud.edit_mode_enabled = lambda: False
    hud._update_leaderboard_strip(positions, drivers, car_f2, 32.0, player=0)
    rows = hud.leaderboard_strip_widget.d["rows"]
    assert len(rows) == 3
    assert [r["position"] for r in rows] == [1, 2, 3]
    assert rows[0]["gap"] == "\u2014"
    assert rows[1]["gap"].startswith("+")


def test_fmt_leader_gap_seconds():
    assert tr.fmt_leader_gap(2.5, 2, 32.0) == "+2.5"
    assert tr.fmt_leader_gap(0.0, 1, 32.0) == "\u2014"
    assert tr.fmt_leader_gap(35.0, 2, 32.0) == "-1L"


def test_leaderboard_edit_preview_without_positions():
    hud = _hud(
        ir=type("IR", (), {"__getitem__": lambda s, k: None})(),
    )
    hud.leaderboard_strip_widget = type(
        "W", (), {"set_data": lambda s, d: setattr(s, "d", d)})()
    hud.edit_mode_enabled = lambda: True
    hud._update_leaderboard_strip(None, {}, None, 32.0, player=0)
    payload = hud.leaderboard_strip_widget.d
    assert payload.get("edit") is True
