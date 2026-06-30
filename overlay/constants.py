"""Hand-tunable constants for the track / pit scanner.

This is the one place to change the per-track scan "dials" without digging
through the scanning logic in ``app.py``. The values below control how long the
drawn pit entry / exit blend lines reach; the rest of the pit thresholds adapt
themselves to the measured lane offset, so these are the ones you'd most likely
want to tweak when a different track's pit lane doesn't look right.

Both are expressed as a lap fraction (e.g. 0.16 = 16% of a lap), so they read
the same regardless of car speed. Note that, being lap fractions, the same value
covers far more ground on a long road course than on a short oval -- if a road
course's exit lane draws far too long, lower ``PIT_EXIT_EXTEND_PCT``.
"""

# The car geometrically regains the racing line (distance -> 0) well before
# iRacing's painted pit-exit commitment line actually ends down the track. Once
# merged, keep tracing the exit until the car has travelled this much further
# around the lap so the drawn lane reaches the real end of the commitment zone.
# With the merge at ~0.27 on the reference oval this lands the end near ~0.43 --
# well past the old too-short result and pulled back from the ~0.5 that read as
# too far. Raise it toward ~0.18 if still short, lower toward ~0.10 if long.
PIT_EXIT_EXTEND_PCT = 0.16

# The entry blend can otherwise reach a long way back up the track (to wherever
# the car first eased off the racing line). Cap it to the last this-much of a lap
# before pit road so the yellow entry line stays short. Companion dial to
# PIT_EXIT_EXTEND_PCT; raise for a longer entry line, lower for a shorter one.
PIT_ENTRY_MAX_PCT = 0.08
