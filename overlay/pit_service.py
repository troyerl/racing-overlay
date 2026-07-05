"""Pit service flag decoding for the pit board widget."""

from __future__ import annotations

# iRacing PitSvFlags bits (irsdk).
LF_TIRE = 0x0001
RF_TIRE = 0x0002
LR_TIRE = 0x0004
RR_TIRE = 0x0008
FUEL_FILL = 0x0010
TEAROFF = 0x0020
FAST_REPAIR = 0x0040

_SERVICES = (
    ("lf_tire", LF_TIRE, "LF tire"),
    ("rf_tire", RF_TIRE, "RF tire"),
    ("lr_tire", LR_TIRE, "LR tire"),
    ("rr_tire", RR_TIRE, "RR tire"),
    ("fuel", FUEL_FILL, "Fuel"),
    ("tearoff", TEAROFF, "Tearoff"),
    ("fast_repair", FAST_REPAIR, "Fast repair"),
)


def decode_flags(raw) -> list[dict]:
    """Return checked pit services as [{key, label, checked}, ...]."""
    try:
        flags = int(raw or 0)
    except (TypeError, ValueError):
        flags = 0
    out = []
    for key, bit, label in _SERVICES:
        out.append({"key": key, "label": label, "checked": bool(flags & bit)})
    return out


def any_requested(raw) -> bool:
    try:
        return int(raw or 0) != 0
    except (TypeError, ValueError):
        return False
