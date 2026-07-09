"""
2D track-map rendering for the overlay.

iRacing does NOT export live X/Y for other cars, so a 2D map is drawn by:
  1. Having a normalized track *path* (a closed loop of points, where position
     along the loop corresponds to lap distance percentage), and
  2. Placing each car onto that path by its CarIdxLapDistPct (0.0 -> 1.0).

Two ways to obtain the path:
  * Authoring: import a members HTML track map (Track Scan) or load a saved
    tracks/<TrackID>.json file (local cache or cloud).
  * Demo: build_demo_path() returns a built-in road-course curve so the map is
    visible immediately without iRacing.
"""

from __future__ import annotations

import json
import math
import os

from PyQt6.QtCore import QPointF, QRectF, Qt, QElapsedTimer, QTimer
from PyQt6.QtGui import (QColor, QFont, QFontMetricsF, QMouseEvent, QPainter,
                         QPainterPath, QPen, QPixmap, QWheelEvent)
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from ..map_markers import wrap_lap_delta
from . import icons
from .. import svgpath
from .chrome import col, draw_card, draw_dark_cell, ease
from .fonts import tabfont, tfont

SCHEMATIC_PIT_SOURCES = frozenset({"schematic", "inactive", "dashes", "manual"})

_PIT_JOINT_EPS = 1e-5
_PIT_EDIT_ZOOM_MIN = 0.5
_PIT_EDIT_ZOOM_MAX = 12.0


def is_schematic_pit_source(source: str | None) -> bool:
    return (source or "").strip().lower() in SCHEMATIC_PIT_SOURCES


def _mcfg() -> dict:
    return config.CFG["map"]


def _mcol(key: str) -> QColor:
    return config.qcolor(_mcfg()["colors"][key])


def _mcol_def(key: str, default: str) -> QColor:
    """Like _mcol but tolerant of older configs missing a newly-added key."""
    return config.qcolor(_mcfg().get("colors", {}).get(key, default))


def _pit_lane_opacity() -> float:
    """Clamped opacity (0..1) for the pit lane + entry/exit blend lines."""
    return max(0.0, min(1.0, _mcfg().get("pit_lane_opacity", 1.0)))


def car_palette() -> list:
    return _mcfg()["palette"]


# Hand-tuned control points for the demo track (a stylized road course).
_DEMO_CONTROL = [
    (0.10, 0.52), (0.11, 0.34), (0.18, 0.20), (0.33, 0.16), (0.47, 0.22),
    (0.55, 0.17), (0.69, 0.14), (0.85, 0.20), (0.92, 0.38), (0.83, 0.53),
    (0.90, 0.66), (0.83, 0.82), (0.66, 0.86), (0.55, 0.76), (0.44, 0.83),
    (0.29, 0.85), (0.16, 0.77), (0.13, 0.64),
]


def _catmull_rom_loop(points: list[tuple[float, float]], per_seg: int = 40):
    """Smooth closed Catmull-Rom spline through the control points."""
    n = len(points)
    out: list[tuple[float, float]] = []
    for i in range(n):
        p0 = points[(i - 1) % n]
        p1 = points[i]
        p2 = points[(i + 1) % n]
        p3 = points[(i + 2) % n]
        for s in range(per_seg):
            t = s / per_seg
            t2 = t * t
            t3 = t2 * t
            x = 0.5 * (
                (2 * p1[0])
                + (-p0[0] + p2[0]) * t
                + (2 * p0[0] - 5 * p1[0] + 4 * p2[0] - p3[0]) * t2
                + (-p0[0] + 3 * p1[0] - 3 * p2[0] + p3[0]) * t3
            )
            y = 0.5 * (
                (2 * p1[1])
                + (-p0[1] + p2[1]) * t
                + (2 * p0[1] - 5 * p1[1] + 4 * p2[1] - p3[1]) * t2
                + (-p0[1] + 3 * p1[1] - 3 * p2[1] + p3[1]) * t3
            )
            out.append((x, y))
    return out


def _resample_by_length(points: list[tuple[float, float]], n: int):
    """Resample a closed loop into n points equally spaced by arc length.

    This makes lap-pct -> index mapping roughly distance-proportional, so cars
    don't bunch up on long straights.
    """
    pts = points + [points[0]]
    seg_len = []
    total = 0.0
    for a, b in zip(pts, pts[1:]):
        d = math.hypot(b[0] - a[0], b[1] - a[1])
        seg_len.append(d)
        total += d
    if total == 0:
        return points[:]

    out = []
    step = total / n
    target = 0.0
    i = 0
    acc = 0.0
    for _ in range(n):
        while i < len(seg_len) and acc + seg_len[i] < target:
            acc += seg_len[i]
            i += 1
        if i >= len(seg_len):
            out.append(pts[-1])
        else:
            local = (target - acc) / seg_len[i] if seg_len[i] else 0.0
            a, b = pts[i], pts[i + 1]
            out.append((a[0] + (b[0] - a[0]) * local, a[1] + (b[1] - a[1]) * local))
        target += step
    return out


def _resample_open(points: list[tuple[float, float]], n: int):
    """Resample an OPEN polyline (e.g. a pit lane) into n arc-length-even points.

    Unlike _resample_by_length this does not close the loop, so the first and
    last points stay put -- important for a pit lane that starts where you leave
    the track and ends where you rejoin it.
    """
    if len(points) < 2 or n < 2:
        return list(points)
    seg_len = []
    total = 0.0
    for a, b in zip(points, points[1:]):
        d = math.hypot(b[0] - a[0], b[1] - a[1])
        seg_len.append(d)
        total += d
    if total == 0:
        return [points[0]] * n
    out = []
    step = total / (n - 1)
    for k in range(n):
        target = step * k
        acc = 0.0
        for i, d in enumerate(seg_len):
            if acc + d >= target or i == len(seg_len) - 1:
                local = (target - acc) / d if d else 0.0
                local = min(max(local, 0.0), 1.0)
                a, b = points[i], points[i + 1]
                out.append((a[0] + (b[0] - a[0]) * local,
                            a[1] + (b[1] - a[1]) * local))
                break
            acc += d
    return out


def _smooth_closed(points, window: int = 2, passes: int = 1):
    """Light circular moving-average smoothing for a closed loop. Rounds out
    discretization kinks (the 'squarish' bits left by sparse/interpolated bins)
    without collapsing the overall shape. Small window keeps real corners."""
    pts = list(points)
    n = len(pts)
    if n < 5 or window < 1:
        return pts
    for _ in range(max(1, passes)):
        out = []
        for i in range(n):
            sx = sy = 0.0
            for k in range(-window, window + 1):
                x, y = pts[(i + k) % n]
                sx += x
                sy += y
            cnt = 2 * window + 1
            out.append((sx / cnt, sy / cnt))
        pts = out
    return pts


def _smooth_open(points, window: int = 2, passes: int = 1):
    """Light moving-average smoothing for an OPEN polyline, holding the two
    endpoints fixed so a pit lane/blend still meets the track where it should.
    Removes the little offset 'steps' the parallel-lane nudge can leave."""
    if not points:
        return points
    pts = list(points)
    n = len(pts)
    if n < 5 or window < 1:
        return pts
    for _ in range(max(1, passes)):
        out = [pts[0]]
        for i in range(1, n - 1):
            lo = max(0, i - window)
            hi = min(n - 1, i + window)
            seg = pts[lo:hi + 1]
            out.append((sum(p[0] for p in seg) / len(seg),
                        sum(p[1] for p in seg) / len(seg)))
        out.append(pts[-1])
        pts = out
    return pts


def build_demo_path(n: int = 720):
    return _resample_by_length(_catmull_rom_loop(_DEMO_CONTROL), n)


def _split_run(run, turn, count: int):
    """Pick ``count`` apex indices within one sustained turning run.

    A long bend can hold more than one numbered corner (e.g. each banked end of
    an oval reads as two turns). The run is sliced into ``count`` equal shares of
    accumulated turn angle and the sharpest point in each share is its apex.
    """
    if count <= 1:
        return [max(run, key=lambda i: abs(turn[i]))]
    per = sum(abs(turn[i]) for i in run) / count
    acc = 0.0
    slot = 0
    out = []
    best_i, best_v = run[0], 0.0
    for i in run:
        v = abs(turn[i])
        if v > best_v:
            best_i, best_v = i, v
        acc += v
        if acc >= per * (slot + 1) and slot < count - 1:
            out.append(best_i)
            slot += 1
            best_i, best_v = i, 0.0
    out.append(best_i)
    return out


def _apexes_to_target(sig, turn, target: int):
    """Choose exactly ``target`` apexes across the significant turning runs.

    ``sig`` is a list of ``(run, total_abs_turn)``. When there are at least as
    many runs as turns we keep the ``target`` sharpest bends (dropping minor
    kinks iRacing doesn't number). When there are fewer runs than turns we split
    the longest bends -- the ones with the most turn angle per corner so far --
    until the counts add up. This single rule covers ovals (2 long bends -> 4
    turns) and road courses (N distinct bends -> N turns) without a type flag.
    """
    m = len(sig)
    if target <= m:
        keep = sorted(range(m), key=lambda j: sig[j][1], reverse=True)[:target]
        return [max(sig[j][0], key=lambda i: abs(turn[i])) for j in keep]
    counts = [1] * m
    for _ in range(target - m):
        j = max(range(m), key=lambda k: sig[k][1] / counts[k])
        counts[j] += 1
    found = []
    for (run, _tot), c in zip(sig, counts):
        found.extend(_split_run(run, turn, c))
    return found


