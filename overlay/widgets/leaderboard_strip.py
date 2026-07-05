"""Leaderboard strip — compact top-N tower with F2Time gaps."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import col, draw_card, draw_dark_cell, draw_row_divider
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "leaderboard_strip"


class LeaderboardStripWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(200, 100)
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
        rows = d.get("rows") or []
        if not rows and not d.get("edit"):
            return
        draw_card(p, w, h, _SECTION)
        pad = max(6.0, h * 0.08)
        y = pad
        row_h = max(18.0, (h - 2 * pad) / max(1, len(rows) or 1))
        data_bold = data_font_bold(_SECTION)
        for i, row in enumerate(rows):
            rect = QRectF(pad, y, w - 2 * pad, row_h)
            if row.get("is_player") and cfg.get("highlight_player", True):
                p.fillRect(rect, col("player_row", _SECTION))
            draw_dark_cell(p, rect, _SECTION, radius=min(row_h * 0.2, 8.0))
            stripe_w = max(3.0, row_h * 0.12)
            if cfg.get("show_class_color", True) and row.get("class_color"):
                c = QColor(str(row["class_color"]))
                p.fillRect(QRectF(rect.left(), rect.top(), stripe_w, rect.height()), c)
            x = rect.left() + stripe_w + 4
            p.setFont(tabfont(row_h * 0.42, bold=True))
            p.setPen(col("pos", _SECTION))
            pos = row.get("position", "")
            p.drawText(QRectF(x, rect.top(), row_h * 1.2, rect.height()),
                       Qt.AlignmentFlag.AlignVCenter, f"P{pos}")
            x += row_h * 1.15
            if cfg.get("show_car_number", True):
                p.setFont(tfont(row_h * 0.38, bold=data_bold))
                p.setPen(col("text", _SECTION))
                num = row.get("car_number", "")
                p.drawText(QRectF(x, rect.top(), row_h * 1.4, rect.height()),
                           Qt.AlignmentFlag.AlignVCenter, f"#{num}")
                x += row_h * 1.35
            if cfg.get("show_name", True):
                p.setFont(tfont(row_h * 0.36, bold=False))
                p.setPen(col("text", _SECTION))
                name = str(row.get("name", ""))[:14]
                gap_w = row_h * 2.2
                p.drawText(QRectF(x, rect.top(), rect.width() - (x - rect.left()) - gap_w,
                                  rect.height()),
                           Qt.AlignmentFlag.AlignVCenter, name)
            p.setFont(tabfont(row_h * 0.40, bold=data_bold))
            gap = row.get("gap", "\u2014")
            gcol = col("muted", _SECTION)
            if isinstance(gap, str) and gap.startswith("+"):
                gcol = col("slower", _SECTION)
            p.setPen(gcol)
            p.drawText(QRectF(rect.right() - row_h * 2.1, rect.top(),
                              row_h * 2.0, rect.height()),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                       str(gap))
            if i + 1 < len(rows):
                draw_row_divider(p, pad, y + row_h, w - 2 * pad, _SECTION)
            y += row_h
