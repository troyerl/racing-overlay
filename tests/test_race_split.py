"""Race-split helpers from results payloads."""

from __future__ import annotations

from overlay.iracing_results import _split_from_results, _split_info_from_results


def test_split_from_results_ranks_by_sof():
    data = {
        "subsession_id": 200,
        "session_splits": [
            {"subsession_id": 100, "event_strength_of_field": 2000},
            {"subsession_id": 200, "event_strength_of_field": 3500},
            {"subsession_id": 300, "event_strength_of_field": 2800},
        ],
    }
    # Highest SOF first => 200 is split 1, 300 is 2, 100 is 3.
    assert _split_from_results(data, 200) == 1
    assert _split_from_results(data, 300) == 2
    assert _split_from_results(data, 100) == 3
    assert _split_from_results(data, 999) is None
    assert _split_info_from_results(data, 300) == (2, 3)


def test_split_from_results_single():
    assert _split_from_results({"subsession_id": 42}, 42) == 1
    assert _split_from_results({"subsession_id": 42}, 99) is None
    assert _split_info_from_results({"subsession_id": 42}, 42) == (1, 1)
