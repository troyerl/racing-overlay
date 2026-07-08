"""Fuel snapshots and pit-strategy recommendations for fuel_calc / pit_advisor."""

from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum

from . import common as oc
from . import pit_strategy_constants as psc
from . import traffic as tr

_MAX_SESSION_SEC = 48 * 3600

# SessionFlags bits (irsdk_Flags) used for caution / pits closed.
_FLAG_CAUTION_WAVING = 0x00008000


class PitRec(str, Enum):
    HOLD = "hold"
    PIT_NOW = "pit_now"
    PIT_NEXT_LAP = "pit_next_lap"
    STAY_OUT = "stay_out"
    MARGINAL = "marginal"


_REC_LABELS = {
    PitRec.HOLD: "HOLD",
    PitRec.PIT_NOW: "PIT NOW",
    PitRec.PIT_NEXT_LAP: "PIT NEXT LAP",
    PitRec.STAY_OUT: "STAY OUT",
    PitRec.MARGINAL: "MARGINAL",
}


@dataclass(frozen=True)
class PitAdvice:
    rec: PitRec
    label: str
    rationale: str
    secondary: str | None = None
    actionable: bool = True


def sane_session_seconds(secs) -> float | None:
    if not isinstance(secs, (int, float)):
        return None
    if secs < 0 or secs > _MAX_SESSION_SEC:
        return None
    return float(secs)


def fuel_capacity(level, fuel_max: float, fuel_level_pct) -> float | None:
    if fuel_max and fuel_max > 0:
        return float(fuel_max)
    if isinstance(level, (int, float)) and isinstance(fuel_level_pct, (int, float)) \
            and fuel_level_pct > 0.01:
        return level / fuel_level_pct
    return None


def fuel_lap_secs(est_lap: float, ll_laps: list[dict]) -> float | None:
    good = [l["secs"] for l in ll_laps if l.get("secs", 0) > 0]
    if good and est_lap > 0:
        avg = sum(good[:10]) / len(good[:10])
        if abs(avg - est_lap) / est_lap <= 0.20:
            return avg
        return est_lap
    if good:
        return sum(good[:10]) / len(good[:10])
    return est_lap if est_lap > 0 else None


def race_remaining(ir, lap_avg: float | None) -> tuple[float | None, float | None]:
    try:
        laps = ir["SessionLapsRemainEx"]
    except (TypeError, ValueError, KeyError):
        laps = None
    if not isinstance(laps, (int, float)) or laps < 0 or laps > 32000:
        try:
            laps = ir["SessionLapsRemain"]
        except (TypeError, ValueError, KeyError):
            laps = None
    if not isinstance(laps, (int, float)) or laps < 0 or laps > 32000:
        laps = None
    try:
        t = ir["SessionTimeRemain"]
    except (TypeError, ValueError, KeyError):
        t = None
    t = sane_session_seconds(t)
    if laps is None and t is not None and lap_avg:
        laps = t / lap_avg
    if t is None and laps is not None and lap_avg:
        t = laps * lap_avg
    return laps, t


def build_fuel_snapshot(
    ir,
    *,
    car_info: dict,
    fc_use: list[float],
    ll_laps: list[dict],
    cfg: dict,
    caution: bool = False,
) -> dict:
    """Build fuel_calc payload fields (shared by fuel_calc and pit_advisor)."""
    try:
        fuel = ir["FuelLevel"]
    except (TypeError, ValueError, KeyError):
        fuel = None
    try:
        fuel_pct_raw = ir["FuelLevelPct"]
    except (TypeError, ValueError, KeyError):
        fuel_pct_raw = None

    fuel_max = float(car_info.get("fuel_max") or 0.0)
    cap = fuel_capacity(fuel, fuel_max, fuel_pct_raw)
    try:
        lap = int(ir["Lap"])
    except (TypeError, ValueError, KeyError):
        lap = None

    est_lap = float(car_info.get("est_lap") or 0.0)
    lap_avg = fuel_lap_secs(est_lap, ll_laps)
    laps_rem, time_rem = race_remaining(ir, lap_avg)

    if fc_use:
        u_avg = sum(fc_use) / len(fc_use)
        u_max = max(fc_use)
        u_min = min(fc_use)
    else:
        est = None
        try:
            per_hr = ir["FuelUsePerHour"]
        except (TypeError, ValueError, KeyError):
            per_hr = None
        lap_s = lap_avg
        if per_hr and lap_s:
            est = per_hr * (lap_s / 3600.0)
        u_avg = u_max = u_min = est

    if caution:
        mult = float(cfg.get("caution_fuel_multiplier", 1.0) or 1.0)
        if mult > 0 and mult != 1.0:
            if u_avg is not None:
                u_avg *= mult
            if u_max is not None:
                u_max *= mult
            if u_min is not None:
                u_min *= mult

    live_burn = None
    if cfg.get("show_live_burn", False):
        if fc_use:
            live_burn = fc_use[0]
        elif u_avg:
            live_burn = u_avg

    fuel_pct = None
    if cfg.get("show_tank_pct", False):
        if fuel_pct_raw is not None:
            fuel_pct = fuel_pct_raw
        elif cap and isinstance(fuel, (int, float)) and cap > 0:
            fuel_pct = 100.0 * fuel / cap

    legal_min = None
    if laps_rem is not None and u_avg and u_avg > 0:
        buf = float(cfg.get("legal_fuel_buffer_l", 2.0) or 0.0)
        legal_min = max(0.0, laps_rem * u_avg + buf)

    stints = None
    stint_laps = float(cfg.get("stint_laps", 0) or 0)
    if cfg.get("show_stints", False) and stint_laps > 0 and u_avg and u_avg > 0 and cap:
        stints = int(cap // (stint_laps * u_avg))

    def scenario(u):
        if not u or u <= 0 or not isinstance(fuel, (int, float)):
            return {"usage": u, "laps": None, "pits": None, "refuel": None}
        laps_on_fuel = fuel / u
        refuel = None
        pits = None
        if laps_rem is not None:
            refuel = max(0.0, laps_rem * u - fuel)
            if cap and cap > 0:
                pits = refuel / cap
        return {"usage": u, "laps": laps_on_fuel, "pits": pits, "refuel": refuel}

    rows = {"avg": scenario(u_avg), "max": scenario(u_max), "min": scenario(u_min)}
    laps_empty = rows["avg"]["laps"]
    time_empty = (laps_empty * lap_avg) if (laps_empty and lap_avg) else None
    laps_margin = (laps_empty - laps_rem) if (laps_empty is not None
                                              and laps_rem is not None) else None
    time_margin = (time_empty - time_rem) if (time_empty is not None
                                              and time_rem is not None) else None
    add = rows["avg"]["refuel"]

    window = None
    win_open = False
    strip = {"total": 0, "window": None, "now": None}
    if lap is not None and rows["max"]["laps"] and rows["min"]["laps"] \
            and add and add > 0:
        a = lap + int(rows["max"]["laps"])
        b = lap + int(rows["min"]["laps"])
        window = (a, b)
        win_open = lap >= a - 1
        if laps_rem is not None and laps_rem > 0:
            total = max(1, min(40, int(round(laps_rem))))
            wa = max(0, min(total - 1, int(rows["max"]["laps"])))
            wb = max(0, min(total - 1, int(rows["min"]["laps"])))
            try:
                total_laps = int(ir["SessionLapsTotal"])
            except (TypeError, ValueError, KeyError):
                total_laps = None
            if total_laps and total_laps > 0:
                elapsed = max(0, lap - 1)
                now_idx = max(0, min(total - 1,
                                    int(round(elapsed / total_laps * total))))
            else:
                now_idx = max(0, min(total - 1, total - int(laps_rem)))
            strip = {"total": total, "window": (wa, wb), "now": now_idx}

    pit_hint = None
    if cfg.get("show_pit_compare", False) and u_avg and u_avg > 0:
        loss = float(cfg.get("pit_loss_seconds", 25.0) or 25.0)
        pit_hint = f"Pit now ~{loss:.0f}s vs +2 laps ~{2 * u_avg:.1f}L"

    alert = False
    if cfg.get("show_low_fuel_alert", True):
        lt = float(cfg.get("low_fuel_laps_threshold", 2.0) or 2.0)
        tt = float(cfg.get("low_fuel_time_threshold", 120.0) or 120.0)
        if laps_margin is not None and laps_margin < lt:
            alert = True
        if time_margin is not None and time_margin < tt:
            alert = True

    return {
        "level": fuel,
        "cap": cap,
        "add": add,
        "window": window,
        "window_open": win_open,
        "rows": rows,
        "time_empty": time_empty,
        "time_margin": time_margin,
        "laps_empty": laps_empty,
        "laps_margin": laps_margin,
        "strip": strip,
        "live_burn": live_burn,
        "fuel_pct": fuel_pct,
        "legal_min": legal_min,
        "stints": stints,
        "pit_hint": pit_hint,
        "alert": alert,
        "lap": lap,
        "laps_remaining": laps_rem,
        "usage_avg": u_avg,
    }


def _nearest_with_numbers(
    est_time,
    player: int | None,
    lap_est: float,
    drivers: dict,
    pace_idxs: set[int] | None,
) -> tuple[float | None, float | None, str | None, str | None]:
    """Gaps and car numbers for nearest ahead / behind on track."""
    if player is None or not est_time or lap_est <= 0 or player >= len(est_time):
        return None, None, None, None
    me = est_time[player]
    if me is None:
        return None, None, None, None
    pace_idxs = pace_idxs or set()
    best_ahead = None
    best_behind = None
    ahead_idx = None
    behind_idx = None
    for idx, t in enumerate(est_time):
        if idx == player or t is None or idx in pace_idxs:
            continue
        delta = tr.wrap_est_delta(t, me, lap_est)
        if delta > 0:
            if best_ahead is None or delta < best_ahead:
                best_ahead = delta
                ahead_idx = idx
        elif delta < 0:
            if best_behind is None or delta > best_behind:
                best_behind = delta
                behind_idx = idx
    behind_gap = abs(best_behind) if best_behind is not None else None

    def _num(idx):
        if idx is None:
            return None
        d = drivers.get(idx) if drivers else None
        if not d:
            return None
        n = str(d.get("CarNumber", "")).strip()
        return n if n else None

    return best_ahead, behind_gap, _num(ahead_idx), _num(behind_idx)


def _green_run_long_enough(caution_hist: dict, cfg: dict) -> bool:
    min_laps = int(cfg.get("green_run_caution_bias_laps", 15) or 0)
    if min_laps <= 0:
        return True
    return (caution_hist.get("green_run_laps") or 0) >= min_laps


def build_pit_context(
    *,
    player: int | None,
    positions,
    est_time,
    lap_est: float,
    drivers: dict,
    pace_idxs: set[int] | None,
    flag: str | None,
    flag_context: str | None,
    session_flags: int,
    pits_open: bool | None = None,
) -> dict:
    position = None
    if player is not None and positions and player < len(positions):
        p = positions[player]
        if p and p > 0:
            position = int(p)

    gap_ahead, gap_behind, car_ahead, car_behind = _nearest_with_numbers(
        est_time, player, lap_est, drivers, pace_idxs)

    if pits_open is not None:
        pits_closed = not bool(pits_open)
    else:
        pits_closed = bool(session_flags & _FLAG_CAUTION_WAVING)
        if flag_context and "pits closed" in flag_context.lower():
            pits_closed = True

    return {
        "position": position,
        "gap_ahead": gap_ahead,
        "gap_behind": gap_behind,
        "car_ahead": car_ahead,
        "car_behind": car_behind,
        "caution": flag == "yellow",
        "pits_closed": pits_closed,
        "flag_context": flag_context,
    }


def _fmt_window(window) -> str:
    if not window or len(window) != 2:
        return ""
    return f"Best stop: laps {window[0]}\u2013{window[1]}"


_REENTRY_PLAIN = {
    "PACK": "Busy merge on exit",
    "TRAFFIC": "One car at merge",
    "LAPPED_DANGER": "Pit now may put you a lap down",
    "CLEAN": "",
}


def _plain_reentry(verdict: str | None) -> str:
    if not verdict:
        return ""
    return _REENTRY_PLAIN.get(verdict, "")


_FIELD_FUEL_NOTE = "We only see who's pitting, not their fuel level"


_PIT_SURFACES = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)


