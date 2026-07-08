"""Pit advisor content-visibility gating (green resume / empty hide)."""

from __future__ import annotations

from overlay.app import AdvancedSimHUD


def test_apply_visibility_hides_empty_pit_advisor(monkeypatch):
    hud = object.__new__(AdvancedSimHUD)
    hud._overlay_running = True
    hud._connected = True
    hud._pit_advisor_has_content = False
    hud.edit_mode_enabled = lambda: False
    hud._is_shown = lambda key: key == "pit_advisor"

    class FakeWin:
        def __init__(self):
            self._vis = True
            self.shown = 0
            self.hidden = 0

        def isVisible(self):
            return self._vis

        def ensure_on_screen(self):
            pass

        def show(self):
            self._vis = True
            self.shown += 1

        def hide(self):
            self._vis = False
            self.hidden += 1

    win = FakeWin()
    hud._win_by_key = {"pit_advisor": win}
    hud._apply_visibility()
    assert win.hidden == 1
    assert not win.isVisible()

    hud._pit_advisor_has_content = True
    hud._apply_visibility()
    assert win.shown == 1
    assert win.isVisible()
