"""Weather panel — skies, rain, temps with trend, optional wind."""

from __future__ import annotations

import math

from PyQt6.QtCore import QPointF, QRectF, Qt
from PyQt6.QtGui import QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import col, draw_card
from .fonts import data_font_bold, tfont

_SECTION = "weather_panel"


class WeatherPanelWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(200, 120)
        self.setSizePolicy(QSizePolicy.Policy.Expanding,
                           QSizePolicy.Policy.Expanding)

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
        if not d and not d.get("edit"):
            pass
        draw_card(p, w, h, _SECTION)
        pad = max(6.0, h * 0.08)
        y = pad
        lh = (h - 2 * pad) / 5.0
        data_bold = data_font_bold(_SECTION)
        lines: list[tuple[str, str, str]] = []
        if cfg.get("show_skies", True):
            skies = d.get("skies") or "\u2014"
            hum = d.get("humidity")
            fog = d.get("fog")
            extra = []
            if hum is not None:
                extra.append(f"{int(hum)}% RH")
            if fog is not None:
                extra.append(f"Fog {float(fog):.0f}%")
            sub = "  ".join(extra) if extra else ""
            lines.append(("SKY", str(skies), sub))
        if cfg.get("show_rain", True):
            wet = d.get("track_wetness")
            rain = d.get("rain_intensity")
            parts = []
            if wet is not None:
                parts.append(f"Track {float(wet):.0f}%")
            if rain is not None:
                parts.append(f"Rain {float(rain):.0f}%")
            lines.append(("WET", "  ".join(parts) if parts else "\u2014", ""))
        if cfg.get("show_temps", True):
            tt = d.get("track_temp")
            at = d.get("air_temp")
            trend = d.get("track_trend")
            ts = []
            if tt is not None:
                t = config.conv_temp(tt)
                ts.append(f"T {t:.0f}\u00b0" if t is not None else "T --")
            if at is not None:
                a = config.conv_temp(at)
                ts.append(f"A {a:.0f}\u00b0" if a is not None else "A --")
            sub = ""
            if cfg.get("show_trend", True) and isinstance(trend, (int, float)):
                arrow = "\u25b2" if trend > 0.05 else ("\u25bc" if trend < -0.05 else "\u2014")
                sub = f"{arrow} {abs(trend):.1f}\u00b0"
            lines.append(("TEMP", "  ".join(ts) if ts else "\u2014", sub))
        if cfg.get("show_wind", False):
            wd = d.get("wind_dir")
            wv = d.get("wind_vel")
            ws = "\u2014"
            if isinstance(wv, (int, float)):
                ws = f"{config.conv_speed(wv):.0f} {config.speed_unit()}"
            lines.append(("WIND", ws, ""))

        for label, val, sub in lines[:5]:
            p.setFont(tfont(lh * 0.32, bold=True))
            p.setPen(col("header", _SECTION))
            p.drawText(QRectF(pad, y, w * 0.22, lh), Qt.AlignmentFlag.AlignLeft, label)
            p.setFont(tfont(lh * 0.34, bold=data_bold))
            p.setPen(col("text", _SECTION))
            p.drawText(QRectF(pad + w * 0.22, y, w * 0.55, lh),
                       Qt.AlignmentFlag.AlignLeft, val)
            if sub:
                p.setFont(tfont(lh * 0.30, bold=False))
                p.setPen(col("muted", _SECTION))
                p.drawText(QRectF(w - pad - w * 0.18, y, w * 0.16, lh),
                           Qt.AlignmentFlag.AlignRight, sub)
            y += lh

        if cfg.get("show_wind", False) and d.get("wind_dir") is not None:
            cx = w - pad - lh * 0.6
            cy = pad + lh * 0.5
            r = lh * 0.35
            wd = float(d["wind_dir"])
            ang = math.radians(wd)
            p.setPen(QPen(col("wind", _SECTION), 1.5))
            p.drawLine(QPointF(cx, cy),
                       QPointF(cx + r * math.sin(ang), cy - r * math.cos(ang)))
