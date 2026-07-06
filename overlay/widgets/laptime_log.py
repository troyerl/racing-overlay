"""
Laptime Log: a scrolling list of your most recent laps with a per-lap delta and
the track temperature at the time, styled to match the timing tables and dash.

The app feeds it pre-built rows in data["rows"] (newest first); each row is a
dict keyed by column id. Delta is the signed second difference against the
configured baseline; None renders as muted dashes.
"""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from . import icons
from .chrome import col, draw_card, draw_dark_cell, draw_edge_band, draw_row_divider, resolve_row_height
from .fonts import data_font_bold, tabfont, tfont
from .formats import signed_delta

_SECTION = "laptime_log"

_HEADERS = {
    "lap": "LAP", "time": "TIME", "delta": "DELTA", "temp": "TEMP.",
    "sectors": "SECT", "fuel": "FUEL", "tires": "TIRE", "incidents": "INC",
    "tag": "TAG",
}
_WIDTHS = {
    "lap": 0.10, "time": 0.22, "delta": 0.14, "temp": 0.14,
    "sectors": 0.18, "fuel": 0.10, "tires": 0.08, "incidents": 0.08,
    "tag": 0.08,
}


def _cfg() -> dict:
    return config.CFG[_SECTION]


def _col_layout(order: list[str]) -> list[tuple[str, float]]:
    weights = [_WIDTHS.get(k, 0.12) for k in order]
    total = sum(weights) or 1.0
    return [(k, w / total) for k, w in zip(order, weights)]


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
        order = (self.data or {}).get("columns") or config.laptime_log_column_order()
        cols = _col_layout(order)
        pad = max(8.0, h * 0.03)
        show_header = cfg.get("show_header", True)
        hscale = max(0.3, cfg.get("header_font_scale", 1.0) or 1.0)
        n = max(1, cfg.get("rows", 8))
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            row_h = fixed_rh
            header_h = round(fixed_rh * 1.1 * hscale) if show_header else 0.0
        else:
            header_h = max(22.0, h * 0.12) if show_header else 0.0
            body_top_est = card.top() + pad + header_h
            est_body_h = h - body_top_est - pad
            row_h = resolve_row_height(body_h=est_body_h, row_count=n,
                                       panel_h=h, cfg=cfg)
        body_top = card.top() + pad + header_h
        inner_w = card.width() - 2 * pad
        inner_x = card.left() + pad

        cells: dict[str, tuple[float, float]] = {}
        cx = inner_x
        for key, frac in cols:
            cw = inner_w * frac
            cells[key] = (cx, cw)
            cx += cw

        if show_header:
            hdr = QRectF(inner_x, card.top() + pad, inner_w, header_h)
            draw_edge_band(p, hdr, "header_bg", _SECTION, bottom_line=True,
                           radius_top=radius)
            p.setFont(tfont(header_h * 0.42, bold=True))
            p.setPen(col("header", _SECTION))
            for key, (x, cw) in cells.items():
                p.drawText(QRectF(x, hdr.top(), cw, hdr.height()),
                           Qt.AlignmentFlag.AlignCenter, _HEADERS.get(key, key.upper()))

        data_bold = data_font_bold(_SECTION)
        shown = rows[:n]
        for i, row in enumerate(shown):
            y = body_top + i * row_h
            row_rect = QRectF(inner_x, y, inner_w, row_h)
            if cfg.get("alt_row_shading", True) and i % 2 == 1:
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(col("row_alt", _SECTION))
                p.drawRect(row_rect)
            self._draw_row(p, row, cells, y, row_h, data_bold, cfg)
            if cfg.get("row_dividers", True) and i < len(shown) - 1:
                draw_row_divider(p, inner_x, y + row_h, inner_w, _SECTION)

    def _draw_row(self, p, row, cells, y, row_h, data_bold, cfg) -> None:
        for key, (x, cw) in cells.items():
            rect = QRectF(x, y, cw, row_h)
            val = row.get(key, "")
            if key == "delta":
                delta = row.get("delta")
                if delta is None:
                    p.setPen(col("muted", _SECTION))
                    p.drawText(rect, Qt.AlignmentFlag.AlignCenter, "\u2014")
                else:
                    p.setPen(col("faster", _SECTION) if delta < 0
                             else col("slower", _SECTION))
                    p.drawText(rect, Qt.AlignmentFlag.AlignCenter,
                               signed_delta(delta, 1))
            elif key == "temp" and cfg.get("temp_icon", True):
                p.setPen(col("text", _SECTION))
                p.setFont(tabfont(row_h * cfg.get("font_scale", 0.42),
                                  bold=data_bold))
                fm = p.fontMetrics()
                text = str(val or "\u2014")
                tw = fm.horizontalAdvance(text)
                icon_w = row_h * 0.35
                total = icon_w + tw + 4
                ox = x + (cw - total) / 2
                if icons.has("track_temp"):
                    p.setFont(icons.icon_font(row_h * 0.36))
                    p.setPen(col("text", _SECTION))
                    p.drawText(
                        QRectF(ox, y + row_h * 0.32, icon_w, row_h * 0.36),
                        Qt.AlignmentFlag.AlignCenter,
                        icons.glyph("track_temp"),
                    )
                p.setFont(tabfont(row_h * cfg.get("font_scale", 0.42),
                                  bold=data_bold))
                p.setPen(col("text", _SECTION))
                p.drawText(QRectF(ox + icon_w + 4, y, tw + 2, row_h),
                           Qt.AlignmentFlag.AlignVCenter, text)
            elif key == "tag" and val:
                chip = QRectF(x + cw * 0.1, y + row_h * 0.22,
                              cw * 0.8, row_h * 0.56)
                draw_dark_cell(p, chip, _SECTION, radius=4)
                p.setPen(col("text", _SECTION))
                p.setFont(tfont(row_h * 0.30, bold=True))
                p.drawText(chip, Qt.AlignmentFlag.AlignCenter, str(val))
            else:
                p.setPen(col("text", _SECTION))
                p.setFont(tabfont(row_h * cfg.get("font_scale", 0.42),
                                  bold=data_bold))
                p.drawText(rect, Qt.AlignmentFlag.AlignCenter, str(val or "\u2014"))
