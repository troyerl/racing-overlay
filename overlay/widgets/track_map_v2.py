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

from PyQt6.QtCore import Qt, QRectF, QTimer, pyqtSignal
from PyQt6.QtGui import QColor, QFont, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import (QFileDialog, QFrame, QHBoxLayout, QLabel,
                             QPushButton, QVBoxLayout, QWidget)

from .. import config
from . import track_map

# iRacing schematic legend colors (BGR order in OpenCV import; Qt uses RGB).
_SCHEMATIC_RED = "#d94040"
_SCHEMATIC_BLUE = "#3aa0ff"
_SCHEMATIC_WHITE = "#f0f0f0"


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
    """Map widget with schematic-track legend rendering."""

    def _paint_extras(self, p: QPainter, rect: QRectF) -> None:
        if self.pit_source != "schematic" or not self.path:
            return
        self._draw_schematic_legend(p)

    def _draw_pit(self, p: QPainter, tx) -> None:
        if self.pit_source == "schematic" and self.pit_path and len(self.pit_path) >= 2:
            self._draw_pit_schematic(p, tx)
            return
        super()._draw_pit(p, tx)

    def _inset_schematic(self, seg: list[tuple[float, float]],
                         frac: float = 0.022) -> list[tuple[float, float]]:
        """Nudge authored lanes slightly infield so they read on top of asphalt."""
        if not seg or not self.path:
            return seg
        cx, cy = self._centroid
        xs = [p[0] for p in self.path]
        ys = [p[1] for p in self.path]
        span = max(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
        d = span * frac
        out = []
        for x, y in seg:
            dx, dy = cx - x, cy - y
            ln = math.hypot(dx, dy) or 1.0
            out.append((x + dx / ln * d, y + dy / ln * d))
        return out

    def _draw_pit_schematic(self, p: QPainter, tx) -> None:
        """Pit geometry styled like the iRacing map key (dashed red/blue lanes)."""
        mc = config.CFG["map"]
        scale = self._layout_scale or 1.0
        lane_w = max(1.4, min(2.0, scale * 0.007))
        entry_w = max(1.8, min(2.4, lane_w * 1.15))
        merge_w = max(2.2, min(3.0, lane_w * 1.5))
        if mc.get("show_pit", True) and self.pit_path and len(self.pit_path) >= 2:
            path = self._inset_schematic(self.pit_path, 0.018)
            self._draw_schematic_lane(p, tx, path, _SCHEMATIC_RED, lane_w, dashed=True)
            self._draw_pit_label(p, tx(path[0]))
        if mc.get("show_pit_blends", True) and self.pit_in and len(self.pit_in) >= 2:
            self._draw_schematic_lane(
                p, tx, self._inset_schematic(self.pit_in, 0.022),
                _SCHEMATIC_RED, entry_w, dashed=True, outline=True)
        if mc.get("show_pit_blends", True) and self.pit_out and len(self.pit_out) >= 2:
            merge = self._inset_schematic(self.pit_out, 0.032)
            self._draw_schematic_lane(p, tx, merge, _SCHEMATIC_BLUE, merge_w,
                                      dashed=True, outline=True)

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
            pen.setDashPattern([6.0, 4.0])
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

    def _draw_schematic_legend(self, p: QPainter) -> None:
        """Compact key matching the iRacing schematic legend."""
        n_in = len(self.pit_in or [])
        n_path = len(self.pit_path or [])
        n_out = len(self.pit_out or [])
        items = (
            (_SCHEMATIC_WHITE, "Track"),
            (_SCHEMATIC_RED, f"Pit road ({n_path})"),
            (_SCHEMATIC_BLUE, f"Merge ({n_out})"),
        )
        if n_in >= 2:
            items = (
                (_SCHEMATIC_WHITE, "Track"),
                (_SCHEMATIC_RED, f"Entry ({n_in})"),
                (_SCHEMATIC_RED, f"Pit road ({n_path})"),
                (_SCHEMATIC_BLUE, f"Merge ({n_out})"),
            )
        fam = config.CFG.get("font_family", "Arial")
        p.setFont(QFont(fam, 8))
        fm = p.fontMetrics()
        line_h = fm.height() + 4
        pad = 8
        w = max(fm.horizontalAdvance(label) for _, label in items) + 28
        h = pad * 2 + line_h * len(items)
        x, y = 10.0, self.height() - h - 10.0
        rect = QRectF(x, y, w, h)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(QColor(20, 22, 26, 210))
        p.drawRoundedRect(rect, 6, 6)
        ty = y + pad + fm.ascent()
        for color, label in items:
            p.setPen(QPen(QColor(color), 2))
            p.drawLine(int(x + pad), int(ty - fm.ascent() // 2),
                       int(x + pad + 14), int(ty - fm.ascent() // 2))
            p.setPen(QColor(220, 224, 230))
            p.drawText(int(x + pad + 20), int(ty), label)
            ty += line_h


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
            "Import the in-sim track map PNG (white loop, red pit road, blue "
            "safe merge). Preview below; Import saves to the current TrackID "
            "and loads on the overlay map with v2 legend styling.")
        hint.setObjectName("enableHint")
        hint.setWordWrap(True)
        v.addWidget(hint)

        row = QHBoxLayout()
        self._path_lbl = QLabel("No file chosen")
        self._path_lbl.setObjectName("enableHint")
        self._path_lbl.setWordWrap(True)
        browse = QPushButton("Choose PNG…")
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
            self, "Choose iRacing schematic PNG", "",
            "Images (*.png *.jpg *.jpeg);;All files (*)")
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
        try:
            from tools.schematic_to_track import import_schematic
        except ImportError:
            self._status.setText(
                "Missing deps: pip install opencv-python-headless numpy")
            self._import_btn.setEnabled(False)
            return
        try:
            raw = import_schematic(path, num_corners=4)
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
