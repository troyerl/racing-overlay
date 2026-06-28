"""
Input telemetry -- a scrolling throttle/brake/clutch trace.

A horizontal panel (styled like the dash/table cards) showing, left to right:

  * a vertical "TELEMETRY" tab with a colored accent bar,
  * a rolling line graph of the driver inputs over the last few seconds
    (throttle green, brake red, optional clutch blue), newest on the right,
  * thin vertical bars with the current value of each input (0..100), and
  * a round medallion with the current gear and speed.

Everything is configurable from config.CFG["inputs"]: which channels show, the
history length, which sections are visible, colors and a per-widget text scale.

Expected data dict (all optional; missing values render as 0 / "--"):
    throttle, brake, clutch   0..1 pedal travel
    gear                      gear number ("R"/"N"/1..)
    speed_ms                  speed in m/s (converted to mph/kph)
"""

from __future__ import annotations

from collections import deque

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import (QColor, QFont, QLinearGradient, QPainter, QPainterPath,
                         QPen)
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config


def _gear_str(g) -> str:
    if g is None:
        return "N"
    g = int(g)
    return "R" if g < 0 else ("N" if g == 0 else str(g))


class InputTraceWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        # Rolling history of (t_seconds, throttle, brake, clutch).
        self._hist: deque = deque()
        self._clock = QElapsedTimer()
        self._clock.start()
        self._font_cache: dict = {}
        self.setMinimumSize(360, 120)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    # -- data --------------------------------------------------------------
    def set_data(self, data: dict) -> None:
        self.data = data or {}
        t = self._clock.elapsed() / 1000.0

        def frac(key, default=0.0):
            v = self.data.get(key)
            return max(0.0, min(1.0, float(v))) if isinstance(v, (int, float)) else default

        # Sample: (t, throttle, brake, clutch, steering, abs). Steering is already
        # normalized 0..1 (0.5 = centered); abs is 1.0 while ABS is engaged.
        abs_on = 1.0 if self.data.get("abs_active") else 0.0
        self._hist.append((t, frac("throttle"), frac("brake"), frac("clutch"),
                           frac("steer", 0.5), abs_on))
        window = float(self._cfg().get("history_seconds", 6.0) or 6.0)
        cutoff = t - window
        while len(self._hist) > 2 and self._hist[0][0] < cutoff:
            self._hist.popleft()
        self.update()

    # -- helpers -----------------------------------------------------------
    def _cfg(self) -> dict:
        return config.CFG["inputs"]

    def _col(self, key: str) -> QColor:
        cols = self._cfg()["colors"]
        return config.qcolor(cols.get(key, "#ff00ff"))

    def _font(self, px: float, bold: bool = True) -> QFont:
        fam = config.CFG.get("font_family", "Segoe UI")
        pxi = max(6, int(round(px * config.text_scale_for("inputs"))))
        key = (fam, pxi, bold)
        f = self._font_cache.get(key)
        if f is None:
            f = QFont(fam)
            f.setStyleHint(QFont.StyleHint.SansSerif)
            f.setPixelSize(pxi)
            f.setBold(bold)
            if len(self._font_cache) > 128:
                self._font_cache.clear()
            self._font_cache[key] = f
        return f

    def _bar_channels(self, c) -> list:
        """[(data_index, color_key)] for the value bars (no steering bar)."""
        out = []
        for di, flag, colk in ((1, "show_throttle", "throttle"),
                               (2, "show_brake", "brake"),
                               (3, "show_clutch", "clutch")):
            if c.get(flag, di < 3):  # throttle + brake default on, clutch off
                out.append((di, colk))
        return out

    def _brake_threshold(self, c) -> float:
        """Threshold as a 0..1 fraction (config stores it as a 0..100 percent)."""
        if not c.get("show_brake_threshold", False):
            return 0.0
        return max(0.0, min(1.0, float(c.get("brake_threshold", 0) or 0) / 100.0))

    def _brake_color(self, value, abs_on, thr, c) -> QColor:
        """Brake color: ABS yellow > over-threshold orange > normal red."""
        if abs_on:
            return self._col("brake_abs")
        if thr > 0 and value > thr:
            return self._col("brake_over")
        return self._col("brake")

    # -- paint -------------------------------------------------------------
    def paintEvent(self, event) -> None:  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())
        c = self._cfg()

        # Panel card matching the other widgets.
        radius = max(8.0, min(w, h) * 0.12)
        grad = QLinearGradient(0, 0, 0, h)
        grad.setColorAt(0.0, self._col("bg_top"))
        grad.setColorAt(1.0, self._col("bg_bottom"))
        p.setBrush(grad)
        p.setPen(QPen(self._col("panel_border"), 1.2))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        pad = max(6.0, h * 0.08)
        left = pad
        right = w - pad
        gap = max(6.0, h * 0.06)
        chans = self._bar_channels(c)

        if c.get("show_label", True):
            left = self._draw_label(p, left, pad, h, c) + gap
        if c.get("show_gauge", True):
            gd = h - 2 * pad
            self._draw_gauge(p, QRectF(right - gd, pad, gd, gd), c)
            right -= gd + gap
        if c.get("show_bars", True) and chans:
            bw = max(14.0, h * 0.13)
            bgap = max(10.0, h * 0.12)
            block = len(chans) * bw + (len(chans) - 1) * bgap
            self._draw_bars(p, QRectF(right - block, pad, block, h - 2 * pad),
                            bw, bgap, chans, c)
            right -= block + gap
        if c.get("show_graph", True):
            self._draw_graph(p, QRectF(left, pad, max(1.0, right - left),
                                       h - 2 * pad), c)

    # -- sections ----------------------------------------------------------
    def _draw_label(self, p, x, pad, h, c) -> float:
        """Accent bar + rotated title; returns the x just past the label tab."""
        bar_w = max(3.0, h * 0.035)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(self._col("accent"))
        p.drawRoundedRect(QRectF(x, pad, bar_w, h - 2 * pad), bar_w / 2, bar_w / 2)
        text = str(c.get("label_text", "TELEMETRY"))
        p.setFont(self._font(max(8.0, h * 0.12), bold=True))
        p.setPen(self._col("label"))
        tab_w = max(14.0, h * 0.20)
        cx = x + bar_w + tab_w / 2
        length = h * 0.92
        p.save()
        p.translate(cx, h / 2)
        p.rotate(-90)
        p.drawText(QRectF(-length / 2, -tab_w / 2, length, tab_w),
                   Qt.AlignmentFlag.AlignCenter, text)
        p.restore()
        return x + bar_w + tab_w

    def _draw_graph(self, p, rect: QRectF, c) -> None:
        # Backing well + faint gridlines at 0 / 50 / 100%.
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(self._col("graph_bg"))
        p.drawRoundedRect(rect, 6, 6)
        p.setPen(QPen(self._col("grid"), 1.0))
        for fr in (0.0, 0.5, 1.0):
            y = rect.bottom() - fr * rect.height()
            p.drawLine(QPointF(rect.left(), y), QPointF(rect.right(), y))

        if len(self._hist) < 2:
            return
        window = float(c.get("history_seconds", 6.0) or 6.0)
        now = self._hist[-1][0]
        lw = float(c.get("line_width", 2.4) or 2.4)

        def to_pt(t, frac):
            x = rect.right() - min(1.0, (now - t) / window) * rect.width()
            y = rect.bottom() - frac * rect.height()
            return QPointF(x, y)

        p.setClipRect(rect)
        # Steady-color channels first; brake (which can change color) on top.
        if c.get("show_throttle", True):
            self._line(p, 1, self._col("throttle"), to_pt, lw)
        if c.get("show_clutch", False):
            self._line(p, 3, self._col("clutch"), to_pt, lw)
        if c.get("show_steering", False):
            self._line(p, 4, self._col("steering"), to_pt, lw)

        thr = self._brake_threshold(c)
        if thr > 0:
            y = rect.bottom() - thr * rect.height()
            pen = QPen(self._col("threshold"), 1.4)
            pen.setStyle(Qt.PenStyle.DashLine)
            p.setPen(pen)
            p.drawLine(QPointF(rect.left(), y), QPointF(rect.right(), y))

        if c.get("show_brake", True):
            self._brake_line(p, to_pt, lw, thr, c)
        p.setClipping(False)

    def _line(self, p, di, color, to_pt, lw) -> None:
        path = QPainterPath()
        for j, sample in enumerate(self._hist):
            pt = to_pt(sample[0], sample[di])
            path.moveTo(pt) if j == 0 else path.lineTo(pt)
        pen = QPen(color, lw)
        pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
        pen.setCapStyle(Qt.PenCapStyle.RoundCap)
        p.setPen(pen)
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawPath(path)

    def _brake_line(self, p, to_pt, lw, thr, c) -> None:
        # Drawn segment by segment so each piece can take the ABS / over-threshold
        # / normal color based on that sample's state.
        prev = None
        for sample in self._hist:
            cur = to_pt(sample[0], sample[2])
            if prev is not None:
                pen = QPen(self._brake_color(sample[2], sample[5] > 0.5, thr, c), lw)
                pen.setJoinStyle(Qt.PenJoinStyle.RoundJoin)
                pen.setCapStyle(Qt.PenCapStyle.RoundCap)
                p.setPen(pen)
                p.drawLine(prev, cur)
            prev = cur

    def _draw_bars(self, p, rect: QRectF, bw, bgap, chans, c) -> None:
        latest = self._hist[-1] if self._hist else (0, 0.0, 0.0, 0.0, 0.5, 0.0)
        abs_on = latest[5] > 0.5
        thr = self._brake_threshold(c)
        label_h = max(10.0, rect.height() * 0.16)
        track_top = rect.top() + label_h
        track_h = rect.height() - label_h
        font = self._font(max(8.0, rect.height() * 0.14), bold=True)
        x = rect.left()
        for di, colk in chans:
            val = max(0.0, min(1.0, latest[di]))
            fill = (self._brake_color(val, abs_on, thr, c) if di == 2
                    else self._col(colk))
            track = QRectF(x, track_top, bw, track_h)
            r = bw * 0.4
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(self._col("bar_track"))
            p.drawRoundedRect(track, r, r)
            fh = val * track_h
            if fh > 0:
                p.setBrush(fill)
                p.drawRoundedRect(QRectF(x, track_top + track_h - fh, bw, fh), r, r)
            p.setFont(font)
            p.setPen(self._col("text"))
            p.drawText(QRectF(x - bgap / 2, rect.top(), bw + bgap, label_h),
                       Qt.AlignmentFlag.AlignCenter, f"{val * 100:.0f}")
            x += bw + bgap

    def _draw_gauge(self, p, rect: QRectF, c) -> None:
        d = self.data or {}
        cx, cy = rect.center().x(), rect.center().y()
        rad = rect.width() / 2

        p.setBrush(self._col("gauge_bg"))
        p.setPen(QPen(self._col("gauge_ring"), max(2.0, rad * 0.10)))
        p.drawEllipse(rect.adjusted(rad * 0.06, rad * 0.06, -rad * 0.06, -rad * 0.06))

        # Small accent marker at the top of the ring (decorative, matches style).
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(self._col("text"))
        m = rad * 0.13
        marker = QPainterPath()
        top = QPointF(cx + rad * 0.62, rect.top() + rad * 0.30)
        marker.moveTo(top.x(), top.y() - m)
        marker.lineTo(top.x() + m, top.y())
        marker.lineTo(top.x(), top.y() + m)
        marker.lineTo(top.x() - m, top.y())
        marker.closeSubpath()
        p.drawPath(marker)

        # Gear (big), then unit + speed stacked below it.
        p.setPen(self._col("text"))
        p.setFont(self._font(rad * 0.95, bold=True))
        p.drawText(QRectF(cx - rad, cy - rad * 0.78, rad * 2, rad * 1.15),
                   Qt.AlignmentFlag.AlignCenter, _gear_str(d.get("gear")))
        speed = config.conv_speed(d.get("speed_ms"))
        p.setFont(self._font(rad * 0.26, bold=True))
        p.setPen(self._col("muted"))
        p.drawText(QRectF(cx - rad, cy + rad * 0.18, rad * 2, rad * 0.34),
                   Qt.AlignmentFlag.AlignCenter, config.speed_unit())
        p.setPen(self._col("text"))
        p.setFont(self._font(rad * 0.34, bold=True))
        p.drawText(QRectF(cx - rad, cy + rad * 0.46, rad * 2, rad * 0.40),
                   Qt.AlignmentFlag.AlignCenter,
                   f"{speed:.0f}" if speed is not None else "--")
