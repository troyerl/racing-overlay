"""Track map v2 — iRacing schematic legend styling and import UI.

TrackMapWidgetV2 extends the base map with pit lines drawn like the in-sim
schematic key (red pit road, blue safe merge) when ``pit_source == "schematic"``.

SchematicImportPanel is a Track Scan tab card: pick a PNG, preview the trace,
and write ``tracks/<TrackID>.json``.
"""

from __future__ import annotations

import json
import math
import os

from PyQt6.QtCore import Qt, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import (QFileDialog, QFrame, QHBoxLayout, QLabel,
                             QPushButton, QVBoxLayout, QWidget)

from .. import config
from . import track_map

# iRacing schematic legend colors (BGR order in OpenCV import; Qt uses RGB).
_SCHEMATIC_RED = "#d94040"
_SCHEMATIC_BLUE = "#3aa0ff"
_SCHEMATIC_DASH = [6.0, 4.0]


def apply_schematic_meta(widget: track_map.TrackMapWidget, meta: dict) -> None:
    """Push schema-2 schematic pit fields onto a map widget."""
    widget.set_pit_source(meta.get("pit_source"))
    span = meta.get("pit_span")
    if span:
        widget.set_pit(span, meta.get("pit_speed", 0.0))
    widget.set_pit_path(meta.get("pit_path") or [])
    widget.set_pit_blends(meta.get("pit_in"), meta.get("pit_out"))
    widget.set_pit_route_pct(meta.get("pit_in_pct"), meta.get("pit_out_pct"))


