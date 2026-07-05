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
