"""Pure helpers for assembling standings table rows."""

from __future__ import annotations

from typing import Callable, TypeVar

T = TypeVar("T")


def empty_row(tag: str) -> dict:
    """Blank placeholder row (keeps the player centered when padding)."""
    return {"key": f"_empty_{tag}", "empty": True}


def _context_slots(window: int, player_on_podium: bool) -> int:
    """Rows below the podium block (at least one when the player isn't on it)."""
    slots = max(0, window - 3)
    if not player_on_podium:
        slots = max(1, slots)
    return slots


def _pick_context_indices(
    ranked: list[int],
    *,
    pidx: int,
    player: int,
    podium_idxs: set[int],
    limit: int,
    rows_ahead: int,
    rows_behind: int,
) -> list[int]:
    """Choose up to ``limit`` cars for the context block, always including the
    player when they aren't already on the podium. Neighbor rows are trimmed
    first if the window is too small."""
    if limit <= 0:
        return []

    total = len(ranked)
    on_podium = player in podium_idxs

    if on_podium:
        chosen: list[int] = []
        off = 1
        while len(chosen) < limit:
            added = False
            for delta in (-off, off):
                slot = pidx + delta
                if 0 <= slot < total:
                    idx = ranked[slot]
                    if idx not in podium_idxs and idx not in chosen:
                        chosen.append(idx)
                        added = True
                        if len(chosen) >= limit:
                            break
            if not added:
                break
            off += 1
        chosen.sort(key=lambda i: ranked.index(i))
        return chosen

    # Player is mandatory; shrink ahead/behind to fit ``limit``.
    need = limit - 1
    above = min(rows_ahead, need)
    below = min(rows_behind, need - above)
    above = min(above, need - below)

    chosen = [player]
    for slot in range(pidx - above, pidx + below + 1):
        if slot == pidx:
            continue
        if 0 <= slot < total:
            idx = ranked[slot]
            if idx not in podium_idxs and idx not in chosen:
                chosen.append(idx)

    off = max(above, below) + 1
    while len(chosen) < limit:
        added = False
        for delta in (-off, off):
            slot = pidx + delta
            if 0 <= slot < total:
                idx = ranked[slot]
                if idx not in podium_idxs and idx not in chosen:
                    chosen.append(idx)
                    added = True
                    if len(chosen) >= limit:
                        break
        if not added:
            break
        off += 1

    chosen.sort(key=lambda i: ranked.index(i))
    return chosen


def standings_row_list(
    ranked: list[int],
    *,
    player: int,
    center_on_player: bool,
    pin_podium: bool,
    rows: int,
    rows_ahead: int,
    rows_behind: int,
    build: Callable[[int], T],
    empty: Callable[[str], T],
) -> list[T]:
    """Build the ordered standings rows for the table widget.

    When ``center_on_player`` is on, show a sliding window centered on the
    player (optionally with P1–P3 pinned in the first three rows). Otherwise
    show the top ``rows`` cars.
    """
    total = len(ranked)
    if total == 0:
        return []

    center = center_on_player and player in ranked
    if not center:
        return [build(idx) for idx in ranked[:rows]]

    above = rows_ahead
    below = rows_behind
    window = above + 1 + below
    pidx = ranked.index(player)

    if pin_podium:
        podium: list[T] = []
        for slot in range(3):
            if slot < total:
                podium.append(build(ranked[slot]))
            else:
                podium.append(empty(f"podium{slot}"))

        podium_idxs = set(ranked[: min(3, total)])
        slots = _context_slots(window, player in podium_idxs)
        picked = _pick_context_indices(
            ranked,
            pidx=pidx,
            player=player,
            podium_idxs=podium_idxs,
            limit=slots,
            rows_ahead=above,
            rows_behind=below,
        )
        context = [build(idx) for idx in picked]
        for i in range(len(context), slots):
            context.append(empty(f"ctx{i}"))
        return podium + context

    out: list[T] = []
    start = pidx - above
    for i, slot in enumerate(range(start, start + window)):
        if 0 <= slot < total:
            out.append(build(ranked[slot]))
        else:
            out.append(empty(f"win{i}"))
    return out
