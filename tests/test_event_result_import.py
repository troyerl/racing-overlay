"""Event-result JSON driver name import."""

from __future__ import annotations

from overlay.event_result_import import merge_driver_entries, parse_event_result_names


_SAMPLE = {
    "type": "event_result",
    "data": {
        "session_results": [
            {
                "simsession_name": "RACE",
                "results": [
                    {"display_name": "Alice Driver", "ai": False},
                    {"display_name": "Bob Racer", "ai": False},
                    {"display_name": "AI Bot", "ai": True},
                    {"display_name": "alice driver", "ai": False},
                ],
            },
            {
                "simsession_name": "QUALIFY",
                "results": [
                    {"display_name": "Bob Racer", "ai": False},
                    {"display_name": "Carol Speed", "ai": False},
                ],
            },
        ],
    },
}


def test_parse_event_result_names_unique_humans():
    names = parse_event_result_names(_SAMPLE)
    assert names == ["Alice Driver", "Bob Racer", "Carol Speed"]


def test_parse_bare_data_object():
    names = parse_event_result_names(_SAMPLE["data"])
    assert "Carol Speed" in names
    assert len(names) == 3


def test_merge_skips_name_and_alias():
    existing = [
        {"name": "Alice Driver", "aliases": []},
        {"name": "Dan", "aliases": ["Carol Speed"]},
    ]
    merged, added, skipped = merge_driver_entries(
        existing, ["Alice Driver", "Carol Speed", "Eve New", ""])
    assert added == 1
    assert skipped == 3
    assert merged[-1] == {"name": "Eve New", "aliases": []}
    assert len(merged) == 3