def detect_corners(path, start_finish: float = 0.0,
                   min_turn_deg: float = 38.0, max_corners: int = 30,
                   corner_turn_deg: float = 80.0, target: int | None = None):
    """Find corners from a track loop's geometry and number them.

    Walks the (arc-length resampled) loop, measures how fast the heading turns,
    and groups sustained turning into corners. Each corner's apex (sharpest
    point) becomes a lap-pct, numbered 1..N in driving order from path[0].

    When ``target`` is given (iRacing's WeekendInfo TrackNumTurns) the result is
    forced to exactly that many corners: the sharpest bends are kept and long
    bends are split as needed to reach the count, so the numbering matches what
    iRacing shows on both ovals and road courses. Without a target it falls back
    to splitting each bend into ~``corner_turn_deg`` chunks.

    Returns a list of (pct, label) like the track-file "corners" array, so it
    can be drawn by the same code. Heuristic, but good enough for labels.
    """
    n = len(path) if path else 0
    if n < 16:
        return []
    step = max(1, n // 144)

    def heading(i):
        a = path[(i - step) % n]
        b = path[(i + step) % n]
        return math.atan2(b[1] - a[1], b[0] - a[0])

    heads = [heading(i) for i in range(n)]
    turn = []
    for i in range(n):
        d = heads[(i + 1) % n] - heads[i]
        d = (d + math.pi) % (2 * math.pi) - math.pi   # wrap to [-pi, pi]
        turn.append(d)

    # Smooth the turning rate so noise doesn't fragment one corner into many.
    win = max(1, n // 240)
    smooth = []
    for i in range(n):
        smooth.append(sum(turn[(i + k) % n] for k in range(-win, win + 1)))

    thr = 0.02  # rad/step: above this we consider the car to be "turning"
    mask = [abs(v) > thr for v in smooth]
    if not any(mask):
        return []

    # Walk circularly starting from a straight so runs don't wrap the seam.
    start0 = next((i for i in range(n) if not mask[i]), 0)
    runs = []
    cur: list[int] | None = None
    for k in range(n):
        idx = (start0 + k) % n
        if mask[idx]:
            cur = [idx] if cur is None else cur + [idx]
        elif cur:
            runs.append(cur)
            cur = None
    if cur:
        runs.append(cur)

    min_turn = math.radians(min_turn_deg)
    budget = math.radians(corner_turn_deg)
    # Significant bends only (drop gentle kinks below min_turn).
    sig = []
    for run in runs:
        total = abs(sum(turn[i] for i in run))
        if total >= min_turn:
            sig.append((run, total))
    if not sig:
        return []

    if target and target > 0:
        # Authoritative count from iRacing: keep/split bends to hit it exactly.
        found = _apexes_to_target(sig, turn, int(target))
    else:
        # No known count: split each bend into ~corner_turn_deg chunks (a
        # ~180-degree oval end -> 2 turns), one apex per chunk.
        found = []
        for run, total in sig:
            found.extend(_split_run(run, turn, max(1, round(total / budget))))

    found = sorted(set(found))  # driving order, de-duplicated
    corners = []
    for label, apex in enumerate(found[:max_corners], 1):
        pct = ((apex / n) + start_finish) % 1.0
        corners.append((pct, str(label)))
    return corners


def _arc_length_chain(pts: list[tuple[float, float]]) -> float:
    total = 0.0
    for a, b in zip(pts, pts[1:]):
        total += math.hypot(b[0] - a[0], b[1] - a[1])
    return total


def _parse_corner(c) -> tuple[float, str, float, float]:
    """Normalize a corner to (lap_pct, label, model_offset_x, model_offset_y)."""
    if isinstance(c, dict):
        return (float(c["pct"]), str(c["label"]),
                float(c.get("ox", 0.0)), float(c.get("oy", 0.0)))
    pct, label = float(c[0]), str(c[1])
    ox = float(c[2]) if len(c) > 2 else 0.0
    oy = float(c[3]) if len(c) > 3 else 0.0
    return pct, label, ox, oy


def corners_to_json(corners) -> list[dict]:
    """Serialize corners for a track file / MongoDB document."""
    out = []
    for pct, label, ox, oy in (_parse_corner(c) for c in (corners or [])):
        d = {"pct": round(pct, 5), "label": label}
        if ox or oy:
            d["ox"] = round(ox, 7)
            d["oy"] = round(oy, 7)
        out.append(d)
    return out


# --- Per-track files --------------------------------------------------------
#
# A track file is keyed by iRacing's TrackID and lives in tracks/<id>.json or
# tracks/<id>.svg. JSON schema:
#   {
#     "track_id": 18,
#     "name": "Circuit de Barcelona-Catalunya",
#     "points": [[x, y], ...],   # closed loop, ordered in driving direction
#     "start_finish": 0.0,        # lap pct of points[0] (default 0.0)
#     "corners": [{"pct": 0.07, "label": "1"}, ...]
#   }
# An .svg file is assumed to contain the track outline as the first <path>,
# drawn in driving direction starting at the start/finish line.


def find_track_file(track_id, tracks_dir: str = "tracks") -> str | None:
    if track_id is None:
        return None
    from .. import track_store

    resolved = track_store.resolve_track_id(tracks_dir, track_id)
    for ext in (".json", ".svg"):
        path = os.path.join(tracks_dir, f"{resolved}{ext}")
        if os.path.exists(path):
            return path
    return None


def save_learned_track(tracks_dir: str, track_id, points, name: str = "",
                       pit_span=None, pit_speed: float = 0.0,
                       num_turns=None) -> str | None:
    """Persist a learned track loop to tracks/<track_id>.json.

    Written in the same schema load_track reads, so the next session at this
    track loads it instantly instead of re-learning. Marked "learned": true so
    it's distinguishable from a hand-made/official file (delete it to re-scan).
    pit_span is an (entry_pct, exit_pct) tuple and pit_speed the learned pit
    limit in m/s; both are optional and only written when known.
    """
    if track_id is None or not points:
        return None
    os.makedirs(tracks_dir, exist_ok=True)
    path = os.path.join(tracks_dir, f"{track_id}.json")
    data = {
        "track_id": track_id,
        "name": name or "",
        "learned": True,
        "points": [[round(float(x), 9), round(float(y), 9)] for x, y in points],
        "start_finish": 0.0,
        "corners": [],
    }
    if pit_span is not None:
        data["pit_span"] = [round(float(pit_span[0]), 5), round(float(pit_span[1]), 5)]
    if pit_speed:
        data["pit_speed"] = round(float(pit_speed), 3)
    if num_turns:
        data["num_turns"] = int(num_turns)
    tmp = f"{path}.tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    os.replace(tmp, path)  # atomic, so a crash mid-write can't corrupt the file
    return path


def ensure_track_file(tracks_dir: str, track_id, points, *, name: str = "",
                      start_finish: float = 0.0, corners=None,
                      pit_span=None, pit_speed: float = 0.0,
                      num_turns=None, pit_path=None, pit_in=None,
                      pit_out=None, pit_in_pct=None, pit_out_pct=None,
                      pit_lane_speed_pct=None, learned: bool = False) -> bool:
    """Create tracks/<id>.json from in-memory state when no local file exists.

    Authoring edits patch the on-disk file; this ensures one exists when the
    overlay is showing a track loaded only from the cloud or an in-session scan.
    """
    if track_id is None or not points or len(points) < 2:
        return False
    from .. import track_store

    track_id = track_store.resolve_track_id(tracks_dir, track_id)
    path = os.path.join(tracks_dir, f"{track_id}.json")
    if os.path.exists(path):
        return True
    data: dict = {
        "track_id": track_id,
        "name": name or "",
        "points": [[round(float(x), 9), round(float(y), 9)] for x, y in points],
        "start_finish": float(start_finish or 0.0),
        "corners": corners if corners is not None else [],
    }
    if learned:
        data["learned"] = True
    if pit_span is not None:
        data["pit_span"] = [round(float(pit_span[0]), 5), round(float(pit_span[1]), 5)]
    if pit_speed:
        data["pit_speed"] = round(float(pit_speed), 3)
    if pit_lane_speed_pct is not None and float(pit_lane_speed_pct) != 1.0:
        data["pit_lane_speed_pct"] = round(float(pit_lane_speed_pct), 4)
    if num_turns:
        data["num_turns"] = int(num_turns)
    for key, seg in (("pit_path", pit_path), ("pit_in", pit_in), ("pit_out", pit_out)):
        if isinstance(seg, list) and len(seg) >= 2:
            data[key] = [[round(float(x), 7), round(float(y), 7)] for x, y in seg]
    for key, val in (("pit_in_pct", pit_in_pct), ("pit_out_pct", pit_out_pct)):
        if val is not None:
            data[key] = round(float(val), 5)
    os.makedirs(tracks_dir, exist_ok=True)
    tmp = f"{path}.tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    os.replace(tmp, path)
    track_store.invalidate_alias_cache()
    return True


def update_track_meta(tracks_dir: str, track_id, **fields) -> bool:
    """Patch extra keys (e.g. pit_span, pit_speed) into an existing track file.

    Used to record pit data learned after the geometry was already saved/loaded.
    A field whose value is None is removed from the file instead. No-op if the
    file doesn't exist yet -- call ``ensure_track_file`` first when authoring.
    """
    if track_id is None or not fields:
        return False
    from .. import track_store

    track_id = track_store.resolve_track_id(tracks_dir, track_id)
    path = os.path.join(tracks_dir, f"{track_id}.json")
    if not os.path.exists(path):
        return False
    try:
        with open(path, "r", encoding="utf-8") as fh:
            data = json.load(fh)
    except (OSError, ValueError):
        return False
    for k, v in fields.items():
        if v is None:
            data.pop(k, None)
        else:
            data[k] = v
    tmp = f"{path}.tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(data, fh)
    os.replace(tmp, path)
    track_store.invalidate_alias_cache()
    return True


def _parse_zone_ranges(raw) -> list[tuple[float, float]]:
    """Normalize track JSON zone lists to [(lo, hi), ...] lap fractions."""
    out: list[tuple[float, float]] = []
    if not isinstance(raw, list):
        return out
    for z in raw:
        if isinstance(z, (list, tuple)) and len(z) >= 2:
            out.append((float(z[0]), float(z[1])))
    return out


def load_track(path: str, n: int = 720):
    """Load a track file -> (points, start_finish_pct, corners, name, meta).

    meta carries optional extras: {"pit_span": (entry, exit), "pit_speed": m/s}.
    """
    if path.lower().endswith(".svg"):
        with open(path, "r", encoding="utf-8") as fh:
            d = svgpath.first_path_d(fh.read())
        if not d:
            raise ValueError(f"No <path> found in {path}")
        raw = svgpath.flatten_path(d)
        points = _resample_by_length(raw, n)
        name = os.path.splitext(os.path.basename(path))[0]
        return points, 0.0, [], name, {}

    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
    raw = [(float(a), float(b)) for a, b in data["points"]]
    points = _resample_by_length(raw, n)
    sf = float(data.get("start_finish", 0.0))
    corners = [_parse_corner(c) for c in data.get("corners", [])
               if isinstance(c, dict) and "pct" in c and "label" in c]
    meta: dict = {}
    if isinstance(data.get("pit_span"), (list, tuple)) and len(data["pit_span"]) == 2:
        meta["pit_span"] = (float(data["pit_span"][0]), float(data["pit_span"][1]))
    if data.get("pit_speed"):
        meta["pit_speed"] = float(data["pit_speed"])
    if data.get("pit_lane_speed_pct") is not None:
        meta["pit_lane_speed_pct"] = float(data["pit_lane_speed_pct"])
    for key in ("pit_path", "pit_in", "pit_out"):
        seg = data.get(key)
        if isinstance(seg, list) and len(seg) >= 2:
            meta[key] = [(float(a), float(b)) for a, b in seg]
    if "pit_path" not in meta:
        seg = data.get("pit_lane_points")
        if isinstance(seg, list) and len(seg) >= 2:
            meta["pit_path"] = [(float(a), float(b)) for a, b in seg]
    for key in ("pit_in_pct", "pit_out_pct"):
        if data.get(key) is not None:
            meta[key] = float(data[key])
    if data.get("num_turns"):
        meta["num_turns"] = int(data["num_turns"])
    if data.get("pit_source"):
        meta["pit_source"] = str(data["pit_source"])
    if data.get("schema"):
        meta["schema"] = int(data["schema"])
    if data.get("import_version") is not None:
        meta["import_version"] = int(data["import_version"])
    if data.get("map_rotation") is not None:
        meta["map_rotation"] = int(data["map_rotation"])
    if "map_mirror" in data:
        meta["map_mirror"] = bool(data["map_mirror"])
    if data.get("updated_at"):
        meta["updated_at"] = str(data["updated_at"])
    if data.get("track_id") is not None:
        meta["track_id"] = data["track_id"]
    aliases = data.get("alias_track_ids")
    if isinstance(aliases, list):
        meta["alias_track_ids"] = aliases
    drs = _parse_zone_ranges(data.get("drs_zones"))
    if drs:
        meta["drs_zones"] = drs
    p2p = _parse_zone_ranges(data.get("p2p_zones"))
    if p2p:
        meta["p2p_zones"] = p2p
    return points, sf, corners, data.get("name", ""), meta


class TrackPathBuilder:
    """Learns a track path from the player's GPS (Lat/Lon) as you drive.

    Rather than waiting for a near-complete lap before showing anything, the map
    appears as a rough loop once a fraction of the lap has been sampled, then
    keeps refining (rebuilding) as more of the track is covered. ``coverage()``
    reports progress so the UI can show a percentage while it learns.
    """

    def __init__(self, bins: int = 720, first_frac: float = 0.55):
        self.bins = bins
        # Per-bin running average: [sum_x, sum_y, count], or None until sampled.
        # Averaging lets multiple laps smooth each other out into one clean line.
        self._samples: list[list[float] | None] = [None] * bins
        self._filled = 0
        self.first_frac = first_frac      # coverage needed for the first preview
        self.ready = False                # a (possibly partial) path is available
        self.complete = False             # one full lap's worth of bins sampled
        self.path: list[tuple[float, float]] | None = None
        self.version = 0                  # bumped whenever path is rebuilt
        self._built_at = 0

    def coverage(self) -> float:
        return self._filled / self.bins

    def reset(self) -> None:
        """Drop sampled points so the next lap is learned from a fresh frame.

        Keeps the last built path (and ready/version) on screen so the map
        doesn't flicker back to the placeholder while a cleaner lap is captured.
        Used by dead reckoning, which must rebuild from one continuous lap (its
        coordinates are only consistent within a single lap origin).
        """
        self._samples = [None] * self.bins
        self._filled = 0
        self._built_at = 0

    def add(self, pct, lat, lon) -> None:
        """Add a sample from GPS (latitude / longitude in degrees)."""
        if lat is None or lon is None:
            return
        # (0, 0) is iRacing's "no GPS fix yet" sentinel -- ignore it so we don't
        # collapse the whole path onto the origin.
        if lat == 0.0 and lon == 0.0:
            return
        # Equirectangular projection to a local flat plane (good enough for a
        # single track). y is negated so North points up on screen.
        x = math.radians(lon) * math.cos(math.radians(lat))
        y = -math.radians(lat)
        self.add_xy(pct, x, y)

    def add_xy(self, pct, x, y) -> None:
        """Add an already-projected (x, y) sample at the given lap pct.

        Used both by the GPS path (via add) and by dead-reckoned positions when
        GPS isn't available. Samples keep accumulating across laps (they feed a
        per-bin running average), so the path never stops refining -- the caller
        decides when enough laps have been driven to finalize the scan.
        """
        if pct is None or x is None or y is None:
            return
        if not (0.0 <= pct <= 1.0):
            return
        i = min(int(pct * self.bins), self.bins - 1)
        s = self._samples[i]
        if s is None:
            self._samples[i] = [x, y, 1.0]
            self._filled += 1
        else:
            s[0] += x
            s[1] += y
            s[2] += 1.0

        cov = self._filled / self.bins
        if cov >= 0.96:
            self.complete = True
        # First preview at first_frac, then rebuild every +4% of new coverage so
        # the shape sharpens while the first lap fills in. Once the loop is fully
        # covered, further refinement comes from rebuild() (once per lap).
        grew = (self._filled - self._built_at) >= max(1, int(self.bins * 0.04))
        if (not self.ready and cov >= self.first_frac) or (self.ready and grew):
            self._build()
            self._built_at = self._filled
            self.ready = True
            self.version += 1

    def rebuild(self) -> None:
        """Force a rebuild from the current per-bin averages (e.g. after a lap)."""
        if not self._filled:
            return
        self._build()
        self.ready = True
        self.version += 1

    def _build(self) -> None:
        n = self.bins
        # Collapse each filled bin's running sum into its average point first.
        avg: list[tuple[float, float] | None] = [None] * n
        filled: list[int] = []
        for i, s in enumerate(self._samples):
            if s and s[2]:
                avg[i] = (s[0] / s[2], s[1] / s[2])
                filled.append(i)
        if not filled:
            return
        path: list[tuple[float, float]] = []
        for i in range(n):
            s = avg[i]
            if s:
                path.append(s)
                continue
            # Circular-interpolate across the nearest filled neighbours.
            back = next(j for j in (filled[::-1]) if j <= i) if any(
                j <= i for j in filled
            ) else filled[-1]
            fwd = next((j for j in filled if j >= i), filled[0])
            a, b = avg[back], avg[fwd]
            span = (fwd - back) % n or 1
            t = ((i - back) % n) / span
            path.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
        # Round out discretization kinks / squarish patches from sparse bins.
        # Two light passes round the shape noticeably more without flattening
        # real corners (the window stays small).
        self.path = _smooth_closed(path, window=2, passes=2)


class TrackMapWidget(QWidget):
    """Draws the track loop and places car dots by lap percentage."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.path: list[tuple[float, float]] | None = None
        self.start_finish = 0.0  # lap pct that path[0] corresponds to
        self.corners: list[tuple[float, str]] = []  # (lap_pct, label)
        # Corners auto-detected from the path shape, used when the track file
        # carries no corner data (e.g. learned tracks). num_turns is iRacing's
        # official corner count (WeekendInfo TrackNumTurns) when known, used to
        # force the auto-detector to that exact count.
        self._auto_corners: list[tuple[float, str]] = []
        self.num_turns: int | None = None
        self._track_is_oval = False
        self._centroid = (0.0, 0.0)
        # Pit lane: the (entry_pct, exit_pct) stretch the pit road runs alongside
        # and the learned speed limit (m/s), shown as a static badge.
        self.pit_span: tuple[float, float] | None = None
        self.pit_speed_ms: float = 0.0
        # Per-track multiplier for pit dot speed along polylines (1.0 = 100%).
        self.pit_lane_speed_pct: float = 1.0
        # The real pit-lane geometry (model-space points, same frame as path),
        # from where you leave the track to where you rejoin. When present it's
        # drawn instead of the inward-offset approximation of pit_span.
        self.pit_path: list[tuple[float, float]] | None = None
        # The entry/exit "blend" lines: the yellow commit lanes joining the
        # track to pit road (pit_in) and pit road back to the track (pit_out).
        self.pit_in: list[tuple[float, float]] | None = None
        self.pit_out: list[tuple[float, float]] | None = None
        # Lap-% extent of the whole pit route (divergence -> rejoin), used to
        # place cars onto the route by their CarIdxLapDistPct.
        self.pit_in_pct: float | None = None
        self.pit_out_pct: float | None = None
        # "schematic" = fixed pit geometry from image import; cars follow polylines.
        self.pit_source: str = ""
        # Per-car lap % when OnPitRoad dropped (schematic exit placement).
        self._schematic_exit_pcts: dict[int, float] = {}
        # The player's live position (model-space x, y) from real GPS, used to
        # draw the player exactly on the pit route while pitting. None when not
        # available / not pitting.
        self.player_xy: tuple[float, float] | None = None
        # Cache: the concatenated pit route (in + lane + out) and its cumulative
        # arc lengths, rebuilt whenever any pit geometry changes.
        self._route_pts: list[tuple[float, float]] | None = None
        self._route_cum: list[float] = []
        self._route_blends: bool = True  # whether the cached route includes blends
        # Wind: bearing the wind blows FROM (radians, clockwise from North) and
        # its speed in m/s. None until telemetry provides it.
        self.wind_dir: float | None = None
        self.wind_speed_ms: float = 0.0
        self.track_wetness: float | None = None
        self.rain_intensity: float | None = None
        self.drs_zones: list[tuple[float, float]] = []
        self.p2p_zones: list[tuple[float, float]] = []
        self._active_sector: tuple[int, list[float]] | None = None
        # Each car: (idx, lap_pct, label, color_hex, is_player, on_route,
        # on_pit, speaking, is_pace[, status_kind]).
        self.cars: list[tuple] = []
        self.sector_boundaries: list[float] = []
        self.traffic_markers: dict[str, dict | None] = {
            "ahead": None, "behind": None, "leader": None,
        }
        self._car_anim: dict[int, dict] = {}
        self._car_animating = False
        mcfg0 = _mcfg()
        self._cfg_rot = int(round((mcfg0.get("rotation", 0) or 0) / 90.0)) * 90 % 360
        self._cfg_mirror = bool(mcfg0.get("mirror", False))
        config.on_change(self._on_map_config_change)
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0
        self._display_corners_cache: list | None = None
        self.placeholder = "No track map loaded"
        self._progress_pct = -1
        # Multi-lap scan UI: a persistent badge while scanning ('LAP n/3' or
        # 'PIT n/3'), plus a transient hint banner that auto-clears.
        self._scan_text = ""
        self._hint_text = ""
        self._hint_timer = QTimer(self)
        self._hint_timer.setSingleShot(True)
        self._hint_timer.timeout.connect(self._clear_hint)
        # Corner authoring (Track Scan tab): drag labels on the map.
        self.corner_edit_mode = False
        self._corner_edit_cb = None
        self._drag_corner: int | None = None
        self._drag_last: QPointF | None = None
        self._corner_hit: list[tuple[QRectF, int]] = []
        self._layout_scale = 1.0
        self._layout_ox = 0.0
        self._layout_oy = 0.0
        self._layout_mirror = False
        self._layout_rot = 0
        self._static_pix: QPixmap | None = None
        self._static_key: tuple | None = None
        # Manual pit authoring (Track Scan v2): road then merge segments.
        self.pit_edit_mode = False
        self.pit_edit_phase = "road"
        self._pit_edit_entry: list[tuple[float, float]] = []
        self._pit_edit_road: list[tuple[float, float]] = []
        self._pit_edit_merge: list[tuple[float, float]] = []
        self._pit_drag_idx: tuple[str, int] | None = None
        self._pit_edit_cb = None
        self._pit_hit: list[tuple[QRectF, str, int]] = []
        self._pit_edit_zoom = 1.0
        self._pit_edit_pan = (0.0, 0.0)
        self._pit_edit_base_scale = 1.0
        self._pit_edit_base_ox = 0.0
        self._pit_edit_base_oy = 0.0
        self._pit_pan_active = False
        self._pit_pan_origin: QPointF | None = None
        self._pit_pan_start = (0.0, 0.0)
        # Start/finish authoring: drag the white tick along the racing line.
        self.sf_edit_mode = False
        self._sf_edit_cb = None
        self._drag_sf = False
        self._sf_hit: QRectF | None = None
        self.setMouseTracking(True)
        self.setFocusPolicy(Qt.FocusPolicy.WheelFocus)
        self.setMinimumHeight(180)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def _on_map_config_change(self, cfg: dict) -> None:
        """Drop stale car easing when layout rotation/mirror changes."""
        mcfg = cfg.get("map", {})
        rot = int(round((mcfg.get("rotation", 0) or 0) / 90.0)) * 90 % 360
        mirror = bool(mcfg.get("mirror", False))
        if rot != self._cfg_rot or mirror != self._cfg_mirror:
            self._cfg_rot = rot
            self._cfg_mirror = mirror
            self._car_anim.clear()
        self._invalidate_static_cache()

    def _invalidate_static_cache(self) -> None:
        self._static_pix = None
        self._static_key = None

    def _use_static_cache(self) -> bool:
        return (bool(self.path) and not self.pit_edit_mode
                and not self.corner_edit_mode and not self.sf_edit_mode)

    def _map_style_token(self, mc: dict) -> tuple:
        cols = mc.get("colors") or {}
        return (
            mc.get("show_panel"), mc.get("show_infield"),
            mc.get("asphalt_width"), mc.get("outline_width"),
            mc.get("show_pit"), mc.get("show_pit_blends"),
            mc.get("show_sector_boundaries"), mc.get("show_drs_zones"),
            mc.get("show_p2p_zones"), mc.get("show_corners"),
            mc.get("show_start_finish"),
            tuple(sorted((k, cols.get(k)) for k in cols)),
            self._cfg_rot, self._cfg_mirror,
        )

    def _static_cache_key(self) -> tuple:
        w, h = self.width(), self.height()
        if w < 1 or h < 1:
            return ()
        dpr = self.devicePixelRatioF()
        mc = _mcfg()
        corners = self.display_corners()
        return (
            int(w * dpr), int(h * dpr), dpr,
            id(self.path), len(self.path or []), self.start_finish,
            id(self.pit_path), id(self.pit_in), id(self.pit_out),
            self.pit_span, tuple(self.sector_boundaries),
            tuple(tuple(z) for z in self.drs_zones),
            tuple(tuple(z) for z in self.p2p_zones),
            self._map_style_token(mc),
            tuple(corners) if corners else (),
        )

    def resizeEvent(self, event) -> None:  # noqa: N802
        self._invalidate_static_cache()
        super().resizeEvent(event)

    def _invalidate_corner_cache(self) -> None:
        self._display_corners_cache = None

    def set_path(self, path) -> None:
        self.set_track(path, start_finish=0.0, corners=[])

    def set_progress(self, frac: float) -> None:
        """Legacy hook — track learning removed; no-op."""
        del frac

    def set_scan_status(self, text: str) -> None:
        """Show (or clear) a small scan badge like 'LAP 2/3' or 'PIT 2/3'."""
        text = text or ""
        if text == self._scan_text:
            return
        self._scan_text = text
        self.update()

    def flash_hint(self, text: str, ms: int = 2600) -> None:
        """Briefly show a hint banner (e.g. when the pit is scanned too early)."""
        self._hint_text = text or ""
        self.update()
        self._hint_timer.start(ms)

    def _clear_hint(self) -> None:
        self._hint_text = ""
        self.update()

    def set_track(self, path, start_finish: float = 0.0, corners=None) -> None:
        self.path = path
        self.start_finish = start_finish
        self.corners = [_parse_corner(c) for c in (corners or [])]
        if path:
            self._centroid = (
                sum(pt[0] for pt in path) / len(path),
                sum(pt[1] for pt in path) / len(path),
            )
            # Cache auto-detected corners (only used if the file has none).
            self._auto_corners = detect_corners(path, start_finish,
                                                target=self.num_turns)
        else:  # cleared (e.g. on rescan) -> forget the pit lane too
            self._auto_corners = []
            self.pit_span = None
            self.pit_speed_ms = 0.0
            self.pit_path = None
            self.pit_in = None
            self.pit_out = None
            self.pit_in_pct = None
            self.pit_out_pct = None
            self.pit_source = ""
            self._schematic_exit_pcts = {}
            self.player_xy = None
            self._invalidate_route()
        self._invalidate_corner_cache()
        self._invalidate_static_cache()
        self.update()

    def set_num_turns(self, n) -> None:
        """Set iRacing's official corner count (TrackNumTurns) and re-number the
        auto-detected corners to match. None/0 clears it (heuristic count)."""
        try:
            val = int(n) if n is not None else None
        except (TypeError, ValueError):
            val = None
        if val is not None and val <= 0:
            val = None
        if val == self.num_turns:
            return
        self.num_turns = val
        if self.path:
            self._auto_corners = detect_corners(self.path, self.start_finish,
                                                target=self.num_turns)
            self._invalidate_corner_cache()
            self._invalidate_static_cache()
            self.update()

    def set_track_is_oval(self, is_oval: bool) -> None:
        """Whether the current session track is an oval (affects corner labels)."""
        if bool(is_oval) == self._track_is_oval:
            return
        self._track_is_oval = bool(is_oval)
        self._invalidate_corner_cache()
        self._invalidate_static_cache()
        self.update()

    @staticmethod
    def _iracing_oval_label(label: str, num_turns: int) -> str:
        """Members SVG ovals label 4,3,2,1 along lap %; iRacing uses 1,2,3,4."""
        try:
            n = int(label)
        except (TypeError, ValueError):
            return label
        if 1 <= n <= num_turns:
            return str(num_turns + 1 - n)
        return label

    def _display_corner_label(self, label: str) -> str:
        n = self.num_turns
        if self._track_is_oval and n and n >= 2:
            return self._iracing_oval_label(label, n)
        return label

    def set_corners(self, corners) -> None:
        """Replace the displayed corner list (manual authoring)."""
        self.corners = [_parse_corner(c) for c in (corners or [])]
        self._invalidate_corner_cache()
        self._invalidate_static_cache()
        self.update()

    def regenerate_corners(self) -> None:
        """Re-detect corners from geometry using the current ``num_turns``."""
        if not self.path:
            return
        detected = detect_corners(self.path, self.start_finish,
                                  target=self.num_turns)
        self.set_corners([(p, l, 0.0, 0.0) for p, l in detected])

    def display_corners(self) -> list[tuple[float, str, float, float]]:
        """Corners to draw: saved manual list, else auto-detected."""
        if self._display_corners_cache is not None:
            return self._display_corners_cache
        if self.corners:
            out = [(p, self._display_corner_label(l), ox, oy)
                   for p, l, ox, oy in self.corners]
        else:
            out = [(p, self._display_corner_label(l), 0.0, 0.0)
                   for p, l in self._auto_corners]
        self._display_corners_cache = out
        return out

    def set_corner_edit(self, enabled: bool, callback=None) -> None:
        """Enable dragging corner labels on the map (write-access authoring)."""
        self.corner_edit_mode = bool(enabled)
        self._corner_edit_cb = callback if enabled else None
        self._drag_corner = None
        self._drag_last = None
        if enabled:
            self.set_sf_edit(False)
        self.setCursor(Qt.CursorShape.OpenHandCursor if enabled
                       else Qt.CursorShape.ArrowCursor)
        self.update()

    def set_sf_edit(self, enabled: bool, callback=None) -> None:
        """Enable dragging the start/finish line along the racing loop."""
        self.sf_edit_mode = bool(enabled)
        self._sf_edit_cb = callback if enabled else None
        self._drag_sf = False
        if enabled:
            self.corner_edit_mode = False
            self._drag_corner = None
            self.pit_edit_mode = False
            self._pit_drag_idx = None
        self.setCursor(Qt.CursorShape.OpenHandCursor if enabled
                       else Qt.CursorShape.ArrowCursor)
        self.update()

    def set_pit_edit(self, enabled: bool, callback=None) -> None:
        """Toggle click-to-draw pit road / merge on the live map."""
        self.pit_edit_mode = bool(enabled)
        self._pit_edit_cb = callback if enabled else None
        if not enabled:
            self._pit_drag_idx = None
            self._pit_pan_active = False
            self._pit_pan_origin = None
        if enabled:
            self.set_sf_edit(False)
            self.reset_pit_edit_view()
        self.setCursor(Qt.CursorShape.CrossCursor if enabled
                       else Qt.CursorShape.ArrowCursor)
        self.update()

    def reset_pit_edit_view(self) -> None:
        """Reset pit-edit zoom and pan to the auto-focused default."""
        self._pit_edit_zoom = 1.0
        self._pit_edit_pan = (0.0, 0.0)
        self.update()

    def _begin_pit_pan(self, pos: QPointF) -> None:
        self._pit_pan_active = True
        self._pit_pan_origin = pos
        self._pit_pan_start = self._pit_edit_pan
        self.setCursor(Qt.CursorShape.ClosedHandCursor)

    @staticmethod
    def _pit_points_coincide(a: tuple[float, float],
                             b: tuple[float, float]) -> bool:
        return (math.hypot(a[0] - b[0], a[1] - b[1])
                <= _PIT_JOINT_EPS)

    def _pit_has_joint(self) -> bool:
        return (len(self._pit_edit_road) >= 1 and len(self._pit_edit_merge) >= 1
                and self._pit_points_coincide(self._pit_edit_road[-1],
                                              self._pit_edit_merge[0]))

    def _pit_has_entry_joint(self) -> bool:
        return (len(self._pit_edit_entry) >= 1 and len(self._pit_edit_road) >= 1
                and self._pit_points_coincide(self._pit_edit_entry[-1],
                                              self._pit_edit_road[0]))

    def _sync_pit_joint(self) -> None:
        """Keep merge start tied to pit-road end."""
        if self._pit_edit_road and self._pit_edit_merge:
            self._pit_edit_merge[0] = self._pit_edit_road[-1]

    def _sync_pit_entry_joint(self) -> None:
        """Keep pit-road start tied to entry end when linked."""
        if self._pit_has_entry_joint():
            self._pit_edit_road[0] = self._pit_edit_entry[-1]

    def _seed_road_from_entry(self) -> None:
        if self._pit_edit_entry and not self._pit_edit_road:
            self._pit_edit_road.append(self._pit_edit_entry[-1])

    def _set_pit_edit_point(self, phase: str, idx: int,
                            x: float, y: float) -> None:
        """Move one pit-edit handle; joint endpoints stay linked."""
        if phase == "joint":
            if self._pit_edit_road:
                self._pit_edit_road[-1] = (x, y)
            if self._pit_edit_merge:
                self._pit_edit_merge[0] = (x, y)
            return
        if phase == "entry_joint":
            if self._pit_edit_entry:
                self._pit_edit_entry[-1] = (x, y)
            if self._pit_edit_road:
                self._pit_edit_road[0] = (x, y)
            return
        if phase == "entry":
            pts = self._pit_edit_entry
        elif phase == "road":
            pts = self._pit_edit_road
        else:
            pts = self._pit_edit_merge
        if not (0 <= idx < len(pts)):
            return
        pts[idx] = (x, y)
        if phase == "entry" and idx == len(self._pit_edit_entry) - 1:
            if self._pit_edit_road:
                self._pit_edit_road[0] = (x, y)
        elif phase == "road" and idx == 0 and self._pit_edit_entry:
            self._pit_edit_entry[-1] = (x, y)
        elif phase == "road" and idx == len(self._pit_edit_road) - 1:
            if self._pit_edit_merge:
                self._pit_edit_merge[0] = (x, y)
        elif phase == "merge" and idx == 0 and self._pit_edit_road:
            self._pit_edit_road[-1] = (x, y)

    def set_pit_edit_phase(self, phase: str) -> None:
        phase = (phase or "road").strip().lower()
        if phase not in ("entry", "road", "merge"):
            phase = "road"
        self.pit_edit_phase = phase
        if phase == "road":
            self._seed_road_from_entry()
        self.update()

    def pit_edit_snapshot(self) -> tuple[list, list, list]:
        return (list(self._pit_edit_entry), list(self._pit_edit_road),
                list(self._pit_edit_merge))

    def load_pit_edit(self, road, merge, entry=None) -> None:
        self._pit_edit_entry = [(float(x), float(y)) for x, y in (entry or [])]
        self._pit_edit_road = [(float(x), float(y)) for x, y in (road or [])]
        self._pit_edit_merge = [(float(x), float(y)) for x, y in (merge or [])]
        self._sync_pit_joint()
        self.update()

    def clear_pit_edit(self) -> None:
        self._pit_edit_entry = []
        self._pit_edit_road = []
        self._pit_edit_merge = []
        self._pit_drag_idx = None
        self.update()

    def clear_pit_edit_phase(self, phase: str) -> None:
        """Drop points for one pit-edit phase (entry, road, or merge)."""
        phase = (phase or "road").strip().lower()
        if phase == "entry":
            self._pit_edit_entry = []
        elif phase == "road":
            self._pit_edit_road = []
            self._pit_edit_merge = []
        elif phase == "merge":
            self._pit_edit_merge = []
        else:
            return
        self._pit_drag_idx = None
        self.update()

    def pop_last_pit_edit_point(self) -> None:
        if self.pit_edit_phase == "merge" and self._pit_edit_merge:
            self._pit_edit_merge.pop()
        elif self.pit_edit_phase == "entry" and self._pit_edit_entry:
            self._pit_edit_entry.pop()
        elif self.pit_edit_phase == "road":
            self._pit_edit_road.pop()
            self._sync_pit_joint()
            self._sync_pit_entry_joint()
        self.update()

    def set_pit(self, span, speed_ms: float = 0.0) -> None:
        """Set the pit-lane stretch (entry_pct, exit_pct) and learned limit."""
        self.pit_span = (float(span[0]), float(span[1])) if span else None
        if speed_ms:
            self.pit_speed_ms = float(speed_ms)
        self.update()

    @staticmethod
    def _clean_poly(path):
        return ([(float(x), float(y)) for x, y in path]
                if path and len(path) >= 2 else None)

    def set_pit_path(self, path) -> None:
        """Set (or clear) the real pit-lane geometry: model-space (x, y) points."""
        self.pit_path = self._clean_poly(path)
        self._invalidate_route()
        self._invalidate_static_cache()
        self.update()

    def set_pit_blends(self, pit_in, pit_out) -> None:
        """Set (or clear) the entry/exit blend lines (model-space polylines)."""
        self.pit_in = self._clean_poly(pit_in)
        self.pit_out = self._clean_poly(pit_out)
        self._invalidate_route()
        self._invalidate_static_cache()
        self.update()

    def set_pit_route_pct(self, in_pct, out_pct) -> None:
        """Set the lap-% extent (divergence -> rejoin) of the full pit route."""
        self.pit_in_pct = float(in_pct) if in_pct is not None else None
        self.pit_out_pct = float(out_pct) if out_pct is not None else None

    def set_pit_source(self, source: str | None) -> None:
        """Mark pit geometry origin: '' learned, 'schematic' from image import."""
        self.pit_source = (source or "").strip().lower()
        self._schematic_exit_pcts.clear()

    def set_schematic_exit_pcts(self, pcts: dict[int, float]) -> None:
        """Per-car lap % when they left pit road (schematic exit placement)."""
        if pcts == self._schematic_exit_pcts:
            return
        self._schematic_exit_pcts = dict(pcts)

    @staticmethod
    def _pct_in_interval(pct: float, lo: float, hi: float) -> bool:
        span = (hi - lo) % 1.0
        if span <= 1e-6:
            return False
        return ((pct - lo) % 1.0) <= span

    def _pos_on_polyline(self, pts: list[tuple[float, float]], t: float):
        if not pts or len(pts) < 2:
            return None
        t = min(max(t, 0.0), 1.0)
        cum = [0.0]
        for a, b in zip(pts, pts[1:]):
            cum.append(cum[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
        total = cum[-1]
        if total <= 0.0:
            return pts[0]
        target = t * total
        for i in range(len(pts) - 1):
            if cum[i + 1] >= target:
                seg = cum[i + 1] - cum[i]
                local = (target - cum[i]) / seg if seg else 0.0
                a, b = pts[i], pts[i + 1]
                return (a[0] + (b[0] - a[0]) * local,
                        a[1] + (b[1] - a[1]) * local)
        return pts[-1]

    @staticmethod
    def _closest_point_on_chain(
        segs: list[list[tuple[float, float]]],
        target: tuple[float, float],
    ) -> tuple[float, float] | None:
        """Nearest point on a polyline chain to *target* (model space)."""
        parts = [s for s in segs if s and len(s) >= 2]
        if not parts:
            return None
        tx, ty = target
        best: tuple[float, float] | None = None
        best_d = float("inf")
        for pts in parts:
            for a, b in zip(pts, pts[1:]):
                ax, ay = a
                bx, by = b
                dx, dy = bx - ax, by - ay
                ln2 = dx * dx + dy * dy
                if ln2 < 1e-12:
                    q = (ax, ay)
                else:
                    t = max(0.0, min(1.0, ((tx - ax) * dx + (ty - ay) * dy) / ln2))
                    q = (ax + dx * t, ay + dy * t)
                d = math.hypot(q[0] - tx, q[1] - ty)
                if d < best_d:
                    best_d = d
                    best = q
        return best

    def _pos_on_polyline_chain(self, segs: list[list[tuple[float, float]]], t: float):
        parts = [s for s in segs if s and len(s) >= 2]
        if not parts:
            return None
        lengths = [_arc_length_chain(s) for s in parts]
        total = sum(lengths)
        if total <= 0.0:
            return parts[0][0]
        target = min(max(t, 0.0), 1.0) * total
        acc = 0.0
        for seg, ln in zip(parts, lengths):
            if acc + ln >= target or seg is parts[-1]:
                local = (target - acc) / ln if ln else 0.0
                return self._pos_on_polyline(seg, local)
            acc += ln
        return parts[-1][-1]

    def _pit_arc_length(self, segments) -> float:
        """Total arc length of a pit segment chain."""
        parts = [s for s in segments if s and len(s) >= 2]
        return sum(_arc_length_chain(s) for s in parts)

    def _loop_arc_between(self, lo: float, hi: float) -> float:
        """Arc distance along the main loop between lap percentages lo and hi."""
        from tools.schematic_to_track import _arc_length

        if not self.path or len(self.path) < 2:
            return 0.0
        span_pct = (hi - lo) % 1.0
        if span_pct <= 1e-9:
            return 0.0
        return span_pct * _arc_length(
            [(float(p[0]), float(p[1])) for p in self.path], closed=True)

    def _pit_lane_bounds(self) -> tuple[float | None, float | None]:
        """Lap-% extent of the pit lane polyline, preferring authored ``pit_span``."""
        if not self.pit_path or len(self.pit_path) < 2:
            lane = self.pit_span
            return (lane[0], lane[1]) if lane else (None, None)
        path_lo = self._loop_pct_at(self.pit_path[0])
        path_hi = self._loop_pct_at(self.pit_path[-1])
        lane = self.pit_span
        if lane and path_lo is not None and path_hi is not None:
            lane_lo, lane_hi = lane
            if lane_lo is not None and self._pct_in_interval(
                    lane_lo, path_lo, path_hi):
                path_lo = lane_lo
            if lane_hi is not None and self._pct_in_interval(
                    lane_hi, path_lo, path_hi):
                path_hi = lane_hi
        return path_lo, path_hi

    def _pit_lane_mapping_interval(self) -> tuple[float | None, float | None]:
        """Lap-% extent for mapping ``OnPitRoad`` cars along ``pit_path``.

        Ovals often store a ``pit_span`` that wraps most of the lap; using it for
        placement clamps ``t`` early. Prefer raw path projection when the span is
        wide; otherwise use ``pit_span`` (narrow span, wide projection case).
        """
        lane = self.pit_span
        lane_lo = lane[0] if lane else None
        lane_hi = lane[1] if lane else None
        path_lo, path_hi = self._pit_lane_bounds()
        if lane_lo is None or lane_hi is None:
            return path_lo, path_hi
        span = (lane_hi - lane_lo) % 1.0
        if span > 0.5 and path_lo is not None and path_hi is not None:
            return path_lo, path_hi
        return lane_lo, lane_hi

    def _pit_route_mapping_interval(self) -> tuple[float | None, float | None]:
        """Lap-% extent for mapping OnPitRoad cars: pit_in_pct → pit_out_pct."""
        lo, hi = self.pit_in_pct, self.pit_out_pct
        if lo is not None and hi is not None:
            return lo, hi
        return self._pit_lane_mapping_interval()

    def _pit_path_handoff_point(self) -> tuple[float, float] | None:
        """Pit-road entry point where pit_in meets pit_path."""
        if self.pit_in and len(self.pit_in) >= 1:
            p = self.pit_in[-1]
            return (float(p[0]), float(p[1]))
        if self.pit_path and len(self.pit_path) >= 1:
            p = self.pit_path[0]
            return (float(p[0]), float(p[1]))
        return None

    def _pit_path_needs_reverse(self) -> bool:
        """True when pit_path[-1] is closer to the entry handoff than pit_path[0]."""
        if not self.pit_path or len(self.pit_path) < 2:
            return False
        handoff = self._pit_path_handoff_point()
        if handoff is None:
            return False
        from tools.schematic_to_track import _dist

        p0 = (float(self.pit_path[0][0]), float(self.pit_path[0][1]))
        p1 = (float(self.pit_path[-1][0]), float(self.pit_path[-1][1]))
        return _dist(p1, handoff) < _dist(p0, handoff)

    def _pit_path_pos_for_route_pct(
        self,
        pct: float,
        lo: float,
        hi: float,
    ) -> tuple[float, float] | None:
        """Place on pit_path using route lap-% order (pit_in → pit_out)."""
        if not self.pit_path or len(self.pit_path) < 2:
            return None
        span_pct = (hi - lo) % 1.0
        if span_pct <= 1e-6:
            return None
        linear = ((pct - lo) % 1.0) / span_pct
        segments = [self.pit_path]
        pit_arc = self._pit_arc_length(segments)
        loop_arc = self._loop_arc_between(lo, hi)
        scale = self.pit_lane_speed_pct
        if pit_arc > 1e-9 and loop_arc > 1e-9:
            t = min(1.0, max(0.0, linear * (loop_arc / pit_arc) * scale))
        else:
            t = min(1.0, max(0.0, linear * scale))
        if self._pit_path_needs_reverse():
            t = 1.0 - t
        return self._pos_on_polyline_chain(segments, t)

    def _loop_pct_at(self, pt) -> float | None:
        """Lap fraction of the nearest point on the main loop."""
        if not self.path or pt is None:
            return None
        from tools.schematic_to_track import _pct_on_loop

        return _pct_on_loop(
            [(float(p[0]), float(p[1])) for p in self.path],
            (float(pt[0]), float(pt[1])),
        )

    def _pit_phase_pos(
        self,
        pct: float,
        lo: float,
        hi: float,
        segments: list,
    ) -> tuple[float, float] | None:
        """Place a car along pit segments for a lap-% interval.

        Lap-% advances at constant rate through the interval; pit dots move at
        ``pit_arc / loop_arc`` of racing-line speed, scaled by
        ``pit_lane_speed_pct`` (no ease-in acceleration).
        """
        span_pct = (hi - lo) % 1.0
        if span_pct <= 1e-6:
            return None
        linear = ((pct - lo) % 1.0) / span_pct
        pit_arc = self._pit_arc_length(segments)
        loop_arc = self._loop_arc_between(lo, hi)
        scale = self.pit_lane_speed_pct
        if pit_arc > 1e-9 and loop_arc > 1e-9:
            t = min(1.0, max(0.0, linear * (loop_arc / pit_arc) * scale))
        else:
            t = min(1.0, max(0.0, linear * scale))
        return self._pos_on_polyline_chain(segments, t)

    def _pit_progress_t(
        self,
        pct: float,
        lo: float,
        hi: float,
        segments: list,
    ) -> float | None:
        """Lap-% fraction through a pit phase (0 at ``lo``, 1 at ``hi``)."""
        span_pct = (hi - lo) % 1.0
        if span_pct <= 1e-6:
            return None
        d_pct = (pct - lo) % 1.0
        return min(1.0, max(0.0, d_pct / span_pct))

    def _loop_point_at_pct(self, pct: float) -> tuple[float, float] | None:
        """Interpolated model-space point on the main loop at lap percentage."""
        if not self.path or len(self.path) < 2:
            return None
        from tools.schematic_to_track import _point_on_loop_at_frac

        frac = (pct - self.start_finish) % 1.0
        return _point_on_loop_at_frac(
            [(float(p[0]), float(p[1])) for p in self.path], frac)

    @staticmethod
    def _blend_xy(a: tuple[float, float], b: tuple[float, float],
                  w: float) -> tuple[float, float]:
        w = min(1.0, max(0.0, w))
        return (a[0] + (b[0] - a[0]) * w, a[1] + (b[1] - a[1]) * w)

    def _feather_schematic_pos(
        self,
        pct: float,
        route_pos: tuple[float, float],
    ) -> tuple[float, float]:
        """Ease between the racing line and pit route near entry/exit handoffs."""
        lo, hi = self.pit_in_pct, self.pit_out_pct
        track = self._loop_point_at_pct(pct)
        if lo is None or hi is None or track is None:
            return route_pos
        span = (hi - lo) % 1.0
        if span <= 1e-6:
            return route_pos
        feather = min(max(span * 0.12, 0.012), span * 0.35)
        d_entry = (pct - lo) % 1.0
        if d_entry < feather:
            w = d_entry / feather
            return self._blend_xy(track, route_pos, w)
        d_exit = (hi - pct) % 1.0
        if d_exit < feather:
            w = 1.0 - d_exit / feather
            return self._blend_xy(route_pos, track, w)
        return route_pos

    def _pos_for_schematic_route(self, idx: int, pct: float, on_route: bool,
                                 on_pit_road: bool, *, raw: bool = False):
        """Place a car on authored pit polylines (schematic tracks only)."""
        if not on_route or not is_schematic_pit_source(self.pit_source):
            return None
        lo = self.pit_in_pct
        hi = self.pit_out_pct
        lane = self.pit_span
        lane_lo = lane[0] if lane else None
        lane_hi = lane[1] if lane else None
        path_lo, path_hi = self._pit_lane_bounds()
        route_lo, route_hi = self._pit_route_mapping_interval()
        entry_end = lane_lo if lane_lo is not None else path_lo
        exit_pct = self._schematic_exit_pcts.get(idx)
        if exit_pct is None:
            exit_pct = lane_hi if lane_hi is not None else path_hi

        # Exit blend: pit lane end -> rejoin (off pit road, latched in demo).
        if (not on_pit_road and hi is not None and exit_pct is not None
                and self.pit_out and len(self.pit_out) >= 2
                and self._pct_in_interval(pct, exit_pct, hi)):
            pos = self._pit_phase_pos(pct, exit_pct, hi, [self.pit_out])
            if pos is not None:
                return pos if raw else self._feather_schematic_pos(pct, pos)

        # Entry blend: pit_in_pct -> pit lane start on pit_in only.
        if (lo is not None and entry_end is not None
                and self.pit_in and len(self.pit_in) >= 2
                and self._pct_in_interval(pct, lo, entry_end)):
            pos = self._pit_phase_pos(pct, lo, entry_end, [self.pit_in])
            if pos is not None:
                return pos if raw else self._feather_schematic_pos(pct, pos)

        # Pit lane: while OnPitRoad (after entry blend), always use pit_path.
        # S/F membership gaps in pit_span must not fall through to the racing line.
        if on_pit_road and self.pit_path and len(self.pit_path) >= 2:
            rlo, rhi = route_lo, route_hi
            if rlo is None or rhi is None:
                rlo = lo if lo is not None else lane_lo
                rhi = hi if hi is not None else lane_hi
            if rlo is not None and rhi is not None:
                pos = self._pit_path_pos_for_route_pct(pct, rlo, rhi)
                if pos is not None:
                    return pos
        return None

    def _pit_blend_weight(
        self,
        pct: float,
        *,
        on_route: bool,
        on_pit: bool,
        in_entry: bool,
        in_exit: bool,
    ) -> float:
        """0 = racing line only, 1 = pit route only (schematic entry/exit ramps)."""
        if not is_schematic_pit_source(self.pit_source):
            return 1.0 if on_route else 0.0
        lo, hi = self.pit_in_pct, self.pit_out_pct
        if lo is None or hi is None:
            return 1.0 if on_route else 0.0
        if not on_route and not on_pit:
            return 0.0
        span = (hi - lo) % 1.0
        if span <= 1e-6:
            return 0.0
        feather = min(max(span * 0.12, 0.012), span * 0.35)
        if in_entry:
            d_entry = (pct - lo) % 1.0
            if d_entry < feather:
                return d_entry / feather
            return 1.0
        if in_exit:
            d_exit = (hi - pct) % 1.0
            if d_exit < feather:
                return d_exit / feather if on_route else 0.0
            return 1.0 if on_route else 0.0
        if on_pit or on_route:
            return 1.0
        return 0.0

    def set_player_xy(self, xy) -> None:
        """Set the player's live (model-space) position, or None to clear.

        Used to draw the player exactly on the pit route from real GPS.
        """
        new = (float(xy[0]), float(xy[1])) if xy is not None else None
        if new == self.player_xy:
            return
        self.player_xy = new
        if self.path is not None:
            self.update()

    def clear_pit(self) -> None:
        """Forget the learned pit lane (used when rescanning the pits)."""
        self.pit_span = None
        self.pit_speed_ms = 0.0
        self.pit_path = None
        self.pit_in = None
        self.pit_out = None
        self.pit_in_pct = None
        self.pit_out_pct = None
        self.pit_source = ""
        self._schematic_exit_pcts = {}
        self.player_xy = None
        self._invalidate_route()
        self.update()

    def _invalidate_route(self) -> None:
        self._route_pts = None
        self._route_cum = []

    def _ensure_route(self) -> None:
        """(Re)build the concatenated pit route and the cumulative arc length at
        each point, used to place cars along it. With blends enabled the route is
        entry blend + lane + exit blend; with them off it's just the pit lane, so
        a car only rides it while actually on pit road."""
        blends = _mcfg().get("show_pit_blends", True)
        if self._route_pts is not None and self._route_blends == blends:
            return
        self._route_blends = blends
        segs = ((self.pit_in, self.pit_path, self.pit_out) if blends
                else (None, self.pit_path, None))
        pts: list[tuple[float, float]] = []
        for seg in segs:
            if not seg:
                continue
            # Avoid duplicating the shared joint point between segments.
            if pts and seg and pts[-1] == seg[0]:
                pts.extend(seg[1:])
            else:
                pts.extend(seg)
        self._route_pts = pts if len(pts) >= 2 else None
        cum = [0.0]
        if self._route_pts:
            for a, b in zip(self._route_pts, self._route_pts[1:]):
                cum.append(cum[-1] + math.hypot(b[0] - a[0], b[1] - a[1]))
        self._route_cum = cum

    def _pos_on_route(self, t: float):
        """Model-space point at arc-length fraction t in [0, 1] of the route."""
        self._ensure_route()
        pts, cum = self._route_pts, self._route_cum
        if not pts:
            return None
        total = cum[-1]
        if total <= 0.0:
            return pts[0]
        target = min(max(t, 0.0), 1.0) * total
        for i in range(len(pts) - 1):
            if cum[i + 1] >= target:
                seg = cum[i + 1] - cum[i]
                local = (target - cum[i]) / seg if seg else 0.0
                a, b = pts[i], pts[i + 1]
                return (a[0] + (b[0] - a[0]) * local,
                        a[1] + (b[1] - a[1]) * local)
        return pts[-1]

    def _route_t_for_pct(self, pct: float):
        """Map a car's lap pct onto an arc-length fraction of the pit route.

        Uses the route's lap-% extent (pit_in_pct -> pit_out_pct), falling back
        to pit_span when the blend extents aren't known (older tracks). With
        blends off the route is just the lane, so map over pit_span instead.
        """
        if _mcfg().get("show_pit_blends", True):
            lo, hi = self.pit_in_pct, self.pit_out_pct
        else:
            lo = hi = None
        if lo is None or hi is None:
            if self.pit_span is None:
                return None
            lo, hi = self.pit_span
        span = (hi - lo) % 1.0
        if span <= 1e-6:
            return None
        return ((pct - lo) % 1.0) / span

    def set_wind(self, wind_dir, speed_ms) -> None:
        """Set wind bearing (radians, the direction it blows FROM, CW from N)
        and speed (m/s). Pass None to hide the compass."""
        new_dir = float(wind_dir) if isinstance(wind_dir, (int, float)) else None
        new_spd = float(speed_ms) if isinstance(speed_ms, (int, float)) else 0.0
        if new_dir == self.wind_dir and abs(new_spd - self.wind_speed_ms) < 0.1:
            return
        self.wind_dir = new_dir
        self.wind_speed_ms = new_spd
        self.update()

    def set_weather(self, track_wetness=None, rain_intensity=None) -> None:
        """Optional wetness (0..100) and rain intensity for expanded weather."""
        wet = (float(track_wetness)
               if isinstance(track_wetness, (int, float)) else None)
        rain = (float(rain_intensity)
                if isinstance(rain_intensity, (int, float)) else None)
        if wet == self.track_wetness and rain == self.rain_intensity:
            return
        self.track_wetness = wet
        self.rain_intensity = rain
        self.update()

    def set_track_zones(
        self,
        *,
        drs_zones=None,
        p2p_zones=None,
    ) -> None:
        drs = list(drs_zones or [])
        p2p = list(p2p_zones or [])
        if drs == self.drs_zones and p2p == self.p2p_zones:
            return
        self.drs_zones = drs
        self.p2p_zones = p2p
        self._invalidate_static_cache()
        self.update()

    def set_active_sector(self, idx: int | None, starts=None) -> None:
        """Highlight the player's current sector arc on the map."""
        if idx is None or not starts:
            new = None
        else:
            new = (int(idx), list(starts))
        if new == self._active_sector:
            return
        self._active_sector = new
        self.update()

    def set_cars(self, cars) -> None:
        prev = self.cars
        self.cars = cars
        if self._car_targets_moved(prev, cars) or self._car_animating:
            self.update()

    @staticmethod
    def _car_targets_moved(prev: list, cars: list, eps: float = 0.0008) -> bool:
        if len(prev) != len(cars):
            return True
        for a, b in zip(prev, cars):
            if a[0] != b[0]:
                return True
            if abs(float(a[1]) - float(b[1])) > eps:
                return True
            if a[2:] != b[2:]:
                return True
        return False

    def set_sector_boundaries(self, starts) -> None:
        starts = list(starts or [])
        if starts == self.sector_boundaries:
            return
        self.sector_boundaries = starts
        self._invalidate_static_cache()
        self.update()

    def set_traffic_markers(self, markers: dict | None) -> None:
        markers = markers or {}
        norm: dict[str, dict | None] = {}
        for slot in ("ahead", "behind", "leader"):
            raw = markers.get(slot)
            if raw is None:
                norm[slot] = None
            elif isinstance(raw, dict):
                norm[slot] = {
                    "idx": raw.get("idx"),
                    "pct": raw.get("pct"),
                    "label": str(raw.get("label") or ""),
                }
            else:
                norm[slot] = {"idx": None, "pct": float(raw), "label": ""}
        if norm == self.traffic_markers:
            return
        self.traffic_markers = norm
        self.update()

    def _marker_slots_by_idx(self) -> dict[int, str]:
        out: dict[int, str] = {}
        for slot in ("leader", "ahead", "behind"):
            m = self.traffic_markers.get(slot)
            if m and m.get("idx") is not None:
                out[int(m["idx"])] = slot
        return out

    def _find_car(self, idx: int | None):
        if idx is None:
            return None
        for car in self.cars:
            if car[0] == idx:
                return car
        return None

    def _resolve_car_point(self, tx, car, cc: QPointF, off: float,
                           schematic: bool, *, pct_override: float | None = None) -> QPointF | None:
        """Screen position of a car dot (shared by car draw + traffic markers)."""
        in_entry = in_exit = False
        if len(car) >= 12:
            (idx, pct, _label, _color, is_player, on_route, on_pit,
             _speaking, _is_pace, _sk, in_entry, in_exit) = car
        elif len(car) >= 10:
            idx, pct, _label, _color, is_player, on_route, on_pit, _speaking, _is_pace, _sk = car
        elif len(car) >= 9:
            idx, pct, _label, _color, is_player, on_route, on_pit, _speaking = car
        elif len(car) >= 7:
            idx, pct, _label, _color, is_player, on_route, on_pit = car
        else:
            idx, pct, _label, _color, is_player = car[:5]
            on_route = on_pit = False
        if pct_override is not None:
            pct = pct_override
        from tools.schematic_to_track import _point_on_loop_at_frac

        frac = self._loop_frac_for_pct(pct)
        track_pos = _point_on_loop_at_frac(self.path, frac)
        weight = self._pit_blend_weight(
            pct,
            on_route=on_route,
            on_pit=on_pit,
            in_entry=in_entry,
            in_exit=in_exit,
        )
        route_pos = None
        if schematic and weight > 0:
            route_pos = self._pos_for_schematic_route(
                idx, pct, True, on_pit, raw=True)
        elif on_route and weight > 0:
            if is_player and self.player_xy is not None:
                route_pos = self.player_xy
            elif self.pit_path and len(self.pit_path) >= 2:
                t = self._route_t_for_pct(pct)
                if t is not None:
                    route_pos = self._pos_on_route(t)
        if route_pos is not None and weight > 0:
            model = (route_pos if weight >= 1.0
                     else self._blend_xy(track_pos, route_pos, weight))
        else:
            model = track_pos
        c = tx(model)
        if on_pit and weight < 0.5:
            dx, dy = c.x() - cc.x(), c.y() - cc.y()
            ln = math.hypot(dx, dy) or 1.0
            c = QPointF(c.x() - dx / ln * off, c.y() - dy / ln * off)
        return c

    def _car_motion_key(self, car, schematic: bool) -> tuple:
        is_player = car[4]
        on_route = car[5] if len(car) > 5 else False
        on_pit = car[6] if len(car) > 6 else False
        in_entry = car[10] if len(car) >= 11 else False
        if (is_player and on_route and self.player_xy is not None
                and not schematic):
            return ("player_xy", on_route, on_pit)
        if schematic and in_entry:
            return ("pct", on_route, on_pit)
        if on_route:
            return ("route", on_route, on_pit)
        return ("pct", on_route, on_pit)

    def _build_smooth_car_screen_points(self, tx, mc: dict) -> tuple[dict[int, QPointF], bool]:
        """Ease car dots toward telemetry targets for smoother map motion."""
        cc = tx(self._centroid)
        off = mc.get("asphalt_width", 12) * 0.85 + 3.0
        schematic = is_schematic_pit_source(self.pit_source)
        targets = self._build_car_screen_points(tx, mc)
        dt = self._dt()
        tau = 0.09
        animating = False
        seen: set[int] = set()
        pts: dict[int, QPointF] = {}

        for car in self.cars:
            idx = car[0]
            seen.add(idx)
            target = targets.get(idx)
            if target is None:
                continue
            pct = float(car[1])
            key = self._car_motion_key(car, schematic)
            st = self._car_anim.get(idx)
            if st is None or st.get("key") != key:
                prev_pt = QPointF(st["pt"]) if st else QPointF(target)
                st = {"key": key, "pct": pct, "pt": prev_pt, "xy": None}
                if key[0] == "player_xy" and self.player_xy is not None:
                    st["xy"] = self.player_xy
                self._car_anim[idx] = st
                if prev_pt != target:
                    pt, moving = self._smooth_marker_point(
                        prev_pt, target, dt, tau=tau, snap=120.0)
                    st["pt"] = pt
                    pts[idx] = pt
                    if moving:
                        animating = True
                else:
                    pts[idx] = target
                continue

            if key[0] == "player_xy" and self.player_xy is not None:
                ox, oy = st["xy"] or self.player_xy
                tx_, ty = self.player_xy
                nx = ease(ox, tx_, dt, tau)
                ny = ease(oy, ty, dt, tau)
                st["xy"] = (nx, ny)
                pt = tx((nx, ny))
                moving = abs(nx - tx_) > 1e-5 or abs(ny - ty) > 1e-5
            elif key[0] == "pct":
                delta = wrap_lap_delta(pct, st["pct"])
                if abs(delta) > 0.35:
                    st["pct"] = pct
                else:
                    st["pct"] = (st["pct"] + ease(0.0, delta, dt, tau)) % 1.0
                pt = self._resolve_car_point(
                    tx, car, cc, off, schematic, pct_override=st["pct"])
                if pt is None:
                    pt = target
                moving = (abs(wrap_lap_delta(pct, st["pct"])) > 1e-5
                          or math.hypot(pt.x() - target.x(), pt.y() - target.y()) > 0.35)
            else:
                pt, moving = self._smooth_marker_point(
                    st["pt"], target, dt, tau=tau, snap=120.0)
                st["pt"] = pt

            pts[idx] = pt
            if moving:
                animating = True

        for dead in [k for k in self._car_anim if k not in seen]:
            del self._car_anim[dead]
        return pts, animating

    def _build_car_screen_points(self, tx, mc: dict) -> dict[int, QPointF]:
        cc = tx(self._centroid)
        off = mc.get("asphalt_width", 12) * 0.85 + 3.0
        schematic = is_schematic_pit_source(self.pit_source)
        pts: dict[int, QPointF] = {}
        for car in self.cars:
            c = self._resolve_car_point(tx, car, cc, off, schematic)
            if c is not None:
                pts[car[0]] = c
        return pts

    def _car_screen_point(self, tx, car) -> QPointF | None:
        """Screen position of a car dot (matches _draw_cars placement)."""
        if car is None:
            return None
        cc = tx(self._centroid)
        mc = _mcfg()
        off = mc.get("asphalt_width", 12) * 0.85 + 3.0
        schematic = is_schematic_pit_source(self.pit_source)
        return self._resolve_car_point(tx, car, cc, off, schematic)

    def _loop_frac_for_pct(self, pct: float) -> float:
        return (pct - self.start_finish) % 1.0

    def _layout_pad(self, mc: dict | None = None) -> float:
        """Screen-space inset so outward labels/icons are not clipped."""
        mc = mc or _mcfg()
        pad = 26.0
        asph = mc.get("asphalt_width", 12)
        outward = 0.0
        if mc.get("show_traffic_markers", True):
            sz = max(8, round(10 * config.text_scale_for("map")))
            icon_off = asph * 1.2 + sz + 10.0
            side = max(sz + 6, 22.0)
            pill_h = sz + 14.0
            outward = max(outward, icon_off + side / 2.0 + pill_h + 4.0)
        if mc.get("show_corners", True):
            sz = max(5, round(8 * config.text_scale_for("map")))
            off = asph * 0.5 + sz + 8.0
            outward = max(outward, off + sz + 8.0)
        if mc.get("show_sector_boundaries", True):
            sz = max(5, round(7 * config.text_scale_for("map")))
            off = asph * 0.5 + sz + 6.0
            outward = max(outward, off + sz + 6.0)
        return pad + outward

    def _outward_point(self, tx, pct: float, extra: float) -> QPointF:
        s = tx(self.path[self._index_for_pct(pct)])
        cc = tx(self._centroid)
        return self._outward_from_point(s, cc, extra)

    @staticmethod
    def _outward_from_point(pt: QPointF, cc: QPointF, extra: float) -> QPointF:
        dx, dy = pt.x() - cc.x(), pt.y() - cc.y()
        ln = math.hypot(dx, dy) or 1.0
        return QPointF(pt.x() + dx / ln * extra, pt.y() + dy / ln * extra)

    def _track_point(self, tx, pct: float) -> QPointF:
        return tx(self.path[self._index_for_pct(pct)])

    def _dt(self) -> float:
        now = self._clock.elapsed()
        dt = (now - self._last_ms) / 1000.0
        self._last_ms = now
        return max(0.0, min(0.1, dt))

    def _smooth_marker_point(self, cur: QPointF, tgt: QPointF, dt: float, *,
                             tau: float = 0.10, snap: float = 72.0) -> tuple[QPointF, bool]:
        dx, dy = tgt.x() - cur.x(), tgt.y() - cur.y()
        if dx * dx + dy * dy > snap * snap:
            return tgt, False
        nx = ease(cur.x(), tgt.x(), dt, tau)
        ny = ease(cur.y(), tgt.y(), dt, tau)
        moving = abs(nx - tgt.x()) > 0.4 or abs(ny - tgt.y()) > 0.4
        return QPointF(nx, ny), moving

    def _draw_perpendicular_tick(self, p: QPainter, tx, pct: float,
                               *, color_key: str, tick: float = 6.0,
                               width: float = 2.0) -> None:
        from tools.schematic_to_track import _point_on_loop_at_frac

        if not self.path:
            return
        n = len(self.path)
        frac = self._loop_frac_for_pct(pct)
        a = _point_on_loop_at_frac(self.path, frac)
        b = _point_on_loop_at_frac(self.path, (frac + 3.0 / max(n, 1)) % 1.0)
        ax, ay = tx(a).x(), tx(a).y()
        bx, by = tx(b).x(), tx(b).y()
        dx, dy = bx - ax, by - ay
        ln = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / ln, dx / ln
        p.setPen(QPen(_mcol_def(color_key, "#ffffff"), width))
        p.drawLine(
            QPointF(ax - nx * tick, ay - ny * tick),
            QPointF(ax + nx * tick, ay + ny * tick),
        )

    def _draw_sector_boundaries(self, p: QPainter, tx) -> None:
        if not self.sector_boundaries:
            return
        mc = _mcfg()
        asph = mc.get("asphalt_width", 12)
        sz = max(5, round(7 * config.text_scale_for("map")))
        fam = config.CFG.get("font_family", "Arial")
        p.setFont(QFont(fam, sz, QFont.Weight.Bold))
        fm = p.fontMetrics()
        label_off = asph * 0.5 + sz + 6.0
        sf_frac = self._sf_loop_frac()
        starts = sorted(set(float(s) for s in self.sector_boundaries))
        label_num = 2
        for start in starts:
            frac = self._loop_frac_for_pct(start)
            if abs(frac) < 1e-4 or abs(frac - sf_frac) < 0.01:
                continue
            self._draw_perpendicular_tick(
                p, tx, start, color_key="sector_line", tick=6.0, width=2.0)
            outward = self._outward_point(tx, start, label_off)
            label = f"S{label_num}"
            label_num += 1
            bw = max(fm.height() + 2, fm.horizontalAdvance(label) + 8)
            bh = fm.height() + 2
            rect = QRectF(outward.x() - bw / 2, outward.y() - bh / 2, bw, bh)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(_mcol_def("sector_line", "#a78bfa"))
            p.drawRoundedRect(rect, 3, 3)
            p.setPen(_mcol_def("sector_text", "#c4b5fd"))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)

    def _draw_traffic_markers(self, p: QPainter, tx, mc: dict,
                              car_pts: dict[int, QPointF]) -> None:
        asph = mc.get("asphalt_width", 12)
        sz = max(8, round(10 * config.text_scale_for("map")))
        icon_off = asph * 1.2 + sz + 10.0
        specs = (
            ("leader", "leader", "marker_leader"),
            ("ahead", "car_ahead", "marker_ahead"),
            ("behind", "car_behind", "marker_behind"),
        )
        cc = tx(self._centroid)
        for slot, glyph, col_key in specs:
            m = self.traffic_markers.get(slot)
            if not m or m.get("pct") is None:
                continue
            pct = m["pct"]
            idx = m.get("idx")
            label = m.get("label") or ""
            car_pt = car_pts.get(idx) if idx is not None else None
            if car_pt is None:
                car_pt = self._track_point(tx, pct)
            icon_pt = self._outward_from_point(car_pt, cc, icon_off)
            line_col = _mcol_def(col_key, "#ffffff")
            p.setPen(QPen(line_col, 2.0))
            p.drawLine(car_pt, icon_pt)
            side = max(sz + 6, 22.0)
            icon_rect = QRectF(icon_pt.x() - side / 2, icon_pt.y() - side / 2,
                               side, side)
            draw_dark_cell(p, icon_rect, "map", radius=5)
            if icons.has(glyph):
                p.setFont(icons.icon_font(sz))
                p.setPen(_mcol_def(col_key, "#ffffff"))
                p.drawText(icon_rect, Qt.AlignmentFlag.AlignCenter,
                           icons.glyph(glyph))
            if label:
                p.setFont(QFont(config.CFG.get("font_family", "Arial"),
                                max(7, sz - 2), QFont.Weight.Bold))
                fm = p.fontMetrics()
                pw = fm.horizontalAdvance(label) + 8
                ph = fm.height() + 4
                pill = QRectF(icon_pt.x() - pw / 2, icon_rect.bottom() + 2, pw, ph)
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(_mcol_def(col_key, "#ffffff"))
                p.drawRoundedRect(pill, 3, 3)
                p.setPen(QColor(20, 20, 20))
                p.drawText(pill, Qt.AlignmentFlag.AlignCenter, label)

    def _draw_car_speaking(self, p: QPainter, c: QPointF, r: float) -> None:
        """Green ring + outward mic badge so radio traffic stands out on the map."""
        ring = _mcol_def("speaking_ring", "#46df7a")
        glow = QColor(ring)
        glow.setAlpha(int(_mcol_def("speaking_glow", "#46df7a55").alpha()))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(glow)
        p.drawEllipse(c, r + 7.5, r + 7.5)
        p.setPen(QPen(ring, 2.8))
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawEllipse(c, r + 4.8, r + 4.8)

        if not icons.has("speaking"):
            return
        sz = max(7, round(r * 1.05))
        side = sz + 6
        badge_c = QPointF(c.x() + r * 0.95, c.y() - r * 1.05)
        badge = QRectF(badge_c.x() - side / 2, badge_c.y() - side / 2, side, side)
        p.setPen(QPen(_mcol_def("speaking_badge_text", "#ffffff"), 1.2))
        p.setBrush(_mcol_def("speaking_badge_bg", "#22c55e"))
        p.drawEllipse(badge)
        p.setPen(_mcol_def("speaking_badge_text", "#ffffff"))
        p.setFont(icons.icon_font(sz))
        p.drawText(badge, Qt.AlignmentFlag.AlignCenter, icons.glyph("speaking"))

    def _index_for_pct(self, pct: float) -> int:
        n = len(self.path)
        return int(((pct - self.start_finish) % 1.0) * n) % n

    def _sf_loop_frac(self) -> float:
        """Loop arc fraction (from path[0]) where the start/finish line sits."""
        return (-self.start_finish) % 1.0

    def _sf_model_point(self) -> tuple[float, float] | None:
        if not self.path:
            return None
        from tools.schematic_to_track import _point_on_loop_at_frac
        return _point_on_loop_at_frac(self.path, self._sf_loop_frac())

    def _sf_screen_point(self, tx) -> QPointF | None:
        pt = self._sf_model_point()
        if pt is None:
            return None
        return tx(pt)

    def _draw_scan_overlays(self, p: QPainter, rect: QRectF) -> None:
        """Scan badge (top, e.g. 'LAP 2/3') and a transient hint banner (bottom)."""
        if self._scan_text:
            p.setFont(QFont("Arial", 9, QFont.Weight.Bold))
            fm = p.fontMetrics()
            bw = fm.horizontalAdvance(self._scan_text) + 14.0
            bh = fm.height() + 4.0
            br = QRectF((rect.width() - bw) / 2.0, 6.0, bw, bh)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(_mcol_def("scan_bg", "#000000c8"))
            p.drawRoundedRect(br, 6, 6)
            p.setPen(_mcol_def("corner_text", "#d6dce2"))
            p.drawText(br, Qt.AlignmentFlag.AlignCenter, self._scan_text)
        if self._hint_text:
            p.setFont(QFont("Arial", 9, QFont.Weight.Bold))
            fm = p.fontMetrics()
            bw = fm.horizontalAdvance(self._hint_text) + 18.0
            bh = fm.height() + 5.0
            br = QRectF((rect.width() - bw) / 2.0, rect.height() - bh - 8.0, bw, bh)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(_mcol_def("hint_bg", "#ff9416e6"))
            p.drawRoundedRect(br, 6, 6)
            p.setPen(_mcol_def("hint_text", "#ffffff"))
            p.drawText(br, Qt.AlignmentFlag.AlignCenter, self._hint_text)

    def _paint_static_map(self, p, rect, tx, mc, qpath) -> None:
        """Track geometry that changes only on resize, config, or track edits."""
        if mc.get("show_panel", True) and "bg_top" in mc["colors"]:
            draw_card(p, rect.width(), rect.height(), "map")
        if mc.get("show_infield", True):
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(_mcol("infield"))
            p.drawPath(qpath)
        asphalt = QPen(_mcol("asphalt"), mc.get("asphalt_width", 12))
        asphalt.setCapStyle(Qt.PenCapStyle.RoundCap)
        asphalt.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(asphalt)
        p.drawPath(qpath)
        p.setPen(QPen(_mcol("outline"), mc.get("outline_width", 6)))
        p.drawPath(qpath)
        if mc.get("show_pit", True) and (self.pit_path or self.pit_in
                                         or self.pit_out
                                         or self.pit_span is not None):
            self._draw_pit(p, tx)
        if self.pit_edit_mode:
            self._draw_pit_edit(p, tx)
        if mc.get("show_sector_boundaries", True):
            self._draw_sector_boundaries(p, tx)
        if mc.get("show_drs_zones", False) and self.drs_zones:
            self._draw_zones(p, tx, self.drs_zones, "drs_zone")
        if mc.get("show_p2p_zones", False) and self.p2p_zones:
            self._draw_zones(p, tx, self.p2p_zones, "p2p_zone")
        corners = self.display_corners()
        if mc.get("show_corners", True):
            self._draw_corners(p, tx, corners)
        if mc.get("show_start_finish", True):
            self._draw_start_finish(p, tx)

    def paintEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        p = QPainter(self)
        try:
            config.use_section("map")
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            p.setRenderHint(QPainter.RenderHint.TextAntialiasing)
            rect = QRectF(self.rect())

            mc = _mcfg()

            if not self.path:
                if mc.get("show_panel", True) and "bg_top" in mc["colors"]:
                    draw_card(p, rect.width(), rect.height(), "map")
                p.setFont(tfont(min(rect.width(), rect.height()) * 0.06, bold=False))
                p.setPen(col("muted", "map"))
                p.drawText(rect, Qt.AlignmentFlag.AlignCenter, self.placeholder)
                self._draw_scan_overlays(p, rect)
                self._paint_extras(p, rect)
                return

            # Orientation: mirror (flip x) then rotate in 90-degree steps. Applied in
            # model space so the bounding-box fit, cars, corners and pit all follow.
            rot = int(round((mc.get("rotation", 0) or 0) / 90.0)) * 90 % 360
            mirror = bool(mc.get("mirror", False))

            def model(pt):
                x, y = pt[0], pt[1]
                if mirror:
                    x = -x
                if rot == 90:
                    x, y = y, -x
                elif rot == 180:
                    x, y = -x, -y
                elif rot == 270:
                    x, y = -y, x
                return x, y

            fit = [model(pt) for pt in self.path]
            for seg in (self.pit_path, self.pit_in, self.pit_out):
                if seg:
                    fit.extend(model(pt) for pt in seg)
            if self.pit_edit_mode:
                phase_seg = {
                    "entry": self._pit_edit_entry,
                    "road": self._pit_edit_road,
                    "merge": self._pit_edit_merge,
                }.get(self.pit_edit_phase, self._pit_edit_road)
                if phase_seg:
                    fit.extend(model(pt) for pt in phase_seg)
            xs = [m[0] for m in fit]
            ys = [m[1] for m in fit]
            minx, maxx, miny, maxy = min(xs), max(xs), min(ys), max(ys)
            pad = self._layout_pad(mc)
            avail_w = rect.width() - 2 * pad
            avail_h = rect.height() - 2 * pad
            span_x = (maxx - minx) or 1e-6
            span_y = (maxy - miny) or 1e-6
            scale = min(avail_w / span_x, avail_h / span_y)
            ox = pad + (avail_w - span_x * scale) / 2 - minx * scale
            oy = pad + (avail_h - span_y * scale) / 2 - miny * scale
            self._pit_edit_base_scale = scale
            self._pit_edit_base_ox = ox
            self._pit_edit_base_oy = oy
            if self.pit_edit_mode:
                scale *= self._pit_edit_zoom
                ox += self._pit_edit_pan[0]
                oy += self._pit_edit_pan[1]
            self._layout_scale = scale
            self._layout_ox = ox
            self._layout_oy = oy
            self._layout_mirror = mirror
            self._layout_rot = rot

            def tx(pt):
                mx, my = model(pt)
                return QPointF(mx * scale + ox, my * scale + oy)

            qpath = QPainterPath()
            qpath.moveTo(tx(self.path[0]))
            for pt in self.path[1:]:
                qpath.lineTo(tx(pt))
            qpath.closeSubpath()

            if self._use_static_cache():
                key = self._static_cache_key()
                if (self._static_pix is not None and key == self._static_key):
                    dpr = self.devicePixelRatioF()
                    pw = max(1, int(rect.width() * dpr))
                    ph = max(1, int(rect.height() * dpr))
                    if (abs(self._static_pix.width() - pw) > 1
                            or abs(self._static_pix.height() - ph) > 1):
                        self._invalidate_static_cache()
                        key = self._static_cache_key()
                if self._static_pix is None or key != self._static_key:
                    dpr = self.devicePixelRatioF()
                    pw = max(1, int(rect.width() * dpr))
                    ph = max(1, int(rect.height() * dpr))
                    pm = QPixmap(pw, ph)
                    pm.setDevicePixelRatio(dpr)
                    pm.fill(Qt.GlobalColor.transparent)
                    sp = QPainter(pm)
                    sp.setRenderHint(QPainter.RenderHint.Antialiasing)
                    sp.setRenderHint(QPainter.RenderHint.TextAntialiasing)
                    self._paint_static_map(sp, rect, tx, mc, qpath)
                    sp.end()
                    self._static_pix = pm
                    self._static_key = key
                p.drawPixmap(0, 0, self._static_pix)
            else:
                self._paint_static_map(p, rect, tx, mc, qpath)

            if self._active_sector is not None:
                self._draw_active_sector(p, tx)
            car_pts, self._car_animating = self._build_smooth_car_screen_points(tx, mc)
            self._draw_cars(p, tx, mc, car_pts)
            if mc.get("show_traffic_markers", True):
                self._draw_traffic_markers(p, tx, mc, car_pts)
            if mc.get("show_wind", True) and self.wind_dir is not None:
                step = max(1, len(self.path) // 180)
                scr = [tx(pt) for pt in self.path[::step]]
                self._draw_wind(p, rect, scr)
            self._draw_scan_overlays(p, rect)
            self._paint_extras(p, rect)
        finally:
            if p.isActive():
                p.end()
        if self._car_animating:
            self.update()

    def _paint_extras(self, p: QPainter, rect: QRectF) -> None:
        """Hook for subclasses to draw overlays in the same paint pass."""

    def _draw_start_finish(self, p: QPainter, tx) -> None:
        from tools.schematic_to_track import _point_on_loop_at_frac

        if not self.path:
            return
        n = len(self.path)
        frac = self._sf_loop_frac()
        a = _point_on_loop_at_frac(self.path, frac)
        b = _point_on_loop_at_frac(self.path, (frac + 3.0 / max(n, 1)) % 1.0)
        ax, ay = tx(a).x(), tx(a).y()
        bx, by = tx(b).x(), tx(b).y()
        dx, dy = bx - ax, by - ay
        ln = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / ln, dx / ln
        tick = 9 if self.sf_edit_mode else 7
        width = 4 if self.sf_edit_mode else 3
        p.setPen(QPen(QColor(255, 255, 255), width))
        p.drawLine(
            QPointF(ax - nx * tick, ay - ny * tick),
            QPointF(ax + nx * tick, ay + ny * tick),
        )
        if self.sf_edit_mode or self._drag_sf:
            pad = 10
            self._sf_hit = QRectF(
                ax - tick - pad, ay - tick - pad,
                2 * (tick + pad), 2 * (tick + pad))
        else:
            self._sf_hit = None

    def _draw_pit(self, p: QPainter, tx) -> None:
        # Prefer the real recorded pit-lane geometry; fall back to the inward
        # offset approximation of pit_span when no geometry is available.
        hide_saved = self.pit_edit_mode
        show_in = not (hide_saved and self._pit_edit_entry)
        show_road = not (hide_saved and self._pit_edit_road)
        show_out = not (hide_saved and self._pit_edit_merge)
        if self.pit_path and len(self.pit_path) >= 2 and show_road:
            # Blend lines first, so the lane reads on top where they join. Entry
            # is yellow, exit is blue; both hidden when show_pit_blends is off.
            if _mcfg().get("show_pit_blends", True):
                if show_in and self.pit_in and len(self.pit_in) >= 2:
                    self._draw_pit_blend(p, tx, self.pit_in, "pit_blend",
                                         "#ffd23a")
                if show_out and self.pit_out and len(self.pit_out) >= 2:
                    self._draw_pit_blend(p, tx, self.pit_out, "pit_blend_out",
                                         "#3aa0ff")
            self._draw_pit_path(p, tx)
            return
        if self.pit_span is None:
            return
        n = len(self.path)
        if n < 3:
            return
        mc = _mcfg()
        start = self._index_for_pct(self.pit_span[0])
        end = self._index_for_pct(self.pit_span[1])
        # Indices from entry -> exit in driving direction (wrapping past the line).
        span = (end - start) % n
        if span == 0 or span > n * 0.6:  # ignore degenerate / implausible spans
            return
        idxs = [(start + k) % n for k in range(span + 1)]

        # Offset the highlight inward (toward the infield) so it reads as a
        # separate lane running beside the track rather than over it.
        cc = tx(self._centroid)
        off = mc.get("asphalt_width", 12) * 0.85 + 3.0
        pts = []
        for i in idxs:
            s = tx(self.path[i])
            dx, dy = s.x() - cc.x(), s.y() - cc.y()
            ln = math.hypot(dx, dy) or 1.0
            pts.append(QPointF(s.x() - dx / ln * off, s.y() - dy / ln * off))

        lane = QPainterPath()
        lane.moveTo(pts[0])
        for q in pts[1:]:
            lane.lineTo(q)
        pen = QPen(_mcol("pit"), 2.2)
        pen.setStyle(Qt.PenStyle.DashLine)
        pen.setCapStyle(Qt.PenCapStyle.FlatCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        pen.setDashPattern([4, 3])  # short "slashed" dashes
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(pen)
        p.setOpacity(_pit_lane_opacity())
        p.drawPath(lane)
        p.setOpacity(1.0)  # keep the speed badge fully opaque

        # Anchor the label just inside the track at the start of the pit lane.
        entry = pts[0]
        dx, dy = entry.x() - cc.x(), entry.y() - cc.y()
        ln = math.hypot(dx, dy) or 1.0
        anchor = QPointF(entry.x() - dx / ln * 22.0, entry.y() - dy / ln * 22.0)
        self._draw_pit_label(p, anchor)

    def _draw_pit_path(self, p: QPainter, tx) -> None:
        """Draw the real pit lane: an asphalt underlay plus a dashed pit line,
        following the recorded route from track-exit to track-rejoin."""
        mc = _mcfg()
        pts = [tx(q) for q in self.pit_path]
        lane = QPainterPath()
        lane.moveTo(pts[0])
        for q in pts[1:]:
            lane.lineTo(q)
        p.setOpacity(_pit_lane_opacity())
        # Asphalt underlay so it reads as a real road surface like the track.
        base = QPen(_mcol("asphalt"), max(3.0, mc.get("asphalt_width", 12) * 0.6))
        base.setCapStyle(Qt.PenCapStyle.RoundCap)
        base.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(base)
        p.drawPath(lane)
        # Dashed pit-colored centre line on top.
        pen = QPen(_mcol("pit"), 2.2)
        pen.setStyle(Qt.PenStyle.DashLine)
        pen.setCapStyle(Qt.PenCapStyle.FlatCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        pen.setDashPattern([4, 3])
        p.setPen(pen)
        p.drawPath(lane)
        p.setOpacity(1.0)  # keep the speed badge fully opaque
        self._draw_pit_label(p, pts[0])

    def _draw_pit_blend(self, p: QPainter, tx, seg, color_key="pit_blend",
                        default="#ffd23a") -> None:
        """Draw a pit entry/exit blend line as dashed 'slash' marks -- the
        commit lane that joins the track to pit road (and back)."""
        pts = [tx(q) for q in seg]
        path = QPainterPath()
        path.moveTo(pts[0])
        for q in pts[1:]:
            path.lineTo(q)
        pen = QPen(_mcol_def(color_key, default), 2.4)
        pen.setStyle(Qt.PenStyle.DashLine)
        pen.setCapStyle(Qt.PenCapStyle.FlatCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        pen.setDashPattern([3, 4])  # short, steep "slash" dashes
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(pen)
        p.setOpacity(_pit_lane_opacity())
        p.drawPath(path)
        p.setOpacity(1.0)

    def _draw_pit_label(self, p: QPainter, anchor: QPointF) -> None:
        # Static badge: just the learned pit speed limit -- no live comparison.
        if not _mcfg().get("show_pit_speed", True):
            return
        limit = self.pit_speed_ms
        if not limit:
            return
        text = f"PIT {round(config.conv_speed(limit))} {config.speed_unit()}"

        fam = config.CFG.get("font_family", "Arial")
        sz = max(5, round(6 * config.text_scale_for("map")))
        p.setFont(QFont(fam, sz, QFont.Weight.Bold))
        fm = p.fontMetrics()
        w = fm.horizontalAdvance(text) + 10
        h = fm.height() + 3
        # Centered on the pit-lane entry, nudged into the infield; clamped so it
        # never spills outside the widget.
        x = min(max(2.0, anchor.x() - w / 2), self.width() - w - 2.0)
        y = min(max(2.0, anchor.y() - h / 2), self.height() - h - 2.0)
        rect = QRectF(x, y, w, h)
        p.setBrush(_mcol("pit"))
        p.setPen(Qt.PenStyle.NoPen)
        p.drawRoundedRect(rect, 4, 4)
        p.setPen(_mcol("pit_text"))
        p.drawText(rect, Qt.AlignmentFlag.AlignCenter, text)

    def _wind_radius(self, rect: QRectF) -> float:
        return max(8.0, min(rect.width(), rect.height()) * 0.034)

    def _wind_center(self, screen_pts, rect: QRectF) -> tuple[QPointF, float]:
        """Place the compass just outside a track-bbox corner (whichever overlaps
        the fewest track points), clamped to stay inside the widget."""
        r = self._wind_radius(rect)
        label_h = r + 14.0
        total_h = r + label_h + 6.0
        total_w = 2 * r + 4.0
        gap = 4.0
        if not screen_pts:
            return QPointF(rect.right() - gap - r, rect.top() + gap + r + 6), r
        xs = [q.x() for q in screen_pts]
        ys = [q.y() for q in screen_pts]
        minx, maxx, miny, maxy = min(xs), max(xs), min(ys), max(ys)
        candidates = {
            "tr": (maxx + gap + r, miny + r + gap),
            "tl": (minx - gap - r, miny + r + gap),
            "br": (maxx + gap + r, maxy - r - gap),
            "bl": (minx - gap - r, maxy - r - gap),
        }

        def hits(cx: float, cy: float) -> int:
            box = QRectF(cx - total_w / 2, cy - r - 6, total_w, total_h)
            return sum(1 for q in screen_pts if box.contains(q))

        order = ("tr", "tl", "br", "bl")
        corner = min(order, key=lambda k: (hits(*candidates[k]), order.index(k)))
        cx, cy = candidates[corner]
        cx = max(rect.left() + gap + r, min(rect.right() - gap - r, cx))
        cy = max(rect.top() + gap + r + 6,
                 min(rect.bottom() - gap - label_h, cy))
        return QPointF(cx, cy), r

    def _draw_wind(self, p: QPainter, rect: QRectF, screen_pts) -> None:
        """Small north-up compass hugging the track bbox: ring, N tick, wind arrow,
        and speed readout."""
        center, r = self._wind_center(screen_pts, rect)
        cx, cy = center.x(), center.y()
        col = _mcol("wind")

        p.setBrush(QColor(10, 13, 17, 190))
        p.setPen(QPen(QColor(255, 255, 255, 40), 1))
        p.drawEllipse(center, r, r)

        fam = config.CFG.get("font_family", "Arial")
        nsz = max(5, round(6 * config.text_scale_for("map")))
        p.setFont(QFont(fam, nsz, QFont.Weight.Bold))
        p.setPen(QColor(170, 178, 188))
        p.drawText(QRectF(cx - r, cy - r - nsz - 1, 2 * r, nsz + 2),
                   Qt.AlignmentFlag.AlignCenter, "N")

        b = self.wind_dir + math.pi
        ux, uy = math.sin(b), -math.cos(b)
        px, py = -uy, ux
        tip = QPointF(cx + ux * r * 0.78, cy + uy * r * 0.78)
        tail = QPointF(cx - ux * r * 0.70, cy - uy * r * 0.70)
        p.setPen(QPen(col, max(1.5, r * 0.14), Qt.PenStyle.SolidLine,
                      Qt.PenCapStyle.RoundCap))
        p.drawLine(tail, tip)
        hl = r * 0.42
        hw = r * 0.26
        base = QPointF(tip.x() - ux * hl, tip.y() - uy * hl)
        head = QPainterPath()
        head.moveTo(tip)
        head.lineTo(QPointF(base.x() + px * hw, base.y() + py * hw))
        head.lineTo(QPointF(base.x() - px * hw, base.y() - py * hw))
        head.closeSubpath()
        p.setBrush(col)
        p.setPen(Qt.PenStyle.NoPen)
        p.drawPath(head)

        spd = round(config.conv_speed(self.wind_speed_ms))
        text = f"{spd} {config.speed_unit()}"
        ssz = max(5, round(6 * config.text_scale_for("map")))
        p.setFont(QFont(fam, ssz, QFont.Weight.Bold))
        fm = p.fontMetrics()
        tw = fm.horizontalAdvance(text) + 6
        th = fm.height() + 2
        lr = QRectF(cx - tw / 2, cy + r + 1, tw, th)
        p.setBrush(QColor(10, 13, 17, 190))
        p.setPen(Qt.PenStyle.NoPen)
        p.drawRoundedRect(lr, 2, 2)
        p.setPen(_mcol("wind_text"))
        p.drawText(lr, Qt.AlignmentFlag.AlignCenter, text)

        if _mcfg().get("show_expanded_weather", False):
            lines: list[str] = []
            if self.track_wetness is not None:
                lines.append(f"Wet {self.track_wetness:.0f}%")
            if self.rain_intensity is not None and self.rain_intensity > 0:
                lines.append(f"Rain {self.rain_intensity:.0f}%")
            if lines:
                ssz2 = max(5, round(6 * config.text_scale_for("map")))
                p.setFont(QFont(fam, ssz2, QFont.Weight.Bold))
                fm2 = p.fontMetrics()
                tw2 = max(fm2.horizontalAdvance(s) for s in lines) + 6
                th2 = fm2.height() * len(lines) + 4
                lr2 = QRectF(cx - tw2 / 2, lr.bottom() + 2, tw2, th2)
                p.setBrush(QColor(10, 13, 17, 190))
                p.setPen(Qt.PenStyle.NoPen)
                p.drawRoundedRect(lr2, 2, 2)
                p.setPen(_mcol("wind_text"))
                y = lr2.top() + 2
                for line in lines:
                    p.drawText(QRectF(lr2.left(), y, lr2.width(), fm2.height() + 1),
                               Qt.AlignmentFlag.AlignCenter, line)
                    y += fm2.height()

    def _draw_zones(self, p: QPainter, tx, zones: list[tuple[float, float]],
                    color_key: str) -> None:
        if not self.path or not zones:
            return
        mc = _mcfg()
        width = max(4.0, mc.get("asphalt_width", 12) * 1.35)
        col = _mcol_def(color_key, "#46df7a88")
        pen = QPen(col, width)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(pen)
        for lo, hi in zones:
            span = (hi - lo) % 1.0
            if span <= 1e-5:
                continue
            steps = max(8, int(span * len(self.path)))
            pts: list[QPointF] = []
            for i in range(steps + 1):
                pct = (lo + span * (i / steps)) % 1.0
                pt = self._loop_point_at_pct(pct)
                if pt is not None:
                    pts.append(tx(pt))
            if len(pts) < 2:
                continue
            path = QPainterPath()
            path.moveTo(pts[0])
            for q in pts[1:]:
                path.lineTo(q)
            p.drawPath(path)

    def _draw_active_sector(self, p: QPainter, tx) -> None:
        if not self.path or not self._active_sector:
            return
        idx, starts = self._active_sector
        n = len(starts)
        if n <= 0 or idx < 0 or idx >= n:
            return
        lo = starts[idx]
        hi = starts[(idx + 1) % n] if n > 1 else 1.0
        if idx + 1 >= n:
            hi = 1.0
        self._draw_zones(p, tx, [(lo, hi)], "active_sector")

    def _status_color(self, kind: str | None) -> QColor | None:
        if not kind:
            return None
        key = f"status_{kind}"
        colors = _mcfg().get("colors", {})
        if key in colors:
            return _mcol(key)
        if kind in ("black", "dq", "furled", "meatball"):
            return _mcol_def(f"status_{kind}", "#ff9416")
        return None

    def _draw_car_status_badge(self, p: QPainter, c: QPointF, r: float,
                               kind: str | None) -> None:
        if not kind:
            return
        fill = self._status_color(kind)
        if fill is None:
            return
        br = max(5.0, r * 0.55)
        bx = c.x() + r * 0.65
        by = c.y() - r * 0.65
        p.setPen(QPen(QColor(0, 0, 0, 180), 1))
        p.setBrush(fill)
        p.drawEllipse(QPointF(bx, by), br, br)
        glyph = {
            "pit": "P", "off": "!", "garage": "G",
            "black": "B", "meatball": "M", "dq": "X", "furled": "W",
        }.get(kind, "")
        if glyph:
            sz = max(5, round(br * 1.1))
            p.setFont(QFont(config.CFG.get("font_family", "Arial"), sz,
                            QFont.Weight.Bold))
            p.setPen(QColor(255, 255, 255) if kind != "furled" else QColor(20, 20, 20))
            p.drawText(QRectF(bx - br, by - br, 2 * br, 2 * br),
                       Qt.AlignmentFlag.AlignCenter, glyph)

    @staticmethod
    def _draw_stroked_center_text(p: QPainter, rect: QRectF, text: str, *,
                                  fill: QColor, stroke: QColor,
                                  stroke_w: float = 1.0) -> None:
        """Bold centered label readable on any dot color."""
        align = Qt.AlignmentFlag.AlignCenter
        w = max(0.5, stroke_w)
        for ox, oy in ((-w, -w), (-w, w), (w, -w), (w, w),
                       (-w, 0), (w, 0), (0, -w), (0, w)):
            p.setPen(stroke)
            p.drawText(rect.translated(ox, oy), align, text)
        p.setPen(fill)
        p.drawText(rect, align, text)

    def _draw_car_number_label(self, p: QPainter, c: QPointF, label: str, *,
                               r: float, is_player: bool, is_pace: bool,
                               show: bool) -> None:
        """Compact on-dot number; skip when a traffic marker already labels it."""
        if not show:
            return
        draw_label = "PC" if is_pace else label
        if not draw_label:
            return
        base = 9.0 if is_player else 7.5
        sz = max(6, round(base * config.text_scale_for("map")))
        p.setFont(tabfont(sz, bold=True, widget_scale=False))
        pad = max(2.0, r * 0.15)
        # Pixel-snap the label center so stroked glyphs don't pulse while cars ease.
        cx, cy = round(c.x()), round(c.y())
        rect = QRectF(cx - r - pad, cy - r - pad, 2 * (r + pad), 2 * (r + pad))
        if is_pace:
            fill = _mcol_def("pace_car_text", "#ffffff")
            stroke = QColor(0, 0, 0, 160)
        else:
            fill = QColor(255, 255, 255)
            stroke = QColor(0, 0, 0, 220)
        self._draw_stroked_center_text(
            p, rect, draw_label, fill=fill, stroke=stroke,
            stroke_w=1.2 if is_player else 1.0)

    def _draw_pit_edit(self, p: QPainter, tx) -> None:
        """In-progress entry (yellow), pit road (red), and merge (blue)."""
        mc = _mcfg()
        self._pit_hit = []
        base_r = max(4.0, mc.get("asphalt_width", 12) * 0.35)
        r = max(8.0, base_r * math.sqrt(max(self._pit_edit_zoom, 1.0)))

        def _polyline(pts, color_key: str, width: float):
            if len(pts) < 2:
                return
            path = QPainterPath()
            path.moveTo(tx(pts[0]))
            for pt in pts[1:]:
                path.lineTo(tx(pt))
            pen = QPen(_mcol(color_key), width)
            pen.setCapStyle(Qt.PenCapStyle.RoundCap)
            pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.setPen(pen)
            p.drawPath(path)

        _polyline(self._pit_edit_entry, "pit_blend",
                  max(2.5, mc.get("asphalt_width", 12) * 0.45))
        _polyline(self._pit_edit_road, "pit", max(3.0, mc.get("asphalt_width", 12) * 0.55))
        _polyline(self._pit_edit_merge, "pit_blend_out",
                  max(2.5, mc.get("asphalt_width", 12) * 0.45))

        joint = self._pit_has_joint()
        joint_pt = (self._pit_edit_road[-1] if joint else None)
        entry_joint = self._pit_has_entry_joint()
        entry_joint_pt = (self._pit_edit_entry[-1] if entry_joint else None)

        for phase, pts, col in (
            ("entry", self._pit_edit_entry, QColor(255, 210, 58)),
            ("road", self._pit_edit_road, QColor(255, 90, 90)),
            ("merge", self._pit_edit_merge, QColor(90, 160, 255)),
        ):
            for idx, pt in enumerate(pts):
                if (entry_joint and entry_joint_pt is not None
                        and ((phase == "entry" and idx == len(pts) - 1)
                             or (phase == "road" and idx == 0))):
                    continue
                if (joint and joint_pt is not None
                        and ((phase == "road" and idx == len(pts) - 1)
                             or (phase == "merge" and idx == 0))):
                    continue
                sp = tx(pt)
                rect = QRectF(sp.x() - r, sp.y() - r, 2 * r, 2 * r)
                self._pit_hit.append((rect, phase, idx))
                active = (self._pit_drag_idx == (phase, idx))
                p.setPen(QPen(col.darker(120), 1.5))
                p.setBrush(col if active else QColor(col.red(), col.green(),
                                                     col.blue(), 200))
                p.drawEllipse(rect)

        if entry_joint and entry_joint_pt is not None:
            jcol = QColor(255, 210, 80)
            sp = tx(entry_joint_pt)
            rect = QRectF(sp.x() - r, sp.y() - r, 2 * r, 2 * r)
            self._pit_hit.append((rect, "entry_joint", 0))
            active = self._pit_drag_idx == ("entry_joint", 0)
            p.setPen(QPen(jcol.darker(120), 1.5))
            p.setBrush(jcol if active else QColor(jcol.red(), jcol.green(),
                                                  jcol.blue(), 220))
            p.drawEllipse(rect)

        if joint and joint_pt is not None:
            jcol = QColor(255, 170, 50)
            sp = tx(joint_pt)
            rect = QRectF(sp.x() - r, sp.y() - r, 2 * r, 2 * r)
            self._pit_hit.append((rect, "joint", 0))
            active = self._pit_drag_idx == ("joint", 0)
            p.setPen(QPen(jcol.darker(120), 1.5))
            p.setBrush(jcol if active else QColor(jcol.red(), jcol.green(),
                                                  jcol.blue(), 220))
            p.drawEllipse(rect)

    def _screen_to_layout(self, pos: QPointF) -> tuple[float, float]:
        """Map widget pixel coords to layout space (post mirror/rotate, pre raw undo)."""
        s = self._layout_scale or 1.0
        return (
            (pos.x() - self._layout_ox) / s,
            (pos.y() - self._layout_oy) / s,
        )

    def _screen_to_model(self, pos: QPointF) -> tuple[float, float]:
        """Map widget pixel coords to normalized track model space."""
        mx, my = self._screen_to_layout(pos)
        rot = self._layout_rot
        if rot == 90:
            x, y = -my, mx
        elif rot == 180:
            x, y = -mx, -my
        elif rot == 270:
            x, y = my, -mx
        else:
            x, y = mx, my
        if self._layout_mirror:
            x = -x
        return x, y

    def _pit_handle_at(self, pos: QPointF) -> tuple[str, int] | None:
        for rect, phase, idx in self._pit_hit:
            if rect.contains(pos):
                return phase, idx
        return None

    def _model_delta(self, dsx: float, dsy: float) -> tuple[float, float]:
        """Convert a screen-space drag delta to model-space offset."""
        s = self._layout_scale or 1.0
        x, y = dsx / s, dsy / s
        rot = self._layout_rot
        if rot == 90:
            x, y = -y, x
        elif rot == 180:
            x, y = -x, -y
        elif rot == 270:
            x, y = y, -x
        if self._layout_mirror:
            x = -x
        return x, y

    def _screen_delta(self, mx: float, my: float) -> tuple[float, float]:
        """Convert a model-space offset to screen-space pixels."""
        x, y = mx, my
        if self._layout_mirror:
            x = -x
        rot = self._layout_rot
        if rot == 90:
            x, y = y, -x
        elif rot == 180:
            x, y = -x, -y
        elif rot == 270:
            x, y = -y, x
        s = self._layout_scale or 1.0
        return x * s, y * s

    def _corner_at(self, pos: QPointF) -> int | None:
        for rect, idx in self._corner_hit:
            if rect.contains(pos):
                return idx
        return None

    def _sf_at(self, pos: QPointF) -> bool:
        return self._sf_hit is not None and self._sf_hit.contains(pos)

    def _set_start_finish_at_model(self, x: float, y: float) -> None:
        from tools.schematic_to_track import _pct_on_loop
        if not self.path:
            return
        frac = _pct_on_loop(self.path, (x, y))
        self.start_finish = (1.0 - frac) % 1.0

    def mousePressEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        if (self.sf_edit_mode and self.path
                and event.button() == Qt.MouseButton.LeftButton):
            if self._sf_at(event.position()):
                self._drag_sf = True
                self.setCursor(Qt.CursorShape.ClosedHandCursor)
                event.accept()
                return
        if (self.pit_edit_mode and self.path
                and event.button() == Qt.MouseButton.MiddleButton):
            self._begin_pit_pan(event.position())
            event.accept()
            return
        if (self.pit_edit_mode and self.path
                and event.button() == Qt.MouseButton.LeftButton
                and event.modifiers() & Qt.KeyboardModifier.ShiftModifier
                and self._pit_drag_idx is None):
            self._begin_pit_pan(event.position())
            event.accept()
            return
        if (self.pit_edit_mode and self.path
                and event.button() == Qt.MouseButton.LeftButton):
            hit = self._pit_handle_at(event.position())
            if hit is not None:
                self._pit_drag_idx = hit
                self.setCursor(Qt.CursorShape.ClosedHandCursor)
                event.accept()
                return
            x, y = self._screen_to_model(event.position())
            if self.pit_edit_phase == "entry":
                self._pit_edit_entry.append((x, y))
            elif self.pit_edit_phase == "road":
                if not self._pit_edit_road and self._pit_edit_entry:
                    self._pit_edit_road.append(self._pit_edit_entry[-1])
                self._pit_edit_road.append((x, y))
                if self._pit_edit_merge:
                    self._sync_pit_joint()
            else:
                if not self._pit_edit_merge and self._pit_edit_road:
                    self._pit_edit_merge.append(self._pit_edit_road[-1])
                self._pit_edit_merge.append((x, y))
            self.update()
            event.accept()
            return
        if (self.pit_edit_mode and self.path
                and event.button() == Qt.MouseButton.RightButton):
            self.pop_last_pit_edit_point()
            event.accept()
            return
        if (self.corner_edit_mode and self.path and event.button()
                == Qt.MouseButton.LeftButton):
            idx = self._corner_at(event.position())
            if idx is not None:
                self._drag_corner = idx
                self._drag_last = event.position()
                self.setCursor(Qt.CursorShape.ClosedHandCursor)
                event.accept()
                return
        super().mousePressEvent(event)

    def mouseMoveEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        if self._drag_sf:
            x, y = self._screen_to_model(event.position())
            self._set_start_finish_at_model(x, y)
            self.update()
            event.accept()
            return
        if self._pit_drag_idx is not None:
            phase, idx = self._pit_drag_idx
            x, y = self._screen_to_model(event.position())
            self._set_pit_edit_point(phase, idx, x, y)
            self.update()
            event.accept()
            return
        if self._pit_pan_active and self._pit_pan_origin is not None:
            pos = event.position()
            dx = pos.x() - self._pit_pan_origin.x()
            dy = pos.y() - self._pit_pan_origin.y()
            self._pit_edit_pan = (
                self._pit_pan_start[0] + dx,
                self._pit_pan_start[1] + dy,
            )
            self.update()
            event.accept()
            return
        if self.pit_edit_mode and self.path:
            hit = self._pit_handle_at(event.position())
            if hit is not None:
                self.setCursor(Qt.CursorShape.OpenHandCursor)
            elif event.modifiers() & Qt.KeyboardModifier.ShiftModifier:
                self.setCursor(Qt.CursorShape.OpenHandCursor)
            else:
                self.setCursor(Qt.CursorShape.CrossCursor)
        if self._drag_corner is not None and self._drag_last is not None:
            pos = event.position()
            dx = pos.x() - self._drag_last.x()
            dy = pos.y() - self._drag_last.y()
            self._drag_last = pos
            mx, my = self._model_delta(dx, dy)
            corners = list(self.display_corners())
            if 0 <= self._drag_corner < len(corners):
                pct, label, ox, oy = _parse_corner(corners[self._drag_corner])
                corners[self._drag_corner] = (pct, label, ox + mx, oy + my)
                self.set_corners(corners)
                self.update()
            event.accept()
            return
        if self.sf_edit_mode and self.path:
            over = self._sf_at(event.position())
            self.setCursor(Qt.CursorShape.OpenHandCursor if over
                           else Qt.CursorShape.ArrowCursor)
        if self.corner_edit_mode and self.path:
            idx = self._corner_at(event.position())
            self.setCursor(Qt.CursorShape.OpenHandCursor if idx is not None
                           else Qt.CursorShape.ArrowCursor)
        super().mouseMoveEvent(event)

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        if self._drag_sf and event.button() == Qt.MouseButton.LeftButton:
            self._drag_sf = False
            self.setCursor(Qt.CursorShape.OpenHandCursor if self.sf_edit_mode
                           else Qt.CursorShape.ArrowCursor)
            if self._sf_edit_cb:
                self._sf_edit_cb()
            event.accept()
            return
        if self._pit_drag_idx is not None and event.button() == Qt.MouseButton.LeftButton:
            self._pit_drag_idx = None
            self.setCursor(Qt.CursorShape.CrossCursor if self.pit_edit_mode
                           else Qt.CursorShape.ArrowCursor)
            if self._pit_edit_cb:
                self._pit_edit_cb()
            event.accept()
            return
        if (self._pit_pan_active
                and event.button() in (Qt.MouseButton.LeftButton,
                                       Qt.MouseButton.MiddleButton)):
            self._pit_pan_active = False
            self._pit_pan_origin = None
            self.setCursor(Qt.CursorShape.CrossCursor if self.pit_edit_mode
                           else Qt.CursorShape.ArrowCursor)
            event.accept()
            return
        if self._drag_corner is not None and event.button() == Qt.MouseButton.LeftButton:
            self._drag_corner = None
            self._drag_last = None
            self.setCursor(Qt.CursorShape.OpenHandCursor
                           if self.corner_edit_mode else Qt.CursorShape.ArrowCursor)
            if self._corner_edit_cb:
                self._corner_edit_cb()
            event.accept()
            return
        super().mouseReleaseEvent(event)

    def wheelEvent(self, event: QWheelEvent) -> None:  # noqa: N802
        if not (self.pit_edit_mode and self.path):
            super().wheelEvent(event)
            return
        delta = event.angleDelta().y()
        if delta == 0:
            delta = event.pixelDelta().y()
        if delta == 0:
            event.accept()
            return
        factor = 1.12 if delta > 0 else 1.0 / 1.12
        pos = event.position()
        mx, my = self._screen_to_layout(pos)
        self._pit_edit_zoom = max(
            _PIT_EDIT_ZOOM_MIN,
            min(_PIT_EDIT_ZOOM_MAX, self._pit_edit_zoom * factor),
        )
        new_scale = self._pit_edit_base_scale * self._pit_edit_zoom
        self._pit_edit_pan = (
            pos.x() - mx * new_scale - self._pit_edit_base_ox,
            pos.y() - my * new_scale - self._pit_edit_base_oy,
        )
        self.update()
        event.accept()

    def _draw_corners(self, p: QPainter, tx, corners=None) -> None:
        corners = self.display_corners() if corners is None else corners
        if not corners:
            self._corner_hit = []
            return
        fam = config.CFG.get("font_family", "Arial")
        sz = max(5, round(8 * config.text_scale_for("map")))
        p.setFont(QFont(fam, sz, QFont.Weight.Bold))
        fm = p.fontMetrics()
        cc = tx(self._centroid)
        asph = _mcfg().get("asphalt_width", 12)
        off = asph * 0.5 + sz + 8.0
        bh = fm.height() + 4
        self._corner_hit = []
        for idx, corner in enumerate(corners):
            pct, label, ox, oy = _parse_corner(corner)
            s = tx(self.path[self._index_for_pct(pct)])
            dx, dy = s.x() - cc.x(), s.y() - cc.y()
            ln = math.hypot(dx, dy) or 1.0
            ax = s.x() + dx / ln * off
            ay = s.y() + dy / ln * off
            if ox or oy:
                sx, sy = self._screen_delta(ox, oy)
                ax += sx
                ay += sy
            bw = max(bh, fm.horizontalAdvance(label) + 12)
            rect = QRectF(ax - bw / 2, ay - bh / 2, bw, bh)
            if self.corner_edit_mode:
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(QColor(255, 200, 60, 220 if idx == self._drag_corner
                                  else 160))
                p.drawRoundedRect(rect, 4, 4)
            else:
                draw_dark_cell(p, rect, "map", radius=4)
            p.setPen(_mcol("corner_text"))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)
            self._corner_hit.append((rect, idx))

    @staticmethod
    def _car_draw_sort_key(car) -> tuple[bool, bool]:
        speaking = len(car) >= 8 and bool(car[7])
        is_player = len(car) >= 5 and bool(car[4])
        return (speaking, is_player)

    @staticmethod
    def _car_dot_style(is_player: bool, on_pit: bool, on_route: bool,
                       pit_opacity: float = 0.45) -> tuple[float, bool, bool]:
        """Return (opacity, use_pit_fill, use_player_glow) for map car dots."""
        in_pit_lane = on_pit or on_route
        if is_player:
            return (1.0, False, True)
        if in_pit_lane:
            return (pit_opacity, True, False)
        return (1.0, False, False)

    @staticmethod
    def _draw_player_car_dot(p: QPainter, c: QPointF, r: float,
                             fill: QColor) -> None:
        """Soft glow halo plus bright double ring around the player dot."""
        glow = QColor(fill)
        glow.setAlpha(70)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(glow)
        p.drawEllipse(c, r + 6.0, r + 6.0)
        p.setBrush(fill)
        p.setPen(QPen(QColor(0, 0, 0), 2))
        p.drawEllipse(c, r, r)
        p.setPen(QPen(QColor(255, 255, 255), 2.4))
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawEllipse(c, r + 2.4, r + 2.4)

    def _draw_cars(self, p: QPainter, tx, mc: dict,
                   car_pts: dict[int, QPointF]) -> None:
        cc = tx(self._centroid)
        off = mc.get("asphalt_width", 12) * 0.85 + 3.0
        # Pit-car styling is the same for every car -- resolve it once.
        pit_opacity = max(0.05, min(1.0, mc.get("pit_dot_opacity", 0.45)))
        pit_fill = _mcol("pit_car")
        dot_frac = mc.get("dot_radius_frac", 0.05) or 0.05
        if dot_frac <= 0:
            dot_frac = 0.05
        rad_scale = max(0.2, min(4.0, dot_frac / 0.05))
        marker_slots = self._marker_slots_by_idx()
        show_status = mc.get("show_car_status", True)
        for car in sorted(self.cars, key=self._car_draw_sort_key):
            speaking = is_pace = False
            status_kind = None
            if len(car) >= 12:
                (idx, pct, label, color, is_player, on_route, on_pit,
                 speaking, is_pace, status_kind, _in_entry, _in_exit) = car
            elif len(car) >= 10:
                (idx, pct, label, color, is_player, on_route, on_pit,
                 speaking, is_pace, status_kind) = car
            elif len(car) >= 9:
                idx, pct, label, color, is_player, on_route, on_pit, speaking, is_pace = car
            elif len(car) >= 8:
                idx, pct, label, color, is_player, on_route, on_pit, speaking = car
            elif len(car) >= 7:
                idx, pct, label, color, is_player, on_route, on_pit = car
            else:
                idx, pct, label, color, is_player = car[:5]
                on_route = on_pit = False
            c = car_pts.get(idx)
            if c is None:
                cc = tx(self._centroid)
                schematic = is_schematic_pit_source(self.pit_source)
                c = self._resolve_car_point(tx, car, cc, off, schematic)
            if c is None:
                continue
            in_pit_lane = on_route or on_pit
            r = (12.5 if is_player else 9.0) * rad_scale
            if is_player and in_pit_lane:
                r *= 1.15
            slot = marker_slots.get(idx)
            if slot and not is_player:
                col_key = f"marker_{slot}"
                p.setOpacity(1.0)
                p.setPen(QPen(_mcol_def(col_key, "#ffffff"), 2.6))
                p.setBrush(Qt.BrushStyle.NoBrush)
                p.drawEllipse(c, r + 5.0, r + 5.0)
            opacity, use_pit_fill, use_player_glow = self._car_dot_style(
                is_player, on_pit, on_route, pit_opacity)
            p.setOpacity(opacity)
            if is_pace:
                fill = _mcol_def("pace_car", "#0b0e12")
            elif use_pit_fill:
                fill = pit_fill
            else:
                fill = QColor(color)
            if use_player_glow:
                self._draw_player_car_dot(p, c, r, fill)
            else:
                p.setBrush(fill)
                p.setPen(QPen(QColor(0, 0, 0), 1))
                p.drawEllipse(c, r, r)
            if speaking and not is_pace:
                self._draw_car_speaking(p, c, r)
            if show_status and status_kind and not is_pace:
                self._draw_car_status_badge(p, c, r, status_kind)
            show_label = is_player or is_pace or not slot
            self._draw_car_number_label(
                p, c, label, r=r,
                is_player=is_player,
                is_pace=is_pace, show=show_label)
            p.setOpacity(1.0)
