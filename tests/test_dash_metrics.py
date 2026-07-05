"""Tests for dash metric formatters."""

from __future__ import annotations

from overlay import traffic as tr
from overlay.widgets.dash import METRICS, _m_str


def test_dash_tires_4_formatter():
    d = {"tire_lf": 0.92, "tire_rf": 0.88, "tire_lr": 0.90, "tire_rr": 0.86}
    lines = METRICS["tires_4"][1](d)
    assert lines[0] == ("FL", "92%")
    assert lines[3] == ("RR", "86%")


def test_dash_fuel_pct():
    assert _m_str("fuel_pct", {"fuel_pct": 0.72}) == "72%"


def test_dash_gap_ahead_behind():
    assert _m_str("gap_ahead", {"gap_ahead": 0.4}) == "+0.4"
    assert _m_str("gap_behind", {"gap_behind": 0.8}) == "+0.8"


def test_dash_incidents_limit():
    assert _m_str("incidents_limit", {"incidents": 12, "incident_limit": 17}) == "12/17x"


def test_dash_engine_warn():
    assert _m_str("engine_warn", {"engine_warnings": tr.ENGINE_LIM}) == "LIM"


def test_dash_active_slots_reads_config(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("dash", {}),
                        "stat_left", "fuel")
    monkeypatch.setitem(config.CFG["dash"], "strip_right", "lap_corners")
    slots = config.dash_active_slots()
    assert "fuel" in slots
    assert "lap_corners" in slots
    assert config.dash_metric_in_use("fuel")
    assert config.dash_uses_any("fuel", "tires")
