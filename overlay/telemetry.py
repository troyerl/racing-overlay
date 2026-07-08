"""Shared telemetry read helpers (gated callers in app.py)."""

from __future__ import annotations

from . import config

TIRE_CORNERS = (
    ("lf", "LFwearM", "LFtempCL", "LFcoldPressure"),
    ("rf", "RFwearM", "RFtempCL", "RFcoldPressure"),
    ("lr", "LRwearM", "LRtempCL", "LRcoldPressure"),
    ("rr", "RRwearM", "RRtempCL", "RRcoldPressure"),
)

PIT_PRESSURE_KEYS = ("PitSvLFP", "PitSvRFP", "PitSvLRP", "PitSvRRP")


def read_tire_corners(ir, *, wear=True, temp=True, pressure=False) -> dict:
    """Return {lf: {wear, temp, pressure}, ...} for corners with data."""
    corners: dict = {}
    for key, wk, tk, pk in TIRE_CORNERS:
        entry: dict = {}
        if wear:
            try:
                entry["wear"] = ir[wk]
            except (TypeError, ValueError, KeyError):
                pass
        if temp:
            try:
                entry["temp"] = ir[tk]
            except (TypeError, ValueError, KeyError):
                pass
        if pressure:
            try:
                entry["pressure"] = ir[pk]
            except (TypeError, ValueError, KeyError):
                pass
        if entry:
            corners[key] = entry
    return corners


def read_track_temp(ir):
    try:
        temp = ir["TrackTemp"]
    except (TypeError, ValueError, KeyError):
        temp = None
    if temp is None:
        try:
            temp = ir["TrackTempCrew"]
        except (TypeError, ValueError, KeyError):
            pass
    return temp


def _read_ir(ir, key):
    try:
        return ir[key]
    except (TypeError, ValueError, KeyError):
        return None


def _int_or_none(val) -> int | None:
    try:
        if val is None:
            return None
        return int(val)
    except (TypeError, ValueError):
        return None


def _bool_or_none(val) -> bool | None:
    if val is None:
        return None
    if isinstance(val, bool):
        return val
    try:
        return bool(int(val))
    except (TypeError, ValueError):
        return None


def resolve_tire_inventory(
    telemetry: dict,
    cfg: dict,
    *,
    pit_stops_count: int = 0,
) -> dict:
    """Resolve tire set counts: SDK first, then manual config fallback."""
    from . import pit_strategy_constants as psc

    available = telemetry.get("tire_sets_available")
    used = telemetry.get("tire_sets_used")
    limit = telemetry.get("dry_tire_set_limit")
    manual_total = int(cfg.get("race_tire_sets_total", 0) or 0)
    reserve = int(cfg.get("tire_sets_reserve", psc.TIRE_SETS_RESERVE) or 0)

    sets_used: int | None = None
    if isinstance(used, (int, float)) and used >= 0:
        sets_used = int(used)
    elif pit_stops_count > 0:
        sets_used = int(pit_stops_count)

    source = "unlimited"
    sets_limited = False
    sets_remaining: int | None = None
    sets_total: int | None = None

    if isinstance(available, (int, float)) and 0 <= int(available) < psc.TIRE_SETS_UNLIMITED:
        sets_remaining = int(available)
        sets_limited = True
        source = "sdk"
        if sets_used is not None:
            sets_total = sets_remaining + sets_used
    elif isinstance(limit, (int, float)) and 0 < int(limit) < psc.TIRE_SETS_UNLIMITED:
        sets_total = int(limit)
        sets_limited = True
        source = "limit"
        sets_used = sets_used or 0
        sets_remaining = max(0, sets_total - sets_used)
    elif manual_total > 0:
        sets_total = manual_total
        sets_limited = True
        source = "manual"
        sets_used = sets_used or 0
        sets_remaining = max(0, sets_total - sets_used)

    current_set = (sets_used + 1) if sets_used is not None else None
    tire_inventory_low = (
        sets_limited and sets_remaining is not None and sets_remaining <= reserve)
    tire_inventory_exhausted = (
        sets_limited and sets_remaining is not None and sets_remaining <= 0)

    return {
        "sets_limited": sets_limited,
        "sets_remaining": sets_remaining,
        "sets_total": sets_total,
        "sets_used": sets_used,
        "current_set": current_set,
        "inventory_source": source,
        "tire_inventory_low": tire_inventory_low,
        "tire_inventory_exhausted": tire_inventory_exhausted,
        "inventory_blocks_window": tire_inventory_exhausted,
    }


