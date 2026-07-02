"""
2D track-map rendering for the overlay.

iRacing does NOT export live X/Y for other cars, so a 2D map is drawn by:
  1. Having a normalized track *path* (a closed loop of points, where position
     along the loop corresponds to lap distance percentage), and
  2. Placing each car onto that path by its CarIdxLapDistPct (0.0 -> 1.0).

Two ways to obtain the path:
  * Live: TrackPathBuilder learns the shape from the player's own GPS (Lat/Lon)
    over a single lap -- works on any track, no track database required.
  * Demo: build_demo_path() returns a built-in road-course curve so the map is
    visible immediately without iRacing.
"""

from __future__ import annotations

import json
import math
import os

from PyQt6.QtCore import QPointF, QRectF, Qt, QTimer
from PyQt6.QtGui import (QColor, QFont, QFontMetricsF, QMouseEvent, QPainter,
                         QPainterPath, QPen)
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .. import svgpath
from .chrome import draw_card, draw_dark_cell

SCHEMATIC_PIT_SOURCES = frozenset({"schematic", "inactive", "dashes", "manual"})


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


# Backwards-compatible module attributes (callers read these for car colors).
CAR_PALETTE = config.CFG["map"]["palette"]
PLAYER_COLOR = config.CFG["map"]["colors"]["player"]

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
    for ext in (".json", ".svg"):
        path = os.path.join(tracks_dir, f"{track_id}{ext}")
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
                      learned: bool = False) -> bool:
    """Create tracks/<id>.json from in-memory state when no local file exists.

    Authoring edits patch the on-disk file; this ensures one exists when the
    overlay is showing a track loaded only from the cloud or an in-session scan.
    """
    if track_id is None or not points or len(points) < 2:
        return False
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
    return True


