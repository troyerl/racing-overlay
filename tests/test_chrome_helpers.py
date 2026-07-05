"""Shared chrome helper tests."""

from overlay.widgets import chrome as ch


def test_panel_pad_scales_with_height():
    assert ch.panel_pad(100) == 8.0
    assert ch.panel_pad(200) == 16.0


def test_cell_radius_bounded():
    assert ch.cell_radius(10) >= 4.0
    assert ch.cell_radius(100) <= 8.0
