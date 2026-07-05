"""Lap compare last-lap reference (stint-aware) and wet-lap exclusion."""

from overlay.widgets.lap_compare import LapCompareEngine, N_BINS


def _fill_lap(engine: LapCompareEngine, pct_end=0.99, lap_time=32.0):
    """Drive a synthetic clean lap through the engine."""
    steps = 200
    for i in range(steps + 1):
        pct = pct_end * i / steps
        engine.update(
            pct, on_pit=False, throttle=0.8, brake=0.0, steer=0.5,
            speed=50.0, laptime=lap_time * pct / pct_end if pct_end else 0,
            last_lap_time=lap_time, gear=3, rpm=6000,
        )
    engine.update(0.01, on_pit=False, throttle=0.8, brake=0.0, steer=0.5,
                  speed=50.0, laptime=0.1, last_lap_time=lap_time)


def test_stint_ref_cleared_on_pit():
    eng = LapCompareEngine()
    _fill_lap(eng, lap_time=32.0)
    assert eng._last_stint_ref is not None
    eng.update(0.5, on_pit=True, throttle=0.0, brake=0.2, steer=0.5,
               speed=10.0, laptime=5.0, last_lap_time=32.0)
    assert eng._last_stint_ref is None


def test_wet_lap_marked_dirty_when_wetness_rises():
    eng = LapCompareEngine()
    eng.update(0.0, on_pit=False, throttle=0.5, brake=0.0, steer=0.5,
               speed=40.0, laptime=0.0, last_lap_time=0.0,
               track_wetness=2.0, exclude_wet=True, wet_threshold=5.0)
    eng.update(0.5, on_pit=False, throttle=0.5, brake=0.0, steer=0.5,
               speed=40.0, laptime=16.0, last_lap_time=0.0,
               track_wetness=10.0, exclude_wet=True, wet_threshold=5.0)
    assert eng._cur_dirty is True


def test_last_lap_reference_mode_uses_stint_ref(monkeypatch):
    from overlay import config

    eng = LapCompareEngine()
    _fill_lap(eng, lap_time=32.0)
    _fill_lap(eng, lap_time=33.0)
    monkeypatch.setitem(config.CFG.setdefault("lap_compare", {}),
                        "reference_mode", "last_lap")
    ref, ref_time, _ = eng._active_ref()
    assert ref is eng._last_stint_ref
    assert ref_time == 33.0
