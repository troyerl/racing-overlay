"""Shared time/delta formatters for overlay widgets."""

from __future__ import annotations


def clock(sec) -> str:
    if not isinstance(sec, (int, float)) or sec <= 0:
        return "--:--.---"
    m = int(sec // 60)
    return f"{m}:{sec - m * 60:06.3f}"


def sec(sec) -> str:
    if not isinstance(sec, (int, float)) or sec <= 0:
        return "--.-"
    return f"{sec:.1f}"


def signed_delta(sec, places: int = 3) -> str:
    if not isinstance(sec, (int, float)):
        return "--"
    if places == 2:
        return f"{sec:+.2f}"
    return f"{'-' if sec < 0 else '+'}{abs(sec):.{places}f}"