class TrackMapWidgetV2(track_map.TrackMapWidget):
    """Map widget with schematic-track pit lane styling."""

    def _draw_pit(self, p: QPainter, tx) -> None:
        if self.pit_source in track_map.SCHEMATIC_PIT_SOURCES and self.pit_path and len(self.pit_path) >= 2:
            self._draw_pit_schematic(p, tx)
            return
        super()._draw_pit(p, tx)

    def _nearest_loop_point(self, x: float, y: float) -> tuple[tuple[float, float], float]:
        """Return closest point on the closed loop polyline and its distance."""
        loop = self.path
        if not loop:
            return (x, y), 0.0
        best_q = loop[0]
        best_d = float("inf")
        n = len(loop)
        for i in range(n):
            a = loop[i]
            b = loop[(i + 1) % n]
            ax, ay = a
            bx, by = b
            dx, dy = bx - ax, by - ay
            if dx == 0 and dy == 0:
                qx, qy = ax, ay
            else:
                t = max(0.0, min(1.0, ((x - ax) * dx + (y - ay) * dy) / (dx * dx + dy * dy)))
                qx, qy = ax + t * dx, ay + t * dy
            d = math.hypot(x - qx, y - qy)
            if d < best_d:
                best_d = d
                best_q = (qx, qy)
        return best_q, best_d

    def _offset_from_loop(self, seg: list[tuple[float, float]],
                          frac: float | None = None) -> list[tuple[float, float]]:
        """Push schematic pit points away from the nearest loop edge."""
        if not seg or not self.path:
            return seg
        xs = [p[0] for p in self.path]
        ys = [p[1] for p in self.path]
        span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
        if frac is None:
            frac = config.CFG["map"].get("pit_lane_inset", 0.035)
        target = span * frac
        dirs: list[tuple[float, float]] = []
        for x, y in seg:
            q, _dist = self._nearest_loop_point(x, y)
            dx, dy = x - q[0], y - q[1]
            ln = math.hypot(dx, dy)
            if ln > 1e-9:
                dirs.append((dx / ln, dy / ln))
        if dirs:
            ux = sum(d[0] for d in dirs) / len(dirs)
            uy = sum(d[1] for d in dirs) / len(dirs)
            ln = math.hypot(ux, uy)
            if ln > 1e-9:
                ux, uy = ux / ln, uy / ln
            else:
                ux, uy = 0.0, 1.0
        else:
            ux, uy = 0.0, 1.0
        out: list[tuple[float, float]] = []
        for x, y in seg:
            q, dist = self._nearest_loop_point(x, y)
            if dist >= target:
                out.append((x, y))
                continue
            push = (target - dist) if dist > 1e-9 else target
            out.append((x + ux * push, y + uy * push))
        return out

    @staticmethod
    def _distance_to_polyline(
        pt: tuple[float, float], poly: list[tuple[float, float]],
    ) -> float:
        if len(poly) < 2:
            return float("inf")
        px, py = pt
        best = float("inf")
        for (ax, ay), (bx, by) in zip(poly, poly[1:]):
            dx, dy = bx - ax, by - ay
            if dx == 0 and dy == 0:
                d = math.hypot(px - ax, py - ay)
            else:
                t = max(0.0, min(1.0, ((px - ax) * dx + (py - ay) * dy)
                                    / (dx * dx + dy * dy)))
                qx, qy = ax + t * dx, ay + t * dy
                d = math.hypot(px - qx, py - qy)
            if d < best:
                best = d
        return best

    def _pit_draw_path(self, seg: list[tuple[float, float]]) -> list[tuple[float, float]]:
        """Inset schematic pit from loop unless import already placed it near the edge."""
        if not seg or not self.path:
            return seg
        mc = config.CFG["map"]
        frac = mc.get("pit_lane_inset", 0.035)
        xs = [p[0] for p in self.path]
        ys = [p[1] for p in self.path]
        span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
        target = span * frac
        dists = [self._nearest_loop_point(x, y)[1] for x, y in seg]
        if sum(dists) / len(dists) < target:
            return seg
        return self._offset_from_loop(seg)

    def _trim_merge_colinear_prefix(
        self, merge: list[tuple[float, float]],
    ) -> list[tuple[float, float]]:
        """Drop blue points that sit on the red pit straight backward of handoff."""
        if len(merge) < 2 or not self.pit_path or not self.path:
            return merge
        pit_path = [(p[0], p[1]) for p in self.pit_path]
        handoff = pit_path[0]
        d_entry = math.hypot(merge[0][0] - handoff[0], merge[0][1] - handoff[1])
        d_exit = math.hypot(merge[0][0] - pit_path[-1][0], merge[0][1] - pit_path[-1][1])
        if d_entry > d_exit:
            return merge
        xs = [p[0] for p in self.path]
        ys = [p[1] for p in self.path]
        span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
        y_eps = span * 0.008
        x_buf = span * 0.005
        lane_tol = span * 0.01
        out: list[tuple[float, float]] = []
        for p in merge:
            if (p[0] < handoff[0] - x_buf
                    and abs(p[1] - handoff[1]) <= y_eps
                    and self._distance_to_polyline(p, pit_path) < lane_tol):
                continue
            out.append(p)
        return out if len(out) >= 2 else merge

    def _draw_pit_schematic(self, p: QPainter, tx) -> None:
        """Pit geometry for schematic imports: solid red lane, solid blue merge."""
        mc = config.CFG["map"]
        scale = self._layout_scale or 1.0
        lane_w = max(1.4, min(2.0, scale * 0.007))
        merge_w = max(2.2, min(3.0, lane_w * 1.5))
        blends = mc.get("show_pit_blends", True)
        dashed = bool(mc.get("pit_lane_dashed", False))
        if mc.get("show_pit", True) and self.pit_path and len(self.pit_path) >= 2:
            path = self._pit_draw_path(self.pit_path)
            use_path_only = self.pit_source == "inactive"
            if blends and self.pit_in and len(self.pit_in) >= 2 and not use_path_only:
                handoff = self.pit_path[0]
                entry_ok = (
                    math.hypot(self.pit_in[0][0] - handoff[0],
                               self.pit_in[0][1] - handoff[1]) <= 0.12)
                if entry_ok:
                    entry = self._pit_draw_path(self.pit_in)
                    red_lane = list(entry) + path[1:]
                else:
                    red_lane = path
            else:
                red_lane = path
            self._draw_schematic_lane(
                p, tx, red_lane, _SCHEMATIC_RED, lane_w,
                dashed=dashed, outline=True)
        if blends and self.pit_out and len(self.pit_out) >= 2:
            merge = self._pit_draw_path(self.pit_out)
            merge = self._trim_merge_colinear_prefix(merge)
            self._draw_schematic_lane(
                p, tx, merge, _SCHEMATIC_BLUE, merge_w,
                dashed=dashed, outline=True)

    @staticmethod
    def _draw_schematic_lane(p: QPainter, tx, seg, color_hex: str, width: float,
                             *, dashed: bool = False, outline: bool = False) -> None:
        pts = [tx(q) for q in seg]
        if len(pts) < 2:
            return
        path = QPainterPath()
        path.moveTo(pts[0])
        for q in pts[1:]:
            path.lineTo(q)
        if outline:
            halo = QPen(QColor(12, 14, 18, 170), width + 1.6)
            halo.setCapStyle(Qt.PenCapStyle.RoundCap)
            halo.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.setPen(halo)
            op = max(0.0, min(1.0, config.CFG["map"].get("pit_lane_opacity", 1.0)))
            p.setOpacity(op)
            p.drawPath(path)
        pen = QPen(QColor(color_hex), width)
        if dashed:
            pen.setStyle(Qt.PenStyle.CustomDashLine)
            pen.setDashPattern(_SCHEMATIC_DASH)
        else:
            pen.setStyle(Qt.PenStyle.SolidLine)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(pen)
        op = max(0.0, min(1.0, config.CFG["map"].get("pit_lane_opacity", 1.0)))
        p.setOpacity(op)
        p.drawPath(path)
        p.setOpacity(1.0)


