"""Shared row-height resolution for table widgets."""

from __future__ import annotations

from overlay.widgets.chrome import resolve_row_height


def test_resolve_row_height_scales_to_fit():
    cfg = {"row_height_px": 0, "max_row_height_frac": 0.0}
    assert resolve_row_height(body_h=400, row_count=10, panel_h=500, cfg=cfg) == 40.0


def test_resolve_row_height_caps_by_panel_fraction():
    cfg = {"row_height_px": 0, "max_row_height_frac": 0.14}
    assert resolve_row_height(body_h=400, row_count=4, panel_h=500, cfg=cfg) == 70.0
