"""Shared track / pit authoring helpers for Python HUD and Rust hybrid."""

from __future__ import annotations

import json
import os
from datetime import datetime, timezone
from typing import Any

from . import config
from . import track_store


def orientation_from_cfg() -> tuple[int, bool]:
    """Map rotation/mirror stamped into saved track JSON."""
    mcfg = config.CFG.get("map", {})
    rot = int(round((mcfg.get("rotation", 0) or 0) / 90.0)) * 90 % 360
    return rot, bool(mcfg.get("mirror", False))


def cloud_blocks_track_save(canonical) -> str | None:
    """Error message when cloud already has this track; None if save may proceed."""
    if not track_store.can_write():
        return None
    exists = track_store.cloud_track_exists(canonical)
    if exists is True:
        return (f"TrackID {canonical} is already in the shared library "
                "— save skipped.")
    return None


def write_track_json(tracks_dir: str, tid, doc: dict) -> str:
    path = os.path.join(tracks_dir, f"{tid}.json")
    os.makedirs(tracks_dir, exist_ok=True)
    stamped = dict(doc)
    stamped["updated_at"] = datetime.now(timezone.utc).isoformat()
    tmp = f"{path}.tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(stamped, fh, indent=2)
        fh.write("\n")
    os.replace(tmp, path)
    track_store.invalidate_alias_cache()
    return path


def build_manual_pit_lane_fields(loop, entry, road, merge) -> dict:
    """Schematic pit pipeline for one lane -> pit_path / blends / pcts."""
    if len(road) < 2 or len(merge) < 2:
        return {}
    from tools.schematic_to_track import (
        _connect_blend_to_loop,
        _pct_on_loop,
        _pit_span_on_loop,
        _resample_open,
    )

    pit_path = _resample_open(road, 140)
    pit_out_raw = _resample_open(merge, 41)
    pit_out = _connect_blend_to_loop(
        pit_out_raw, loop, attach_end=True, n_loop=20, pit_path=pit_path)
    pit_out = _resample_open(pit_out, 41)

    lane_lo, lane_hi = _pit_span_on_loop(loop, pit_path)
    pit_out_pct = round(_pct_on_loop(loop, pit_out[-1]), 5)
    fields: dict = {
        "pit_path": [[round(x, 7), round(y, 7)] for x, y in pit_path],
        "pit_out": [[round(x, 7), round(y, 7)] for x, y in pit_out],
        "pit_in_pct": None,
        "pit_span": [round(lane_lo, 5), round(lane_hi, 5)],
        "pit_out_pct": pit_out_pct,
    }
    if len(entry) >= 2:
        pit_in_seed = _resample_open(entry, 24)
        pit_in = _connect_blend_to_loop(
            pit_in_seed, loop, attach_end=False, n_loop=12, max_pts=24)
        if pit_path:
            pit_in = list(pit_in)
            pit_in[-1] = pit_path[0]
        pit_in = _resample_open(pit_in, 24)
        fields["pit_in"] = [[round(x, 7), round(y, 7)] for x, y in pit_in]
        fields["pit_in_pct"] = round(_pct_on_loop(loop, pit_in[0]), 5)
    else:
        fields["pit_in_pct"] = round(lane_lo, 5)
    return fields


def suffix_pit_lane_keys(fields: dict, suffix: str) -> dict:
    if not suffix:
        return dict(fields)
    return {f"{key}{suffix}": val for key, val in fields.items()}


def xy_list(raw) -> list[tuple[float, float]]:
    """Normalize IPC / JSON point arrays to (x, y) tuples."""
    out: list[tuple[float, float]] = []
    if not raw:
        return out
    for pt in raw:
        if isinstance(pt, (list, tuple)) and len(pt) >= 2:
            out.append((float(pt[0]), float(pt[1])))
        elif isinstance(pt, dict) and "x" in pt and "y" in pt:
            out.append((float(pt["x"]), float(pt["y"])))
    return out