@dataclass
class CautionTracker:
    caution_count: int = 0
    caution_laps: list[int] = field(default_factory=list)
    green_run_laps: int = 0
    green_run_secs: float = 0.0
    was_yellow: bool = False
    last_lap: int | None = None
    last_session_time: float | None = None
    fuel_ema_reset: bool = False

    def as_dict(self) -> dict:
        laps_since = None
        if self.last_lap is not None and self.caution_laps:
            laps_since = max(0, self.last_lap - self.caution_laps[-1])
        rate = self.caution_count / max(1, self.last_lap or 1)
        avg_between = None
        if len(self.caution_laps) >= 2:
            gaps = [self.caution_laps[i] - self.caution_laps[i - 1]
                    for i in range(1, len(self.caution_laps))]
            avg_between = sum(gaps) / len(gaps)
        return {
            "caution_count": self.caution_count,
            "caution_laps": list(self.caution_laps),
            "green_run_laps": self.green_run_laps,
            "green_run_secs": self.green_run_secs,
            "laps_since_last_caution": laps_since,
            "caution_rate": rate,
            "avg_laps_between_cautions": avg_between,
            "fuel_ema_reset": self.fuel_ema_reset,
            "was_yellow": self.was_yellow,
        }


def update_caution_tracker(
    tracker: CautionTracker,
    *,
    yellow: bool,
    lap: int | None,
    session_time: float | None,
) -> CautionTracker:
    """Update session caution history; set fuel_ema_reset on yellow→green."""
    tracker.fuel_ema_reset = False
    if tracker.was_yellow and not yellow:
        tracker.fuel_ema_reset = True
        tracker.green_run_laps = 0
        tracker.green_run_secs = 0.0

    if yellow and not tracker.was_yellow:
        tracker.caution_count += 1
        if lap is not None:
            tracker.caution_laps.append(int(lap))
        tracker.green_run_laps = 0
        tracker.green_run_secs = 0.0

    if not yellow:
        if lap is not None and tracker.last_lap is not None and lap > tracker.last_lap:
            tracker.green_run_laps += lap - tracker.last_lap
        if session_time is not None and tracker.last_session_time is not None:
            dt = session_time - tracker.last_session_time
            if dt > 0:
                tracker.green_run_secs += dt

    tracker.was_yellow = yellow
    if lap is not None:
        tracker.last_lap = int(lap)
    if session_time is not None:
        tracker.last_session_time = float(session_time)
    return tracker


def _car_pitting(
    idx: int,
    on_pit_road,
    surface,
) -> bool:
    if on_pit_road is not None and idx < len(on_pit_road) and on_pit_road[idx]:
        return True
    if surface is not None and idx < len(surface):
        try:
            return int(surface[idx]) in _PIT_SURFACES
        except (TypeError, ValueError):
            pass
    return False


def _position_map(positions, pace_idxs: set[int] | None) -> dict[int, int]:
    """CarIdx -> race position (positive only)."""
    pace_idxs = pace_idxs or set()
    out: dict[int, int] = {}
    if not positions:
        return out
    for idx, p in enumerate(positions):
        if idx in pace_idxs:
            continue
        if p and p > 0:
            out[idx] = int(p)
    return out


def _idx_at_position(pos_map: dict[int, int], pos: int) -> int | None:
    for idx, p in pos_map.items():
        if p == pos:
            return idx
    return None


def pace_falloff_s(ll_laps: list[dict]) -> float:
    good = [float(l["secs"]) for l in ll_laps if l.get("secs", 0) > 0][:5]
    if len(good) < 2:
        return 0.0
    avg3 = sum(good[:3]) / len(good[:3])
    return max(0.0, avg3 - min(good))


def _wear_min_pct(corners: dict) -> float | None:
    wears: list[float] = []
    for entry in (corners or {}).values():
        w = entry.get("wear") if isinstance(entry, dict) else None
        if isinstance(w, (int, float)):
            fv = float(w)
            wears.append(fv * 100.0 if fv <= 1.0 else fv)
    return min(wears) if wears else None


def _laps_since_pit_idx(
    idx: int | None,
    pit_state: dict,
    car_lap,
) -> int | None:
    if idx is None or not pit_state or not car_lap:
        return None
    st = pit_state.get(idx)
    if not st or st.get("lap") is None or idx >= len(car_lap):
        return None
    try:
        return max(0, int(car_lap[idx]) - int(st["lap"]))
    except (TypeError, ValueError):
        return None


def _player_pit_stop_count(player: int | None, pit_state: dict) -> int:
    if player is None or not pit_state:
        return 0
    st = pit_state.get(player)
    if not st:
        return 0
    return int(st.get("pit_count") or 0)


def resolve_pit_loss(cfg: dict, pit_state: dict, player: int | None) -> float:
    """Config pit loss, replaced by player EMA when measured stops exist."""
    default = float(cfg.get("pit_loss_seconds", 28.0) or 28.0)
    if not cfg.get("use_measured_pit_loss", True) or player is None or pit_state is None:
        return default
    st = pit_state.get(player)
    if not st:
        return default
    measured = st.get("pit_loss_ema")
    if not isinstance(measured, (int, float)) or measured <= 0:
        return default
    lo = float(cfg.get("pit_loss_measured_min_s", psc.PIT_LOSS_MEASURED_MIN_S) or 0.0)
    hi = float(cfg.get("pit_loss_measured_max_s", psc.PIT_LOSS_MEASURED_MAX_S) or 999.0)
    return max(lo, min(hi, float(measured)))


def record_pit_loss_sample(
    pit_state: dict,
    player: int | None,
    duration: float,
    *,
    cfg: dict | None = None,
) -> None:
    """Update player pit-loss EMA from a completed stop duration."""
    cfg = cfg or {}
    if player is None or pit_state is None:
        return
    lo = float(cfg.get("pit_loss_duration_min_s", psc.PIT_LOSS_DURATION_MIN_S) or 0.0)
    hi = float(cfg.get("pit_loss_duration_max_s", psc.PIT_LOSS_DURATION_MAX_S) or 999.0)
    if duration < lo or duration > hi:
        return
    st = pit_state.setdefault(player, {})
    alpha = float(cfg.get("pit_loss_ema_alpha", psc.PIT_LOSS_EMA_ALPHA) or 0.35)
    prev = st.get("pit_loss_ema")
    if prev is None:
        st["pit_loss_ema"] = duration
    else:
        st["pit_loss_ema"] = alpha * duration + (1.0 - alpha) * float(prev)