def update_track_meta(tracks_dir: str, track_id, **fields) -> bool:
    """Patch extra keys (e.g. pit_span, pit_speed) into an existing track file.

    Used to record pit data learned after the geometry was already saved/loaded.
    A field whose value is None is removed from the file instead. No-op if the
    file doesn't exist yet -- call ``ensure_track_file`` first when authoring.
    """
    if track_id is None or not fields:
        return False
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
    return True


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
        self._centroid = (0.0, 0.0)
        # Pit lane: the (entry_pct, exit_pct) stretch the pit road runs alongside
        # and the learned speed limit (m/s), shown as a static badge.
        self.pit_span: tuple[float, float] | None = None
        self.pit_speed_ms: float = 0.0
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
        # Each car: (idx, lap_pct, label, color_hex, is_player, on_route, on_pit).
        self.cars: list[tuple[int, float, str, str, bool, bool, bool]] = []
        self.placeholder = "LEARNING TRACK\u2026  drive a lap"
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
        # Manual pit authoring (Track Scan v2): road then merge segments.
        self.pit_edit_mode = False
        self.pit_edit_phase = "road"
        self._pit_edit_road: list[tuple[float, float]] = []
        self._pit_edit_merge: list[tuple[float, float]] = []
        self._pit_drag_idx: tuple[str, int] | None = None
        self._pit_edit_cb = None
        self._pit_hit: list[tuple[QRectF, str, int]] = []
        self.setMouseTracking(True)
        self.setMinimumHeight(180)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_path(self, path) -> None:
        self.set_track(path, start_finish=0.0, corners=[])

    def set_progress(self, frac: float) -> None:
        """Update the 'learning track' percentage shown before a path exists."""
        pct = max(0, min(100, int(frac * 100)))
        if pct == self._progress_pct:
            return
        self._progress_pct = pct
        self.placeholder = f"LEARNING TRACK\u2026  {pct}%  \u00b7  drive a lap"
        if self.path is None:  # only the placeholder needs repainting
            self.update()

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
            self.update()

    def set_corners(self, corners) -> None:
        """Replace the displayed corner list (manual authoring)."""
        self.corners = [_parse_corner(c) for c in (corners or [])]
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
        if self.corners:
            return list(self.corners)
        return [(p, l, 0.0, 0.0) for p, l in self._auto_corners]

    def set_corner_edit(self, enabled: bool, callback=None) -> None:
        """Enable dragging corner labels on the map (write-access authoring)."""
        self.corner_edit_mode = bool(enabled)
        self._corner_edit_cb = callback if enabled else None
        self._drag_corner = None
        self._drag_last = None
        self.setCursor(Qt.CursorShape.OpenHandCursor if enabled
                       else Qt.CursorShape.ArrowCursor)
        self.update()

    def set_pit_edit(self, enabled: bool, callback=None) -> None:
        """Toggle click-to-draw pit road / merge on the live map."""
        self.pit_edit_mode = bool(enabled)
        self._pit_edit_cb = callback if enabled else None
        if not enabled:
            self._pit_drag_idx = None
        self.setCursor(Qt.CursorShape.CrossCursor if enabled
                       else Qt.CursorShape.ArrowCursor)
        self.update()

    def set_pit_edit_phase(self, phase: str) -> None:
        phase = (phase or "road").strip().lower()
        if phase not in ("road", "merge"):
            phase = "road"
        self.pit_edit_phase = phase
        self.update()

    def pit_edit_snapshot(self) -> tuple[list, list]:
        return (list(self._pit_edit_road), list(self._pit_edit_merge))

    def load_pit_edit(self, road, merge) -> None:
        self._pit_edit_road = [(float(x), float(y)) for x, y in (road or [])]
        self._pit_edit_merge = [(float(x), float(y)) for x, y in (merge or [])]
        self.update()

    def clear_pit_edit(self) -> None:
        self._pit_edit_road = []
        self._pit_edit_merge = []
        self._pit_drag_idx = None
        self.update()

    def pop_last_pit_edit_point(self) -> None:
        if self.pit_edit_phase == "merge" and self._pit_edit_merge:
            self._pit_edit_merge.pop()
        elif self._pit_edit_road:
            self._pit_edit_road.pop()
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
        self.update()

    def set_pit_blends(self, pit_in, pit_out) -> None:
        """Set (or clear) the entry/exit blend lines (model-space polylines)."""
        self.pit_in = self._clean_poly(pit_in)
        self.pit_out = self._clean_poly(pit_out)
        self._invalidate_route()
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

    def _pos_for_schematic_route(self, idx: int, pct: float, on_route: bool,
                                 on_pit_road: bool):
        """Place a car on authored pit polylines (schematic tracks only)."""
        if not on_route or not is_schematic_pit_source(self.pit_source):
            return None
        lo = self.pit_in_pct
        hi = self.pit_out_pct
        lane = self.pit_span
        exit_pct = self._schematic_exit_pcts.get(idx)
        if exit_pct is None and lane:
            exit_pct = lane[1]

        # Exit blend only: left pit road, still before rejoin.
        if (not on_pit_road and hi is not None and exit_pct is not None
                and self.pit_out
                and self._pct_in_interval(pct, exit_pct, hi)):
            span = (hi - exit_pct) % 1.0
            if span > 1e-6:
                t = ((pct - exit_pct) % 1.0) / span
                return self._pos_on_polyline(self.pit_out, min(max(t, 0.0), 1.0))

        # Entry + lane: on pit road or lap % within pit lane span.
        in_lane = on_pit_road
        if not in_lane and lane:
            in_lane = self._pct_in_interval(pct, lane[0], lane[1])
        if in_lane and self.path:
            chain = [s for s in (self.pit_in, self.pit_path) if s]
            if chain:
                loop_pt = self.path[self._index_for_pct(pct)]
                pos = self._closest_point_on_chain(chain, loop_pt)
                if pos is not None:
                    return pos
        if in_lane and lo is not None:
            end = lane[1] if lane else hi
            if end is None:
                end = hi
            span = (end - lo) % 1.0
            if span > 1e-6:
                t = ((pct - lo) % 1.0) / span
                t = min(max(t, 0.0), 1.0)
                segs = [s for s in (self.pit_in, self.pit_path) if s]
                return self._pos_on_polyline_chain(segs, t)
        return None

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

    def set_cars(self, cars) -> None:
        if cars == self.cars:  # nothing moved -> no repaint needed
            return
        self.cars = cars
        self.update()

    def _index_for_pct(self, pct: float) -> int:
        n = len(self.path)
        return int(((pct - self.start_finish) % 1.0) * n) % n

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

    def paintEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        p = QPainter(self)
        try:
            p.setRenderHint(QPainter.RenderHint.Antialiasing)
            rect = QRectF(self.rect())

            mc = _mcfg()
            # Rounded card behind the map so it matches the dash/table panels.
            if mc.get("show_panel", True) and "bg_top" in mc["colors"]:
                draw_card(p, rect.width(), rect.height(), "map")

            if not self.path:
                p.setPen(QColor(220, 220, 220))
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

            mpts = [model(pt) for pt in self.path]
            # Include the pit-lane geometry (lane + entry/exit blends) in the fit so
            # it isn't clipped if it runs a little outside the racing loop's bounds.
            fit = list(mpts)
            for seg in (self.pit_path, self.pit_in, self.pit_out):
                if seg:
                    fit.extend(model(pt) for pt in seg)
            if self.pit_edit_mode:
                for seg in (self._pit_edit_road, self._pit_edit_merge):
                    if seg:
                        fit.extend(model(pt) for pt in seg)
            xs = [m[0] for m in fit]
            ys = [m[1] for m in fit]
            minx, maxx, miny, maxy = min(xs), max(xs), min(ys), max(ys)
            pad = 26.0
            avail_w = rect.width() - 2 * pad
            avail_h = rect.height() - 2 * pad
            span_x = (maxx - minx) or 1e-6
            span_y = (maxy - miny) or 1e-6
            scale = min(avail_w / span_x, avail_h / span_y)
            ox = pad + (avail_w - span_x * scale) / 2 - minx * scale
            oy = pad + (avail_h - span_y * scale) / 2 - miny * scale
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

            # Background only fills the infield enclosed by the track loop.
            if mc.get("show_infield", True):
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(_mcol("infield"))
                p.drawPath(qpath)

            asphalt = QPen(_mcol("asphalt"), mc.get("asphalt_width", 11))
            asphalt.setCapStyle(Qt.PenCapStyle.RoundCap)
            asphalt.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.setPen(asphalt)
            p.drawPath(qpath)
            p.setPen(QPen(_mcol("outline"), mc.get("outline_width", 2)))
            p.drawPath(qpath)

            if mc.get("show_pit", True) and (self.pit_path or self.pit_in
                                             or self.pit_out
                                             or self.pit_span is not None):
                self._draw_pit(p, tx)
            if self.pit_edit_mode:
                self._draw_pit_edit(p, tx)
            if mc.get("show_corners", True):
                self._draw_corners(p, tx, self.display_corners())
            if mc.get("show_start_finish", True):
                self._draw_start_finish(p, tx)
            self._draw_cars(p, tx)
            if mc.get("show_wind", True) and self.wind_dir is not None:
                # Drop the compass in whichever corner the track intrudes on least,
                # so it stops sitting on top of the layout (e.g. road-course ends).
                step = max(1, len(self.path) // 180)
                scr = [tx(pt) for pt in self.path[::step]]
                self._draw_wind(p, rect, self._best_wind_corner(scr, rect))
            self._draw_scan_overlays(p, rect)
            self._paint_extras(p, rect)
        finally:
            if p.isActive():
                p.end()

    def _paint_extras(self, p: QPainter, rect: QRectF) -> None:
        """Hook for subclasses to draw overlays in the same paint pass."""

    def _draw_start_finish(self, p: QPainter, tx) -> None:
        sf_idx = self._index_for_pct(0.0)
        a = self.path[sf_idx]
        b = self.path[(sf_idx + 3) % len(self.path)]
        ax, ay = tx(a).x(), tx(a).y()
        bx, by = tx(b).x(), tx(b).y()
        dx, dy = bx - ax, by - ay
        ln = math.hypot(dx, dy) or 1.0
        nx, ny = -dy / ln, dx / ln
        p.setPen(QPen(QColor(255, 255, 255), 3))
        p.drawLine(
            QPointF(ax - nx * 7, ay - ny * 7),
            QPointF(ax + nx * 7, ay + ny * 7),
        )

    def _draw_pit(self, p: QPainter, tx) -> None:
        # Prefer the real recorded pit-lane geometry; fall back to the inward
        # offset approximation of pit_span when no geometry is available.
        if self.pit_path and len(self.pit_path) >= 2:
            # Blend lines first, so the lane reads on top where they join. Entry
            # is yellow, exit is blue; both hidden when show_pit_blends is off.
            if _mcfg().get("show_pit_blends", True):
                if self.pit_in and len(self.pit_in) >= 2:
                    self._draw_pit_blend(p, tx, self.pit_in, "pit_blend",
                                         "#ffd23a")
                if self.pit_out and len(self.pit_out) >= 2:
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
        off = mc.get("asphalt_width", 11) * 0.85 + 3.0
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
        base = QPen(_mcol("asphalt"), max(3.0, mc.get("asphalt_width", 11) * 0.6))
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

    @staticmethod
    def _wind_footprint(rect: QRectF) -> tuple[float, float]:
        """(width, height) the compass + its labels occupy in a corner."""
        r = max(13.0, min(rect.width(), rect.height()) * 0.06)
        return (2 * r + 40.0, 2 * r + 52.0)

    def _best_wind_corner(self, screen_pts, rect: QRectF) -> str:
        """Pick the widget corner the track covers least, so the compass doesn't
        sit on the layout. Ties prefer the original top-right, then top-left."""
        w, h = self._wind_footprint(rect)
        boxes = {
            "tr": QRectF(rect.right() - w, rect.top(), w, h),
            "tl": QRectF(rect.left(), rect.top(), w, h),
            "br": QRectF(rect.right() - w, rect.bottom() - h, w, h),
            "bl": QRectF(rect.left(), rect.bottom() - h, w, h),
        }
        counts = {k: sum(1 for q in screen_pts if box.contains(q))
                  for k, box in boxes.items()}
        order = ["tr", "tl", "br", "bl"]  # tie-break preference
        return min(order, key=lambda k: (counts[k], order.index(k)))

    def _draw_wind(self, p: QPainter, rect: QRectF, corner: str = "tr") -> None:
        """A small north-up compass in a corner: a ring with an 'N' tick and an
        arrow pointing the way the wind blows, plus the speed. ``corner`` is one
        of tr/tl/br/bl, chosen to avoid overlapping the track."""
        mc = _mcfg()
        r = max(13.0, min(rect.width(), rect.height()) * 0.06)
        mx = r + 30.0          # horizontal margin from the chosen side
        top_m = r + 28.0       # room for the dial + 'N' tick above center
        bot_m = r + 40.0       # room for the dial + speed badge below center
        cx = (rect.right() - mx) if corner in ("tr", "br") else (rect.left() + mx)
        cy = (rect.top() + top_m) if corner in ("tr", "tl") \
            else (rect.bottom() - bot_m)
        center = QPointF(cx, cy)
        col = _mcol("wind")

        # Dial.
        p.setBrush(QColor(10, 13, 17, 190))
        p.setPen(QPen(QColor(255, 255, 255, 40), 1))
        p.drawEllipse(center, r, r)

        # North tick at the top.
        fam = config.CFG.get("font_family", "Arial")
        nsz = max(6, round(7 * config.text_scale_for("map")))
        p.setFont(QFont(fam, nsz, QFont.Weight.Bold))
        p.setPen(QColor(170, 178, 188))
        p.drawText(QRectF(cx - r, cy - r - nsz - 1, 2 * r, nsz + 2),
                   Qt.AlignmentFlag.AlignCenter, "N")

        # Arrow points downwind (the way the wind pushes): bearing + pi. Screen
        # is north-up, so a bearing b maps to (sin b, -cos b).
        b = self.wind_dir + math.pi
        ux, uy = math.sin(b), -math.cos(b)
        px, py = -uy, ux  # perpendicular, for the arrow head
        tip = QPointF(cx + ux * r * 0.78, cy + uy * r * 0.78)
        tail = QPointF(cx - ux * r * 0.70, cy - uy * r * 0.70)
        p.setPen(QPen(col, max(2.0, r * 0.16), Qt.PenStyle.SolidLine,
                      Qt.PenCapStyle.RoundCap))
        p.drawLine(tail, tip)
        # Arrow head.
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

        # Speed label under the dial.
        spd = round(config.conv_speed(self.wind_speed_ms))
        text = f"{spd} {config.speed_unit()}"
        ssz = max(6, round(8 * config.text_scale_for("map")))
        p.setFont(QFont(fam, ssz, QFont.Weight.Bold))
        fm = p.fontMetrics()
        tw = fm.horizontalAdvance(text) + 8
        th = fm.height() + 2
        lr = QRectF(cx - tw / 2, cy + r + 2, tw, th)
        p.setBrush(QColor(10, 13, 17, 190))
        p.setPen(Qt.PenStyle.NoPen)
        p.drawRoundedRect(lr, 3, 3)
        p.setPen(_mcol("wind_text"))
        p.drawText(lr, Qt.AlignmentFlag.AlignCenter, text)

    def _draw_pit_edit(self, p: QPainter, tx) -> None:
        """In-progress pit road (red) and merge (blue) polylines + handles."""
        mc = _mcfg()
        self._pit_hit = []
        r = max(4.0, mc.get("asphalt_width", 11) * 0.35)

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

        _polyline(self._pit_edit_road, "pit", max(3.0, mc.get("asphalt_width", 11) * 0.55))
        _polyline(self._pit_edit_merge, "pit_blend_out",
                  max(2.5, mc.get("asphalt_width", 11) * 0.45))

        for phase, pts, col in (
            ("road", self._pit_edit_road, QColor(255, 90, 90)),
            ("merge", self._pit_edit_merge, QColor(90, 160, 255)),
        ):
            for idx, pt in enumerate(pts):
                sp = tx(pt)
                rect = QRectF(sp.x() - r, sp.y() - r, 2 * r, 2 * r)
                self._pit_hit.append((rect, phase, idx))
                active = (self._pit_drag_idx == (phase, idx))
                p.setPen(QPen(col.darker(120), 1.5))
                p.setBrush(col if active else QColor(col.red(), col.green(),
                                                     col.blue(), 200))
                p.drawEllipse(rect)

    def _screen_to_model(self, pos: QPointF) -> tuple[float, float]:
        """Map widget pixel coords to normalized track model space."""
        s = self._layout_scale or 1.0
        mx = (pos.x() - self._layout_ox) / s
        my = (pos.y() - self._layout_oy) / s
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

    def mousePressEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        if (self.pit_edit_mode and self.path
                and event.button() == Qt.MouseButton.LeftButton):
            hit = self._pit_handle_at(event.position())
            if hit is not None:
                self._pit_drag_idx = hit
                self.setCursor(Qt.CursorShape.ClosedHandCursor)
                event.accept()
                return
            x, y = self._screen_to_model(event.position())
            if self.pit_edit_phase == "road":
                self._pit_edit_road.append((x, y))
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
        if self._pit_drag_idx is not None:
            phase, idx = self._pit_drag_idx
            x, y = self._screen_to_model(event.position())
            pts = (self._pit_edit_road if phase == "road"
                   else self._pit_edit_merge)
            if 0 <= idx < len(pts):
                pts[idx] = (x, y)
                self.update()
            event.accept()
            return
        if self.pit_edit_mode and self.path:
            hit = self._pit_handle_at(event.position())
            self.setCursor(Qt.CursorShape.OpenHandCursor if hit is not None
                           else Qt.CursorShape.CrossCursor)
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
        if self.corner_edit_mode and self.path:
            idx = self._corner_at(event.position())
            self.setCursor(Qt.CursorShape.OpenHandCursor if idx is not None
                           else Qt.CursorShape.ArrowCursor)
        super().mouseMoveEvent(event)

    def mouseReleaseEvent(self, event: QMouseEvent) -> None:  # noqa: N802
        if self._pit_drag_idx is not None and event.button() == Qt.MouseButton.LeftButton:
            self._pit_drag_idx = None
            self.setCursor(Qt.CursorShape.CrossCursor if self.pit_edit_mode
                           else Qt.CursorShape.ArrowCursor)
            if self._pit_edit_cb:
                self._pit_edit_cb()
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
        asph = _mcfg().get("asphalt_width", 11)
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

    def _draw_cars(self, p: QPainter, tx) -> None:
        sz = max(5, round(8 * config.text_scale_for("map")))
        p.setFont(QFont(config.CFG.get("font_family", "Arial"), sz, QFont.Weight.Bold))
        cc = tx(self._centroid)
        mc = _mcfg()
        off = mc.get("asphalt_width", 11) * 0.85 + 3.0
        has_route = bool(self.pit_path and len(self.pit_path) >= 2)
        # Pit-car styling is the same for every car -- resolve it once.
        pit_opacity = max(0.05, min(1.0, mc.get("pit_dot_opacity", 0.45)))
        pit_fill = _mcol("pit_car")
        # Dot size scales off the configured radius (0.05 == the default size).
        # Treat a non-positive value (e.g. a stale 0.0 from before this setting
        # was honored) as the default rather than shrinking dots to nothing.
        dot_frac = mc.get("dot_radius_frac", 0.05) or 0.05
        if dot_frac <= 0:
            dot_frac = 0.05
        rad_scale = max(0.2, min(4.0, dot_frac / 0.05))
        # Draw the player last so it sits on top of traffic.
        schematic = is_schematic_pit_source(self.pit_source)
        for car in sorted(self.cars, key=lambda c: c[4]):
            if len(car) >= 7:
                idx, pct, label, color, is_player, on_route, on_pit = car
            else:
                idx, pct, label, color, is_player = car[:5]
                on_route = on_pit = False
            c = None
            if on_route:
                if schematic:
                    pos = self._pos_for_schematic_route(idx, pct, on_route, on_pit)
                    if pos is not None:
                        c = tx(pos)
                elif is_player and self.player_xy is not None:
                    c = tx(self.player_xy)
                elif self.pit_path and len(self.pit_path) >= 2:
                    t = self._route_t_for_pct(pct)
                    if t is not None:
                        pos = self._pos_on_route(t)
                        if pos is not None:
                            c = tx(pos)
            if c is None:
                c = tx(self.path[self._index_for_pct(pct)])
                # No real geometry: nudge an on-pit car toward the infield so it
                # reads as a separate lane beside the racing line.
                if on_pit:
                    dx, dy = c.x() - cc.x(), c.y() - cc.y()
                    ln = math.hypot(dx, dy) or 1.0
                    c = QPointF(c.x() - dx / ln * off, c.y() - dy / ln * off)
            r = (12.5 if is_player else 9.0) * rad_scale
            # Cars in the pits are grayed out and faded back.
            if on_route or on_pit:
                p.setOpacity(pit_opacity)
            fill = pit_fill if (on_route or on_pit) else QColor(color)
            # Make the player unmistakable: a soft glow halo plus a bright
            # double ring around a larger dot.
            if is_player and not on_route and not on_pit:
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
            else:
                p.setBrush(fill)
                p.setPen(QPen(QColor(0, 0, 0), 1))
                p.drawEllipse(c, r, r)
            p.setPen(QColor(20, 20, 20) if is_player else QColor(255, 255, 255))
            p.drawText(
                QRectF(c.x() - r, c.y() - r, 2 * r, 2 * r),
                Qt.AlignmentFlag.AlignCenter,
                label,
            )
            p.setOpacity(1.0)
