"""
Sector / lap timing -- current, last and best lap with live sector splits.

iRacing doesn't stream finished sector times directly, so SectorTimer derives
them from the player's lap-distance crossings: each time you pass a sector
boundary it records the split, tracks your best per sector, and rolls the lap
when you cross the start/finish line. The widget shows the running lap time, the
last/best laps, and a row of sector cells colored purple when you've just matched
your best for that sector.

Widget data dict (built by SectorTimer.snapshot()):
    cur_lap, last_lap, best_lap, predicted_lap
    sectors   list of {"time", "status", "active", "delta"} per sector
"""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import (cell_radius, col, draw_card, draw_dark_cell, draw_edge_band,
                     draw_metric_row, panel_pad)
from .fonts import data_font_bold, tabfont, tfont
from .formats import clock, sec, signed_delta

_SECTION = "sector_timing"


class SectorTimer:
    """Derives sector splits from lap-distance crossings (owned by the app)."""

    def __init__(self):
        self.starts: list[float] | None = None
        self.cur: list = []          # completed splits for the in-progress lap
        self.last: list = []         # previous lap's splits
        self.best: list = []         # best split seen per sector (personal)
        self.session_best: list = []  # fastest sector this session
        self.idx = 0                 # current sector index
        self._seg_start_t = 0.0      # lap time when the current sector began
        self._cur_lap_t = 0.0
        self._prev_pct = None

    def set_boundaries(self, starts) -> None:
        """Set sector start percentages (e.g. from SplitTimeInfo). Re-inits state
        if they changed (new track / session)."""
        if not starts:
            return
        s = sorted(float(x) for x in starts)
        if s != self.starts:
            self.starts = s
            self.cur, self.idx, self._seg_start_t, self._prev_pct = [], 0, 0.0, None

    def reset_session(self) -> None:
        self.session_best = []

    def update(self, pct, cur_lap_time, last_lap_time) -> None:
        if self.starts is None:
            self.starts = [i / 3.0 for i in range(3)]  # default 3 equal sectors
        if not isinstance(pct, (int, float)) or pct < 0:
            return
        if isinstance(cur_lap_time, (int, float)):
            self._cur_lap_t = cur_lap_time
        n = len(self.starts)

        # Lap rollover: big backward jump in lap distance = crossed start/finish.
        if self._prev_pct is not None and pct + 0.5 < self._prev_pct:
            if isinstance(last_lap_time, (int, float)) and last_lap_time > 0:
                self._finish_sector(last_lap_time - self._seg_start_t)
            self.last = self.cur[:] if self.cur else self.last
            self.cur, self.idx, self._seg_start_t = [], 0, 0.0
            self._prev_pct = pct
            return

        new_idx = 0
        for i in range(n):
            if pct >= self.starts[i]:
                new_idx = i
        if new_idx > self.idx and isinstance(cur_lap_time, (int, float)):
            self._finish_sector(cur_lap_time - self._seg_start_t)
            self._seg_start_t = cur_lap_time
            self.idx = new_idx
        self._prev_pct = pct

    def _finish_sector(self, t) -> None:
        i = len(self.cur)
        if not isinstance(t, (int, float)) or t <= 0:
            self.cur.append(None)
            return
        self.cur.append(t)
        while len(self.best) <= i:
            self.best.append(None)
        if self.best[i] is None or t < self.best[i]:
            self.best[i] = t
        while len(self.session_best) <= i:
            self.session_best.append(None)
        if self.session_best[i] is None or t < self.session_best[i]:
            self.session_best[i] = t

    def predicted_lap(self) -> float | None:
        """Sum of best sectors + current sector pace."""
        n = len(self.starts or [])
        if n <= 0:
            return None
        total = 0.0
        have = False
        for i in range(n):
            if i < len(self.cur):
                t = self.cur[i]
                if isinstance(t, (int, float)) and t > 0:
                    total += t
                    have = True
            elif i == self.idx:
                running = max(0.0, (self._cur_lap_t or 0.0) - self._seg_start_t)
                if running > 0:
                    total += running
                    have = True
            else:
                ref = None
                if i < len(self.session_best) and self.session_best[i]:
                    ref = self.session_best[i]
                elif i < len(self.best) and self.best[i]:
                    ref = self.best[i]
                if isinstance(ref, (int, float)) and ref > 0:
                    total += ref
                    have = True
        return total if have else None

    def snapshot(self, cur_lap, last_lap, best_lap, *, show_delta=False) -> dict:
        n = len(self.starts or [0, 0, 0])
        sectors = []
        for i in range(n):
            delta = None
            if i < len(self.cur):
                t = self.cur[i]
                best = self.best[i] if i < len(self.best) else None
                status = ("best" if (t is not None and best is not None
                                     and t <= best + 1e-6) else "done")
                if show_delta and t is not None and best is not None:
                    delta = t - best
                sectors.append({"time": t, "status": status, "active": False,
                                "delta": delta})
            elif i == self.idx:
                running = max(0.0, (self._cur_lap_t or 0.0) - self._seg_start_t)
                best = self.best[i] if i < len(self.best) else None
                if show_delta and best is not None and running > 0:
                    delta = running - best
                sectors.append({"time": running, "status": "running",
                                "active": True, "delta": delta})
            else:
                last = self.last[i] if i < len(self.last) else None
                sectors.append({"time": last, "status": "idle", "active": False,
                                "delta": None})
        return {
            "cur_lap": cur_lap,
            "last_lap": last_lap,
            "best_lap": best_lap,
            "predicted_lap": self.predicted_lap(),
            "sectors": sectors,
            "active_idx": self.idx,
        }


class SectorTimingWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(220, 120)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        data = data or {}
        if data == self.data:
            return
        self.data = data
        self.update()

    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section(_SECTION)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())
        d = self.data or {}
        cfg = config.CFG.get(_SECTION, {})
        card, radius = draw_card(p, w, h, _SECTION)
        data_bold = data_font_bold(_SECTION)

        pad = panel_pad(h)
        iw = card.width() - 2 * pad
        cur_h = h * 0.26 if cfg.get("show_predicted_lap") else h * 0.30
        p.setFont(tabfont(cur_h, bold=data_bold))
        p.setPen(col("text", _SECTION))
        p.drawText(QRectF(card.left() + pad, card.top() + pad, iw, cur_h),
                   Qt.AlignmentFlag.AlignCenter, clock(d.get("cur_lap")))
        if cfg.get("show_predicted_lap") and d.get("predicted_lap"):
            p.setFont(tfont(h * 0.11, bold=False))
            p.setPen(col("muted", _SECTION))
            p.drawText(QRectF(card.left() + pad, card.top() + pad + cur_h * 0.85,
                              iw, h * 0.08),
                       Qt.AlignmentFlag.AlignCenter,
                       f"Pred {clock(d.get('predicted_lap'))}")

        sub_top = card.top() + pad + (h * 0.34 if cfg.get("show_predicted_lap") else h * 0.30)
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            sub_h = fixed_rh
        else:
            sub_h = h * 0.18
            max_frac = float(cfg.get("max_row_height_frac", 0) or 0)
            if max_frac > 0:
                sub_h = min(sub_h, h * max_frac)
        sub_h = max(18.0, sub_h)
        sub = QRectF(card.left() + pad, sub_top, iw, sub_h)
        draw_edge_band(p, sub, "header_bg", _SECTION, bottom_line=True)
        half = sub.width() / 2
        self._pair(p, QRectF(sub.left(), sub.top(), half, sub.height()),
                   "LAST", clock(d.get("last_lap")), data_bold)
        self._pair(p, QRectF(sub.left() + half, sub.top(), half, sub.height()),
                   "BEST", clock(d.get("best_lap")), data_bold)

        sectors = d.get("sectors") or []
        if sectors:
            top = sub.bottom() + h * 0.04
            ch = card.bottom() - pad - top
            gap = iw * 0.03
            cw = (iw - gap * (len(sectors) - 1)) / len(sectors)
            x = card.left() + pad
            show_delta = cfg.get("show_sector_delta", False)
            for i, s in enumerate(sectors):
                self._cell(p, QRectF(x, top, cw, ch), i + 1, s, data_bold,
                           show_delta=show_delta)
                x += cw + gap

    def _pair(self, p, rect, label, value, data_bold) -> None:
        draw_metric_row(p, rect.adjusted(10, 0, -10, 0), label, value, _SECTION,
                        data_bold=data_bold)

    def _cell(self, p, rect, num, s, data_bold, *, show_delta=False) -> None:
        status = s.get("status")
        bg_key = {"best": "sec_best", "running": "sec_running",
                  "done": "sec_done"}.get(status, "sec_idle")
        inner = rect.adjusted(1, 1, -1, -1)
        rad = cell_radius(rect.height())
        if status in (None, "idle"):
            draw_dark_cell(p, inner, _SECTION, radius=rad)
        else:
            p.setPen(QPen(col("cell_border", _SECTION), 1))
            p.setBrush(col(bg_key, _SECTION))
            p.drawRoundedRect(inner, rad, rad)
        if s.get("active"):
            p.setPen(QPen(col("sec_running_edge", _SECTION), 1.6))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(inner, rad, rad)
        p.setFont(tfont(rect.height() * 0.26, bold=False))
        p.setPen(col("sec_text", _SECTION))
        p.drawText(QRectF(rect.left(), rect.top() + rect.height() * 0.08,
                          rect.width(), rect.height() * 0.40),
                   Qt.AlignmentFlag.AlignCenter, f"S{num}")
        p.setFont(tabfont(rect.height() * 0.34, bold=data_bold))
        p.drawText(QRectF(rect.left(), rect.center().y() - rect.height() * 0.05,
                          rect.width(), rect.height() * 0.40),
                   Qt.AlignmentFlag.AlignCenter, sec(s.get("time")))
        if show_delta:
            delta = s.get("delta")
            if isinstance(delta, (int, float)) and abs(delta) >= 0.005:
                p.setFont(tfont(rect.height() * 0.22, bold=False))
                dc = col("slower", _SECTION) if delta > 0 else col("faster", _SECTION)
                p.setPen(dc)
                p.drawText(QRectF(rect.left(), rect.bottom() - rect.height() * 0.32,
                                  rect.width(), rect.height() * 0.28),
                           Qt.AlignmentFlag.AlignCenter, signed_delta(delta, 2))
