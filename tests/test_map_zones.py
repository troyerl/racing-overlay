"""Tests for track map zone parsing."""

from __future__ import annotations

import json
import tempfile
from pathlib import Path

from overlay.widgets.track_map import _parse_zone_ranges, load_track


def test_parse_zone_ranges():
    assert _parse_zone_ranges([[0.1, 0.2], (0.5, 0.55)]) == [
        (0.1, 0.2), (0.5, 0.55),
    ]
    assert _parse_zone_ranges([]) == []
    assert _parse_zone_ranges("bad") == []


def test_load_track_reads_drs_and_p2p_zones():
    data = {
        "points": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        "start_finish": 0.0,
        "drs_zones": [[0.55, 0.62]],
        "p2p_zones": [[0.2, 0.25], [0.8, 0.85]],
    }
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "test_track.json"
        path.write_text(json.dumps(data), encoding="utf-8")
        _, _, _, _, meta = load_track(str(path), n=32)
    assert meta["drs_zones"] == [(0.55, 0.62)]
    assert meta["p2p_zones"] == [(0.2, 0.25), (0.8, 0.85)]
