"""Hand-tunable constants for the track / pit scanner.

This is the one place to change the per-track scan "dials" without digging
through the scanning logic in ``app.py``. They control how far the drawn pit
entry / exit blend lines reach *beyond* the real blend lines.

The blend lines themselves now come straight from iRacing's own track-surface
zones (OnTrack <-> ApproachingPits), which are exact and drift-free. The
values below control how much *extra* length to draw past those lines so the
lane matches painted commitment zones (often longer than the surface flip).
Tune per track in the Track Scan tab sliders; these are session-start defaults.

Both are expressed as a lap fraction (e.g. 0.16 = 16% of a lap), so they read
the same regardless of car speed.
"""

# Extra distance (lap fraction) to keep tracing the exit lane *past* iRacing's
# exit blend line (the ApproachingPits -> OnTrack surface flip, ~0.108 at
# Watkins Glen). 0 ends exactly at that line, which is usually too short --
# the painted commitment line runs further. Raise for longer exits (ovals often
# need ~0.12-0.16; road courses often ~0.04-0.08). Tune live in Track Scan.
PIT_EXIT_EXTEND_PCT = 0.05

# Extra lap fraction to extend the entry blend *past* iRacing's entry blend line
# (OnTrack -> ApproachingPits). Added on top of the surface boundary when that
# zone is reported; otherwise used alone as the back-trace cap from pit road.
PIT_ENTRY_MAX_PCT = 0.08
