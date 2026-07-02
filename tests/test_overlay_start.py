"""Overlay startup when iRacing is not connected."""

from __future__ import annotations

import pytest
from PyQt6.QtWidgets import QApplication

from overlay.app import AdvancedSimHUD


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_start_overlay_offline_edit_mode(qapp):
    """Starting the overlay in edit mode without iRacing must not crash."""
    hud = AdvancedSimHUD(demo=False, click_through=False)
    hud.start_overlay()
    hud.process_telemetry_tick()
    assert hud.ir is not None
    assert hud._demo_active


def test_demo_mode_ticks(qapp):
    hud = AdvancedSimHUD(demo=True, click_through=False)
    hud.start_overlay()
    for _ in range(5):
        hud.process_telemetry_tick()
    assert hud.overlay_running()
