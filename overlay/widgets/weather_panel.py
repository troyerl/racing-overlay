"""Weather panel — skies, rain, temps with trend, optional wind."""

from __future__ import annotations

import math

from PyQt6.QtCore import QPointF, QRectF, Qt
from PyQt6.QtGui import QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import (cell_radius, col, draw_card, draw_dark_cell, draw_metric_row,
                     draw_section_header, panel_pad, resolve_row_height)
from .fonts import data_font_bold, tfont

_SECTION = "weather_panel"
_PREVIEW = ("SKY", "WET", "TEMP", "WIND")


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
        card, radius = draw_card(p, w, h, _SECTION)
        pad = panel_pad(h)
        y = card.top() + pad
        data_bold = data_font_bold(_SECTION)
        if cfg.get("show_title", True):
            hh = max(22.0, h * 0.12)
            hdr = QRectF(card.left() + pad, y, card.width() - 2 * pad, hh)
            draw_section_header(p, hdr, str(cfg.get("title", "WEATHER")), _SECTION,
                                radius_top=radius)
            y += hh + pad * 0.35

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
            lines.append(("SKY", str(skies), "  ".join(extra) if extra else ""))
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
            wv = d.get("wind_vel")
            ws = "\u2014"
            if isinstance(wv, (int, float)):
                ws = f"{config.conv_speed(wv):.0f} {config.speed_unit()}"
            lines.append(("WIND", ws, ""))

        if not lines and d.get("edit"):
            lines = [(lbl, "\u2014", "") for lbl in _PREVIEW[:4]]

        body_h = max(card.bottom() - pad - y, 40.0)
        n = max(1, len(lines))
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            row_h = fixed_rh
        else:
            row_h = resolve_row_height(body_h=body_h, row_count=n, panel_h=h, cfg=cfg)
        row_h = max(18.0, row_h)
        rad = cell_radius(row_h)
        for label, val, sub in lines[:5]:
            rect = QRectF(card.left() + pad, y, card.width() - 2 * pad, row_h - 3)
            draw_dark_cell(p, rect, _SECTION, radius=rad)
            draw_metric_row(p, rect.adjusted(8, 0, -8, 0), label, val, _SECTION,
                            sub=sub, data_bold=data_bold)
            y += row_h

        if cfg.get("show_wind", False) and d.get("wind_dir") is not None:
            cx = card.right() - pad - row_h * 0.4
            cy = card.top() + pad + row_h * 0.55
            r = row_h * 0.25
            wd = float(d["wind_dir"])
            ang = math.radians(wd)
            p.setPen(QPen(col("wind", _SECTION), 1.5))
            p.drawLine(QPointF(cx, cy),
                       QPointF(cx + r * math.sin(ang), cy - r * math.cos(ang)))
