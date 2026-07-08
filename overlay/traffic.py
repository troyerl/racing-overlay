"""Shared gap, status, and traffic helpers for tables and dash."""

from __future__ import annotations

from . import common as oc
from .map_markers import wrap_lap_delta

# CarIdxSessionFlags bits (subset for per-car row display).
FLAG_BLACK = 0x00010000
FLAG_DQ = 0x00020000
FLAG_FURLED = 0x00080000
FLAG_REPAIR = 0x00100000

# EngineWarnings bits (irsdk).
ENGINE_WATER = 0x01
ENGINE_OIL = 0x02
ENGINE_FUEL = 0x04
ENGINE_LIM = 0x10

PIT_SURFACES = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)


def wrap_est_delta(other: float, me: float, lap_est: float) -> float:
    """Signed EstTime delta from *me* to *other*, wrapped within one lap."""
    delta = other - me
    if lap_est <= 0:
        return delta
    half = lap_est / 2.0
    if delta > half:
        delta -= lap_est
    elif delta < -half:
        delta += lap_est
    return delta


def position_ahead_idx(positions, idx: int) -> int | None:
    """CarIdx of the car at race position P-1 relative to *idx*."""
    if not positions or idx >= len(positions):
        return None
    pos = positions[idx]
    if not pos or pos <= 1:
        return None
    target = pos - 1
    for i, p in enumerate(positions):
        if i != idx and p == target:
            return i
    return None


def est_interval(est, a: int, b: int, lap_est: float) -> float | None:
    """Absolute EstTime interval between two cars."""
    if not est or a >= len(est) or b >= len(est):
        return None
    ta, tb = est[a], est[b]
    if ta is None or tb is None:
        return None
    return abs(wrap_est_delta(tb, ta, lap_est))


def f2_interval(f2, a: int, b: int) -> float | None:
    """Absolute F2Time interval between two cars (both vs leader)."""
    if not f2 or a >= len(f2) or b >= len(f2):
        return None
    fa, fb = f2[a], f2[b]
    if fa is None or fb is None:
        return None
    return abs(fb - fa)


def fmt_leader_gap(f2_val, pos, lap_est: float) -> str:
    """Format CarIdxF2Time as +seconds or -NL to leader."""
    if pos == 1 or f2_val is None or (isinstance(f2_val, (int, float)) and f2_val <= 0):
        return "\u2014"
    if lap_est and f2_val >= lap_est:
        return f"-{int(f2_val // lap_est)}L"
    return f"+{f2_val:.1f}"


def fmt_interval_gap(seconds: float | None, lap_est: float) -> str:
    """Format a positive interval as +seconds or -NL."""
    if seconds is None:
        return "\u2014"
    if lap_est and seconds >= lap_est:
        return f"-{int(seconds // lap_est)}L"
    return f"+{seconds:.1f}"


def fmt_est_delta(delta: float | None) -> str:
    if delta is None:
        return "\u2014"
    if delta == 0:
        return "0.0"
    return f"{abs(delta):.1f}"


def nearest_ahead_behind(
    est,
    player: int,
    lap_est: float,
    *,
    include_fn=None,
    pace_idxs: set[int] | None = None,
) -> tuple[float | None, float | None]:
    """Return (gap_ahead_secs, gap_behind_secs) for *player* using EstTime."""
    if player is None or not est or lap_est <= 0 or player >= len(est):
        return None, None
    me = est[player]
    if me is None:
        return None, None
    pace_idxs = pace_idxs or set()
    best_ahead = None
    best_behind = None
    for idx, t in enumerate(est):
        if idx == player or t is None or idx in pace_idxs:
            continue
        if include_fn is not None and not include_fn(idx):
            continue
        delta = wrap_est_delta(t, me, lap_est)
        if delta > 0:
            if best_ahead is None or delta < best_ahead:
                best_ahead = delta
        elif delta < 0:
            if best_behind is None or delta > best_behind:
                best_behind = delta
    behind_gap = abs(best_behind) if best_behind is not None else None
    return best_ahead, behind_gap


def closing_rate(
    state: dict,
    idx: int,
    delta: float,
    now: float,
    *,
    min_rate: float = 0.05,
    max_age: float = 3.0,
) -> float | None:
    """Gap-closing rate (positive = closing on reference car)."""
    prev = state.get(idx)
    state[idx] = (delta, now)
    if prev is None:
        return None
    prev_delta, prev_time = prev
    dt = now - prev_time
    if dt <= 0.01 or dt > max_age:
        return None
    rate = (abs(prev_delta) - abs(delta)) / dt
    if abs(rate) < min_rate:
        return None
    return rate


def fmt_closing_rate(rate: float | None) -> str:
    if rate is None:
        return "\u2014"
    sign = "+" if rate >= 0 else "\u2212"
    return f"{sign}{abs(rate):.1f}/s"


def car_status_text(surface_val, on_pit: bool | None = None) -> str:
    if on_pit:
        return "PIT"
    if surface_val is None:
        return "\u2014"
    if surface_val in PIT_SURFACES:
        return "PIT"
    if surface_val == oc.TRK_ON_TRACK:
        return "OUT"
    if surface_val == oc.TRK_OFF_TRACK:
        return "OFF"
    if surface_val == oc.TRK_NOT_IN_WORLD:
        return "GARAGE"
    return "OUT"


def is_standings_inactive(surface_val, lap_pct_val=None) -> bool:
    """True when a driver is in the garage or disconnected (standings grey-out)."""
    if surface_val == oc.TRK_NOT_IN_WORLD:
        return True
    if lap_pct_val is not None and lap_pct_val < 0:
        return True
    return False


