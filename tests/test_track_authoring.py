"""Unit tests for shared track authoring helpers."""

from __future__ import annotations

import json

from overlay import track_authoring as ta
from overlay import track_store


def test_xy_list_normalizes():
    assert ta.xy_list([[1, 2], (3.5, 4)]) == [(1.0, 2.0), (3.5, 4.0)]
    assert ta.xy_list([{"x": 1, "y": 2}]) == [(1.0, 2.0)]
    assert ta.xy_list(None) == []


def test_build_manual_pit_lane_fields(tmp_path, monkeypatch):
    monkeypatch.setattr(track_store, "can_write", lambda: False)
    loop = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    road = [(0.2, 0.4), (0.5, 0.42), (0.7, 0.45)]
    merge = [(0.7, 0.45), (0.9, 0.48)]
    fields = ta.build_manual_pit_lane_fields(loop, [], road, merge)
    assert len(fields["pit_path"]) >= 2
    assert len(fields["pit_out"]) >= 2
    assert "pit_span" in fields


def test_save_manual_track_writes(tmp_path, monkeypatch):
    monkeypatch.setattr(track_store, "can_write", lambda: False)
    loop = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    road = [(0.2, 0.4), (0.5, 0.42)]
    merge = [(0.5, 0.42), (0.9, 0.48)]
    ok, msg, lane1 = ta.save_manual_track(
        str(tmp_path),
        tid=99,
        loop=loop,
        entry=[],
        road=road,
        merge=merge,
        name="Test",
    )
    assert ok is True
    assert lane1
    path = tmp_path / "99.json"
    assert path.is_file()
    doc = json.loads(path.read_text())
    assert doc["track_id"] == 99
    assert len(doc["pit_path"]) >= 2
    assert "Saved" in msg
