"""
Delta bar -- a big, standalone live time delta.

Shows how far ahead (faster, green) or behind (slower, red) you are versus a
reference lap, as both a large signed number and a center-anchored bar that
deflects left (faster) or right (slower). The reference is chosen with
`delta_bar.mode`: the session best, your own best, or iRacing's optimal lap.

Expected data dict:
    delta   seconds vs the reference (negative = faster), or None when unknown
"""

from __future__ import annotations

import math

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QLinearGradient, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config


class DeltaBarWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self._eased = 0.0
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0
        self._animating = False
        self._font_cache: dict = {}
        self.setMinimumSize(220, 90)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        self.data = data or {}
        self.update()

    def _cfg(self) -> dict:
        return config.CFG["delta_bar"]

    def _col(self, key: str) -> QColor:
        return config.qcolor(self._cfg()["colors"].get(key, "#ff00ff"))

    def _font(self, px: float, bold: bool = True):
        from PyQt6.QtGui import QFont
        fam = config.CFG.get("font_family", "Segoe UI")
        pxi = max(6, int(round(px * config.text_scale_for("delta_bar"))))
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

    def _dt(self) -> float:
        now = self._clock.elapsed()
        dt = (now - self._last_ms) / 1000.0
        self._last_ms = now
        return max(0.0, min(0.1, dt))

    def paintEvent(self, event) -> None:  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())
        c = self._cfg()
        d = self.data or {}

        radius = max(8.0, min(w, h) * 0.14)
        grad = QLinearGradient(0, 0, 0, h)
        grad.setColorAt(0.0, self._col("bg_top"))
        grad.setColorAt(1.0, self._col("bg_bottom"))
        p.setBrush(grad)
        p.setPen(QPen(self._col("panel_border"), 1.2))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        delta = d.get("delta")
        have = isinstance(delta, (int, float))
        rng = float(c.get("range", 1.0) or 1.0)
        target = max(-1.0, min(1.0, (delta / rng) if have and rng > 0 else 0.0))
        self._eased = self._eased + (target - self._eased) * (
            1.0 - math.exp(-self._dt() / 0.12))
        self._animating = abs(self._eased - target) > 0.004
        if self._animating:
            self.update()

        pad = max(6.0, h * 0.12)
        show_val = c.get("show_value", True)
        # Numeric readout takes the top ~55%, the bar sits beneath it.
        if show_val:
            txt = f"{delta:+.2f}" if have else "--.--"
            col = (self._col("muted") if not have or abs(delta) < 0.005
                   else (self._col("faster") if delta < 0 else self._col("slower")))
            p.setFont(self._font(h * 0.46))
            p.setPen(col)
            p.drawText(QRectF(pad, pad * 0.4, w - 2 * pad, h * 0.5),
                       Qt.AlignmentFlag.AlignCenter, txt)
            bar = QRectF(pad, h * 0.62, w - 2 * pad, h * 0.24)
        else:
            bar = QRectF(pad, h * 0.40, w - 2 * pad, h * 0.20)

        # Track + center tick.
        r = bar.height() / 2
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(self._col("track"))
        p.drawRoundedRect(bar, r, r)
        cx = bar.center().x()

        # Fill from center toward the side matching the sign.
        if have and abs(self._eased) > 0.001:
            half = bar.width() / 2
            mag = abs(self._eased) * half
            if self._eased < 0:  # faster -> grow left
                fill = QRectF(cx - mag, bar.top(), mag, bar.height())
                p.setBrush(self._col("faster"))
            else:                # slower -> grow right
                fill = QRectF(cx, bar.top(), mag, bar.height())
                p.setBrush(self._col("slower"))
            p.drawRoundedRect(fill, r, r)

        p.setPen(QPen(self._col("center"), max(1.5, h * 0.02)))
        p.drawLine(QPointF(cx, bar.top() - h * 0.03),
                   QPointF(cx, bar.bottom() + h * 0.03))