def read_pit_advisor_telemetry(ir, car_info: dict | None = None) -> dict:
    """Single gate for pit-advisor SDK reads (live race telemetry only)."""
    car_info = car_info or {}
    out: dict = {
        "player": _int_or_none(_read_ir(ir, "PlayerCarIdx")),
        "lap": _int_or_none(_read_ir(ir, "Lap")),
        "session_time": _float_or_none(_read_ir(ir, "SessionTime")),
        "fuel_level": _float_or_none(_read_ir(ir, "FuelLevel")),
        "fuel_level_pct": _float_or_none(_read_ir(ir, "FuelLevelPct")),
        "fuel_use_per_hour": _float_or_none(_read_ir(ir, "FuelUsePerHour")),
        "fuel_max": _float_or_none(car_info.get("fuel_max")),
        "est_lap": _float_or_none(car_info.get("est_lap")),
        "tire_corners": read_tire_corners(ir, wear=True, temp=False),
        "tire_sets_available": _int_or_none(_read_ir(ir, "TireSetsAvailable")),
        "tire_sets_used": _int_or_none(_read_ir(ir, "TireSetsUsed")),
        "dry_tire_set_limit": _int_or_none(_read_ir(ir, "PlayerCarDryTireSetLimit")),
        "dc_tire_set": _int_or_none(_read_ir(ir, "dcTireSet")),
        "positions": _read_ir(ir, "CarIdxPosition"),
        "car_lap": _read_ir(ir, "CarIdxLap"),
        "on_pit_road": _read_ir(ir, "CarIdxOnPitRoad"),
        "surface": _read_ir(ir, "CarIdxTrackSurface"),
        "lap_pcts": _read_ir(ir, "CarIdxLapDistPct"),
        "est_time": _read_ir(ir, "CarIdxEstTime"),
        "f2_time": _read_ir(ir, "CarIdxF2Time"),
        "car_last": _read_ir(ir, "CarIdxLastLapTime"),
        "car_flags": _read_ir(ir, "CarIdxSessionFlags"),
        "session_flags": _int_or_none(_read_ir(ir, "SessionFlags")) or 0,
        "pits_open": _bool_or_none(_read_ir(ir, "PitsOpen")),
        "session_laps_remain_ex": _float_or_none(_read_ir(ir, "SessionLapsRemainEx")),
        "session_laps_remain": _float_or_none(_read_ir(ir, "SessionLapsRemain")),
        "session_time_remain": _float_or_none(_read_ir(ir, "SessionTimeRemain")),
        "session_laps_total": _int_or_none(_read_ir(ir, "SessionLapsTotal")),
        "pit_sv_flags": _int_or_none(_read_ir(ir, "PitSvFlags")),
        "pit_sv_fuel": _float_or_none(_read_ir(ir, "PitSvFuel")),
        "pit_sv_tire_compound": _int_or_none(_read_ir(ir, "PitSvTireCompound")),
        "track_wetness": _float_or_none(_read_ir(ir, "TrackWetness")),
    }
    return out


def read_pit_menu(telemetry: dict) -> dict:
    """Decode queued pit services from telemetry dict."""
    from . import pit_service as ps

    flags = telemetry.get("pit_sv_flags") or 0
    services = {s["key"]: s["checked"] for s in ps.decode_flags(flags)}
    tires = any(services.get(k) for k in ("lf_tire", "rf_tire", "lr_tire", "rr_tire"))
    return {
        "fuel_queued": bool(services.get("fuel")),
        "tires_queued": tires,
        "fast_repair_queued": bool(services.get("fast_repair")),
        "tearoff_queued": bool(services.get("tearoff")),
        "pit_sv_fuel": telemetry.get("pit_sv_fuel"),
    }


def _float_or_none(val):
    try:
        return float(val) if val is not None else None
    except (TypeError, ValueError):
        return None


