"""
Sector / lap timing -- current, last and best lap with live sector splits.

iRacing doesn't stream finished sector times directly, so SectorTimer derives
them from the player's lap-distance crossings: each time you pass a sector
boundary it records the split, tracks your best per sector, and rolls the lap
when you cross the start/finish line. The widget shows the running lap time, the
last/best laps, and a row of sector cells colored purple when you've just matched
your best for that sector.

Widget data dict (built by SectorTimer.snapshot()):
    cur_lap, last_lap, best_lap   lap times (seconds)
    sectors   list of {"time", "status", "active"} per sector
"""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QFont, QLinearGradient, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config


def _clock(sec) -> str:
    if not isinstance(sec, (int, float)) or sec <= 0:
        return "--:--.---"
    m = int(sec // 60)
    return f"{m}:{sec - m * 60:06.3f}"


def _sec(sec) -> str:
    if not isinstance(sec, (int, float)) or sec <= 0:
        return "--.-"
    return f"{sec:.1f}"


class SectorTimer:
    """Derives sector splits from lap-distance crossings (owned by the app)."""

    def __init__(self):
        self.starts: list[float] | None = None
        self.cur: list = []          # completed splits for the in-progress lap
        self.last: list = []         # previous lap's splits
        self.best: list = []         # best split seen per sector
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

    def snapshot(self, cur_lap, last_lap, best_lap) -> dict:
        n = len(self.starts or [0, 0, 0])
        sectors = []
        for i in range(n):
            if i < len(self.cur):
                t = self.cur[i]
                best = self.best[i] if i < len(self.best) else None
                status = ("best" if (t is not None and best is not None
                                     and t <= best + 1e-6) else "done")
                sectors.append({"time": t, "status": status, "active": False})
            elif i == self.idx:
                running = max(0.0, (self._cur_lap_t or 0.0) - self._seg_start_t)
                sectors.append({"time": running, "status": "running", "active": True})
            else:
                last = self.last[i] if i < len(self.last) else None
                sectors.append({"time": last, "status": "idle", "active": False})
        return {"cur_lap": cur_lap, "last_lap": last_lap, "best_lap": best_lap,
                "sectors": sectors}


class SectorTimingWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self._font_cache: dict = {}
        self.setMinimumSize(220, 120)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        self.data = data or {}
        self.update()

    def _cfg(self) -> dict:
        return config.CFG["sector_timing"]

    def _col(self, key: str) -> QColor:
        return config.qcolor(self._cfg()["colors"].get(key, "#ff00ff"))

    def _font(self, px: float, bold: bool = True) -> QFont:
        fam = config.CFG.get("font_family", "Segoe UI")
        pxi = max(6, int(round(px * config.text_scale_for("sector_timing"))))
        key = (fam, pxi, bold)
        f = self._font_cache.get(key)
        if f is None:
            f = QFont(fam)
            f.setStyleHint(QFont.StyleHint.SansSerif)
            f.setPixelSize(pxi)
            f.setBold(bold)
            if len(self._font_cache) > 64:
                self._font_cache.clear()
            self._font_cache[key] = f
        return f

    def paintEvent(self, event) -> None:  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())
        c = self._cfg()
        d = self.data or {}

        radius = max(8.0, min(w, h) * 0.10)
        grad = QLinearGradient(0, 0, 0, h)
        grad.setColorAt(0.0, self._col("bg_top"))
        grad.setColorAt(1.0, self._col("bg_bottom"))
        p.setBrush(grad)
        p.setPen(QPen(self._col("panel_border"), 1.2))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        pad = max(6.0, h * 0.08)
        iw = w - 2 * pad
        # Big current lap time on top.
        p.setFont(self._font(h * 0.30))
        p.setPen(self._col("text"))
        p.drawText(QRectF(pad, pad, iw, h * 0.30),
                   Qt.AlignmentFlag.AlignCenter, _clock(d.get("cur_lap")))

        # Last / best lap on a sub row.
        sub = QRectF(pad, pad + h * 0.30, iw, h * 0.18)
        half = sub.width() / 2
        self._pair(p, QRectF(sub.left(), sub.top(), half, sub.height()),
                   "LAST", _clock(d.get("last_lap")))
        self._pair(p, QRectF(sub.left() + half, sub.top(), half, sub.height()),
                   "BEST", _clock(d.get("best_lap")))

        # Sector cells.
        sectors = d.get("sectors") or []
        if sectors:
            top = pad + h * 0.52
            ch = h - pad - top
            gap = iw * 0.03
            cw = (iw - gap * (len(sectors) - 1)) / len(sectors)
            x = pad
            for i, s in enumerate(sectors):
                self._cell(p, QRectF(x, top, cw, ch), i + 1, s)
                x += cw + gap

    def _pair(self, p, rect, label, value) -> None:
        p.setFont(self._font(rect.height() * 0.62))
        p.setPen(self._col("muted"))
        p.drawText(rect, Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   f"  {label}")
        p.setPen(self._col("text"))
        p.drawText(rect, Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                   value + "  ")

    def _cell(self, p, rect, num, s) -> None:
        status = s.get("status")
        bg = {"best": "sec_best", "running": "sec_running",
              "done": "sec_done"}.get(status, "sec_idle")
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(self._col(bg))
        p.drawRoundedRect(rect, 6, 6)
        if s.get("active"):
            p.setPen(QPen(self._col("sec_running_edge"), 1.6))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(rect, 6, 6)
        p.setFont(self._font(rect.height() * 0.26))
        p.setPen(self._col("sec_text"))
        p.drawText(QRectF(rect.left(), rect.top() + rect.height() * 0.08,
                          rect.width(), rect.height() * 0.40),
                   Qt.AlignmentFlag.AlignCenter, f"S{num}")
        p.setFont(self._font(rect.height() * 0.34))
        p.drawText(QRectF(rect.left(), rect.center().y(),
                          rect.width(), rect.height() * 0.45),
                   Qt.AlignmentFlag.AlignCenter, _sec(s.get("time")))
