"""Tests for config.slot_in_use multi-key OR."""

from __future__ import annotations

from overlay import config


def test_slot_in_use_multi_key_or(monkeypatch):
    monkeypatch.setitem(config.CFG, "relative", {
        "show_footer": True,
        "header": {"left": "weather", "center": "none", "right": "none"},
        "footer": {"left": "none", "center": "none", "right": "none"},
    })
    monkeypatch.setitem(config.CFG, "standings", {
        "show_footer": True,
        "header": {"left": "none", "center": "none", "right": "none"},
        "footer": {"left": "none", "center": "none", "right": "none"},
    })
    assert config.slot_in_use("weather")
    assert config.slot_in_use("weather", "incident_limit", "race_split")
    assert not config.slot_in_use("incident_limit", "race_split")
    assert not config.slot_in_use()


def test_slot_in_use_respects_hidden_footer(monkeypatch):
    monkeypatch.setitem(config.CFG, "relative", {
        "show_footer": False,
        "header": {"left": "none", "center": "none", "right": "none"},
        "footer": {"left": "race_split", "center": "none", "right": "none"},
    })
    monkeypatch.setitem(config.CFG, "standings", {
        "show_footer": True,
        "header": {"left": "none", "center": "none", "right": "none"},
        "footer": {"left": "none", "center": "none", "right": "none"},
    })
    assert not config.slot_in_use("race_split")
