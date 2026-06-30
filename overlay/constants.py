"""Hand-tunable constants for the track / pit scanner.

This is the one place to change the per-track scan "dials" without digging
through the scanning logic in ``app.py``. They control how far the drawn pit
entry / exit blend lines reach *beyond* the real blend lines.

The blend lines themselves now come straight from iRacing's own track-surface
zones (OnTrack <-> ApproachingPits), which are exact and drift-free. The
values below control how much *extra* length to draw past those lines so the
lane matches painted commitment zones (often longer than the surface flip).
Tune per track in the Track Scan tab sliders; session defaults are chosen from
``WeekendInfo.TrackType`` / ``Category`` when a session starts.

Both are expressed as a lap fraction (e.g. 0.16 = 16% of a lap), so they read
the same regardless of car speed.
"""

# Road-course defaults (also used for dirt road).
PIT_EXIT_EXTEND_PCT_ROAD = 0.05
PIT_ENTRY_MAX_PCT_ROAD = 0.03

# Oval defaults (asphalt and dirt oval).
PIT_EXIT_EXTEND_PCT_OVAL = 0.16
PIT_ENTRY_MAX_PCT_OVAL = 0.08


def is_oval_track(weekend: dict | None) -> bool:
    """True when the current layout is an oval (not road / dirt road)."""
    wk = weekend or {}
    tt = str(wk.get("TrackType") or "")
    cat = str(wk.get("Category") or "")
    cfg = str(wk.get("TrackConfigName") or "")
    blob = f"{tt} {cat} {cfg}".lower()
    return "oval" in blob and "road" not in blob


def pit_blend_defaults(weekend: dict | None) -> tuple[float, float]:
    """Return (entry_max_pct, exit_extend_pct) from WeekendInfo track type.

    Uses ``TrackType`` and ``Category`` (case-insensitive). Dirt road counts
    as road; dirt oval counts as oval.
    """
    if is_oval_track(weekend):
        return PIT_ENTRY_MAX_PCT_OVAL, PIT_EXIT_EXTEND_PCT_OVAL
    return PIT_ENTRY_MAX_PCT_ROAD, PIT_EXIT_EXTEND_PCT_ROAD