def build_loop_doc(
    tid,
    *,
    loop: list[tuple[float, float]],
    name: str | None = None,
    start_finish: float = 0.0,
    corners: list | None = None,
    num_turns: int | None = None,
    alias_track_ids: list | None = None,
    pit_source: str = "manual",
) -> dict:
    """Racing loop (+ optional corners) for v2 track save."""
    try:
        doc_tid = int(tid)
    except (TypeError, ValueError):
        doc_tid = tid
    rot, mirror = orientation_from_cfg()
    doc: dict[str, Any] = {
        "schema": 2,
        "import_version": 2,
        "pit_source": pit_source,
        "track_id": doc_tid,
        "name": name or str(tid),
        "start_finish": float(start_finish),
        "points": [[round(p[0], 7), round(p[1], 7)] for p in loop],
        "corners": list(corners or []),
        "map_rotation": rot,
        "map_mirror": mirror,
    }
    if num_turns:
        doc["num_turns"] = int(num_turns)
    if alias_track_ids:
        doc["alias_track_ids"] = list(alias_track_ids)
    return doc


def save_manual_track(
    tracks_dir: str,
    *,
    tid,
    loop: list[tuple[float, float]],
    entry,
    road,
    merge,
    entry2=None,
    road2=None,
    merge2=None,
    name: str | None = None,
    start_finish: float = 0.0,
    corners: list | None = None,
    num_turns: int | None = None,
    alias_track_ids: list | None = None,
    pit_speed_ms: float = 0.0,
    pit_lane_speed_pct: float = 1.0,
    pit_lane_speed_pct_2: float = 1.0,
    demo: bool = False,
    upload_async=None,
) -> tuple[bool, str, dict | None]:
    """Write full track JSON (loop + pit). Returns (ok, msg, lane1_fields)."""
    if tid is None:
        return False, ("No TrackID — join a session on track, or import "
                       "members HTML with id=\"track-map-123\"."), None
    if not loop or len(loop) < 3:
        return False, "No track loop loaded.", None
    if len(road) < 2:
        return False, "Need at least 2 pit road points.", None
    if len(merge) < 2:
        return False, "Need at least 2 merge points.", None

    canonical = track_store.resolve_track_id(tracks_dir, tid) or tid
    block = cloud_blocks_track_save(canonical)
    if block:
        return False, block, None

    lane1 = build_manual_pit_lane_fields(loop, entry, road, merge)
    if not lane1:
        return False, "Could not build pit geometry.", None

    doc = build_loop_doc(
        tid,
        loop=loop,
        name=name,
        start_finish=start_finish,
        corners=corners,
        num_turns=num_turns,
        alias_track_ids=alias_track_ids,
    )
    doc.update(lane1)
    if pit_speed_ms > 0:
        doc["pit_speed"] = round(pit_speed_ms, 3)
    if pit_lane_speed_pct != 1.0:
        doc["pit_lane_speed_pct"] = round(pit_lane_speed_pct, 4)

    _PIT2_KEYS = (
        "pit_path_2", "pit_in_2", "pit_out_2", "pit_span_2",
        "pit_in_pct_2", "pit_out_pct_2", "pit_lane_speed_pct_2",
    )
    for key in _PIT2_KEYS:
        doc.pop(key, None)
    lane2 = build_manual_pit_lane_fields(
        loop, entry2 or [], road2 or [], merge2 or [])
    if lane2:
        doc.update(suffix_pit_lane_keys(lane2, "_2"))
        if pit_lane_speed_pct_2 != 1.0:
            doc["pit_lane_speed_pct_2"] = round(pit_lane_speed_pct_2, 4)

    path = write_track_json(tracks_dir, canonical, doc)
    if upload_async is not None and track_store.can_write():
        upload_async(tracks_dir, canonical)

    pit_path = lane1.get("pit_path") or []
    pit_out = lane1.get("pit_out") or []
    pit_in = lane1.get("pit_in") or []
    entry_note = f"entry {len(pit_in)}" if pit_in else "no entry"
    msg = (f"Saved {path} — {entry_note}, road {len(pit_path)}, "
           f"merge {len(pit_out)} pts")
    if lane2:
        msg += f"; lane 2 road {len(lane2.get('pit_path') or [])} pts"
    if track_store.can_write():
        msg += " Uploaded to cloud."
    if demo:
        msg += " Demo map updated for this session."
    return True, msg, lane1


