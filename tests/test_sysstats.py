"""Smoke tests for overlay.sysstats (CPU / memory / GPU readouts)."""

from __future__ import annotations

from unittest import mock

from overlay import sysstats


def test_cpu_percent_never_raises():
    val = sysstats.cpu_percent()
    assert val is None or (0.0 <= val <= 100.0)


def test_mem_percent_never_raises():
    val = sysstats.mem_percent()
    assert val is None or (0.0 <= val <= 100.0)


def test_gpu_percent_never_raises():
    val = sysstats.gpu_percent()
    assert val is None or (0.0 <= val <= 100.0)


def test_linux_gpu_percent_parses_nvidia_smi():
    with mock.patch.object(sysstats.sys, "platform", "linux"):
        with mock.patch.object(sysstats.shutil, "which", return_value="/usr/bin/nvidia-smi"):
            with mock.patch.object(
                sysstats.subprocess,
                "check_output",
                return_value="12\n34\n",
            ):
                assert sysstats.gpu_percent() == 34.0


def test_linux_gpu_percent_missing_binary():
    with mock.patch.object(sysstats.sys, "platform", "linux"):
        with mock.patch.object(sysstats.shutil, "which", return_value=None):
            assert sysstats.gpu_percent() is None


def test_macos_gpu_percent_returns_none():
    with mock.patch.object(sysstats.sys, "platform", "darwin"):
        assert sysstats.gpu_percent() is None


def test_win_gpu_percent_uses_sampler(monkeypatch):
    class FakeSampler:
        def read(self):
            return 55.0

    monkeypatch.setattr(sysstats, "_win_gpu_sampler", None)
    with mock.patch.object(sysstats.sys, "platform", "win32"):
        with mock.patch.object(sysstats, "_WinGpuSampler", FakeSampler):
            assert sysstats.gpu_percent() == 55.0


def test_wifi_signal_never_raises():
    val = sysstats.wifi_signal()
    assert val is None or (
        isinstance(val, dict)
        and "quality_pct" in val
        and "rssi_dbm" in val
    )


def test_linux_wifi_signal_parses_proc():
    sample = (
        "Inter-| sta-|   Quality        |   Discarded packets\n"
        " face | tus | link level noise |  nwid  crypt   frag\n"
        " wlan0: 0000   52.  -58.  -256        0      0      0\n"
    )
    with mock.patch.object(sysstats.sys, "platform", "linux"):
        with mock.patch("builtins.open", mock.mock_open(read_data=sample)):
            sig = sysstats.wifi_signal()
    assert sig == {"rssi_dbm": -58, "quality_pct": 74}


def test_macos_wifi_signal_returns_none():
    with mock.patch.object(sysstats.sys, "platform", "darwin"):
        assert sysstats.wifi_signal() is None
