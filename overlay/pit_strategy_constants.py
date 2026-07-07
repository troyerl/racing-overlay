"""Scalars and helpers for pit_advisor strategy (aligned with live-engineer reference)."""

from __future__ import annotations

REENTRY_WINDOW_PCT = 0.035
LAP_DIST_WRAP_HALF = 0.5

CAUTION_PIT_PRA_THRESHOLD = 0.60
CAUTION_PIT_LEAD_LOSS_MAX = 3
FIELD_PIT_FOLLOW_THRESHOLD = 0.45

POST_PIT_QUIET_MIN_LAPS = 6
LAPPED_DANGER_FUEL_MIN_LAPS = 1.5
GREEN_RUN_CAUTION_BIAS_LAPS = 15

GREEN_POSITION_SCAN = 5

# Tire advisor defaults
TIRE_WARN_WEAR_PCT = 35.0
TIRE_CRITICAL_WEAR_PCT = 25.0
LOW_TIRE_LAPS_THRESHOLD = 3.0
MIN_STINT_LAPS = 4
TIRE_SETS_RESERVE = 1
FRESH_TIRE_LAP_DELTA = 3
AHEAD_PACE_DELTA_S = 0.3

# Caution outlook
CAUTION_OVERDUE_RATIO = 1.15
FIELD_CHAOS_HIGH_THRESHOLD = 0.25
CAUTION_WAIT_MIN_FUEL_LAPS = 3.0
AHEAD_SCAN_POSITIONS = 5

# Field closing / position cost
COVER_CLOSING_MIN_RATE = 0.15
GREEN_POS_LOST_MAX = 2
CAUTION_PRB_STAY_OUT_THRESHOLD = 0.50
CAUTION_PRB_PIT_THRESHOLD = 0.35

# Session phase
FINAL_LAPS_OPTIONAL_SUPPRESS = 3
TRACK_WETNESS_TIRE_SUPPRESS = 0.1

# Measured pit loss EMA
PIT_LOSS_EMA_ALPHA = 0.35
PIT_LOSS_MEASURED_MIN_S = 15.0
PIT_LOSS_MEASURED_MAX_S = 90.0
PIT_LOSS_DURATION_MIN_S = 8.0
PIT_LOSS_DURATION_MAX_S = 120.0

# SessionFlags bits (irsdk)
FLAG_CHECKERED = 0x00000001
FLAG_WHITE = 0x00000002

TIRE_SETS_UNLIMITED = 255

# Opponent tire inference / strategic pit
AHEAD_PROFILE_SCAN_POSITIONS = 15
STRATEGIC_PIT_MIN_NET_POSITIONS = 3
OPPONENT_SPLASH_PIT_MAX_S = 0
OPPONENT_STINT_DUE_LAPS = 25
CAUTION_BANKRUPT_AHEAD_MIN = 3


def wrap_lap_distance_delta(a: float, b: float) -> float:
    """Signed lap-distance delta from *b* to *a*, wrapped within one lap."""
    delta = a - b
    if delta > LAP_DIST_WRAP_HALF:
        delta -= 1.0
    elif delta < -LAP_DIST_WRAP_HALF:
        delta += 1.0
    return delta


def lap_pct_interval(a: float, b: float) -> float:
    """Absolute shortest distance along a lap between two lap fractions."""
    return abs(wrap_lap_distance_delta(a, b))


def triangular_payback_lap(falloff_s: float, pit_loss_s: float) -> int:
    """Laps until cumulative triangular wear cost exceeds pit_loss (§3.5)."""
    if falloff_s <= 0 or pit_loss_s <= 0:
        return 1
    cumulative = 0.0
    for lap in range(1, 200):
        cumulative += lap * falloff_s
        if cumulative >= pit_loss_s:
            return lap
    return 200
