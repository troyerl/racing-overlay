"""Unit tests for Windows Startup autostart helper."""

from __future__ import annotations

import os

from overlay import autostart


def test_shortcut_path_none_on_non_windows(monkeypatch):
    monkeypatch.setattr(autostart.sys, "platform", "darwin")
    assert autostart.shortcut_path() is None
    assert autostart.is_enabled() is False


def test_set_enabled_noop_on_non_windows(monkeypatch):
    monkeypatch.setattr(autostart.sys, "platform", "linux")
    autostart.set_enabled(True)  # must not raise


def test_enable_disable_with_mocked_paths(monkeypatch, tmp_path):
    monkeypatch.setattr(autostart.sys, "platform", "win32")
    monkeypatch.setenv("APPDATA", str(tmp_path))
    created = {}

    def fake_create(path, target, arguments):
        created["path"] = path
        created["target"] = target
        created["arguments"] = arguments
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as fh:
            fh.write(b"lnk")

    monkeypatch.setattr(autostart, "_create_windows_shortcut", fake_create)
    monkeypatch.setattr(autostart.sys, "executable", r"C:\GridGlance\GridGlance.exe")

    # Prefer overlay-on-launch args
    from overlay import config
    monkeypatch.setitem(config.CFG, "start_overlay_on_launch", True)

    assert not autostart.is_enabled()
    autostart.set_enabled(True)
    assert autostart.is_enabled()
    assert created["path"].endswith("GridGlance.lnk")
    assert "Startup" in created["path"]
    assert created["arguments"] == "--no-settings"

    monkeypatch.setitem(config.CFG, "start_overlay_on_launch", False)
    autostart.set_enabled(True)
    assert created["arguments"] == ""

    autostart.set_enabled(False)
    assert not autostart.is_enabled()


def test_sync_from_preference(monkeypatch, tmp_path):
    monkeypatch.setattr(autostart.sys, "platform", "win32")
    monkeypatch.setenv("APPDATA", str(tmp_path))
    calls: list[bool] = []

    monkeypatch.setattr(autostart, "is_enabled", lambda: False)
    monkeypatch.setattr(
        autostart, "set_enabled", lambda on: calls.append(bool(on)))

    autostart.sync_from_preference(True)
    assert calls == [True]

    calls.clear()
    monkeypatch.setattr(autostart, "is_enabled", lambda: True)
    autostart.sync_from_preference(True)
    assert calls == []
