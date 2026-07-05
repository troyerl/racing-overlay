"""Hybrid / ERS helper tests."""

from overlay import hybrid as hy


class _FakeIR:
    def __init__(self, mapping):
        self._m = mapping

    def __getitem__(self, key):
        if key not in self._m:
            raise KeyError(key)
        return self._m[key]


def test_probe_detects_battery():
    ir = _FakeIR({"EnergyERSBattery": 1_500_000.0})
    assert hy.probe(ir) is True


def test_snapshot_battery_pct():
    ir = _FakeIR({
        "EnergyERSBattery": 1_000_000.0,
        "EnergyBudgetBattToMGU_KLap": 2_000_000.0,
        "EnergyBatteryToMGU_KLap": 100_000.0,
        "ManualBoost": 1,
        "PushToPass": 0,
    })
    snap = hy.snapshot(ir)
    assert snap["have_hybrid"] is True
    assert snap["battery_pct"] == 50.0
    assert snap["boost_active"] is True


def test_fmt_kj():
    assert "kJ" in hy.fmt_kj(150_000.0)