class SchematicImportPanel(QFrame):
    """Track Scan card: import an iRacing schematic PNG into tracks/<TrackID>.json."""

    imported = pyqtSignal(str)

    def __init__(self, overlay=None, parent=None):
        super().__init__(parent)
        self.setObjectName("enableCard")
        self._overlay = overlay
        self._pending_path: str | None = None
        self._pending_doc: dict | None = None

        v = QVBoxLayout(self)
        v.setContentsMargins(15, 11, 15, 12)
        v.setSpacing(8)

        t = QLabel("Schematic map (v2)")
        t.setObjectName("enableTitle")
        v.addWidget(t)

        hint = QLabel(
            "Import the iRacing members track map: save the track page HTML "
            "from DevTools (preferred — vector SVG layers, no API token), or "
            "a schematic PNG (white loop, red pit, blue merge). Preview "
            "below; Import saves to the current TrackID.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(hint)

        row = QHBoxLayout()
        self._path_lbl = QLabel("No file chosen")
        self._path_lbl.setObjectName("enableHint")
        self._path_lbl.setWordWrap(True)
        browse = QPushButton("Choose file…")
        browse.setCursor(Qt.CursorShape.PointingHandCursor)
        browse.clicked.connect(self._browse)
        row.addWidget(self._path_lbl, 1)
        row.addWidget(browse, 0)
        v.addLayout(row)

        self._preview = TrackMapWidgetV2()
        self._preview.setMinimumHeight(200)
        self._preview.setMaximumHeight(280)
        v.addWidget(self._preview)

        btn_row = QHBoxLayout()
        self._import_btn = QPushButton("Import && load track")
        self._import_btn.setEnabled(False)
        self._import_btn.setCursor(Qt.CursorShape.PointingHandCursor)
        self._import_btn.clicked.connect(self._import)
        btn_row.addWidget(self._import_btn)
        btn_row.addStretch(1)
        v.addLayout(btn_row)

        self._status = QLabel("")
        self._status.setObjectName("enableHint")
        self._status.setWordWrap(True)
        v.addWidget(self._status)

    def set_overlay(self, overlay) -> None:
        self._overlay = overlay

    def _browse(self) -> None:
        path, _ = QFileDialog.getOpenFileName(
            self, "Choose iRacing track map", "",
            "Track maps (*.html *.htm *.png *.jpg *.jpeg);;"
            "HTML (*.html *.htm);;Images (*.png *.jpg *.jpeg);;All files (*)")
        if not path:
            return
        QTimer.singleShot(0, lambda: self._load_preview(path))

    @staticmethod
    def _sanitize_points(raw) -> list[tuple[float, float]]:
        out: list[tuple[float, float]] = []
        for pt in raw or []:
            try:
                x, y = float(pt[0]), float(pt[1])
            except (TypeError, ValueError, IndexError):
                continue
            if not (math.isfinite(x) and math.isfinite(y)):
                continue
            out.append((x, y))
        return out

    def _load_preview(self, path: str) -> None:
        ext = os.path.splitext(path)[1].lower()
        try:
            if ext in (".png", ".jpg", ".jpeg"):
                from tools.schematic_to_track import import_schematic
                raw = import_schematic(path, num_corners=4)
            else:
                from tools.svg_layers_to_track import import_track_source
                raw = import_track_source(path, num_corners=4)
        except ImportError:
            self._status.setText(
                "PNG import needs: pip install opencv-python-headless numpy")
            self._import_btn.setEnabled(False)
            return
        except Exception as exc:
            self._status.setText(str(exc))
            self._import_btn.setEnabled(False)
            self._pending_path = None
            self._pending_doc = None
            return
        doc = {k: v for k, v in raw.items() if not str(k).startswith("_")}
        pts = self._sanitize_points(doc.get("points"))
        if len(pts) < 3:
            self._status.setText("Trace produced too few track points.")
            self._import_btn.setEnabled(False)
            return
        self._pending_path = path
        self._pending_doc = doc
        self._path_lbl.setText(os.path.basename(path))
        corners = [(c["pct"], c["label"]) for c in doc.get("corners", [])
                   if isinstance(c, dict)]
        self._preview.set_track(pts, doc.get("start_finish", 0.0), corners)
        apply_schematic_meta(self._preview, doc)
        tid = self._track_id()
        if tid is not None:
            self._import_btn.setEnabled(True)
            n_in = len(doc.get("pit_in") or [])
            n_path = len(doc.get("pit_path") or [])
            n_out = len(doc.get("pit_out") or [])
            self._status.setText(
                f"Ready for TrackID {tid} — pit road {n_path} pts, "
                f"entry {n_in} pts, merge {n_out} pts.")
        else:
            self._import_btn.setEnabled(False)
            self._status.setText(
                "Start overlay (demo or iRacing) so a TrackID is available.")

    def _track_id(self):
        if self._overlay is None:
            return None
        if hasattr(self._overlay, "effective_track_id"):
            return self._overlay.effective_track_id()
        return getattr(self._overlay, "_track_id", None)

    def _import(self) -> None:
        if not self._pending_doc or self._overlay is None:
            return
        doc = self._pending_doc
        png_path = self._pending_path

        def _run() -> None:
            try:
                ok, msg = self._overlay.import_schematic_track(
                    png_path, doc=doc)
            except Exception as exc:
                ok, msg = False, str(exc)
            self._status.setText(msg)
            if ok:
                self.imported.emit(msg)
            elif png_path and self._pending_doc:
                n_in = len(self._pending_doc.get("pit_in") or [])
                n_path = len(self._pending_doc.get("pit_path") or [])
                n_out = len(self._pending_doc.get("pit_out") or [])
                self._status.setText(
                    f"{msg}\nPreview still shows entry {n_in}, road {n_path}, "
                    f"merge {n_out} pts.")

        QTimer.singleShot(0, _run)

    def refresh(self) -> None:
        if self._pending_path and self._track_id() is not None:
            self._import_btn.setEnabled(True)
