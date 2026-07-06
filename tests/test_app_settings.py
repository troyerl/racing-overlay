"""Shared Mongo app settings (demo track id) helpers."""

from __future__ import annotations

from unittest.mock import MagicMock

from overlay import track_store


def test_save_app_settings_requires_write(monkeypatch):
    monkeypatch.setattr(track_store, "can_write", lambda: False)
    assert track_store.save_app_settings({"demo_track_id": 451}) is False


def test_save_app_settings_upserts(monkeypatch):
    mock_col = MagicMock()
    monkeypatch.setattr(track_store, "can_write", lambda: True)
    monkeypatch.setattr(
        track_store, "_settings_collection", lambda write=False: mock_col)
    ok = track_store.save_app_settings({
        "demo_track_id": 451,
        "demo_track_name": "Road America",
    })
    assert ok is True
    mock_col.update_one.assert_called_once()
    payload = mock_col.update_one.call_args[0][1]["$set"]
    assert payload["demo_track_id"] == 451
    assert payload["demo_track_name"] == "Road America"
    assert "updated_at" in payload


def test_fetch_app_settings_returns_doc(monkeypatch):
    mock_col = MagicMock()
    mock_col.find_one.return_value = {
        "_id": "global",
        "demo_track_id": 451,
        "demo_track_name": "Road America",
    }
    monkeypatch.setattr(
        track_store, "_settings_collection", lambda write=False: mock_col)
    doc = track_store.fetch_app_settings()
    assert doc == {
        "demo_track_id": 451,
        "demo_track_name": "Road America",
    }


def test_app_settings_cache_roundtrip(tmp_path):
    td = str(tmp_path)
    track_store.write_app_settings_cache(td, {
        "demo_track_id": 451,
        "demo_track_name": "Road America",
    })
    loaded = track_store.load_app_settings_cache(td)
    assert loaded["demo_track_id"] == 451
    assert loaded["demo_track_name"] == "Road America"
