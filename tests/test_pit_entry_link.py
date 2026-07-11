"""Pit-edit entry/road joint linking."""

from __future__ import annotations

from types import SimpleNamespace

from overlay.widgets.track_map import TrackMapWidget


def _map_with_road() -> TrackMapWidget:
    w = SimpleNamespace()
    w.pit_edit_lane = 1
    w.pit_edit_phase = "entry"
    w._pit_edit_entry = []
    w._pit_edit_road = [(0.5, 0.4), (0.7, 0.4)]
    w._pit_edit_merge = []
    w._pit_edit_entry_2 = []
    w._pit_edit_road_2 = []
    w._pit_edit_merge_2 = []
    w._pit_edit_bufs = TrackMapWidget._pit_edit_bufs.__get__(w)
    w._pit_has_entry_joint = TrackMapWidget._pit_has_entry_joint.__get__(w)
    w._pit_points_coincide = staticmethod(TrackMapWidget._pit_points_coincide)
    w._sync_pit_joint = TrackMapWidget._sync_pit_joint.__get__(w)
    w._append_pit_edit_at = TrackMapWidget._append_pit_edit_at.__get__(w)
    w.pit_edit_snapshot = TrackMapWidget.pit_edit_snapshot.__get__(w)
    return w


def test_entry_first_click_links_to_road_start():
    w = _map_with_road()
    w._append_pit_edit_at(0.1, 0.2)
    entry, road, _ = w.pit_edit_snapshot(lane=1)
    assert len(entry) == 2
    assert entry[0] == (0.1, 0.2)
    assert entry[-1] == road[0]
    assert w._pit_has_entry_joint(1)


def test_entry_further_clicks_insert_before_joint():
    w = _map_with_road()
    w._append_pit_edit_at(0.1, 0.2)
    w._append_pit_edit_at(0.2, 0.25)
    entry, road, _ = w.pit_edit_snapshot(lane=1)
    assert entry == [(0.1, 0.2), (0.2, 0.25), road[0]]
    assert w._pit_has_entry_joint(1)
