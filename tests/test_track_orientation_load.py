"""Cross-device track orientation and cloud-freshness load tests."""

from __future__ import annotations

import json
from unittest.mock import MagicMock

from overlay import config, track_store
from overlay.app import AdvancedSimHUD
from overlay.widgets import track_map


class _FakeMap:
    path = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    start_finish = 0.0
    num_turns = 4
    placeholder = "No track"

    def set_track(self, path, sf=0.0, corners=None):
        self.path = list(path)
        self.start_finish = sf

    def set_num_turns(self, n):
        self.num_turns = n

    def set_track_is_oval(self, _v):
        pass

    def set_track_zones(self, **kwargs):
        pass

    def display_corners(self):
        return [{"pct": 0.25, "label": 1}]

    def flash_hint(self, _msg):
        pass

    def update(self):
        pass

    def pit_edit_snapshot(self):
        return [], []


def _hud(tmp_path, **kwargs) -> AdvancedSimHUD:
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
    hud._track_loaded = False
    hud._loaded_track_updated_at = None
    hud._track_file_checked = False
    hud._track_id = kwargs.get("track_id", 451)
    hud._track_is_oval = False
    hud._track_zones = {"drs_zones": [], "p2p_zones": []}
    hud._pit_path = None
    hud._remote_tried = set()
    hud._no_track_hint = False
    hud.map_widget = _FakeMap()
    hud._track_sync = MagicMock()
    hud._refresh_settings_authoring = MagicMock()
    hud.ir = {"WeekendInfo": {"TrackID": 451, "TrackDisplayName": "Test"}}
    return hud


def _write_track(tmp_path, tid, *, updated_at=None, points=None, **extra):
    doc = {
        "schema": 2,
        "import_version": 2,
        "track_id": tid,
        "name": "Test",
        "points": points or [[0.0, 0.0], [1.0, 0.0], [0.5, 0.5]],
        "start_finish": 0.0,
        "map_rotation": 0,
        "map_mirror": False,
    }
    if updated_at is not None:
        doc["updated_at"] = updated_at
    doc.update(extra)
    path = tmp_path / f"{tid}.json"
    path.write_text(json.dumps(doc), encoding="utf-8")
    return path


def test_needs_cloud_refresh_when_local_lacks_updated_at():
    manifest = {451: "2026-07-06T12:00:00+00:00"}
    local = {"track_id": 451, "points": [[0, 0]]}
    assert track_store.needs_cloud_refresh(451, local, manifest) is True


def test_needs_cloud_refresh_when_timestamps_differ():
    manifest = {451: "2026-07-06T13:00:00+00:00"}
    local = {"track_id": 451, "updated_at": "2026-07-06T12:00:00+00:00"}
    assert track_store.needs_cloud_refresh(451, local, manifest) is True


def test_needs_cloud_refresh_false_when_current():
    ts = "2026-07-06T12:00:00+00:00"
    manifest = {451: ts}
    local = {"track_id": 451, "updated_at": ts}
    assert track_store.needs_cloud_refresh(451, local, manifest) is False


def test_normalize_passes_orientation_fields():
    doc = track_store.normalize({
        "track_id": 1,
        "points": [[0, 0], [1, 0], [0.5, 0.5]],
        "schema": 2,
        "import_version": 2,
        "map_rotation": 0,
        "map_mirror": False,
    })
    assert doc["map_rotation"] == 0
    assert doc["map_mirror"] is False


def test_build_loop_doc_includes_orientation(tmp_path):
    hud = _hud(tmp_path)
    doc = hud._build_loop_doc(451)
    assert doc["map_rotation"] == 0
    assert doc["map_mirror"] is False


def test_write_track_json_stamps_updated_at(tmp_path):
    hud = _hud(tmp_path)
    path = hud._write_track_json(451, {"track_id": 451, "points": [[0, 0]]})
    doc = json.loads(open(path, encoding="utf-8").read())
    assert doc.get("updated_at")


