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
from PyQt6.QtGui import QFontMetricsF, QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from . import icons
from .chrome import col, draw_card, draw_dark_cell, draw_edge_band, draw_row_divider
from .fonts import data_font_bold, tabfont, tfont

_VC_LEFT = Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft
_SECTION = "laptime_log"

# Column layout as fractions of the body width: LAP | TIME | DELTA | TEMP.
_COLS = (("lap", 0.16), ("time", 0.33), ("delta", 0.27), ("temp", 0.24))
_HEADERS = {"lap": "LAP", "time": "TIME", "delta": "DELTA", "temp": "TEMP."}


def _cfg() -> dict:
    return config.CFG[_SECTION]


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
        config.use_section(_SECTION)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        cfg = _cfg()
        card, radius = draw_card(p, w, h, _SECTION)

        rows = (self.data or {}).get("rows", [])
        pad = max(8.0, h * 0.03)
        show_header = cfg.get("show_header", True)
        header_h = max(22.0, h * 0.12) if show_header else 0.0
        body_top = card.top() + pad + header_h
        body_h = h - body_top - pad
        n = max(1, cfg.get("rows", 8))
        row_h = body_h / n
        inner_w = card.width() - 2 * pad
        inner_x = card.left() + pad

        cells: dict[str, tuple[float, float]] = {}
        cx = inner_x
        for key, frac in _COLS:
            cw = inner_w * frac
            cells[key] = (cx, cw)
            cx += cw

        if show_header:
            band = QRectF(card.left(), card.top(), card.width(), pad + header_h)
            draw_edge_band(p, band, "header_bg", _SECTION, bottom_line=True,
                           radius_top=radius)
            self._draw_header(p, cells, inner_x, card.top() + pad, header_h)

        if not rows:
            p.setPen(col("muted", _SECTION))
            p.setFont(tfont(max(11.0, row_h * 0.4)))
            p.drawText(QRectF(inner_x, body_top, inner_w, body_h),
                       Qt.AlignmentFlag.AlignCenter, "WAITING FOR LAPS\u2026")
            return

        fs = row_h * cfg.get("font_scale", 0.42)
        data_bold = data_font_bold(_SECTION)
        shown = rows[:n]
        for i, row in enumerate(shown):
            y = body_top + i * row_h
            if cfg.get("alt_row_shading", True) and i % 2 == 1:
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(col("row_alt", _SECTION))
                p.drawRect(QRectF(inner_x, y, inner_w, row_h))
            self._draw_row(p, row, cells, y, row_h, fs, data_bold)
            if cfg.get("row_dividers", True) and i < len(shown) - 1:
                draw_row_divider(p, inner_x, y + row_h, inner_w, _SECTION)

    def _draw_header(self, p, cells, x0, y, header_h) -> None:
        cfg = _cfg()
        fs = header_h * 0.4 * (cfg.get("header_font_scale", 1.0) or 1.0)
        p.setFont(tfont(max(8.0, fs), bold=False))
        p.setPen(col("header", _SECTION))
        for key, _frac in _COLS:
            x, cw = cells[key]
            p.drawText(QRectF(x + cw * 0.06, y, cw, header_h),
                       _VC_LEFT, _HEADERS[key])

    def _draw_row(self, p, row, cells, y, row_h, fs, data_bold) -> None:
        cfg = _cfg()
        lead = row_h * 0.10

        x, cw = cells["lap"]
        p.setFont(tfont(fs, bold=True))
        p.setPen(col("text", _SECTION))
        p.drawText(QRectF(x + lead, y, cw, row_h), _VC_LEFT,
                   str(row.get("lap", "")))

        x, cw = cells["time"]
        p.setFont(tabfont(fs, bold=data_bold))
        p.setPen(col("text", _SECTION))
        p.drawText(QRectF(x + lead, y, cw, row_h), _VC_LEFT,
                   str(row.get("time", "\u2014")))

        x, cw = cells["delta"]
        delta = row.get("delta")
        cell = QRectF(x + lead, y + row_h * 0.18, cw - lead * 2, row_h * 0.64)
        draw_dark_cell(p, cell, _SECTION, radius=3)
        if not isinstance(delta, (int, float)):
            p.setFont(tabfont(fs * 0.9, bold=False))
            p.setPen(col("muted", _SECTION))
            text = "\u2013 \u2013 \u2013"
        else:
            p.setFont(tabfont(fs * 0.9, bold=data_bold))
            p.setPen(col("faster", _SECTION) if delta < 0 else col("slower", _SECTION))
            text = f"{'-' if delta < 0 else '+'}{abs(delta):.3f}"
        p.drawText(cell, Qt.AlignmentFlag.AlignCenter, text)

        x, cw = cells["temp"]
        tx = x + lead
        if cfg.get("temp_icon", True) and icons.has("track_temp"):
            p.setFont(icons.icon_font(fs * 0.92))
            p.setPen(col("muted", _SECTION))
            glyph = icons.glyph("track_temp")
            gw = QFontMetricsF(p.font()).horizontalAdvance(glyph)
            p.drawText(QRectF(tx, y, gw + 4, row_h), _VC_LEFT, glyph)
            tx += gw + fs * 0.4
        p.setFont(tfont(fs, bold=False))
        p.setPen(col("text", _SECTION))
        p.drawText(QRectF(tx, y, cw, row_h), _VC_LEFT, str(row.get("temp", "\u2014")))