def build_tire_snapshot(
    telemetry: dict,
    *,
    ll_laps: list[dict],
    cfg: dict,
    pit_state: dict,
    player: int | None,
    car_lap,
    laps_remaining: float | None,
    pit_loss: float | None = None,
) -> dict:
    from . import telemetry as tele

    laps_since = _laps_since_pit_idx(player, pit_state, car_lap) or 0
    inventory = tele.resolve_tire_inventory(
        telemetry, cfg,
        pit_stops_count=_player_pit_stop_count(player, pit_state),
    )
    wear_min = _wear_min_pct(telemetry.get("tire_corners") or {})
    falloff = pace_falloff_s(ll_laps)
    if pit_loss is None:
        pit_loss = resolve_pit_loss(cfg, pit_state, player)
    else:
        pit_loss = float(pit_loss)
    payback = psc.triangular_payback_lap(falloff, pit_loss)
    wear_warn = float(cfg.get("tire_warn_wear_pct", psc.TIRE_WARN_WEAR_PCT))
    wear_crit = float(cfg.get("tire_critical_wear_pct", psc.TIRE_CRITICAL_WEAR_PCT))
    low_tire_laps = float(cfg.get("low_tire_laps_threshold", psc.LOW_TIRE_LAPS_THRESHOLD))
    min_stint = int(cfg.get("min_stint_laps", psc.MIN_STINT_LAPS) or 0)

    wear_rate = None
    if player is not None and pit_state and wear_min is not None and laps_since > 0:
        st = pit_state.get(player) or {}
        wear_at = st.get("wear_at_pit")
        if isinstance(wear_at, (int, float)) and wear_at > wear_min:
            wear_rate = (float(wear_at) - wear_min) / laps_since

    laps_to_warn = None
    if wear_min is not None and wear_rate and wear_rate > 0:
        laps_to_warn = max(0.0, (wear_min - wear_warn) / wear_rate)

    tire_critical = False
    if wear_min is not None:
        tire_critical = wear_min <= wear_crit
        if laps_to_warn is not None and laps_to_warn < low_tire_laps:
            tire_critical = True
    elif falloff > 0 and laps_since >= payback and laps_since >= min_stint:
        tire_critical = False

    tire_window_open = False
    wet_thresh = float(cfg.get("track_wetness_tire_suppress", psc.TRACK_WETNESS_TIRE_SUPPRESS))
    track_wet = telemetry.get("track_wetness")
    dry_track = track_wet is None or float(track_wet) <= wet_thresh

    if dry_track and not inventory.get("inventory_blocks_window"):
        if wear_min is not None and wear_min <= wear_warn:
            tire_window_open = True
        elif falloff > 0 and laps_since >= max(min_stint, payback):
            tire_window_open = True
        if inventory.get("tire_inventory_low") and not tire_critical:
            tire_window_open = False

    tire_stops_needed = None
    stint_laps = max(min_stint, payback, 1)
    if laps_remaining is not None and laps_remaining > 0:
        tire_stops_needed = max(0, int((float(laps_remaining) + stint_laps - 1) // stint_laps) - 1)

    stints_affordable = None
    if inventory.get("sets_limited") and inventory.get("sets_remaining") is not None:
        stints_affordable = int(inventory["sets_remaining"])

    if stints_affordable is not None and not tire_critical:
        if stints_affordable <= 1:
            tire_window_open = False
        elif tire_stops_needed is not None and stints_affordable < tire_stops_needed:
            tire_window_open = True

    return {
        **inventory,
        "wear_min_pct": wear_min,
        "laps_since_pit": laps_since,
        "pace_falloff_s": falloff,
        "pit_payback_laps": payback,
        "wear_rate_per_lap": wear_rate,
        "laps_to_warn": laps_to_warn,
        "window_open": tire_window_open,
        "tire_critical": tire_critical,
        "laps_remaining": laps_remaining,
        "stints_affordable": stints_affordable,
        "tire_stops_needed": tire_stops_needed,
        "dry_track": dry_track,
        "pit_loss_s": pit_loss,
    }


def _field_chaos_score(
    surface,
    car_flags,
    pace_idxs: set[int] | None,
    field_size: int,
    caution_rate: float,
) -> float:
    if not field_size:
        return 0.0
    off_track = 0
    flagged = 0
    pace_idxs = pace_idxs or set()
    n = 0
    if surface is not None:
        n = max(n, len(surface))
    if car_flags is not None:
        n = max(n, len(car_flags))
    for idx in range(n):
        if idx in pace_idxs:
            continue
        if surface is not None and idx < len(surface):
            try:
                if int(surface[idx]) in (oc.TRK_OFF_TRACK, oc.TRK_NOT_IN_WORLD):
                    off_track += 1
            except (TypeError, ValueError):
                pass
        if car_flags is not None and idx < len(car_flags):
            if tr.car_flag_kind(car_flags[idx]):
                flagged += 1
    chaos = (off_track + flagged) / max(1, field_size)
    chaos = min(1.0, chaos + min(0.5, caution_rate))
    return chaos


def build_caution_outlook(
    caution_hist: dict,
    field_intel: dict,
    *,
    current_lap: int | None,
    cfg: dict,
) -> dict:
    chaos = field_intel.get("field_chaos_score", 0.0) or 0.0
    caution_rate = caution_hist.get("caution_rate", 0.0) or 0.0
    chaos = min(1.0, chaos + min(0.5, caution_rate))
    green_run = caution_hist.get("green_run_laps", 0) or 0
    caution_count = caution_hist.get("caution_count", 0) or 0
    caution_laps = caution_hist.get("caution_laps") or []
    avg_between = caution_hist.get("avg_laps_between_cautions")
    overdue_ratio = float(cfg.get("caution_overdue_ratio", psc.CAUTION_OVERDUE_RATIO))
    chaos_thresh = float(cfg.get("field_chaos_high_threshold",
                                 psc.FIELD_CHAOS_HIGH_THRESHOLD))

    trend = "STEADY"
    if len(caution_laps) >= 2 and current_lap and current_lap > 0:
        mid = max(1, current_lap // 2)
        early = [lap for lap in caution_laps if lap <= mid]
        late = [lap for lap in caution_laps if lap > mid]
        if early and not late and green_run >= 5:
            trend = "CALMING"
        elif len(caution_laps) >= 3:
            gaps = [caution_laps[i] - caution_laps[i - 1]
                    for i in range(1, len(caution_laps))]
            if len(gaps) >= 2 and gaps[-1] < gaps[0] * 0.7:
                trend = "HEATING_UP"
            elif len(gaps) >= 2 and gaps[-1] > gaps[0] * 1.3:
                trend = "CALMING"

    overdue = 0.0
    if avg_between and avg_between > 0:
        overdue = green_run / avg_between

    likelihood = "MEDIUM"
    if chaos >= chaos_thresh or trend == "HEATING_UP" or overdue >= overdue_ratio:
        likelihood = "HIGH"
    elif trend == "CALMING" and green_run >= 8 and chaos < chaos_thresh * 0.5:
        likelihood = "LOW"

    summary = None
    if likelihood == "HIGH" and chaos >= chaos_thresh:
        summary = "Messy field \u2014 yellow could come soon"
    elif trend == "CALMING" and caution_count > 0:
        summary = f"{caution_count} caution{'s' if caution_count != 1 else ''} early \u2014 field calming"
    elif likelihood == "LOW":
        summary = "Field has been clean lately"

    return {
        "likelihood": likelihood,
        "trend": trend,
        "overdue": overdue,
        "summary": summary,
        "field_chaos_score": chaos,
    }


def _fmt_tire_inventory(tire: dict | None) -> str | None:
    if not tire or not tire.get("sets_limited"):
        return None
    cur = tire.get("current_set")
    total = tire.get("sets_total")
    rem = tire.get("sets_remaining")
    if cur is None and rem is None:
        return None
    parts = []
    if cur is not None and total is not None:
        src = tire.get("inventory_source")
        suffix = " (manual)" if src == "manual" else ""
        parts.append(f"Set {cur} of {total}{suffix}")
    elif rem is not None:
        parts.append(f"{rem} set{'s' if rem != 1 else ''} left")
    return parts[0] if parts else None


# Opponent tire-status labels (inferred from pit history).
TIRE_STATUS_FRESH = "FRESH"
TIRE_STATUS_OK = "OK"
TIRE_STATUS_DUE = "DUE"
TIRE_STATUS_BANKRUPT = "BANKRUPT"
TIRE_STATUS_DONE = "DONE"


def _effective_tire_stop_count(idx: int, pit_state: dict) -> int:
    st = pit_state.get(idx) or {}
    if "tire_stop_count" in st:
        return int(st["tire_stop_count"])
    return int(st.get("pit_count") or 0)


def _resolve_race_tire_sets(cfg: dict, sets_total: int | None = None) -> int | None:
    manual = int(cfg.get("race_tire_sets_total", 0) or 0)
    if manual > 0:
        return manual
    if isinstance(sets_total, int) and 0 < sets_total < psc.TIRE_SETS_UNLIMITED:
        return sets_total
    return None


def _infer_opponent_tire_status(
    *,
    tire_stops: int,
    stint_laps: int,
    race_sets: int | None,
    fresh_delta: int,
    min_stint: int,
    due_laps: int,
    laps_remaining: float | None,
    recent_pit_window: int,
) -> str:
    if stint_laps <= fresh_delta and tire_stops > 0:
        return TIRE_STATUS_FRESH

    stops_remaining = None
    if race_sets is not None and race_sets > 0:
        stops_remaining = max(0, race_sets - 1 - tire_stops)

    if race_sets is not None and race_sets > 0:
        if stops_remaining == 0 and stint_laps >= min_stint:
            return TIRE_STATUS_BANKRUPT
        if (stops_remaining == 0 and stint_laps <= recent_pit_window
                and laps_remaining is not None and laps_remaining > due_laps):
            return TIRE_STATUS_DONE

    if stint_laps >= due_laps:
        return TIRE_STATUS_DUE
    return TIRE_STATUS_OK


def _opponent_gain_opportunity(
    status: str,
    *,
    stops_remaining: int | None,
    stint_laps: int,
    min_stint: int,
) -> bool:
    if status == TIRE_STATUS_BANKRUPT:
        return True
    if status == TIRE_STATUS_DUE and stops_remaining == 0 and stint_laps >= min_stint:
        return True
    return False


def build_opponent_profiles(
    *,
    ahead_idxs: list[int],
    pit_state: dict,
    car_lap,
    cfg: dict,
    laps_remaining: float | None = None,
    sets_total: int | None = None,
) -> list[dict]:
    """Per-car ahead profiles with inferred tire-set status."""
    if not cfg.get("opponent_tire_inference_enabled", True):
        return []

    scan = int(cfg.get("ahead_profile_scan_positions",
                       psc.AHEAD_PROFILE_SCAN_POSITIONS)
               or psc.AHEAD_PROFILE_SCAN_POSITIONS)
    fresh_delta = int(cfg.get("fresh_tire_lap_delta", psc.FRESH_TIRE_LAP_DELTA) or 0)
    min_stint = int(cfg.get("min_stint_laps", psc.MIN_STINT_LAPS) or 0)
    due_laps = int(cfg.get("opponent_stint_due_laps", psc.OPPONENT_STINT_DUE_LAPS)
                   or psc.OPPONENT_STINT_DUE_LAPS)
    recent_win = int(cfg.get("recent_pit_laps_window", 3) or 3)
    race_sets = _resolve_race_tire_sets(cfg, sets_total)

    profiles: list[dict] = []
    for idx in ahead_idxs[:scan]:
        stint = _laps_since_pit_idx(idx, pit_state, car_lap)
        if stint is None:
            continue
        tire_stops = _effective_tire_stop_count(idx, pit_state)
        stops_remaining = None
        if race_sets is not None:
            stops_remaining = max(0, race_sets - 1 - tire_stops)
        status = _infer_opponent_tire_status(
            tire_stops=tire_stops,
            stint_laps=stint,
            race_sets=race_sets,
            fresh_delta=fresh_delta,
            min_stint=min_stint,
            due_laps=due_laps,
            laps_remaining=laps_remaining,
            recent_pit_window=recent_win,
        )
        profiles.append({
            "idx": idx,
            "stint_laps": stint,
            "tire_stops": tire_stops,
            "stops_remaining": stops_remaining,
            "tire_status": status,
        })
    return profiles


def compute_green_position_tradeoff(
    *,
    profiles: list[dict],
    positions_lost_now: int,
    cfg: dict,
) -> dict:
    """Net positions gained by pitting before tire-bankrupt cars ahead."""
    min_stint = int(cfg.get("min_stint_laps", psc.MIN_STINT_LAPS) or 0)
    gain = sum(
        1 for p in profiles
        if _opponent_gain_opportunity(
            p.get("tire_status", ""),
            stops_remaining=p.get("stops_remaining"),
            stint_laps=int(p.get("stint_laps") or 0),
            min_stint=min_stint,
        )
    )
    lost = int(positions_lost_now or 0)
    net = gain - lost
    min_net = int(cfg.get("strategic_pit_min_net_positions",
                           psc.STRATEGIC_PIT_MIN_NET_POSITIONS)
                  or psc.STRATEGIC_PIT_MIN_NET_POSITIONS)
    return {
        "positions_lost_now": lost,
        "positions_gain_ahead": gain,
        "net_gain": net,
        "strategic_pit_viable": net >= min_net,
    }


def compute_reentry_verdict(
    *,
    player_lap_pct: float | None,
    lap_pcts,
    pit_loss: float,
    lap_est: float,
    gap_to_leader: float | None,
    on_pit_road,
    surface,
    pace_idxs: set[int] | None,
    player: int | None,
    reentry_window: float,
) -> dict:
    if player_lap_pct is None or not lap_pcts or lap_est <= 0:
        return {"v": "CLEAN", "bv": "CLEAN", "n": 0}

    pit_frac = (pit_loss / lap_est) % 1.0
    reentry_dist = (float(player_lap_pct) + pit_frac) % 1.0
    pace_idxs = pace_idxs or set()
    count = 0
    for idx, pct in enumerate(lap_pcts):
        if idx == player or idx in pace_idxs or pct is None:
            continue
        if _car_pitting(idx, on_pit_road, surface):
            continue
        if psc.lap_pct_interval(float(pct), reentry_dist) <= reentry_window:
            count += 1

    if count == 0:
        bv = "CLEAN"
    elif count == 1:
        bv = "TRAFFIC"
    else:
        bv = "PACK"

    verdict = bv
    pll = None
    if gap_to_leader is not None and pit_loss > gap_to_leader and lap_est > 0:
        pll = int((pit_loss - gap_to_leader) / lap_est)
        if pll >= 1:
            verdict = "LAPPED_DANGER"

    return {"v": verdict, "bv": bv, "n": count, "gtl": gap_to_leader, "pll": pll}


def compute_caution_impact(
    *,
    player_pos: int | None,
    positions,
    car_lap,
    pra: float,
    pace_idxs: set[int] | None,
) -> dict:
    if player_pos is None or not positions or not car_lap:
        return {"lost_lead": 0, "exit_rank_lead": player_pos or 0}

    pos_map = _position_map(positions, pace_idxs)
    laps = []
    for idx, p in pos_map.items():
        if idx < len(car_lap) and car_lap[idx] is not None:
            try:
                laps.append(int(car_lap[idx]))
            except (TypeError, ValueError):
                pass
    if not laps:
        return {"lost_lead": 0, "exit_rank_lead": player_pos}

    leader_lap = max(laps)
    lead_positions = sorted(p for idx, p in pos_map.items()
                            if idx < len(car_lap) and car_lap[idx] == leader_lap)
    if player_pos not in lead_positions:
        lead_ahead = sum(1 for p in lead_positions if p < player_pos)
        player_rank_lead = lead_ahead + 1
    else:
        lead_ahead = sum(1 for p in lead_positions if p < player_pos)
        player_rank_lead = lead_positions.index(player_pos) + 1

    stay_out_lead = max(lead_ahead - int(round(lead_ahead * pra)),
                        int(round(lead_ahead * (1.0 - pra))))
    pitting_lead = max(0, lead_ahead - stay_out_lead)
    exit_rank = min(len(lead_positions) + 1, stay_out_lead + pitting_lead + 1)
    lost_lead = max(0, exit_rank - player_rank_lead)
    return {
        "lost_lead": lost_lead,
        "exit_rank_lead": exit_rank,
        "player_rank_lead": player_rank_lead,
        "lead_ahead": lead_ahead,
    }


def build_field_intel(
    *,
    player: int | None,
    positions,
    car_lap,
    on_pit_road,
    surface,
    pit_state: dict,
    lap_pcts,
    f2_time,
    est_time,
    lap_est: float,
    pace_idxs: set[int] | None,
    current_lap: int | None,
    pit_loss: float,
    cfg: dict,
    ll_laps: list[dict] | None = None,
    car_last=None,
    car_flags=None,
    session_time: float | None = None,
    closing_state: dict | None = None,
    laps_remaining: float | None = None,
    sets_total: int | None = None,
) -> dict:
    """Field-ahead scan, pitting ratios, reentry verdict, caution impact."""
    pos_map = _position_map(positions, pace_idxs)
    player_pos = pos_map.get(player) if player is not None else None
    recent_win = int(cfg.get("recent_pit_laps_window", 3) or 3)
    reentry_window = float(cfg.get("reentry_window_pct", psc.REENTRY_WINDOW_PCT)
                           or psc.REENTRY_WINDOW_PCT)
    ahead_scan = int(cfg.get("ahead_scan_positions", psc.AHEAD_SCAN_POSITIONS)
                     or psc.AHEAD_SCAN_POSITIONS)
    pace_delta = float(cfg.get("ahead_pace_delta_s", psc.AHEAD_PACE_DELTA_S))
    fresh_delta = int(cfg.get("fresh_tire_lap_delta", psc.FRESH_TIRE_LAP_DELTA) or 0)

    ahead_idxs: list[int] = []
    behind_idxs: list[int] = []
    if player_pos is not None:
        for idx, p in pos_map.items():
            if p < player_pos:
                ahead_idxs.append(idx)
            elif p > player_pos:
                behind_idxs.append(idx)
        ahead_idxs.sort(key=lambda i: pos_map[i])

    def _pitted_recent(idx: int) -> bool:
        st = pit_state.get(idx) if pit_state else None
        if not st or st.get("lap") is None or current_lap is None:
            return False
        if idx >= len(car_lap) or car_lap[idx] is None:
            return False
        return (int(car_lap[idx]) - int(st["lap"])) <= recent_win

    def _pitted_this_lap(idx: int) -> bool:
        st = pit_state.get(idx) if pit_state else None
        if not st or st.get("lap") is None or current_lap is None:
            return False
        return int(st["lap"]) == int(current_lap)

    ahead_on_pit = sum(1 for i in ahead_idxs if _car_pitting(i, on_pit_road, surface))
    behind_on_pit = sum(1 for i in behind_idxs if _car_pitting(i, on_pit_road, surface))
    ahead_total = len(ahead_idxs)
    behind_total = len(behind_idxs)
    pra = (ahead_on_pit / ahead_total) if ahead_total > 0 else 0.0
    prb = (behind_on_pit / behind_total) if behind_total > 0 else 0.0

    leader_on_pit = False
    leader_pitted_recent = False
    p1 = _idx_at_position(pos_map, 1)
    if p1 is not None:
        leader_on_pit = _car_pitting(p1, on_pit_road, surface)
        leader_pitted_recent = _pitted_recent(p1)

    ahead_pitted_recent = sum(1 for i in ahead_idxs if _pitted_recent(i))
    ahead_pitted_this_lap = sum(1 for i in ahead_idxs if _pitted_this_lap(i))

    player_stint = _laps_since_pit_idx(player, pit_state, car_lap) or 0
    your_pace = lap_est if lap_est > 0 else 90.0
    if ll_laps:
        good = [l["secs"] for l in ll_laps if l.get("secs", 0) > 0][:3]
        if good:
            your_pace = sum(good) / len(good)

    ahead_faster_count = 0
    ahead_fresh_tires_count = 0
    ahead_stale_stint_count = 0
    nearest_ahead_stint_laps = None
    nearest_ahead_last_lap = None
    nearest_ahead_closing = None
    nearest_behind_closing = None
    closing_state = closing_state or {}
    now = session_time if session_time is not None else 0.0

    for idx in ahead_idxs[:ahead_scan]:
        stint = _laps_since_pit_idx(idx, pit_state, car_lap)
        if stint is not None:
            if nearest_ahead_stint_laps is None:
                nearest_ahead_stint_laps = stint
            if stint < player_stint - fresh_delta:
                ahead_fresh_tires_count += 1
            elif stint > player_stint:
                ahead_stale_stint_count += 1
        if car_last and idx < len(car_last):
            try:
                ll = float(car_last[idx])
            except (TypeError, ValueError):
                ll = None
            if ll and ll > 0:
                if nearest_ahead_last_lap is None:
                    nearest_ahead_last_lap = ll
                if ll < your_pace - pace_delta:
                    ahead_faster_count += 1

    if (player is not None and est_time and lap_est > 0 and now > 0):
        for idx in ahead_idxs[:ahead_scan]:
            if _car_pitting(idx, on_pit_road, surface):
                continue
            gap = tr.est_interval(est_time, idx, player, lap_est)
            if gap is not None:
                nearest_ahead_closing = tr.closing_rate(
                    closing_state, idx, gap, now)
                break
        behind_sorted = sorted(behind_idxs, key=lambda i: pos_map[i])
        for idx in behind_sorted[:1]:
            if _car_pitting(idx, on_pit_road, surface):
                continue
            gap = tr.est_interval(est_time, idx, player, lap_est)
            if gap is not None:
                nearest_behind_closing = tr.closing_rate(
                    closing_state, idx, gap, now)
                break

    off_track_count = 0
    incident_flag_count = 0
    field_size = len(pos_map) if pos_map else 0
    pace_idxs = pace_idxs or set()
    n = 0
    if surface is not None:
        n = max(n, len(surface))
    if car_flags is not None:
        n = max(n, len(car_flags))
    for idx in range(n):
        if idx in pace_idxs:
            continue
        if surface is not None and idx < len(surface):
            try:
                if int(surface[idx]) in (oc.TRK_OFF_TRACK, oc.TRK_NOT_IN_WORLD):
                    off_track_count += 1
            except (TypeError, ValueError):
                pass
        if car_flags is not None and idx < len(car_flags):
            if tr.car_flag_kind(car_flags[idx]):
                incident_flag_count += 1

    field_chaos_score = _field_chaos_score(
        surface, car_flags, pace_idxs, field_size, caution_rate=0.0)

    gap_to_leader = None
    if player is not None and f2_time and player < len(f2_time):
        fv = f2_time[player]
        if isinstance(fv, (int, float)) and fv > 0:
            gap_to_leader = float(fv)

    player_lap_pct = None
    if player is not None and lap_pcts and player < len(lap_pcts):
        try:
            player_lap_pct = float(lap_pcts[player])
        except (TypeError, ValueError):
            pass

    reentry = compute_reentry_verdict(
        player_lap_pct=player_lap_pct,
        lap_pcts=lap_pcts,
        pit_loss=pit_loss,
        lap_est=lap_est,
        gap_to_leader=gap_to_leader,
        on_pit_road=on_pit_road,
        surface=surface,
        pace_idxs=pace_idxs,
        player=player,
        reentry_window=reentry_window,
    )

    caution_impact = compute_caution_impact(
        player_pos=player_pos,
        positions=positions,
        car_lap=car_lap,
        pra=pra,
        pace_idxs=pace_idxs,
    )

    green_pos_lost = _green_position_loss(
        player_pos=player_pos,
        pos_map=pos_map,
        est_time=est_time,
        f2_time=f2_time,
        lap_est=lap_est,
        pit_loss=pit_loss,
        ll_laps=ll_laps,
        player_idx=player,
    )

    opponent_profiles = build_opponent_profiles(
        ahead_idxs=ahead_idxs,
        pit_state=pit_state,
        car_lap=car_lap,
        cfg=cfg,
        laps_remaining=laps_remaining,
        sets_total=sets_total,
    )
    ahead_bankrupt_count = sum(
        1 for p in opponent_profiles
        if p.get("tire_status") == TIRE_STATUS_BANKRUPT)
    position_tradeoff = compute_green_position_tradeoff(
        profiles=opponent_profiles,
        positions_lost_now=green_pos_lost,
        cfg=cfg,
    )

    return {
        "player_pos": player_pos,
        "ahead_total": ahead_total,
        "ahead_on_pit": ahead_on_pit,
        "behind_total": behind_total,
        "behind_on_pit": behind_on_pit,
        "pra": pra,
        "prb": prb,
        "leader_on_pit": leader_on_pit,
        "leader_pitted_recent": leader_pitted_recent,
        "ahead_pitted_recent": ahead_pitted_recent,
        "ahead_pitted_this_lap": ahead_pitted_this_lap,
        "ahead_faster_count": ahead_faster_count,
        "ahead_fresh_tires_count": ahead_fresh_tires_count,
        "ahead_stale_stint_count": ahead_stale_stint_count,
        "nearest_ahead_stint_laps": nearest_ahead_stint_laps,
        "nearest_ahead_last_lap": nearest_ahead_last_lap,
        "nearest_ahead_closing": nearest_ahead_closing,
        "nearest_behind_closing": nearest_behind_closing,
        "player_stint_laps": player_stint,
        "off_track_count": off_track_count,
        "incident_flag_count": incident_flag_count,
        "field_chaos_score": field_chaos_score,
        "reentry": reentry,
        "caution_impact": caution_impact,
        "green_positions_lost": green_pos_lost,
        "opponent_profiles": opponent_profiles,
        "ahead_bankrupt_count": ahead_bankrupt_count,
        "position_tradeoff": position_tradeoff,
    }


def _green_position_loss(
    *,
    player_pos: int | None,
    pos_map: dict[int, int],
    est_time,
    f2_time,
    lap_est: float,
    pit_loss: float,
    ll_laps: list[dict] | None,
    player_idx: int | None = None,
) -> int:
    """Positions likely lost pitting now (cars P+1..P+5 behind closing pit_loss)."""
    if player_pos is None:
        return 0
    your_pace = lap_est if lap_est > 0 else 90.0
    if ll_laps:
        good = [l["secs"] for l in ll_laps if l.get("secs", 0) > 0][:3]
        if good:
            your_pace = sum(good) / len(good)

    lost = 0
    for offset in range(1, psc.GREEN_POSITION_SCAN + 1):
        target_pos = player_pos + offset
        idx = _idx_at_position(pos_map, target_pos)
        if idx is None:
            continue
        gap_behind = None
        if player_idx is not None and est_time and lap_est > 0:
            gap_behind = tr.est_interval(est_time, idx, player_idx, lap_est)
        elif f2_time and player_idx is not None and idx < len(f2_time):
            fp = f2_time[player_idx] if player_idx < len(f2_time) else None
            ft = f2_time[idx]
            if fp is not None and ft is not None:
                gap_behind = abs(float(fp) - float(ft))
        if gap_behind is None:
            continue
        if gap_behind < pit_loss:
            lost += 1
    return lost


def build_session_phase(telemetry: dict, cfg: dict) -> dict:
    """Near-finish detection for critical-only pit advice."""
    laps_rem = telemetry.get("session_laps_remain_ex")
    if laps_rem is None:
        laps_rem = telemetry.get("session_laps_remain")
    final_n = int(cfg.get("final_laps_optional_suppress",
                            psc.FINAL_LAPS_OPTIONAL_SUPPRESS) or 0)
    sf = int(telemetry.get("session_flags") or 0)
    white = bool(sf & psc.FLAG_WHITE)
    checkered = bool(sf & psc.FLAG_CHECKERED)
    near_finish = (
        (isinstance(laps_rem, (int, float)) and final_n > 0 and laps_rem <= final_n)
        or white or checkered
    )
    return {
        "near_finish": near_finish,
        "critical_only": near_finish,
        "laps_remaining": laps_rem,
        "white_flag": white,
        "checkered": checkered,
    }


def _green_pit_too_costly(field_intel: dict, cfg: dict) -> bool:
    if cfg.get("green_pos_tradeoff_override", True):
        tradeoff = field_intel.get("position_tradeoff") or {}
        net = int(tradeoff.get("net_gain", 0) or 0)
        lost = int(field_intel.get("green_positions_lost", 0) or 0)
        if net > 0 and net > lost:
            return False
    lost = field_intel.get("green_positions_lost", 0) or 0
    max_lost = int(cfg.get("green_pos_lost_max", psc.GREEN_POS_LOST_MAX) or 0)
    return max_lost > 0 and lost >= max_lost


def _pit_menu_note(
    pit_menu: dict | None,
    *,
    need_fuel: bool = False,
    need_tires: bool = False,
) -> str | None:
    pit_menu = pit_menu or {}
    parts: list[str] = []
    if need_tires and not pit_menu.get("tires_queued"):
        parts.append("add tires to pit menu")
    if need_fuel and not pit_menu.get("fuel_queued"):
        parts.append("add fuel to pit menu")
    return "; ".join(parts) if parts else None


def _pit_menu_missing(
    pit_menu: dict | None,
    *,
    need_fuel: bool = False,
    need_tires: bool = False,
) -> list[str]:
    pit_menu = pit_menu or {}
    missing: list[str] = []
    if need_fuel and not pit_menu.get("fuel_queued"):
        missing.append("fuel")
    if need_tires and not pit_menu.get("tires_queued"):
        missing.append("tires")
    return missing


def _gate_pit_menu_advice(
    advice: PitAdvice,
    pit_menu: dict | None,
    cfg: dict,
    *,
    need_fuel: bool = False,
    need_tires: bool = False,
    on_pit_road: bool = False,
) -> PitAdvice:
    """Block pit-now calls until required services are queued on the pit menu."""
    if not cfg.get("pit_menu_hard_gate", True):
        return advice
    if on_pit_road or advice.rec not in (PitRec.PIT_NOW, PitRec.PIT_NEXT_LAP):
        return advice
    missing = _pit_menu_missing(pit_menu, need_fuel=need_fuel, need_tires=need_tires)
    if not missing:
        return advice
    label = " and ".join(missing)
    return PitAdvice(
        PitRec.MARGINAL, _REC_LABELS[PitRec.MARGINAL],
        f"Add {label} to pit menu before stopping",
        secondary=advice.secondary,
        actionable=True,
    )


def _downgrade_pit_advice(advice: PitAdvice, field_intel: dict, cfg: dict) -> PitAdvice:
    """Downgrade PIT_NOW to MARGINAL when green pit costs too many positions."""
    if advice.rec != PitRec.PIT_NOW or not _green_pit_too_costly(field_intel, cfg):
        return advice
    lost = int(field_intel.get("green_positions_lost", 0) or 0)
    return PitAdvice(
        PitRec.MARGINAL, _REC_LABELS[PitRec.MARGINAL],
        f"Could pit now \u2014 may lose ~{lost} spot{'s' if lost != 1 else ''}",
        secondary=advice.secondary,
        actionable=True,
    )


def build_strategy_extras(
    snapshot: dict,
    *,
    ll_laps: list[dict],
    cfg: dict,
    pit_state: dict,
    player: int | None,
    car_lap,
    on_pit_road,
    field_intel: dict,
    tire_snapshot: dict | None = None,
    session_phase: dict | None = None,
    pit_loss: float | None = None,
) -> dict:
    if pit_loss is None:
        pit_loss = resolve_pit_loss(cfg, pit_state, player)
    else:
        pit_loss = float(pit_loss)
    falloff = pace_falloff_s(ll_laps)
    payback = psc.triangular_payback_lap(falloff, pit_loss)
    lap = snapshot.get("lap")
    laps_margin = snapshot.get("laps_margin")
    laps_empty = snapshot.get("laps_empty")
    fuel_win_open = bool(snapshot.get("window_open"))
    can_make_fuel = laps_margin is not None and laps_margin >= 0

    if can_make_fuel:
        fuel_window = laps_margin is not None and laps_margin <= payback
    elif lap is not None and laps_empty:
        target_stop = lap + int(laps_empty)
        fuel_window = lap >= (target_stop - payback) or fuel_win_open
        if laps_margin is not None and laps_margin <= payback:
            fuel_window = True
    else:
        fuel_window = fuel_win_open

    tire_snapshot = tire_snapshot or {}
    tire_window = bool(tire_snapshot.get("window_open")) and not tire_snapshot.get(
        "inventory_blocks_window")
    inside_window = fuel_window or tire_window

    laps_since_pit = _laps_since_pit_idx(player, pit_state, car_lap) or 0

    quiet_min = int(cfg.get("post_pit_quiet_min_laps", psc.POST_PIT_QUIET_MIN_LAPS)
                    or psc.POST_PIT_QUIET_MIN_LAPS)
    post_pit_lockout = laps_since_pit < quiet_min

    on_pit = False
    if player is not None and on_pit_road and player < len(on_pit_road):
        on_pit = bool(on_pit_road[player])

    fuel_critical = _fuel_critical(snapshot, cfg)
    tire_critical = bool(tire_snapshot.get("tire_critical"))
    must_stop = fuel_critical or tire_critical
    wear_min = tire_snapshot.get("wear_min_pct")
    tire_ok = not tire_critical and not tire_window
    session_phase = session_phase or {}
    near_finish = bool(session_phase.get("near_finish"))

    return {
        "inside_window": inside_window,
        "fuel_window": fuel_window,
        "tire_window": tire_window,
        "post_pit_lockout": post_pit_lockout,
        "post_pit_quiet": post_pit_lockout,
        "laps_since_pit": laps_since_pit,
        "on_pit_road": on_pit,
        "can_make_to_end": can_make_fuel and tire_ok,
        "must_stop": must_stop,
        "fuel_critical": fuel_critical,
        "tire_critical": tire_critical,
        "critical_only": near_finish and not must_stop,
        "near_finish": near_finish,
        "pace_falloff_s": falloff,
        "pit_payback_laps": payback,
        "fuel_laps_left": laps_empty,
        "wear_min_pct": wear_min,
        "pit_loss_s": pit_loss,
        "stop_needs_fuel": fuel_critical or fuel_window,
        "stop_needs_tires": tire_critical or tire_window,
    }


def _format_intel_secondary(
    caution: dict | None,
    field: dict | None,
    cfg: dict,
    *,
    caution_outlook: dict | None = None,
    tire_snapshot: dict | None = None,
    on_green: bool = True,
    green_run_ok: bool = True,
) -> str | None:
    if not cfg.get("show_field_context", True):
        inv_only = cfg.get("show_tire_inventory", True) and tire_snapshot
        if inv_only:
            inv_txt = _fmt_tire_inventory(tire_snapshot)
            return inv_txt
        return None
    parts: list[str] = []
    show_outlook = on_green and green_run_ok
    if show_outlook and caution_outlook and caution_outlook.get("summary"):
        parts.append(caution_outlook["summary"])
    elif caution:
        cc = caution.get("caution_count", 0)
        if cc:
            n = int(cc)
            parts.append(f"{n} caution{'s' if n != 1 else ''} so far")
        if show_outlook:
            gr = caution.get("green_run_laps", 0)
            if gr and not caution.get("was_yellow", False):
                parts.append(f"{int(gr)} laps since yellow")
    if field:
        faster = field.get("ahead_faster_count", 0) or 0
        if faster:
            parts.append(f"{faster} ahead faster")
        pra = field.get("pra")
        if pra is not None and field.get("ahead_total", 0) > 0:
            parts.append(f"{pra:.0%} of cars ahead pitting")
        rej = field.get("reentry", {})
        plain_re = _plain_reentry(rej.get("v"))
        if plain_re:
            parts.append(plain_re)
        ll = field.get("caution_impact", {}).get("lost_lead")
        if ll is not None and ll > 0:
            n = int(ll)
            parts.append(f"~{n} spot{'s' if n != 1 else ''} at risk this stop")
        bankrupt = field.get("ahead_bankrupt_count", 0) or 0
        if bankrupt:
            parts.append(f"{bankrupt} ahead likely out of tires")
        tradeoff = field.get("position_tradeoff") or {}
        net = tradeoff.get("net_gain")
        if isinstance(net, int) and net > 0:
            parts.append(f"Net ~+{net} positions if you pit now")
    if tire_snapshot:
        if tire_snapshot.get("tire_inventory_exhausted"):
            parts.append("No tire sets left \u2014 run these to the end")
        elif cfg.get("show_tire_inventory", True):
            inv_txt = _fmt_tire_inventory(tire_snapshot)
            if inv_txt:
                parts.append(inv_txt)
    return " \u00b7 ".join(parts) if parts else None


def _fuel_critical(snapshot: dict, cfg: dict) -> bool:
    lt = float(cfg.get("low_fuel_laps_threshold", 2.0) or 2.0)
    margin = snapshot.get("laps_margin")
    return margin is not None and margin < lt


def _tire_critical(tire_snapshot: dict | None) -> bool:
    return bool((tire_snapshot or {}).get("tire_critical"))


def advise_pit_strategy(
    snapshot: dict,
    ctx: dict,
    cfg: dict,
    *,
    field_intel: dict | None = None,
    caution_hist: dict | None = None,
    strategy: dict | None = None,
    tire_snapshot: dict | None = None,
    caution_outlook: dict | None = None,
    pit_menu: dict | None = None,
    session_phase: dict | None = None,
) -> PitAdvice:
    """Return pit recommendation from fuel, tires, context, field intel, and caution."""
    field_intel = field_intel or {}
    caution_hist = caution_hist or {}
    strategy = strategy or {}
    tire_snapshot = tire_snapshot or {}
    caution_outlook = caution_outlook or {}
    pit_menu = pit_menu or {}
    session_phase = session_phase or {}

    pit_loss = float(cfg.get("pit_loss_seconds", 28.0) or 28.0)
    undercut_max = float(cfg.get("undercut_gap_max_s", 12.0) or 12.0)
    cover_max = float(cfg.get("cover_gap_max_s", 8.0) or 8.0)
    top_stay = int(cfg.get("top_positions_stay_out", 5) or 5)
    pra_thresh = float(cfg.get("caution_pit_pra_threshold", psc.CAUTION_PIT_PRA_THRESHOLD)
                       or psc.CAUTION_PIT_PRA_THRESHOLD)
    follow_thresh = float(cfg.get("field_pit_follow_threshold", psc.FIELD_PIT_FOLLOW_THRESHOLD)
                          or psc.FIELD_PIT_FOLLOW_THRESHOLD)
    lead_loss_max = int(cfg.get("caution_pit_lead_loss_max", psc.CAUTION_PIT_LEAD_LOSS_MAX)
                        or psc.CAUTION_PIT_LEAD_LOSS_MAX)
    lapped_fuel_min = float(cfg.get("lapped_danger_fuel_min_laps", psc.LAPPED_DANGER_FUEL_MIN_LAPS)
                            or psc.LAPPED_DANGER_FUEL_MIN_LAPS)
    caution_wait_min = float(cfg.get("caution_wait_min_fuel_laps", psc.CAUTION_WAIT_MIN_FUEL_LAPS)
                             or psc.CAUTION_WAIT_MIN_FUEL_LAPS)
    fresh_delta = int(cfg.get("fresh_tire_lap_delta", psc.FRESH_TIRE_LAP_DELTA) or 0)
    cover_closing_min = float(cfg.get("cover_closing_min_rate", psc.COVER_CLOSING_MIN_RATE)
                              or psc.COVER_CLOSING_MIN_RATE)
    prb_stay_thresh = float(cfg.get("caution_prb_stay_out_threshold",
                                    psc.CAUTION_PRB_STAY_OUT_THRESHOLD)
                            or psc.CAUTION_PRB_STAY_OUT_THRESHOLD)
    prb_pit_thresh = float(cfg.get("caution_prb_pit_threshold", psc.CAUTION_PRB_PIT_THRESHOLD)
                           or psc.CAUTION_PRB_PIT_THRESHOLD)
    pit_loss = float(
        strategy.get("pit_loss_s")
        or tire_snapshot.get("pit_loss_s")
        or cfg.get("pit_loss_seconds", 28.0)
        or 28.0
    )
    window = snapshot.get("window")
    win_open = bool(strategy.get("inside_window", snapshot.get("window_open")))
    fuel_window = bool(strategy.get("fuel_window", snapshot.get("window_open")))
    tire_window = bool(strategy.get("tire_window", tire_snapshot.get("window_open")))
    add = snapshot.get("add")
    pos = ctx.get("position")
    gap_ahead = ctx.get("gap_ahead")
    gap_behind = ctx.get("gap_behind")
    car_ahead = ctx.get("car_ahead")
    car_behind = ctx.get("car_behind")
    win_txt = _fmt_window(window)
    pra = field_intel.get("pra", 0.0) or 0.0
    reentry_v = (field_intel.get("reentry") or {}).get("v", "CLEAN")
    lost_lead = (field_intel.get("caution_impact") or {}).get("lost_lead", 0) or 0
    green_run_ok = _green_run_long_enough(caution_hist, cfg)
    intel_sec = _format_intel_secondary(
        caution_hist, field_intel, cfg,
        caution_outlook=caution_outlook,
        tire_snapshot=tire_snapshot,
        on_green=not ctx.get("caution"),
        green_run_ok=green_run_ok,
    )
    field_note = _FIELD_FUEL_NOTE
    fuel_crit = strategy.get("fuel_critical", _fuel_critical(snapshot, cfg))
    tire_crit = strategy.get("tire_critical", _tire_critical(tire_snapshot))
    must_stop = strategy.get("must_stop", fuel_crit or tire_crit)
    outlook = caution_outlook.get("likelihood", "MEDIUM")
    margin = snapshot.get("laps_margin")
    wear_min = tire_snapshot.get("wear_min_pct")
    prb = field_intel.get("prb", 0.0) or 0.0
    ahead_closing = field_intel.get("nearest_ahead_closing")
    behind_closing = field_intel.get("nearest_behind_closing")
    critical_only = bool(strategy.get("critical_only") or session_phase.get("critical_only"))

    def _sec(extra: str | None = None) -> str | None:
        bits = [b for b in (intel_sec, extra, ctx.get("flag_context")) if b]
        return " \u00b7 ".join(bits) if bits else None

    def _finish(advice: PitAdvice) -> PitAdvice:
        if ctx.get("caution"):
            return advice
        return _downgrade_pit_advice(advice, field_intel, cfg)

    def _emit(
        advice: PitAdvice,
        *,
        need_fuel: bool | None = None,
        need_tires: bool | None = None,
    ) -> PitAdvice:
        nf = strategy.get("stop_needs_fuel") if need_fuel is None else need_fuel
        nt = strategy.get("stop_needs_tires") if need_tires is None else need_tires
        advice = _gate_pit_menu_advice(
            advice, pit_menu, cfg,
            need_fuel=bool(nf),
            need_tires=bool(nt),
            on_pit_road=bool(strategy.get("on_pit_road")),
        )
        return _finish(advice)

    if strategy.get("on_pit_road"):
        return PitAdvice(
            PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
            "Already on pit road",
            secondary=_sec(),
            actionable=False,
        )

    if strategy.get("post_pit_lockout") or strategy.get("post_pit_quiet"):
        return PitAdvice(
            PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
            "You just pitted \u2014 no need to stop again yet",
            secondary=_sec(),
            actionable=False,
        )

    if ctx.get("caution"):
        if ctx.get("pits_closed"):
            pos_txt = f"P{pos}" if pos else "your spot"
            return PitAdvice(
                PitRec.HOLD, _REC_LABELS[PitRec.HOLD],
                f"Hold {pos_txt} \u2014 pits are closed",
                secondary=_sec(),
                actionable=True,
            )
        if fuel_crit and tire_crit:
            add_l = add if isinstance(add, (int, float)) else None
            fuel_txt = f" (add {add_l:.0f} L)" if add_l and add_l > 0 else ""
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit this yellow \u2014 need fuel and tires{fuel_txt}",
                secondary=_sec(),
                actionable=True,
            ), need_fuel=True, need_tires=True)
        if fuel_crit:
            add_l = add if isinstance(add, (int, float)) else None
            fuel_txt = f" (add {add_l:.0f} L)" if add_l and add_l > 0 else ""
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit this yellow \u2014 need fuel to finish{fuel_txt}",
                secondary=_sec(),
                actionable=True,
            ), need_fuel=True, need_tires=False)
        if tire_crit:
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                "Pit this yellow \u2014 tires won't last",
                secondary=_sec(),
                actionable=True,
            ), need_fuel=False, need_tires=True)
        must_pit = must_stop or (win_open and add and add > 0)
        comfortable = isinstance(margin, (int, float)) and margin >= caution_wait_min
        if (must_pit and not must_stop and comfortable
                and prb >= prb_stay_thresh and pra < pra_thresh):
            return PitAdvice(
                PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
                "Stay out \u2014 cars behind are pitting, you'll gain spots",
                secondary=_sec(win_txt or None),
                actionable=True,
            )
        if must_pit and pos is not None and pos <= top_stay and pra < pra_thresh:
            return PitAdvice(
                PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
                f"Stay out \u2014 you're P{pos} and leaders aren't pitting",
                secondary=_sec(win_txt or None),
                actionable=True,
            )
        if must_pit and (pra >= pra_thresh or (0 < lost_lead <= lead_loss_max)):
            n = field_intel.get("ahead_on_pit", 0)
            m = field_intel.get("ahead_total", 0)
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit this yellow \u2014 {n} of {m} ahead already pitting",
                secondary=_sec(field_note),
                actionable=True,
            ), need_fuel=bool(win_open and add), need_tires=bool(tire_window))
        if must_pit and pra >= follow_thresh:
            add_l = float(add) if add else 0.0
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit with the field \u2014 add {add_l:.0f} L",
                secondary=_sec(),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        bankrupt_ahead = int(field_intel.get("ahead_bankrupt_count", 0) or 0)
        bankrupt_min = int(cfg.get("caution_bankrupt_ahead_min",
                                   psc.CAUTION_BANKRUPT_AHEAD_MIN)
                           or psc.CAUTION_BANKRUPT_AHEAD_MIN)
        if (must_pit and bankrupt_ahead >= bankrupt_min
                and pra >= follow_thresh * 0.8):
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit this yellow \u2014 {bankrupt_ahead} ahead out of tires",
                secondary=_sec(field_note),
                actionable=True,
            ), need_fuel=bool(win_open and add), need_tires=bool(tire_window))
        if must_pit and prb <= prb_pit_thresh and comfortable:
            add_l = float(add) if add else 0.0
            fuel_txt = f"add {add_l:.0f} L" if add_l > 0 else "pit now"
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit this yellow \u2014 field behind staying out, {fuel_txt}",
                secondary=_sec(),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        if must_pit:
            return PitAdvice(
                PitRec.MARGINAL, _REC_LABELS[PitRec.MARGINAL],
                "Could pit for fuel \u2014 most ahead staying out",
                secondary=_sec(field_note),
                actionable=True,
            )
        return PitAdvice(
            PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
            "Fuel and tires are fine \u2014 skip this caution",
            secondary=_sec(),
            actionable=False,
        )

    # Green flag
    if fuel_crit and tire_crit:
        return _emit(PitAdvice(
            PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
            "Pit now \u2014 not enough fuel or tire life to finish",
            secondary=_sec(),
            actionable=True,
        ), need_fuel=True, need_tires=True)
    if fuel_crit:
        return _emit(PitAdvice(
            PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
            "Pit now \u2014 not enough fuel to finish",
            secondary=_sec(),
            actionable=True,
        ), need_fuel=True, need_tires=False)
    if tire_crit:
        return _emit(PitAdvice(
            PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
            "Pit now \u2014 tires won't last to the end",
            secondary=_sec(),
            actionable=True,
        ), need_fuel=False, need_tires=True)

    if critical_only and win_open and not must_stop:
        laps_txt = ""
        rem = session_phase.get("laps_remaining")
        if isinstance(rem, (int, float)) and rem > 0:
            laps_txt = f" ({int(rem)} laps left)"
        return PitAdvice(
            PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
            f"Finish on this stint \u2014 too late for an optional stop{laps_txt}",
            secondary=_sec(),
            actionable=False,
        )

    fuel_laps = strategy.get("fuel_laps_left") or snapshot.get("laps_empty")
    lapped_block = (
        win_open
        and reentry_v == "LAPPED_DANGER"
        and isinstance(fuel_laps, (int, float))
        and fuel_laps > lapped_fuel_min
        and not tire_crit
    )
    if lapped_block:
        return PitAdvice(
            PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
            "Stay out \u2014 pitting now could lose a lap",
            secondary=_sec(),
            actionable=True,
        )

    if win_open and reentry_v == "PACK":
        tradeoff = field_intel.get("position_tradeoff") or {}
        net_gain = int(tradeoff.get("net_gain", 0) or 0)
        min_net = int(cfg.get("strategic_pit_min_net_positions",
                               psc.STRATEGIC_PIT_MIN_NET_POSITIONS)
                      or psc.STRATEGIC_PIT_MIN_NET_POSITIONS)
        if net_gain < min_net + 2:
            return PitAdvice(
                PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
                "Wait a lap \u2014 pit exit will be into traffic",
                secondary=_sec(win_txt or None),
                actionable=True,
            )

    comfortable_margin = isinstance(margin, (int, float)) and margin >= caution_wait_min
    if (win_open and outlook == "HIGH"
            and comfortable_margin and not must_stop
            and reentry_v == "CLEAN" and not tire_window
            and not ctx.get("caution") and green_run_ok):
        return PitAdvice(
            PitRec.MARGINAL, _REC_LABELS[PitRec.MARGINAL],
            "Stay out a lap \u2014 caution may save a stop",
            secondary=_sec(win_txt or None),
            actionable=True,
        )

    if win_open:
        tradeoff = field_intel.get("position_tradeoff") or {}
        net_gain = int(tradeoff.get("net_gain", 0) or 0)
        gain_ahead = int(tradeoff.get("positions_gain_ahead", 0) or 0)
        min_net = int(cfg.get("strategic_pit_min_net_positions",
                               psc.STRATEGIC_PIT_MIN_NET_POSITIONS)
                      or psc.STRATEGIC_PIT_MIN_NET_POSITIONS)
        sets_rem = tire_snapshot.get("sets_remaining")
        player_has_sets = (
            not tire_snapshot.get("tire_inventory_exhausted")
            and (sets_rem is None or int(sets_rem) > 0)
        )
        reentry_ok = reentry_v not in ("PACK", "LAPPED_DANGER")
        pack_absorb = net_gain >= min_net + 2
        strategic_ok = (
            cfg.get("opponent_tire_inference_enabled", True)
            and tradeoff.get("strategic_pit_viable")
            and player_has_sets
            and (tire_window or win_open)
            and not critical_only
            and (reentry_ok or pack_absorb)
        )
        if strategic_ok:
            recover_pos = None
            if pos is not None and net_gain > 0:
                recover_pos = max(1, int(pos) - net_gain)
            recover_txt = (
                f"P{recover_pos}" if recover_pos is not None else "spots")
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit now \u2014 ~{gain_ahead} ahead out of tires; "
                f"expect to recover {recover_txt} on fresh rubber",
                secondary=_sec(win_txt or None),
                actionable=True,
            ), need_fuel=bool(fuel_window), need_tires=True)

        player_stint = field_intel.get("player_stint_laps", 0) or 0
        fresh_ahead = field_intel.get("ahead_fresh_tires_count", 0) or 0
        faster_ahead = field_intel.get("ahead_faster_count", 0) or 0
        nearest_stint = field_intel.get("nearest_ahead_stint_laps")
        if faster_ahead > 0 and fresh_ahead > 0:
            return PitAdvice(
                PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
                "Stay out \u2014 faster cars ahead on fresh tires",
                secondary=_sec(win_txt or None),
                actionable=True,
            )
        undercut_fresh = (
            gap_ahead is not None and gap_ahead <= undercut_max
            and reentry_v == "CLEAN"
            and (fresh_ahead > 0 or (
                nearest_stint is not None and nearest_stint < player_stint - fresh_delta))
        )
        if undercut_fresh:
            num = f"#{car_ahead}" if car_ahead else "the car ahead"
            return _emit(PitAdvice(
                PitRec.PIT_NEXT_LAP, _REC_LABELS[PitRec.PIT_NEXT_LAP],
                f"Pit next lap \u2014 {num} just stopped, undercut chance",
                secondary=_sec(win_txt or None),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        undercut_gap_ok = (
            gap_ahead is not None and gap_ahead <= undercut_max and reentry_v == "CLEAN"
        )
        ahead_not_pulling = ahead_closing is None or ahead_closing < cover_closing_min
        if undercut_gap_ok and ahead_not_pulling:
            num = f"#{car_ahead}" if car_ahead else "the car ahead"
            return _emit(PitAdvice(
                PitRec.PIT_NEXT_LAP, _REC_LABELS[PitRec.PIT_NEXT_LAP],
                f"Pit next lap to pass {num} \u2014 {gap_ahead:.1f}s ahead, "
                f"stop costs ~{pit_loss:.0f}s",
                secondary=_sec(win_txt or None),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        cover_gap_ok = gap_behind is not None and gap_behind <= cover_max
        cover_closing = (
            behind_closing is not None and behind_closing >= cover_closing_min
            and gap_behind is not None and gap_behind <= cover_max * 1.5
        )
        if cover_gap_ok or cover_closing:
            num = f"#{car_behind}" if car_behind else "the car behind"
            closing_txt = " and closing fast" if cover_closing and not cover_gap_ok else ""
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit now \u2014 {num} is {gap_behind:.1f}s behind{closing_txt}",
                secondary=_sec(win_txt or None),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        ahead_on = field_intel.get("ahead_on_pit", 0) or 0
        stale_ahead = field_intel.get("ahead_stale_stint_count", 0) or 0
        if ahead_on >= 2 or pra >= follow_thresh or stale_ahead >= 2:
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                "Pit now \u2014 most of the field is stopping",
                secondary=_sec(field_note),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        if tire_window and not fuel_window:
            wear_txt = f" ({wear_min:.0f}% wear)" if isinstance(wear_min, (int, float)) else ""
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit now \u2014 tires fading, fuel is fine{wear_txt}",
                secondary=_sec(win_txt or None),
                actionable=True,
            ), need_fuel=False, need_tires=True)
        if fuel_window and tire_window:
            win_part = f" ({win_txt})" if win_txt else ""
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit now \u2014 fuel and tires both due{win_part}",
                secondary=_sec(),
                actionable=True,
            ), need_fuel=True, need_tires=True)
        if outlook == "CALMING" and fuel_window and add and add > 0:
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                "Pit now on green \u2014 cautions are tapering off",
                secondary=_sec(win_txt or None),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        if field_intel.get("leader_pitted_recent") or (
                field_intel.get("ahead_pitted_recent", 0)
                > (field_intel.get("ahead_total", 0) // 2)):
            return PitAdvice(
                PitRec.MARGINAL, _REC_LABELS[PitRec.MARGINAL],
                "Leaders already pitted \u2014 ok to stop for fuel",
                secondary=_sec(win_txt or None),
                actionable=True,
            )
        if reentry_v == "CLEAN" and fuel_window and add and add > 0:
            win_part = f" ({win_txt})" if win_txt else ""
            return _emit(PitAdvice(
                PitRec.PIT_NOW, _REC_LABELS[PitRec.PIT_NOW],
                f"Pit now \u2014 clear track when you merge back{win_part}",
                secondary=_sec(),
                actionable=True,
            ), need_fuel=True, need_tires=bool(tire_window))
        if fuel_window or tire_window:
            return PitAdvice(
                PitRec.MARGINAL, _REC_LABELS[PitRec.MARGINAL],
                f"Could pit now{(' \u2014 ' + win_txt) if win_txt else ''}",
                secondary=_sec(),
                actionable=True,
            )

    stay_parts: list[str] = []
    if isinstance(margin, (int, float)):
        stay_parts.append(f"Fuel +{margin:.0f} lap margin")
    if isinstance(wear_min, (int, float)):
        stay_parts.append(f"tires {wear_min:.0f}%")
    inv_txt = _fmt_tire_inventory(tire_snapshot)
    if inv_txt:
        stay_parts.append(inv_txt.lower())
    rationale = " \u00b7 ".join(stay_parts) if stay_parts else "Fuel and tires look good"
    return PitAdvice(
        PitRec.STAY_OUT, _REC_LABELS[PitRec.STAY_OUT],
        rationale,
        secondary=_sec(),
        actionable=False,
    )
