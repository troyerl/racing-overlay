"""Estimate projected iRating changes from live race positions.

iRacing does not expose projected iRating in telemetry. This implements the
community SOF formula (same as Turbo87/irating-rs and iRacing's Excel calc):
finish order and registered field size (including DNS) drive the estimate.
"""

from __future__ import annotations

import math
from collections import defaultdict

_LN2 = math.log(2.0)
_BR1 = 1600.0 / _LN2


def _chance(a: float, b: float) -> float:
    ea = math.exp(-a / _BR1)
    eb = math.exp(-b / _BR1)
    return (1.0 - ea) * eb / ((1.0 - eb) * ea + (1.0 - ea) * eb)


def calculate_deltas(entries: list[tuple[int, bool]]) -> list[int]:
    """Return rounded iRating deltas for finish-ordered (start_ir, started) pairs."""
    n = len(entries)
    if n < 2:
        return [0] * n

    ratings = [float(ir) for ir, _ in entries]
    chances = [[_chance(a, b) for b in ratings] for a in ratings]
    expected = [sum(row) - 0.5 for row in chances]

    num_reg = n
    num_starters = sum(1 for _, started in entries if started)
    num_non = num_reg - num_starters
    if num_starters < 1:
        return [0] * n

    fudge = []
    for rank, (_, started) in enumerate(entries, start=1):
        if not started:
            fudge.append(0.0)
        else:
            x = num_reg - num_non / 2.0
            fudge.append((x / 2.0 - rank) / 100.0)

    changes_starters: list[float | None] = []
    for rank, ((_, started), exp, fud) in enumerate(
            zip(entries, expected, fudge), start=1):
        if not started:
            changes_starters.append(None)
        else:
            changes_starters.append(
                (num_reg - rank - exp - fud) * 200.0 / num_starters)

    sum_starters = sum(c for c in changes_starters if c is not None)

    exp_non = [exp if not started else None
               for (_, started), exp in zip(entries, expected)]
    sum_exp_non = sum(e for e in exp_non if e is not None)

    changes_non: list[float | None] = []
    for exp in exp_non:
        if exp is None:
            changes_non.append(None)
        elif num_non <= 0 or sum_exp_non <= 0:
            changes_non.append(0.0)
        else:
            changes_non.append(
                -sum_starters / num_non * exp / (sum_exp_non / num_non))

    out: list[int] = []
    for cs, cn in zip(changes_starters, changes_non):
        if cs is not None:
            out.append(int(round(cs)))
        elif cn is not None:
            out.append(int(round(cn)))
        else:
            out.append(0)
    return out


def _rank(idx: int, class_positions, positions) -> int:
    if class_positions and idx < len(class_positions):
        cp = class_positions[idx]
        if isinstance(cp, int) and cp > 0:
            return cp
    if positions and idx < len(positions):
        p = positions[idx]
        if isinstance(p, int) and p > 0:
            return p
    return 0


def _driver_started(
    idx: int,
    class_positions,
    positions,
    started_by_idx: dict[int, bool] | None,
) -> bool:
    if started_by_idx is not None and idx in started_by_idx:
        return started_by_idx[idx]
    return _rank(idx, class_positions, positions) > 0


def project_deltas_by_class(
    drivers: dict[int, dict],
    class_positions,
    positions,
    pace_idxs: set[int],
    *,
    started_by_idx: dict[int, bool] | None = None,
    results_class_positions=None,
    results_positions=None,
) -> dict[int, int]:
    """CarIdx -> projected iRating change, computed per CarClassID."""
    if not drivers:
        return {}

    rank_class = results_class_positions if results_class_positions else class_positions
    rank_overall = results_positions if results_positions else positions

    by_class: dict[int, list[int]] = defaultdict(list)
    for idx in drivers:
        if idx in pace_idxs:
            continue
        d = drivers[idx]
        if d.get("IsSpectator"):
            continue
        ir = d.get("IRating")
        if not isinstance(ir, (int, float)) or ir <= 0:
            continue
        cid = d.get("CarClassID")
        if cid is None:
            cid = 0
        by_class[int(cid)].append(idx)

    result: dict[int, int] = {}
    for idxs in by_class.values():
        starters = []
        non_starters = []
        for idx in idxs:
            ir = int(drivers[idx]["IRating"])
            if _driver_started(idx, class_positions, positions, started_by_idx):
                rank = _rank(idx, rank_class, rank_overall)
                if rank <= 0:
                    rank = 10_000
                starters.append((idx, rank, ir))
            else:
                non_starters.append((idx, ir))
        starters.sort(key=lambda t: t[1])
        ordered = [(ir, True) for _, _, ir in starters]
        ordered += [(ir, False) for _, ir in non_starters]
        if len(ordered) < 2:
            continue
        deltas = calculate_deltas(ordered)
        order_idxs = [idx for idx, _, _ in starters] + [idx for idx, _ in non_starters]
        for idx, delta in zip(order_idxs, deltas):
            result[idx] = delta
    return result
