"""Loop-only track save and session demo preview."""

from __future__ import annotations

import json
from unittest.mock import MagicMock

from overlay import track_store
from overlay.app import AdvancedSimHUD


class _FakeMap:
    path = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    start_finish = 0.0
    num_turns = 4

    def display_corners(self):
        return [{"pct": 0.25, "label": 1}]

    def pit_edit_snapshot(self):
        return [], []


def _author_hud(tmp_path, **kwargs) -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud._v2_authoring_track_id = kwargs.get("tid", 451)
    hud._v2_authoring_name = "Road America"
    hud._learn_name = ""
    hud._track_turns = None
    hud._v2_loop_doc = None
    hud._pit_speed_ms = 0.0
    hud._pit_lane_speed_pct = 1.0
    hud.tracks_dir = str(tmp_path)
    hud.demo = kwargs.get("demo", False)
    hud._session_demo_track_id = None
    hud.map_widget = _FakeMap()
    hud._track_sync = MagicMock()
    return hud


def test_save_loop_v2_writes_without_pit(tmp_path, monkeypatch):
    hud = _author_hud(tmp_path)
    monkeypatch.setattr(track_store, "can_write", lambda: False)
    ok, msg = hud.save_loop_v2()
    assert ok is True
    assert "no pit lane" in msg
    path = tmp_path / "451.json"
    assert path.is_file()
    doc = json.loads(path.read_text())
    assert doc["track_id"] == 451
    assert len(doc["points"]) == 4
    assert "pit_path" not in doc
    assert hud._session_demo_track_id == "451"


def test_save_loop_v2_uploads_when_author(tmp_path, monkeypatch):
    hud = _author_hud(tmp_path)
    monkeypatch.setattr(track_store, "can_write", lambda: True)
    ok, _msg = hud.save_loop_v2()
    assert ok is True
    hud._track_sync.upload_local_async.assert_called_once_with(str(tmp_path), 451)


def test_save_manual_track_v2_requires_pit(tmp_path):
    hud = _author_hud(tmp_path)
    ok, msg = hud.save_manual_track_v2()
    assert ok is False
    assert "pit road" in msg


def test_upload_doc_loop_only_unsets_pit(monkeypatch):
    mock_col = MagicMock()
    monkeypatch.setattr(track_store, "can_write", lambda: True)
    monkeypatch.setattr(track_store, "_collection", lambda write=False: mock_col)
    doc = {
        "track_id": 451,
        "points": [[0.0, 0.0], [1.0, 0.0], [0.5, 0.5]],
        "name": "Test",
    }
    assert track_store.upload_doc(doc, loop_only=True) is True
    update = mock_col.update_one.call_args[0][1]
    assert "$unset" in update
    assert "pit_path" in update["$unset"]


def test_local_is_loop_only():
    assert track_store._local_is_loop_only({"points": [[0, 0]]}) is True
    assert track_store._local_is_loop_only({
        "points": [[0, 0]],
        "pit_path": [[0, 0], [1, 1]],
    }) is False


def test_preview_uploaded_track_in_demo_sets_session_id(tmp_path):
    hud = _author_hud(tmp_path, demo=False)
    hud._load_demo_track = MagicMock()
    hud._preview_uploaded_track_in_demo(451)
    assert hud._session_demo_track_id == "451"
    hud._load_demo_track.assert_not_called()


def test_preview_uploaded_track_in_demo_reloads_when_demo(tmp_path):
    hud = _author_hud(tmp_path, demo=True)
    hud._load_demo_track = MagicMock()
    hud.map_widget.flash_hint = MagicMock()
    hud._preview_uploaded_track_in_demo(451)
    hud._load_demo_track.assert_called_once()
    hud.map_widget.flash_hint.assert_called_once()