def car_flag_text(flags_val) -> str:
    if not flags_val or not isinstance(flags_val, (int, float)):
        return "\u2014"
    sf = int(flags_val)
    if sf & FLAG_REPAIR:
        return "MEAT"
    if sf & FLAG_BLACK:
        return "BLK"
    if sf & FLAG_DQ:
        return "DQ"
    if sf & FLAG_FURLED:
        return "WARN"
    return "\u2014"


def car_flag_kind(flags_val) -> str | None:
    """Style key for car_flag pill: meatball, black, dq, furled, or None."""
    if not flags_val or not isinstance(flags_val, (int, float)):
        return None
    sf = int(flags_val)
    if sf & FLAG_REPAIR:
        return "meatball"
    if sf & FLAG_BLACK:
        return "black"
    if sf & FLAG_DQ:
        return "dq"
    if sf & FLAG_FURLED:
        return "furled"
    return None


def engine_warning_text(warnings_val) -> str:
    if warnings_val is None or not isinstance(warnings_val, (int, float)):
        return "\u2014"
    w = int(warnings_val)
    parts = []
    if w & ENGINE_LIM:
        parts.append("LIM")
    if w & ENGINE_WATER:
        parts.append("H2O")
    if w & ENGINE_OIL:
        parts.append("OIL")
    if w & ENGINE_FUEL:
        parts.append("FUEL")
    return " ".join(parts) if parts else "\u2014"


def map_car_status_kind(
    surface_val,
    on_pit: bool | None = None,
    car_flag=None,
) -> str | None:
    """Map overlay badge key: flag kinds override surface (pit/off/garage)."""
    kind = car_flag_kind(car_flag)
    if kind:
        return kind
    if on_pit:
        return "pit"
    if surface_val is None:
        return None
    if surface_val in PIT_SURFACES:
        return "pit"
    if surface_val == oc.TRK_OFF_TRACK:
        return "off"
    if surface_val == oc.TRK_NOT_IN_WORLD:
        return "garage"
    return None


def alongside_candidates(
    lap_pct,
    player: int,
    *,
    alongside_zone: float = 0.004,
    include_fn=None,
    pace_idxs: set[int] | None = None,
) -> list[tuple[int, float]]:
    """Cars within *alongside_zone* lap distance of *player*, nearest first."""
    if player is None or not lap_pct or player >= len(lap_pct):
        return []
    me = lap_pct[player]
    if me is None or me < 0:
        return []
    pace_idxs = pace_idxs or set()
    found: list[tuple[int, float]] = []
    for idx, pct in enumerate(lap_pct):
        if idx == player or pct is None or pct < 0:
            continue
        if idx in pace_idxs:
            continue
        if include_fn is not None and not include_fn(idx):
            continue
        delta = wrap_lap_delta(pct, me)
        if abs(delta) <= alongside_zone:
            found.append((idx, delta))
    found.sort(key=lambda item: abs(item[1]))
    return found


def pick_alongside_car(
    candidates: list[tuple[int, float]],
    *,
    exclude: set[int] | None = None,
) -> tuple[int | None, float | None]:
    """Pick the nearest alongside car not in *exclude*."""
    exclude = exclude or set()
    for idx, delta in candidates:
        if idx not in exclude:
            return idx, delta
    return None, None


def nearest_alongside(
    lap_pct,
    player: int,
    est_time,
    lap_est: float,
    *,
    alongside_zone: float = 0.004,
    include_fn=None,
    pace_idxs: set[int] | None = None,
    exclude: set[int] | None = None,
) -> tuple[int | None, float | None, float | None]:
    """Nearest alongside rival: (idx, lap_delta, est_delta).

    When EstTime is available, prefer the smallest absolute EstTime gap among
    cars in the alongside lap-% window; otherwise use lap distance.
    """
    cands = alongside_candidates(
        lap_pct, player,
        alongside_zone=alongside_zone,
        include_fn=include_fn,
        pace_idxs=pace_idxs,
    )
    exclude = exclude or set()
    cands = [(i, d) for i, d in cands if i not in exclude]
    if not cands:
        return None, None, None
    if est_time and lap_est > 0 and player < len(est_time):
        me = est_time[player]
        if me is not None:
            best_idx = None
            best_lap = None
            best_est = None
            for idx, lap_delta in cands:
                if idx >= len(est_time):
                    continue
                t = est_time[idx]
                if t is None:
                    continue
                est_delta = wrap_est_delta(t, me, lap_est)
                if best_est is None or abs(est_delta) < abs(best_est):
                    best_idx = idx
                    best_lap = lap_delta
                    best_est = est_delta
            if best_idx is not None:
                return best_idx, best_lap, best_est
    idx, lap_delta = cands[0]
    return idx, lap_delta, None


def closing_rate_tint(rate: float | None, full_rate: float) -> float | None:
    """0..1 intensity for closing-rate color boost (None when inactive)."""
    if rate is None or full_rate <= 0:
        return None
    if rate <= 0:
        return 0.0
    return max(0.0, min(1.0, rate / full_rate))


def radar_clear_seconds(clear_since: float | None, now: float) -> float | None:
    """Seconds the blind spot has been clear (None while occupied)."""
    if clear_since is None:
        return None
    return max(0.0, now - clear_since)


def is_multiclass(class_positions, positions) -> bool:
    """True when any car's class position differs from overall position."""
    if not class_positions or not positions:
        return False
    n = min(len(class_positions), len(positions))
    for i in range(n):
        cp = class_positions[i]
        p = positions[i]
        if cp and p and cp != p:
            return True
    return False
