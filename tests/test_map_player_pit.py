"""Map car dot styling: player visible on pit road and exit."""

from __future__ import annotations

from overlay.widgets.track_map import TrackMapWidget


def test_competitor_on_pit_faded_gray():
    opacity, use_pit_fill, use_player_glow = TrackMapWidget._car_dot_style(
        False, on_pit=True, on_route=False, pit_opacity=0.45)
    assert opacity == 0.45
    assert use_pit_fill is True
    assert use_player_glow is False


def test_player_on_pit_full_glow():
    opacity, use_pit_fill, use_player_glow = TrackMapWidget._car_dot_style(
        True, on_pit=True, on_route=False)
    assert opacity == 1.0
    assert use_pit_fill is False
    assert use_player_glow is True


def test_player_on_route_exit_glow():
    opacity, use_pit_fill, use_player_glow = TrackMapWidget._car_dot_style(
        True, on_pit=False, on_route=True)
    assert opacity == 1.0
    assert use_pit_fill is False
    assert use_player_glow is True


def test_player_on_track_glow():
    opacity, use_pit_fill, use_player_glow = TrackMapWidget._car_dot_style(
        True, on_pit=False, on_route=False)
    assert opacity == 1.0
    assert use_pit_fill is False
    assert use_player_glow is True


def test_competitor_on_track_normal():
    opacity, use_pit_fill, use_player_glow = TrackMapWidget._car_dot_style(
        False, on_pit=False, on_route=False)
    assert opacity == 1.0
    assert use_pit_fill is False
    assert use_player_glow is False
