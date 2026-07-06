"""Demo mode defaults to Chicagoland and syncs pit extents from track meta."""

from __future__ import annotations

import pytest

from overlay import demo_data
from overlay.app import AdvancedSimHUD


def _demo_hud(**kwargs) -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud.demo = True
    hud._demo_track_id = kwargs.get("demo_track")
    hud._shared_demo_track_id = kwargs.get("shared_id")
    hud._session_demo_track_id = kwargs.get("session_id")
    hud.tracks_dir = kwargs.get("tracks_dir", "/tmp")
    return hud


def test_resolve_demo_track_id_defaults_to_chicagoland():
    hud = _demo_hud()
    assert hud._resolve_demo_track_id() == "123"


def test_resolve_demo_track_id_prefers_shared():
    hud = _demo_hud(shared_id="451")
    assert hud._resolve_demo_track_id() == "451"


def test_resolve_demo_track_id_prefers_session_over_shared():
    hud = _demo_hud(shared_id="451", session_id="999")
    assert hud._resolve_demo_track_id() == "999"


def test_resolve_demo_track_id_ignores_cli_when_shared_set():
    hud = _demo_hud(demo_track="999", shared_id="451")
    assert hud._resolve_demo_track_id() == "451"


def test_resolve_demo_track_id_from_cache(tmp_path):
    from overlay import track_store

    track_store.write_app_settings_cache(str(tmp_path), {"demo_track_id": 522})
    hud = _demo_hud(tracks_dir=str(tmp_path))
    hud._apply_app_settings_cache()
    assert hud._resolve_demo_track_id() == "522"


def test_effective_track_id_in_demo():
    hud = _demo_hud()
    hud._track_id = None
    assert hud.effective_track_id() == "123"


def test_configure_pit_extents_updates_demo_pit_helpers():
    demo_data.configure_pit_extents(0.84, 0.35, 0.86, 0.95)
    assert demo_data.DEMO_PIT_IN_PCT == pytest.approx(0.84)
    assert demo_data.DEMO_PIT_OUT_PCT == pytest.approx(0.35)
    assert demo_data.DEMO_PIT_LANE_LO == pytest.approx(0.86)
    assert demo_data.DEMO_PIT_LANE_HI == pytest.approx(0.95)
    assert demo_data._in_demo_pit(0.90)
    assert demo_data._on_demo_pit_lane(0.90)
    # Restore defaults for other tests.
    demo_data.configure_pit_extents(0.90, 0.12, 0.95, 0.06)


def test_weekend_info_reports_chicagoland():
    demo_data.configure_weekend_info(
        demo_data.DEMO_TRACK_ID,
        name="Chicagoland Speedway",
        num_turns=4,
    )
    ir = demo_data.FakeIRSDK()
    wk = ir["WeekendInfo"]
    assert wk["TrackID"] == demo_data.DEMO_TRACK_ID
    assert wk["TrackDisplayName"] == "Chicagoland Speedway"


def test_configure_weekend_info_updates_telemetry():
    demo_data.configure_weekend_info(451, name="Road America", num_turns=14)
    ir = demo_data.FakeIRSDK()
    wk = ir["WeekendInfo"]
    assert wk["TrackID"] == 451
    assert wk["TrackDisplayName"] == "Road America"
    assert wk["TrackNumTurns"] == 14
    demo_data.configure_weekend_info(
        demo_data.DEMO_TRACK_ID,
        name="Chicagoland Speedway",
        num_turns=4,
    )


def test_demo_lap_pct_not_pinned_to_player():
    """Lap-% should follow simulation, not fixed offsets from the player."""
    ir = demo_data.FakeIRSDK()
    pcts = ir._lap_pct()
    player = ir.player_idx
    # Old _MAP_SHOWCASE pinned car 5 at player + 0.32 lap-%.
    if 5 < ir.num_cars and 5 != player:
        delta = (pcts[5] - pcts[player]) % 1.0
        assert abs(delta - 0.32) > 0.05

def test_leader_icon_is_crown():
    from overlay.widgets import icons

    assert icons._CODEPOINTS["leader"] == 0xF521
    assert icons._CODEPOINTS["leader"] != icons._CODEPOINTS["best_lap"]


def test_load_demo_track_offline_fallback(tmp_path):
    """Demo mode shows a bundled/synthetic map when the cloud track is missing."""
    import shutil
    import sys
    from PyQt6.QtWidgets import QApplication

    from overlay.app import AdvancedSimHUD

    app = QApplication.instance()
    if app is None:
        app = QApplication(sys.argv)
    hud = AdvancedSimHUD(click_through=False, demo=True, tracks_dir=str(tmp_path))
    assert hud.map_widget.path and len(hud.map_widget.path) >= 2

    shutil.copy(
        "tracks/_demo.json",
        tmp_path / "_demo.json",
    )
    hud2 = AdvancedSimHUD(click_through=False, demo=True, tracks_dir=str(tmp_path))
    assert hud2.map_widget.path and len(hud2.map_widget.path) >= 2
