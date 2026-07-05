"""SectorTimer session best, delta, and predicted lap."""

from overlay.widgets.sector_timing import SectorTimer


def _cross_sectors(timer: SectorTimer, pct_seq, lap_times):
    """Simulate lap-distance crossings with paired lap times."""
    for pct, cur, last in zip(pct_seq, lap_times, lap_times):
        timer.update(pct, cur, last)


def test_session_best_tracks_fastest_sector():
    st = SectorTimer()
    st.set_boundaries([0.0, 0.34, 0.68])
    # Lap 1: finish three sectors
    _cross_sectors(st, [0.0, 0.35, 0.69, 0.99], [0.0, 11.0, 22.0, 33.0])
    assert st.session_best[0] == 11.0
    assert st.session_best[1] == 11.0
    # Lap rollover
    _cross_sectors(st, [0.01], [0.5, 33.5])
    # Lap 2: faster S1
    _cross_sectors(st, [0.35, 0.69, 0.99], [10.5, 21.0, 32.0])
    assert st.session_best[0] == 10.5


def test_predicted_lap_uses_session_best_and_running_sector():
    st = SectorTimer()
    st.set_boundaries([0.0, 0.34, 0.68])
    _cross_sectors(st, [0.0, 0.35, 0.69, 0.99], [0.0, 11.0, 22.0, 33.0])
    _cross_sectors(st, [0.01], [0.5, 33.5])
    st.update(0.15, 5.0, 33.5)
    pred = st.predicted_lap()
    assert pred is not None
    assert pred >= 5.0


def test_snapshot_sector_delta_when_enabled():
    st = SectorTimer()
    st.set_boundaries([0.0, 0.34, 0.68])
    _cross_sectors(st, [0.35], [11.0, 0.0])
    _cross_sectors(st, [0.35], [11.5, 0.0])
    snap = st.snapshot(11.5, None, None, show_delta=True)
    s0 = snap["sectors"][0]
    assert s0["status"] in ("done", "best")
    assert s0["delta"] is not None


def test_reset_session_clears_session_best():
    st = SectorTimer()
    st.set_boundaries([0.0, 0.34, 0.68])
    _cross_sectors(st, [0.35], [11.0, 0.0])
    assert st.session_best[0] == 11.0
    st.reset_session()
    assert st.session_best == []
