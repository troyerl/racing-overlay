"""Leaderboard strip — compact top-N tower with F2Time gaps."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import (cell_radius, col, draw_card, draw_dark_cell, draw_player_tint,
                     draw_row_divider, panel_pad, resolve_row_height)
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "leaderboard_strip"
_PREVIEW_ROWS = (
    {"position": 1, "car_number": "42", "name": "Preview Leader",
     "class_color": "#888888", "gap": "+0.0", "is_player": False},
    {"position": 2, "car_number": "07", "name": "Preview P2",
     "class_color": "#888888", "gap": "+1.2", "is_player": False},
    {"position": 3, "car_number": "12", "name": "You",
     "class_color": "#888888", "gap": "+2.4", "is_player": True},
)


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
        if not rows and d.get("edit"):
            rows = _PREVIEW_ROWS
        if not rows:
            return
        card, _radius = draw_card(p, w, h, _SECTION)
        pad = panel_pad(h)
        y = card.top() + pad
        body_h = card.height() - 2 * pad
        n = max(1, len(rows))
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            row_h = fixed_rh
        else:
            row_h = resolve_row_height(body_h=body_h, row_count=n, panel_h=h, cfg=cfg)
        row_h = max(18.0, row_h)
        rad = cell_radius(row_h)
        data_bold = data_font_bold(_SECTION)
        show_gap = cfg.get("show_gap", True)
        for i, row in enumerate(rows):
            rect = QRectF(card.left() + pad, y, card.width() - 2 * pad, row_h - 2)
            if row.get("is_player") and cfg.get("highlight_player", True):
                draw_player_tint(p, rect, _SECTION)
            draw_dark_cell(p, rect, _SECTION, radius=rad)
            stripe_w = max(3.0, row_h * 0.12)
            if cfg.get("show_class_color", True) and row.get("class_color"):
                c = QColor(str(row["class_color"]))
                p.fillRect(QRectF(rect.left(), rect.top(), stripe_w, rect.height()), c)
            x = rect.left() + stripe_w + 4
            if cfg.get("show_position", True):
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
                name = str(row.get("name", ""))
                gap_w = row_h * 2.2 if show_gap else 0.0
                name_rect = QRectF(x, rect.top(),
                                   rect.width() - (x - rect.left()) - gap_w,
                                   rect.height())
                p.drawText(name_rect,
                           Qt.AlignmentFlag.AlignVCenter | Qt.TextFlag.TextSingleLine,
                           p.fontMetrics().elidedText(
                               name, Qt.TextElideMode.ElideRight,
                               int(name_rect.width())))
            if show_gap:
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
                draw_row_divider(p, card.left() + pad, y + row_h,
                                 card.width() - 2 * pad, _SECTION)
            y += row_h
