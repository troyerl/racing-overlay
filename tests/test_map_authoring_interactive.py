"""Map panel click-through during pit/corner/SF authoring."""

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


class _FakeMapPanel:
    def __init__(self):
        self.click_through = True
        self.calls: list[bool] = []

    def set_click_through(self, value: bool) -> None:
        self.calls.append(bool(value))
        self.click_through = bool(value)


class _FakeMapWidget:
    pit_edit_mode = False
    corner_edit_mode = False
    sf_edit_mode = False

    def set_pit_edit(self, enabled: bool, callback=None) -> None:
        self.pit_edit_mode = bool(enabled)

    def set_pit_edit_phase(self, phase: str) -> None:
        pass

    def set_corner_edit(self, enabled: bool, callback=None) -> None:
        self.corner_edit_mode = bool(enabled)

    def set_sf_edit(self, enabled: bool, callback=None) -> None:
        self.sf_edit_mode = bool(enabled)

    def flash_hint(self, _msg: str) -> None:
        pass

    def clear_pit_edit_phase(self, phase: str) -> None:
        pass


def _hud() -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    hud._map_authoring_depth = 0
    hud._map_click_through_saved = None
    hud._win_by_key = {"map": _FakeMapPanel()}
    hud.map_widget = _FakeMapWidget()
    hud._settings_window = None
    hud.load_pit_into_editor = lambda **kwargs: True  # type: ignore[method-assign]
    return hud


def test_map_authoring_interactive_refcount(qapp):
    hud = _hud()
    win = hud._win_by_key["map"]

    hud._set_map_authoring_interactive(True)
    assert hud._map_authoring_depth == 1
    assert win.click_through is False

    hud._set_map_authoring_interactive(True)
    assert hud._map_authoring_depth == 2
    assert win.click_through is False

    hud._set_map_authoring_interactive(False)
    assert hud._map_authoring_depth == 1
    assert win.click_through is False

    hud._set_map_authoring_interactive(False)
    assert hud._map_authoring_depth == 0
    assert win.click_through is True


def test_pit_edit_mode_toggles_map_interactive(qapp):
    hud = _hud()
    win = hud._win_by_key["map"]

    hud.set_pit_edit_mode(True)
    assert hud._map_authoring_depth == 1
    assert win.click_through is False

    hud.set_pit_edit_mode(False)
    assert hud._map_authoring_depth == 0
    assert win.click_through is True
