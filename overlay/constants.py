"""Hand-tunable constants for the track / pit scanner.

This is the one place to change the per-track scan "dials" without digging
through the scanning logic in ``app.py``. They control how far the drawn pit
entry / exit blend lines reach *beyond* the real blend lines.

The blend lines themselves now come straight from iRacing's own track-surface
zones (OnTrack <-> ApproachingPits), which are exact and drift-free, so the
defaults below are 0: by default the lane ends exactly where iRacing says the
car leaves / regains the track, which is correct for both ovals and road
courses with no per-track tuning. They survive only as optional fallbacks /
overrides for the rare case the surface zone isn't reported, or to deliberately
stretch the drawn lane out to a painted commitment line that runs past the
blend (e.g. a superspeedway exit) -- raise PIT_EXIT_EXTEND_PCT for that.

Both are expressed as a lap fraction (e.g. 0.16 = 16% of a lap), so they read
the same regardless of car speed.
"""

# Extra distance (lap fraction) to keep tracing the exit lane *past* iRacing's
# exit blend line. 0 = end exactly at the blend line (most accurate). Raise it
# only to stretch the drawn lane out to a longer painted commitment line.
PIT_EXIT_EXTEND_PCT = 0.0

# Fallback cap for how far back up the track the entry blend may reach when the
# surface zone's entry blend line isn't available; the back-trace otherwise
# stops at that drift-free line. Lap fraction before pit road.
PIT_ENTRY_MAX_PCT = 0.08
