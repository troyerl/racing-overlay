"""Hybrid / ERS telemetry helpers."""

from __future__ import annotations


def _read(ir, key):
    try:
        return ir[key]
    except (TypeError, ValueError, KeyError):
        return None


def probe(ir) -> bool:
    """True when hybrid energy telemetry appears present."""
    for key in ("EnergyERSBattery", "EnergyBatteryToMGU_KLap",
                "EnergyBudgetBattToMGU_KLap"):
        v = _read(ir, key)
        if isinstance(v, (int, float)) and v != 0:
            return True
    return False


def snapshot(ir, *, have_budget: float | None = None) -> dict:
    """Build hybrid payload from telemetry reads."""
    battery = _read(ir, "EnergyERSBattery")
    used = _read(ir, "EnergyBatteryToMGU_KLap")
    budget = _read(ir, "EnergyBudgetBattToMGU_KLap")
    boost = _read(ir, "ManualBoost")
    p2p = _read(ir, "PushToPass")

    have = probe(ir) or (
        isinstance(battery, (int, float)) and battery > 0
    )

    pct = None
    if isinstance(battery, (int, float)) and isinstance(budget, (int, float)):
        if budget > 0:
            pct = max(0.0, min(100.0, 100.0 * battery / budget))
    elif isinstance(battery, (int, float)) and have_budget and have_budget > 0:
        pct = max(0.0, min(100.0, 100.0 * battery / have_budget))

    return {
        "have_hybrid": have,
        "battery_j": battery if isinstance(battery, (int, float)) else None,
        "battery_pct": pct,
        "used_lap": used if isinstance(used, (int, float)) else None,
        "budget_lap": budget if isinstance(budget, (int, float)) else None,
        "boost_active": bool(boost),
        "p2p_active": bool(p2p),
    }


def fmt_kj(joules) -> str:
    if not isinstance(joules, (int, float)):
        return "\u2014"
    kj = joules / 1000.0
    if abs(kj) >= 1000:
        return f"{kj / 1000.0:.1f} MJ"
    return f"{kj:.0f} kJ"
