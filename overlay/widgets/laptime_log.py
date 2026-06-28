"""
Laptime Log: a scrolling list of your most recent laps with a per-lap delta and
the track temperature at the time, styled to match the timing tables and dash.

The app feeds it pre-built rows in data["rows"] (newest first); each row is
{"lap": int, "time": str, "delta": float|None, "temp": str}. Delta is the signed
second difference against the configured baseline (previous lap or session best);
None renders as muted dashes (e.g. an out-lap or the baseline lap itself).
"""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QLinearGradient, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from . import icons
from .table import tfont

_VC_LEFT = Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft

# Column layout as fractions of the body width: LAP | TIME | DELTA | TEMP.
_COLS = (("lap", 0.16), ("time", 0.33), ("delta", 0.27), ("temp", 0.24))
_HEADERS = {"lap": "LAP", "time": "TIME", "delta": "DELTA", "temp": "TEMP."}


def _cfg() -> dict:
    return config.CFG["laptime_log"]


def _col(key: str) -> QColor:
    return config.qcolor(_cfg()["colors"][key])


class LaptimeLogWidget(QWidget):
    """Lists recent laps (newest at the top) with delta + track temp."""

    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict | None = None
        self.setMinimumSize(300, 200)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        if data == self.data:
            return
        self.data = data
        self.update()

    def paintEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        config.use_section("laptime_log")
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        cfg = _cfg()
        radius = max(10.0, h * cfg.get("corner_radius_frac", 0.05))

        grad = QLinearGradient(0, 0, 0, h)
        grad.setColorAt(0.0, _col("bg_top"))
        grad.setColorAt(1.0, _col("bg_bottom"))
        p.setBrush(grad)
        p.setPen(QPen(_col("border"), 1))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        rows = (self.data or {}).get("rows", [])
        pad = max(8.0, h * 0.03)
        show_header = cfg.get("show_header", True)
        header_h = max(22.0, h * 0.12) if show_header else 0.0
        body_top = pad + header_h
        body_h = h - body_top - pad
        n = max(1, cfg.get("rows", 8))
        row_h = body_h / n

        # Column x positions/widths from the fractional layout.
        inner_w = w - 2 * pad
        cells: dict[str, tuple[float, float]] = {}
        cx = pad
        for key, frac in _COLS:
            cw = inner_w * frac
            cells[key] = (cx, cw)
            cx += cw

        if show_header:
            self._draw_header(p, cells, pad, header_h)

        if not rows:
            p.setPen(_col("muted"))
            p.setFont(tfont(max(11.0, row_h * 0.4)))
            p.drawText(QRectF(pad, body_top, inner_w, body_h),
                       Qt.AlignmentFlag.AlignCenter, "WAITING FOR LAPS\u2026")
            return

        fs = row_h * cfg.get("font_scale", 0.42)
        for i, row in enumerate(rows[:n]):
            y = body_top + i * row_h
            if cfg.get("alt_row_shading", True) and i % 2 == 1:
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(_col("row_alt"))
                p.drawRect(QRectF(pad, y, inner_w, row_h))
            self._draw_row(p, row, cells, y, row_h, fs)

    def _draw_header(self, p, cells, pad, header_h) -> None:
        cfg = _cfg()
        fs = header_h * 0.4 * (cfg.get("header_font_scale", 1.0) or 1.0)
        p.setFont(tfont(max(8.0, fs)))
        p.setPen(_col("header"))
        for key, _frac in _COLS:
            x, cw = cells[key]
            p.drawText(QRectF(x + cw * 0.06, pad, cw, header_h),
                       _VC_LEFT, _HEADERS[key])

    def _draw_row(self, p, row, cells, y, row_h, fs) -> None:
        cfg = _cfg()
        lead = row_h * 0.10  # small left inset inside each cell

        # LAP
        x, cw = cells["lap"]
        p.setFont(tfont(fs))
        p.setPen(_col("text"))
        p.drawText(QRectF(x + lead, y, cw, row_h), _VC_LEFT,
                   str(row.get("lap", "")))

        # TIME
        x, cw = cells["time"]
        p.setFont(tfont(fs))
        p.setPen(_col("text"))
        p.drawText(QRectF(x + lead, y, cw, row_h), _VC_LEFT,
                   str(row.get("time", "\u2014")))

        # DELTA (green when faster, red when slower, muted dashes if unknown)
        x, cw = cells["delta"]
        delta = row.get("delta")
        p.setFont(tfont(fs))
        if not isinstance(delta, (int, float)):
            p.setPen(_col("muted"))
            text = "\u2013 \u2013 \u2013"
        else:
            p.setPen(_col("faster") if delta < 0 else _col("slower"))
            text = f"{'-' if delta < 0 else '+'}{abs(delta):.3f}"
        p.drawText(QRectF(x + lead, y, cw, row_h), _VC_LEFT, text)

        # TEMP. (optional thermometer icon + value)
        x, cw = cells["temp"]
        tx = x + lead
        if cfg.get("temp_icon", True) and icons.has("track_temp"):
            p.setFont(icons.icon_font(fs * 0.92))
            p.setPen(_col("muted"))
            glyph = icons.glyph("track_temp")
            gw = p.fontMetrics().horizontalAdvance(glyph)
            p.drawText(QRectF(tx, y, gw + 4, row_h), _VC_LEFT, glyph)
            tx += gw + fs * 0.4
        p.setFont(tfont(fs))
        p.setPen(_col("text"))
        p.drawText(QRectF(tx, y, cw, row_h), _VC_LEFT, str(row.get("temp", "\u2014")))
