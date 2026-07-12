"""iRating projection field assembly and SOF formula tests."""

from overlay import irating_calc as ic


def test_calculate_deltas_irating_rs_reference():
    """Turbo87/irating-rs example: P5 @ 1250 in a 5-starter + 1 DNS field."""
    entries = [
        (3203, True), (3922, True), (2974, True), (1739, True), (1250, True),
        (2588, False),
    ]
    deltas = ic.calculate_deltas(entries)
    assert deltas[4] == -7


def test_round_half_away():
    assert ic._round_half_away(66.5) == 67
    assert ic._round_half_away(-66.5) == -67
    assert ic._round_half_away(-66.4) == -66
    assert ic._round_half_away(66.4) == 66


def test_delta_rounds_new_rating_not_raw_change():
    """irating-rs: delta = round(start + change) - start (not round(change))."""
    # Banker's round on the raw change disagrees with the new-rating rule.
    change = -67.5
    assert int(round(change)) == -68
    assert ic._delta_from_change(2000, change) == -67
    change_pos = 66.5
    assert int(round(change_pos)) == 66
    assert ic._delta_from_change(2000, change_pos) == 67


def test_dns_count_affects_p7_low_irating():
    """More registered DNS shifts a below-SoF P7 from negative toward positive."""
    base = [1314] * 6 + [1184] + [1314] * 3
    d0 = ic.calculate_deltas([(ir, True) for ir in base])[6]
    d1 = ic.calculate_deltas([(ir, True) for ir in base] + [(1314, False)])[6]
    d3 = ic.calculate_deltas(
        [(ir, True) for ir in base] + [(1314, False)] * 3)[6]
    assert d0 < d1 < d3
    assert abs(d1 - (-13)) <= 2
    assert abs(d3 - 8) <= 2


def test_started_flag_overrides_zero_position():
    drivers = {
        0: {"IRating": 1500, "CarClassID": 1},
        1: {"IRating": 1400, "CarClassID": 1},
        2: {"IRating": 1300, "CarClassID": 1},
    }
    positions = [0, 1, 2]
    class_pos = [0, 1, 2]
    dns_deltas = ic.project_deltas_by_class(
        drivers, class_pos, positions, set())
    started = {0: True, 1: True, 2: True}
    starter_deltas = ic.project_deltas_by_class(
        drivers, class_pos, positions, set(), started_by_idx=started)
    assert dns_deltas[0] != starter_deltas[0]


def test_dns_included_in_class_projection():
    """Registered DNS (position 0) stays in num_reg as a non-starter."""
    drivers = {
        0: {"IRating": 1500, "CarClassID": 0, "IsSpectator": False},
        1: {"IRating": 1400, "CarClassID": 0, "IsSpectator": False},
        2: {"IRating": 1300, "CarClassID": 0, "IsSpectator": False},
        3: {"IRating": 1200, "CarClassID": 0, "IsSpectator": False},
    }
    positions = [1, 2, 3, 0]
    class_pos = [1, 2, 3, 0]
    started = {0: True, 1: True, 2: True, 3: False}
    deltas_with_dns = ic.project_deltas_by_class(
        drivers, class_pos, positions, set(), started_by_idx=started)
    started_all = {i: True for i in drivers}
    deltas_no_dns = ic.project_deltas_by_class(
        drivers, class_pos, positions, set(), started_by_idx=started_all)
    assert deltas_with_dns[0] != deltas_no_dns[0]


def test_results_positions_used_for_finish_order():
    drivers = {
        0: {"IRating": 2000, "CarClassID": 1},
        1: {"IRating": 1500, "CarClassID": 1},
        2: {"IRating": 1000, "CarClassID": 1},
    }
    live_pos = [2, 1, 3]
    live_cls = [2, 1, 3]
    results_pos = [0] * 3
    results_cls = [0] * 3
    results_pos[2] = 1
    results_cls[2] = 1
    results_pos[0] = 2
    results_cls[0] = 2
    results_pos[1] = 3
    results_cls[1] = 3
    started = {0: True, 1: True, 2: True}
    live_deltas = ic.project_deltas_by_class(
        drivers, live_cls, live_pos, set(), started_by_idx=started)
    result_deltas = ic.project_deltas_by_class(
        drivers, live_cls, live_pos, set(),
        started_by_idx=started,
        results_class_positions=results_cls,
        results_positions=results_pos,
    )
    assert live_deltas[2] != result_deltas[2]
