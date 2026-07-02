"""Tests for standings row assembly (podium pin + centered window)."""

from overlay.standings_rows import empty_row, standings_row_list


def _ranked(n: int) -> list[int]:
    return list(range(n))


def _build(idx: int) -> dict:
    return {"key": f"car{idx}", "pos": idx + 1}


def _positions(rows: list[dict]) -> list[int]:
    return [r.get("pos", 0) for r in rows if not r.get("empty")]


def test_podium_then_window_around_player():
    ranked = _ranked(20)
    player = ranked[9]  # P10
    rows = standings_row_list(
        ranked,
        player=player,
        center_on_player=True,
        pin_podium=True,
        rows=10,
        rows_ahead=4,
        rows_behind=5,
        build=_build,
        empty=empty_row,
    )
    assert len(rows) == 10
    assert _positions(rows[:3]) == [1, 2, 3]
    assert any(r.get("key") == "car9" for r in rows[3:])


def test_podium_dedupes_player_in_top_three():
    ranked = _ranked(20)
    player = ranked[1]  # P2
    rows = standings_row_list(
        ranked,
        player=player,
        center_on_player=True,
        pin_podium=True,
        rows=10,
        rows_ahead=4,
        rows_behind=5,
        build=_build,
        empty=empty_row,
    )
    assert len(rows) == 10
    assert _positions(rows[:3]) == [1, 2, 3]
    keys = [r.get("key") for r in rows if not r.get("empty")]
    assert keys.count("car1") == 1
    assert rows[1]["key"] == "car1"  # player stays on podium


def test_podium_always_shows_player_when_not_on_podium():
    ranked = _ranked(20)
    player = ranked[19]  # P20
    rows = standings_row_list(
        ranked,
        player=player,
        center_on_player=True,
        pin_podium=True,
        rows=10,
        rows_ahead=4,
        rows_behind=5,
        build=_build,
        empty=empty_row,
    )
    assert any(r.get("key") == "car19" for r in rows)


def test_podium_tight_window_still_shows_player():
    ranked = _ranked(20)
    player = ranked[15]  # P16
    rows = standings_row_list(
        ranked,
        player=player,
        center_on_player=True,
        pin_podium=True,
        rows=4,
        rows_ahead=0,
        rows_behind=0,
        build=_build,
        empty=empty_row,
    )
    assert len(rows) == 4
    assert _positions(rows[:3]) == [1, 2, 3]
    assert rows[3]["key"] == "car15"


def test_podium_trims_neighbors_before_dropping_player():
    ranked = _ranked(20)
    player = ranked[9]  # P10
    rows = standings_row_list(
        ranked,
        player=player,
        center_on_player=True,
        pin_podium=True,
        rows=10,
        rows_ahead=1,
        rows_behind=0,
        build=_build,
        empty=empty_row,
    )
    assert len(rows) == 4  # 3 podium + 1 context slot (player only)
    assert rows[3]["key"] == "car9"


def test_pin_off_matches_centered_window():
    ranked = _ranked(20)
    player = ranked[9]
    rows = standings_row_list(
        ranked,
        player=player,
        center_on_player=True,
        pin_podium=False,
        rows=10,
        rows_ahead=4,
        rows_behind=5,
        build=_build,
        empty=empty_row,
    )
    assert len(rows) == 10
    assert _positions(rows) == list(range(6, 16))


def test_small_field_pads_podium():
    ranked = _ranked(2)
    player = ranked[0]
    rows = standings_row_list(
        ranked,
        player=player,
        center_on_player=True,
        pin_podium=True,
        rows=10,
        rows_ahead=4,
        rows_behind=5,
        build=_build,
        empty=empty_row,
    )
    assert len(rows) == 10
    assert rows[0]["key"] == "car0"
    assert rows[1]["key"] == "car1"
    assert rows[2].get("empty") is True


def test_top_n_when_not_centered():
    ranked = _ranked(20)
    rows = standings_row_list(
        ranked,
        player=ranked[9],
        center_on_player=False,
        pin_podium=True,
        rows=5,
        rows_ahead=4,
        rows_behind=5,
        build=_build,
        empty=empty_row,
    )
    assert len(rows) == 5
    assert _positions(rows) == [1, 2, 3, 4, 5]