def save_pit_patch(
    tracks_dir: str,
    *,
    tid,
    loop: list[tuple[float, float]],
    entry,
    road,
    merge,
    entry2=None,
    road2=None,
    merge2=None,
    pit_speed_ms: float = 0.0,
    pit_lane_speed_pct: float = 1.0,
    pit_lane_speed_pct_2: float = 1.0,
    demo: bool = False,
    upload_async=None,
    ensure_file=None,
) -> tuple[bool, str, dict | None]:
    """Patch pit into existing track file. Returns (ok, msg, meta)."""
    if tid is None:
        return False, ("No TrackID — join a session on track, or import "
                       "members HTML with id=\"track-map-123\"."), None
    if not loop or len(loop) < 3:
        return False, "No track loop loaded.", None
    if len(road) < 2:
        return False, "Need at least 2 pit road points.", None
    if len(merge) < 2:
        return False, "Need at least 2 merge points.", None

    canonical = track_store.resolve_track_id(tracks_dir, tid) or tid
    if ensure_file is not None and not ensure_file(canonical):
        return False, "Could not create local track file.", None

    lane1 = build_manual_pit_lane_fields(loop, entry, road, merge)
    if not lane1:
        return False, "Could not build pit geometry.", None

    _PIT2_KEYS = (
        "pit_path_2", "pit_in_2", "pit_out_2", "pit_span_2",
        "pit_in_pct_2", "pit_out_pct_2", "pit_lane_speed_pct_2",
    )
    meta: dict = dict(lane1)
    meta["pit_source"] = "manual"
    if pit_speed_ms > 0:
        meta["pit_speed"] = round(pit_speed_ms, 3)
    if pit_lane_speed_pct != 1.0:
        meta["pit_lane_speed_pct"] = round(pit_lane_speed_pct, 4)
    if "pit_in" not in meta:
        meta["pit_in"] = None

    lane2 = build_manual_pit_lane_fields(
        loop, entry2 or [], road2 or [], merge2 or [])
    if lane2:
        meta.update(suffix_pit_lane_keys(lane2, "_2"))
        if pit_lane_speed_pct_2 != 1.0:
            meta["pit_lane_speed_pct_2"] = round(pit_lane_speed_pct_2, 4)
        if "pit_in_2" not in meta:
            meta["pit_in_2"] = None
    else:
        for key in _PIT2_KEYS:
            meta[key] = None

    from .widgets import track_map

    try:
        ok = track_map.update_track_meta(tracks_dir, canonical, **meta)
    except Exception as exc:
        return False, f"Could not save pit: {exc}", None
    if not ok:
        return False, "Could not update local track file.", None

    if upload_async is not None and track_store.can_write():
        upload_async(tracks_dir, canonical)

    pit_path = lane1.get("pit_path") or []
    pit_out = lane1.get("pit_out") or []
    pit_in = lane1.get("pit_in") or []
    entry_note = f"entry {len(pit_in)}" if pit_in else "no entry"
    msg = (f"Saved pit — {entry_note}, road {len(pit_path)}, "
           f"merge {len(pit_out)} pts")
    if lane2:
        msg += f"; lane 2 road {len(lane2.get('pit_path') or [])} pts"
    if track_store.can_write():
        msg += " Uploaded to cloud."
    if demo:
        msg += " Demo map updated for this session."
    return True, msg, meta
