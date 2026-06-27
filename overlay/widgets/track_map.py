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

from PyQt6.QtCore import QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QFont, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .. import svgpath


def _mcfg() -> dict:
    return config.CFG["map"]


def _mcol(key: str) -> QColor:
    return config.qcolor(_mcfg()["colors"][key])


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


def build_demo_path(n: int = 720):
    return _resample_by_length(_catmull_rom_loop(_DEMO_CONTROL), n)


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


def load_track(path: str, n: int = 720):
    """Load a track file -> (points, start_finish_pct, corners, name)."""
    if path.lower().endswith(".svg"):
        with open(path, "r", encoding="utf-8") as fh:
            d = svgpath.first_path_d(fh.read())
        if not d:
            raise ValueError(f"No <path> found in {path}")
        raw = svgpath.flatten_path(d)
        points = _resample_by_length(raw, n)
        return points, 0.0, [], os.path.splitext(os.path.basename(path))[0]

    with open(path, "r", encoding="utf-8") as fh:
        data = json.load(fh)
    raw = [(float(a), float(b)) for a, b in data["points"]]
    points = _resample_by_length(raw, n)
    sf = float(data.get("start_finish", 0.0))
    corners = [(float(c["pct"]), str(c["label"])) for c in data.get("corners", [])]
    return points, sf, corners, data.get("name", "")


class TrackPathBuilder:
    """Learns a track path from the player's GPS (Lat/Lon) over a lap."""

    def __init__(self, bins: int = 720):
        self.bins = bins
        self._samples: list[tuple[float, float] | None] = [None] * bins
        self.ready = False
        self.path: list[tuple[float, float]] | None = None

    def add(self, pct, lat, lon) -> None:
        if pct is None or lat is None or lon is None:
            return
        if not (0.0 <= pct <= 1.0):
            return
        i = min(int(pct * self.bins), self.bins - 1)
        # Equirectangular projection to a local flat plane (good enough for a
        # single track). y is negated so North points up on screen.
        x = math.radians(lon) * math.cos(math.radians(lat))
        y = -math.radians(lat)
        self._samples[i] = (x, y)
        if not self.ready and sum(1 for s in self._samples if s) > self.bins * 0.9:
            self._build()

    def _build(self) -> None:
        n = self.bins
        filled = [i for i, s in enumerate(self._samples) if s]
        if not filled:
            return
        path: list[tuple[float, float]] = []
        for i in range(n):
            s = self._samples[i]
            if s:
                path.append(s)
                continue
            # Circular-interpolate across the nearest filled neighbours.
            back = next(j for j in (filled[::-1]) if j <= i) if any(
                j <= i for j in filled
            ) else filled[-1]
            fwd = next((j for j in filled if j >= i), filled[0])
            a, b = self._samples[back], self._samples[fwd]
            span = (fwd - back) % n or 1
            t = ((i - back) % n) / span
            path.append((a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t))
        self.path = path
        self.ready = True


class TrackMapWidget(QWidget):
    """Draws the track loop and places car dots by lap percentage."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.path: list[tuple[float, float]] | None = None
        self.start_finish = 0.0  # lap pct that path[0] corresponds to
        self.corners: list[tuple[float, str]] = []  # (lap_pct, label)
        self._centroid = (0.0, 0.0)
        # Each car: (lap_pct, label, color_hex, is_player)
        self.cars: list[tuple[float, str, str, bool]] = []
        self.placeholder = "LEARNING TRACK\u2026  drive a lap"
        self.setMinimumHeight(180)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_path(self, path) -> None:
        self.set_track(path, start_finish=0.0, corners=[])

    def set_track(self, path, start_finish: float = 0.0, corners=None) -> None:
        self.path = path
        self.start_finish = start_finish
        self.corners = corners or []
        if path:
            self._centroid = (
                sum(pt[0] for pt in path) / len(path),
                sum(pt[1] for pt in path) / len(path),
            )
        self.update()

    def set_cars(self, cars) -> None:
        if cars == self.cars:  # nothing moved -> no repaint needed
            return
        self.cars = cars
        self.update()

    def _index_for_pct(self, pct: float) -> int:
        n = len(self.path)
        return int(((pct - self.start_finish) % 1.0) * n) % n

    def paintEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        rect = QRectF(self.rect())

        if not self.path:
            p.setPen(QColor(220, 220, 220))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, self.placeholder)
            return

        xs = [pt[0] for pt in self.path]
        ys = [pt[1] for pt in self.path]
        minx, maxx, miny, maxy = min(xs), max(xs), min(ys), max(ys)
        pad = 26.0
        avail_w = rect.width() - 2 * pad
        avail_h = rect.height() - 2 * pad
        span_x = (maxx - minx) or 1e-6
        span_y = (maxy - miny) or 1e-6
        scale = min(avail_w / span_x, avail_h / span_y)
        ox = pad + (avail_w - span_x * scale) / 2 - minx * scale
        oy = pad + (avail_h - span_y * scale) / 2 - miny * scale

        def tx(pt):
            return QPointF(pt[0] * scale + ox, pt[1] * scale + oy)

        qpath = QPainterPath()
        qpath.moveTo(tx(self.path[0]))
        for pt in self.path[1:]:
            qpath.lineTo(tx(pt))
        qpath.closeSubpath()

        mc = _mcfg()
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

        if mc.get("show_corners", True):
            self._draw_corners(p, tx)
        if mc.get("show_start_finish", True):
            self._draw_start_finish(p, tx)
        self._draw_cars(p, tx)

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

    def _draw_corners(self, p: QPainter, tx) -> None:
        if not self.corners:
            return
        fam = config.CFG.get("font_family", "Arial")
        sz = max(5, round(8 * config.text_scale_for("map")))
        p.setFont(QFont(fam, sz, QFont.Weight.Bold))
        cxc, cyc = self._centroid
        for pct, label in self.corners:
            pt = self.path[self._index_for_pct(pct)]
            # Offset the label outward from the track centroid for legibility.
            ox, oy = pt[0] - cxc, pt[1] - cyc
            ln = math.hypot(ox, oy) or 1.0
            anchor = tx((pt[0] + ox / ln * 0.04, pt[1] + oy / ln * 0.04))
            rect = QRectF(anchor.x() - 16, anchor.y() - 9, 32, 18)
            p.setBrush(_mcol("corner_bg"))
            p.setPen(Qt.PenStyle.NoPen)
            p.drawRoundedRect(rect, 4, 4)
            p.setPen(_mcol("corner_text"))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)

    def _draw_cars(self, p: QPainter, tx) -> None:
        sz = max(5, round(8 * config.text_scale_for("map")))
        p.setFont(QFont(config.CFG.get("font_family", "Arial"), sz, QFont.Weight.Bold))
        # Draw the player last so it sits on top of traffic.
        for pct, label, color, is_player in sorted(self.cars, key=lambda c: c[3]):
            c = tx(self.path[self._index_for_pct(pct)])
            r = 11.0 if is_player else 9.0
            p.setBrush(QColor(color))
            p.setPen(QPen(QColor(0, 0, 0), 2 if is_player else 1))
            p.drawEllipse(c, r, r)
            p.setPen(QColor(20, 20, 20) if is_player else QColor(255, 255, 255))
            p.drawText(
                QRectF(c.x() - r, c.y() - r, 2 * r, 2 * r),
                Qt.AlignmentFlag.AlignCenter,
                label,
            )
