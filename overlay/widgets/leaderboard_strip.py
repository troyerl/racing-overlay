"""Leaderboard strip — IMS scoring-pylon style top-N tower."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QFontMetrics, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import col, resolve_row_height
from .fonts import tabfont, tfont
from .scoreboard_digits import draw_scoreboard_text

_SECTION = "leaderboard_strip"
_PREVIEW_ROWS = (
    {"position": 1, "car_number": "45", "is_player": False},
    {"position": 2, "car_number": "10", "is_player": False},
    {"position": 3, "car_number": "12", "is_player": True},
)


def _position_column_width(rows, pos_size: float) -> float:
    """Width of the widest position label at the given font size."""
    font = tfont(pos_size, bold=True)
    fm = QFontMetrics(font)
    w = 0.0
    for row in rows:
        pos = row.get("position", "")
        if pos == "":
            continue
        w = max(w, float(fm.horizontalAdvance(str(pos))))
    return w


def _draw_dot_separator(p: QPainter, x: float, y0: float, y1: float,
                        *, dot: float = 2.0, gap: float = 5.0) -> None:
    """Vertical column of dots between position and car number."""
    if y1 <= y0:
        return
    span = y1 - y0
    n = max(3, int(span / (dot + gap)))
    step = span / (n + 1)
    c = QColor(255, 255, 255, 90)
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(c)
    for i in range(1, n + 1):
        cy = y0 + step * i
        p.drawEllipse(QRectF(x - dot / 2, cy - dot / 2, dot, dot))


class LeaderboardStripWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(72, 120)
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
        p.setRenderHint(QPainter.RenderHint.TextAntialiasing)
        w, h = float(self.width()), float(self.height())
        d = self.data or {}
        cfg = config.CFG.get(_SECTION, {})
        rows = d.get("rows") or []
        if not rows and d.get("edit"):
            rows = _PREVIEW_ROWS
        if not rows:
            return

        bg = col("pylon_bg", _SECTION, "#000000")
        p.fillRect(QRectF(0, 0, w, h), bg)

        pad_x = max(6.0, w * 0.08)
        pad_y = max(4.0, h * 0.03)
        inner_w = w - 2 * pad_x
        inner_h = h - 2 * pad_y

        show_lap = cfg.get("show_lap", False)
        show_mph = cfg.get("show_mph", False)
        show_pos = cfg.get("show_position", True)
        show_num = cfg.get("show_car_number", True)
        show_name = cfg.get("show_name", False)
        show_gap = cfg.get("show_gap", False)
        highlight = cfg.get("highlight_player", True)
        show_header = show_lap or show_mph
        extra_row = show_name or show_gap

        lap_w = inner_w * 0.14 if show_lap else 0.0
        mph_w = inner_w * 0.16 if show_mph else 0.0
        sep_w = inner_w * 0.10 if show_pos and show_num else 0.0
        core_w = inner_w - lap_w - mph_w

        n = max(1, len(rows))
        header_h = max(14.0, inner_h * 0.11) if show_header else 0.0
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        body_h = inner_h - header_h
        if fixed_rh > 0:
            row_h = fixed_rh
        else:
            row_h = resolve_row_height(body_h=body_h, row_count=n, panel_h=h, cfg=cfg)
        row_h = max(22.0, row_h)
        if extra_row:
            row_h = max(row_h, 28.0)

        pos_size = row_h * (0.62 if not extra_row else 0.50)
        data_size = row_h * (0.34 if not extra_row else 0.28)
        lap_size = row_h * 0.34

        if show_pos and show_num:
            pos_w = _position_column_width(rows, pos_size)
            num_w = max(0.0, core_w - pos_w - sep_w)
        elif show_pos:
            pos_w = min(_position_column_width(rows, pos_size), core_w)
            num_w = 0.0
        elif show_num:
            pos_w = 0.0
            num_w = core_w - sep_w
        else:
            pos_w = num_w = 0.0

        x_lap = pad_x
        x_pos = x_lap + lap_w
        x_sep = x_pos + pos_w
        x_num = x_sep + sep_w
        x_mph = x_num + num_w

        header_color = col("header", _SECTION, "#e8e8e8")
        pos_color = col("pos", _SECTION, "#ffffff")
        num_color = col("car_number", _SECTION, "#ff8c00")
        data_color = col("text", _SECTION, "#d8d8d8")
        player_bg = col("player_row", _SECTION, "#ffffff18")

        if show_header:
            hdr_font = tfont(max(7.0, header_h * 0.42), bold=False)
            p.setFont(hdr_font)
            p.setPen(header_color)
            if show_lap:
                p.drawText(QRectF(x_lap, pad_y, lap_w, header_h),
                           Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignBottom,
                           "LAP")
            if show_mph:
                p.drawText(QRectF(x_mph, pad_y, mph_w, header_h),
                           Qt.AlignmentFlag.AlignRight | Qt.AlignmentFlag.AlignBottom,
                           "MPH")

        y = pad_y + header_h

        for row in rows:
            row_top = y
            row_rect = QRectF(pad_x, row_top, inner_w, row_h - 2)
            if row.get("is_player") and highlight:
                p.fillRect(row_rect, player_bg)

            if show_lap:
                lap = row.get("lap")
                lap_txt = str(lap) if lap is not None else ""
                p.setFont(tabfont(lap_size, bold=False))
                p.setPen(data_color)
                p.drawText(QRectF(x_lap, row_top, lap_w, row_h - 2),
                           Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                           lap_txt)

            if show_pos:
                p.setFont(tfont(pos_size, bold=True))
                p.setPen(pos_color)
                pos = row.get("position", "")
                p.drawText(QRectF(x_pos, row_top, pos_w, row_h - 2),
                           Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                           str(pos))

            if show_pos and show_num and sep_w > 0:
                _draw_dot_separator(p, x_sep + sep_w * 0.5, row_top + row_h * 0.18,
                                    row_top + row_h * 0.82)

            if show_num:
                num = str(row.get("car_number", "")).strip()
                draw_scoreboard_text(
                    p, QRectF(x_num, row_top, num_w, row_h - 2),
                    num, num_color, min_digits=2)

            if show_mph:
                mph = row.get("speed_mph")
                mph_txt = str(int(mph)) if mph is not None else ""
                p.setFont(tabfont(lap_size, bold=False))
                p.setPen(num_color if mph_txt else data_color)
                p.drawText(QRectF(x_mph, row_top, mph_w, row_h - 2),
                           Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                           mph_txt)

            if extra_row:
                meta_y = row_top + row_h * 0.52
                meta_h = row_h * 0.42
                meta_x = x_pos
                meta_w = inner_w - (x_pos - pad_x)
                if show_name:
                    p.setFont(tfont(data_size * 0.9, bold=False))
                    p.setPen(data_color)
                    name = str(row.get("name", ""))
                    name_rect = QRectF(meta_x, meta_y, meta_w * 0.62, meta_h)
                    p.drawText(name_rect,
                               Qt.AlignmentFlag.AlignVCenter | Qt.TextFlag.TextSingleLine,
                               p.fontMetrics().elidedText(
                                   name, Qt.TextElideMode.ElideRight,
                                   int(name_rect.width())))
                if show_gap:
                    gap = row.get("gap", "\u2014")
                    gcol = col("muted", _SECTION)
                    if isinstance(gap, str) and gap.startswith("+"):
                        gcol = col("slower", _SECTION)
                    p.setFont(tabfont(data_size, bold=False))
                    p.setPen(gcol)
                    p.drawText(QRectF(pad_x + inner_w * 0.55, meta_y,
                                      inner_w * 0.45, meta_h),
                               Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                               str(gap))

            y += row_h

        border = col("panel_border", _SECTION, "#ffffff10")
        p.setPen(QPen(border, 1.0))
        p.setBrush(Qt.BrushStyle.NoBrush)
        p.drawRect(QRectF(0.5, 0.5, w - 1, h - 1))
