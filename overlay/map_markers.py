"""Map traffic-marker selection and hold-before-switch debouncing."""

from __future__ import annotations

from . import common as oc

MARKER_SLOTS = ("ahead", "behind", "leader")


def wrap_lap_delta(them: float, me: float) -> float:
    """Signed lap-distance delta (them - me), wrapped to (-0.5, 0.5]."""
    delta = them - me
    if delta > 0.5:
        delta -= 1.0
    elif delta < -0.5:
        delta += 1.0
    return delta


def marker_car_valid(
    idx: int,
    *,
    surface,
    on_pit_arr,
    pace_idxs: set[int],
    pit_surfaces: tuple[int, ...],
) -> bool:
    if idx in pace_idxs:
        return False
    if on_pit_arr is not None and idx < len(on_pit_arr) and on_pit_arr[idx]:
        return False
    if surface is None or idx >= len(surface):
        return False
    if surface[idx] in pit_surfaces:
        return False
    return surface[idx] == oc.TRK_ON_TRACK


def select_marker_candidates(
    player: int | None,
    lap_pct,
    surface,
    positions,
    *,
    pace_idxs: set[int],
    on_pit_arr,
    pit_surfaces: tuple[int, ...] = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS),
    alongside_zone: float = 0.004,
) -> dict[str, int | None]:
    """Raw CarIdx targets for ahead / behind / leader (no hold debounce).

    Ahead/behind are race-position neighbors (N-1 / N+1), not lap-distance.
    If that exact car is invalid (pits / off-track / pace), the slot is empty
    — there is no fallthrough to the next position.
    ``alongside_zone`` is accepted for call-site compatibility and ignored.
    """
    del alongside_zone  # race-position neighbors; kept for API compatibility
    out: dict[str, int | None] = {"ahead": None, "behind": None, "leader": None}
    if player is None or not lap_pct or player >= len(lap_pct):
        return out
    me = lap_pct[player]
    if me is None or me < 0:
        return out
    if not positions or player >= len(positions):
        return out
    my_pos = positions[player]
    try:
        my_pos = int(my_pos) if my_pos is not None else 0
    except (TypeError, ValueError):
        my_pos = 0
    if my_pos < 1:
        return out

    def _idx_at_pos(target: int) -> int | None:
        if target < 1:
            return None
        for idx, pos in enumerate(positions):
            try:
                p = int(pos) if pos is not None else 0
            except (TypeError, ValueError):
                continue
            if p != target or idx == player:
                continue
            # Found the car in that race position — valid or hide (no fallthrough).
            if marker_car_valid(
                idx, surface=surface, on_pit_arr=on_pit_arr,
                pace_idxs=pace_idxs, pit_surfaces=pit_surfaces,
            ):
                return idx
            return None
        return None

    out["ahead"] = _idx_at_pos(my_pos - 1)
    out["behind"] = _idx_at_pos(my_pos + 1)

    for idx, pos in enumerate(positions):
        try:
            p = int(pos) if pos is not None else 0
        except (TypeError, ValueError):
            continue
        if p != 1:
            continue
        if marker_car_valid(
            idx, surface=surface, on_pit_arr=on_pit_arr,
            pace_idxs=pace_idxs, pit_surfaces=pit_surfaces,
        ):
            out["leader"] = idx
        break
    return out


def apply_marker_hold(
    state: dict,
    candidate_idx: int | None,
    now: float,
    hold_sec: float,
    *,
    locked_valid: bool,
) -> int | None:
    """Update one slot's hold state; return the locked CarIdx to display."""
    if not locked_valid:
        state["locked"] = None

    if candidate_idx is None:
        state["pending"] = None
        state["pending_since"] = None
        return None

    locked = state.get("locked")
    if locked == candidate_idx:
        state["pending"] = None
        state["pending_since"] = None
        return locked

    pending = state.get("pending")
    if pending != candidate_idx:
        state["pending"] = candidate_idx
        state["pending_since"] = now

    pending_since = state.get("pending_since")
    if pending_since is not None and (now - pending_since) >= hold_sec:
        state["locked"] = candidate_idx
        state["pending"] = None
        state["pending_since"] = None
        return candidate_idx

    return locked if locked_valid else None


def resolve_traffic_markers(
    hold_states: dict[str, dict],
    candidates: dict[str, int | None],
    lap_pct,
    *,
    now: float,
    hold_sec: float,
    surface,
    on_pit_arr,
    pace_idxs: set[int],
    pit_surfaces: tuple[int, ...] = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS),
) -> dict[str, dict | None]:
    """Apply hold debounce and return idx + lap-% for each marker slot."""
    out: dict[str, dict | None] = {"ahead": None, "behind": None, "leader": None}
    for slot in MARKER_SLOTS:
        state = hold_states.setdefault(
            slot, {"locked": None, "pending": None, "pending_since": None})
        candidate = candidates.get(slot)
        locked = state.get("locked")
        locked_valid = (
            locked is not None
            and marker_car_valid(
                locked, surface=surface, on_pit_arr=on_pit_arr,
                pace_idxs=pace_idxs, pit_surfaces=pit_surfaces,
            )
        )
        idx = apply_marker_hold(
            state, candidate, now, hold_sec, locked_valid=locked_valid)
        if idx is not None and lap_pct and idx < len(lap_pct):
            pct = lap_pct[idx]
            if pct is not None and pct >= 0:
                out[slot] = {"idx": idx, "pct": float(pct)}
    return out


def fresh_hold_states() -> dict[str, dict]:
    return {slot: {"locked": None, "pending": None, "pending_since": None}
            for slot in MARKER_SLOTS}
