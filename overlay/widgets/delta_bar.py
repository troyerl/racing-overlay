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
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import col, draw_card
from .fonts import data_font_bold, tabfont
from .formats import signed_delta

_SECTION = "delta_bar"


class DeltaBarWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self._eased = 0.0
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0
        self._animating = False
        self.setMinimumSize(220, 90)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        self.data = data or {}
        self.update()

    def _cfg(self) -> dict:
        return config.CFG[_SECTION]

    def _dt(self) -> float:
        now = self._clock.elapsed()
        dt = (now - self._last_ms) / 1000.0
        self._last_ms = now
        return max(0.0, min(0.1, dt))

    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section(_SECTION)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())
        c = self._cfg()
        d = self.data or {}

        draw_card(p, w, h, _SECTION)

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
        data_bold = data_font_bold(_SECTION)
        if show_val:
            txt = signed_delta(delta, 2) if have else "--.--"
            tcol = (col("muted", _SECTION) if not have or abs(delta) < 0.005
                    else (col("faster", _SECTION) if delta < 0
                          else col("slower", _SECTION)))
            p.setFont(tabfont(h * 0.46, bold=data_bold))
            p.setPen(tcol)
            p.drawText(QRectF(pad, pad * 0.4, w - 2 * pad, h * 0.5),
                       Qt.AlignmentFlag.AlignCenter, txt)
            bar = QRectF(pad, h * 0.62, w - 2 * pad, h * 0.24)
        else:
            bar = QRectF(pad, h * 0.40, w - 2 * pad, h * 0.20)

        r = bar.height() / 2
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(col("track", _SECTION))
        p.drawRoundedRect(bar, r, r)
        cx = bar.center().x()

        if have and abs(self._eased) > 0.001:
            half = bar.width() / 2
            mag = abs(self._eased) * half
            if self._eased < 0:
                fill = QRectF(cx - mag, bar.top(), mag, bar.height())
                p.setBrush(col("faster", _SECTION))
            else:
                fill = QRectF(cx, bar.top(), mag, bar.height())
                p.setBrush(col("slower", _SECTION))
            p.drawRoundedRect(fill, r, r)

        from PyQt6.QtGui import QPen
        p.setPen(QPen(col("center", _SECTION), max(1.5, h * 0.02)))
        p.drawLine(QPointF(cx, bar.top() - h * 0.03),
                   QPointF(cx, bar.bottom() + h * 0.03))