def weather_snapshot(ir, weekend: dict, *, cfg: dict | None = None) -> dict:
    """Build weather panel payload fields from telemetry + WeekendInfo."""
    cfg = cfg or {}
    wk = weekend or {}
    out: dict = {}
    if cfg.get("show_skies", True):
        out["skies"] = wk.get("Skies")
        out["humidity"] = _float_or_none(wk.get("RelativeHumidity"))
        out["fog"] = _float_or_none(wk.get("FogLevel"))
    if cfg.get("show_rain", True):
        try:
            out["track_wetness"] = ir["TrackWetness"]
        except (TypeError, ValueError, KeyError):
            pass
        try:
            out["rain_intensity"] = ir["RainIntensity"]
        except (TypeError, ValueError, KeyError):
            pass
    if cfg.get("show_temps", True):
        out["track_temp"] = read_track_temp(ir)
        try:
            out["air_temp"] = ir["AirTemp"]
        except (TypeError, ValueError, KeyError):
            pass
    if cfg.get("show_wind", False):
        try:
            out["wind_dir"] = ir["WindDir"]
            out["wind_vel"] = ir["WindVel"]
        except (TypeError, ValueError, KeyError):
            pass
    return out


def channel_usable(quality, latency) -> bool:
    """True when iRacing online channel telemetry is meaningful (not offline zeros)."""
    if quality is not None and float(quality) > 0:
        return True
    if latency is not None and float(latency) > 0:
        return True
    return False


def perf_snapshot(ir, *, cfg: dict | None = None) -> dict:
    """Build system-panel FPS and online channel fields from telemetry."""
    cfg = cfg or {}
    out: dict = {}
    if ir is None:
        return out
    if cfg.get("show_fps", True):
        try:
            fps = ir["FrameRate"]
            if fps is not None:
                out["fps"] = max(0, int(round(float(fps))))
        except (TypeError, ValueError, KeyError):
            pass
    if cfg.get("show_network", True):
        try:
            q = _float_or_none(ir["ChanQuality"])
            if q is not None and q > 0:
                out["chan_quality"] = max(0.0, min(100.0, q))
        except (TypeError, ValueError, KeyError):
            pass
        try:
            lat = _float_or_none(ir["ChanLatency"])
            if lat is not None and lat > 0:
                out["chan_latency"] = max(0.0, lat)
        except (TypeError, ValueError, KeyError):
            pass
    return out


def fuel_payload_key(data: dict) -> tuple:
    """Hashable key for fuel-calc skip-update comparison."""
    strip = data.get("strip") or {}
    rows = data.get("rows") or {}
    return (
        data.get("level"), data.get("cap"), data.get("add"),
        data.get("window"), data.get("window_open"),
        data.get("alert"), data.get("live_burn"), data.get("fuel_pct"),
        data.get("legal_min"), data.get("stints"), data.get("pit_hint"),
        data.get("time_empty"), data.get("time_margin"),
        data.get("laps_empty"), data.get("laps_margin"),
        strip.get("total"), strip.get("window"), strip.get("now"),
        tuple((k, rows.get(k, {}).get("usage"),
               rows.get(k, {}).get("laps"),
               rows.get(k, {}).get("refuel"))
              for k in ("avg", "max", "min")),
    )


_DASH_EASE_KEYS = ("rpm", "throttle", "brake", "clutch")


def dash_discrete_key(data: dict) -> tuple:
    """Hashable dash snapshot excluding continuous easing inputs."""
    if not data:
        return ()
    skip = set(_DASH_EASE_KEYS)
    items = []
    for k, v in sorted(data.items()):
        if k in skip:
            continue
        if isinstance(v, float):
            items.append((k, round(v, 2)))
        else:
            items.append((k, v))
    return tuple(items)


def dash_easing_moved(prev: dict | None, data: dict, eps: float = 0.003) -> bool:
    """True when eased pedal/RPM targets moved enough to warrant a repaint kick."""
    if not prev:
        return True
    for k in _DASH_EASE_KEYS:
        a, b = prev.get(k), data.get(k)
        if isinstance(a, (int, float)) and isinstance(b, (int, float)):
            if abs(float(a) - float(b)) > eps:
                return True
        elif a != b:
            return True
    return False


def _time_centis(v) -> object:
    if isinstance(v, (int, float)):
        return round(float(v), 2)
    return v


def sector_timing_snap_key(snap: dict) -> tuple:
    """Hashable sector-timing snapshot (lap clocks rounded to centiseconds)."""
    sectors = []
    for s in snap.get("sectors") or []:
        sectors.append((
            _time_centis(s.get("time")),
            s.get("status"),
            s.get("active"),
            _time_centis(s.get("delta")) if s.get("delta") is not None else None,
        ))
    return (
        _time_centis(snap.get("cur_lap")),
        _time_centis(snap.get("last_lap")),
        _time_centis(snap.get("best_lap")),
        _time_centis(snap.get("predicted_lap")),
        snap.get("active_idx"),
        tuple(sectors),
    )