def test_apply_track_orientation_updates_base(tmp_path):
    full = config.base_cfg()
    full.setdefault("map", {})["rotation"] = 90
    full["map"]["mirror"] = True
    config.apply_base(full, notify=False)
    try:
        hud = _hud(tmp_path)
        hud._apply_track_orientation({
            "schema": 2,
            "import_version": 2,
            "map_rotation": 0,
            "map_mirror": False,
        })
        assert config.CFG["map"]["rotation"] == 0
        assert config.CFG["map"]["mirror"] is False
    finally:
        config.reload()


def test_apply_track_from_path_applies_orientation(tmp_path):
    _write_track(tmp_path, 451, map_rotation=0, map_mirror=False)
    path = str(tmp_path / "451.json")
    full = config.base_cfg()
    full.setdefault("map", {})["rotation"] = 270
    config.apply_base(full, notify=False)
    try:
        hud = _hud(tmp_path)
        assert hud._apply_track_from_path(path, "451") is True
        assert config.CFG["map"]["rotation"] == 0
        assert hud._loaded_track_updated_at is None
    finally:
        config.reload()


def test_load_track_meta_includes_orientation(tmp_path):
    _write_track(tmp_path, 451, updated_at="ts1",
                  map_rotation=0, map_mirror=False)
    _pts, _sf, _corners, _name, meta = track_map.load_track(
        str(tmp_path / "451.json"), n=3)
    assert meta["map_rotation"] == 0
    assert meta["map_mirror"] is False
    assert meta["updated_at"] == "ts1"


def test_ensure_track_fetches_when_stale(tmp_path, monkeypatch):
    _write_track(tmp_path, 451, updated_at="old-ts",
                  points=[[0, 0], [1, 0], [0.5, 0.5]])
    hud = _hud(tmp_path)
    monkeypatch.setattr("overlay.app.config.cloud_tracks", lambda: True)
    monkeypatch.setattr(track_store, "cached_manifest",
                        lambda: {451: "new-ts"})
    monkeypatch.setattr(track_store, "needs_cloud_refresh", lambda *_a: True)
    hud._ensure_track(None, None)
    hud._track_sync.fetch_async.assert_called_once_with(451)
    assert hud._track_loaded is False


def test_ensure_track_loads_when_fresh(tmp_path, monkeypatch):
    ts = "2026-07-06T12:00:00+00:00"
    _write_track(tmp_path, 451, updated_at=ts)
    hud = _hud(tmp_path)
    monkeypatch.setattr("overlay.app.config.cloud_tracks", lambda: True)
    monkeypatch.setattr(track_store, "cached_manifest", lambda: {451: ts})
    monkeypatch.setattr(track_store, "needs_cloud_refresh", lambda *_a: False)
    hud._ensure_track(None, None)
    hud._track_sync.fetch_async.assert_not_called()
    assert hud._track_loaded is True
    assert hud._loaded_track_updated_at == ts


def test_on_remote_track_replaces_stale_load(tmp_path):
    _write_track(tmp_path, 451, updated_at="old-ts")
    hud = _hud(tmp_path)
    hud._track_loaded = True
    hud._loaded_track_updated_at = "old-ts"
    new_doc = {
        "track_id": 451,
        "points": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        "start_finish": 0.0,
        "updated_at": "new-ts",
        "schema": 2,
        "import_version": 2,
        "map_rotation": 0,
        "map_mirror": False,
    }
    hud._on_remote_track(451, new_doc)
    assert hud._loaded_track_updated_at == "new-ts"
    saved = json.loads((tmp_path / "451.json").read_text(encoding="utf-8"))
    assert saved["updated_at"] == "new-ts"
    assert len(saved["points"]) == 3


def test_on_remote_track_skips_when_already_current(tmp_path):
    ts = "same-ts"
    _write_track(tmp_path, 451, updated_at=ts)
    hud = _hud(tmp_path)
    hud._track_loaded = True
    hud._loaded_track_updated_at = ts
    doc = {
        "track_id": 451,
        "points": [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        "updated_at": ts,
    }
    with patch.object(track_store, "write_local") as write_local:
        hud._on_remote_track(451, doc)
    write_local.assert_not_called()
