"""Tests for system panel telemetry and NET row formatting."""

from __future__ import annotations

from overlay import telemetry as tele
from overlay.widgets.system_panel import format_network_value


class FakeIR:
    def __init__(self, values: dict):
        self._values = values

    def __getitem__(self, key):
        if key not in self._values:
            raise KeyError(key)
        return self._values[key]


def test_perf_snapshot_reads_fps_and_channel():
    ir = FakeIR({
        "FrameRate": 143.6,
        "ChanQuality": 97.2,
        "ChanLatency": 28.4,
    })
    snap = tele.perf_snapshot(ir, cfg={
        "show_fps": True,
        "show_network": True,
    })
    assert snap["fps"] == 144
    assert snap["chan_quality"] == 97.2
    assert snap["chan_latency"] == 28.4


def test_perf_snapshot_respects_toggles():
    ir = FakeIR({
        "FrameRate": 60.0,
        "ChanQuality": 90.0,
        "ChanLatency": 20.0,
    })
    snap = tele.perf_snapshot(ir, cfg={
        "show_fps": False,
        "show_network": False,
    })
    assert snap == {}


def test_perf_snapshot_empty_when_ir_missing():
    assert tele.perf_snapshot(None) == {}


def test_perf_snapshot_omits_zero_channel():
    ir = FakeIR({"FrameRate": 60.0, "ChanQuality": 0.0, "ChanLatency": 0.0})
    snap = tele.perf_snapshot(ir, cfg={"show_fps": True, "show_network": True})
    assert snap["fps"] == 60
    assert "chan_quality" not in snap
    assert "chan_latency" not in snap


def test_channel_usable():
    assert tele.channel_usable(97.0, 28.0) is True
    assert tele.channel_usable(None, 28.0) is True
    assert tele.channel_usable(97.0, None) is True
    assert tele.channel_usable(0.0, 0.0) is False
    assert tele.channel_usable(None, 0.0) is False


def test_format_network_value_zero_latency_uses_wifi():
    assert format_network_value(0.0, 0.0, {"quality_pct": 82}) == "WiFi 82%"
    assert format_network_value(None, 0.0, {"quality_pct": 82}) == "WiFi 82%"


def test_format_network_value_prefers_channel():
    assert format_network_value(98.0, 32.0, {"quality_pct": 80}) == "98% \u00b7 32 ms"


def test_format_network_value_channel_partial():
    assert format_network_value(None, 15.0, None) == "15 ms"
    assert format_network_value(88.0, None, None) == "88%"


def test_format_network_value_wifi_fallback():
    assert format_network_value(None, None, {"quality_pct": 82}) == "WiFi 82%"
    assert format_network_value(None, None, {"rssi_dbm": -58}) == "WiFi -58 dBm"


def test_format_network_value_wifi_compact():
    assert format_network_value(None, None, {"quality_pct": 82}, compact=True) == "82%"
    assert format_network_value(None, None, {"rssi_dbm": -58}, compact=True) == "-58 dBm"


def test_format_network_value_empty():
    assert format_network_value(None, None, None) == "\u2014"


def test_row_specs_cover_all_metrics():
    from overlay.widgets.system_panel import _ROW_SPECS

    assert set(_ROW_SPECS) == {
        "show_cpu", "show_mem", "show_gpu", "show_fps", "show_network",
    }
