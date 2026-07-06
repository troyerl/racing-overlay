"""Track ID aliases: one map file for multiple iRacing TrackIDs."""

from __future__ import annotations

import json
from unittest.mock import MagicMock, patch

from overlay import track_store
from overlay.app import AdvancedSimHUD
from overlay.widgets import track_map


def _write_track(tmp_path, tid, *, aliases=None, points=None):
    doc = {
        "track_id": tid,
        "name": "Test",
        "points": points or [[0.0, 0.0], [1.0, 0.0], [0.5, 0.5]],
        "start_finish": 0.0,
        "updated_at": "2026-07-06T12:00:00+00:00",
    }
    if aliases:
        doc["alias_track_ids"] = aliases
    path = tmp_path / f"{tid}.json"
    path.write_text(json.dumps(doc), encoding="utf-8")
    return path


def test_normalize_alias_track_ids():
    doc = track_store.normalize({
        "track_id": 447,
        "points": [[0, 0], [1, 0]],
        "alias_track_ids": [53, "53", 447, 99],
    })
    assert doc["alias_track_ids"] == [53, 99]


def test_resolve_track_id_direct_file(tmp_path):
    _write_track(tmp_path, 53)
    assert track_store.resolve_track_id(str(tmp_path), 53) == 53


def test_resolve_track_id_via_alias(tmp_path):
    _write_track(tmp_path, 447, aliases=[53])
    track_store.invalidate_alias_cache()
    assert track_store.resolve_track_id(str(tmp_path), 53) == 447
    assert track_store.resolve_track_id(str(tmp_path), 447) == 447


def test_tracks_equivalent_with_aliases(tmp_path):
    _write_track(tmp_path, 447, aliases=[53])
    track_store.invalidate_alias_cache()
    assert track_store.tracks_equivalent(str(tmp_path), 53, 447)
    assert not track_store.tracks_equivalent(str(tmp_path), 53, 123)


def test_track_doc_matches_session_alias(tmp_path):
    _write_track(tmp_path, 447, aliases=[53])
    track_store.invalidate_alias_cache()
    doc = {"track_id": 447, "alias_track_ids": [53]}
    assert track_store.track_doc_matches_session(str(tmp_path), 53, doc)
    assert track_store.track_doc_matches_session(str(tmp_path), 447, doc)
    assert not track_store.track_doc_matches_session(str(tmp_path), 123, doc)


def test_remote_manifest_indexes_alias_ids():
    col = MagicMock()
    col.find.return_value = [
        {
            "track_id": 447,
            "updated_at": "2026-07-06T13:00:00+00:00",
            "alias_track_ids": [53],
        },
    ]
    with patch.object(track_store, "_collection", return_value=col):
        track_store._manifest_cache = None
        manifest = track_store.remote_manifest()
    assert manifest[447] == "2026-07-06T13:00:00+00:00"
    assert manifest[53] == "2026-07-06T13:00:00+00:00"


def test_find_track_file_resolves_alias(tmp_path):
    path = _write_track(tmp_path, 447, aliases=[53])
    track_store.invalidate_alias_cache()
    found = track_map.find_track_file(53, str(tmp_path))
    assert found == str(path)


def test_load_local_resolves_alias(tmp_path):
    _write_track(tmp_path, 447, aliases=[53], points=[[0, 0], [2, 0], [1, 1]])
    track_store.invalidate_alias_cache()
    doc = track_store.load_local(str(tmp_path), 53)
    assert doc is not None
    assert doc["track_id"] == 447


def test_fetch_track_query_includes_aliases():
    q = track_store._fetch_track_query(53)
    assert {"$or": [{"track_id": 53}, {"alias_track_ids": 53}]} in [q] or q == {
        "$or": [{"track_id": 53}, {"alias_track_ids": 53},
                {"track_id": "53"}, {"alias_track_ids": "53"}]}
    assert any(c.get("alias_track_ids") == 53 for c in q["$or"])


def test_fetch_track_finds_by_alias():
    col = MagicMock()
    col.find_one.return_value = {"track_id": 447, "points": [[0, 0], [1, 0]]}
    with patch.object(track_store, "_collection", return_value=col):
        doc = track_store.fetch_track(53)
    assert doc is not None
    assert doc["track_id"] == 447
    col.find_one.assert_called_once()


def test_needs_cloud_refresh_resolves_alias_manifest(tmp_path):
    _write_track(tmp_path, 447, aliases=[53])
    track_store.invalidate_alias_cache()
    local = track_store.load_local(str(tmp_path), 53)
    manifest = {447: "2026-07-06T13:00:00+00:00"}
    assert track_store.needs_cloud_refresh(
        53, local, manifest, str(tmp_path)) is True


class _FakeMap:
    path = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
    start_finish = 0.0
    num_turns = 4

    def set_track(self, path, sf=0.0, corners=None):
        self.path = list(path)

    def set_num_turns(self, n):
        self.num_turns = n

    def set_track_is_oval(self, _v):
        pass

    def set_track_zones(self, **kwargs):
        pass

    def display_corners(self):
        return []

    def flash_hint(self, _msg):
        pass

    def update(self):
        pass

    def pit_edit_snapshot(self):
        return [], []


def _hud(tmp_path, **kwargs) -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud.tracks_dir = str(tmp_path)
    hud.demo = kwargs.get("demo", False)
    hud._track_id = kwargs.get("track_id", 53)
    hud._learn_name = "EchoPark"
    hud._v2_authoring_name = ""
    hud._v2_authoring_track_id = None
    hud._track_turns = 4
    hud._alias_track_ids = []
    hud._track_zones = {"drs_zones": [], "p2p_zones": []}
    hud._pit_path = None
    hud._pit_span = None
    hud._pit_in = None
    hud._pit_out = None
    hud._pit_in_pct = None
    hud._pit_out_pct = None
    hud._pit_speed_ms = 0.0
    hud._pit_lane_speed_pct = 1.0
    hud._v2_loop_doc = None
    hud.map_widget = _FakeMap()
    hud._track_sync = MagicMock()
    hud._refresh_settings_authoring = MagicMock()
    return hud


def test_apply_track_from_path_loads_aliases(tmp_path):
    _write_track(tmp_path, 447, aliases=[53])
    hud = _hud(tmp_path)
    path = str(tmp_path / "447.json")
    assert hud._apply_track_from_path(path, "53") is True
    assert hud._alias_track_ids == [53]


def test_set_alias_track_ids_authoring_persists(tmp_path):
    _write_track(tmp_path, 447)
    hud = _hud(tmp_path, track_id=447)
    hud._apply_track_from_path(str(tmp_path / "447.json"), "447")
    assert hud.set_alias_track_ids_authoring([53, 99])
    saved = json.loads((tmp_path / "447.json").read_text(encoding="utf-8"))
    assert saved["alias_track_ids"] == [53, 99]
    track_store.invalidate_alias_cache()
    assert track_map.find_track_file(53, str(tmp_path)) == str(tmp_path / "447.json")
