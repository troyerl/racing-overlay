"""ERS / hybrid energy gauge widget."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .. import hybrid as hy
from .chrome import (cell_radius, col, draw_card, draw_dark_cell, draw_metric_row,
                     draw_section_header, draw_status_chip, panel_pad)
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "ers_hybrid"


class ErsHybridWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(180, 100)
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
        edit = d.get("edit")
        if not d.get("have_hybrid") and not edit:
            p.setFont(tfont(h * 0.22, bold=False))
            p.setPen(col("muted", _SECTION))
            p.drawText(QRectF(pad, pad, w - 2 * pad, h - 2 * pad),
                       Qt.AlignmentFlag.AlignCenter,
                       str(cfg.get("empty_text", "No hybrid data")))
            return
        data_bold = data_font_bold(_SECTION)
        y = card.top() + pad
        if cfg.get("show_title", True):
            hh = max(20.0, h * 0.10)
            hdr = QRectF(card.left() + pad, y, card.width() - 2 * pad, hh)
            draw_section_header(p, hdr, str(cfg.get("title", "HYBRID")), _SECTION,
                                radius_top=radius)
            y += hh + pad * 0.25
        if cfg.get("show_battery", True):
            pct = d.get("battery_pct") if d.get("have_hybrid") else None
            if edit and pct is None:
                pct = 62.0
            bar_h = max(28.0, h * 0.28)
            bar = QRectF(card.left() + pad, y, card.width() - 2 * pad, bar_h)
            draw_dark_cell(p, bar, _SECTION, radius=cell_radius(bar_h))
            inner = bar.adjusted(6, 6, -6, -6)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("gauge_bg", _SECTION))
            p.drawRoundedRect(inner, 4, 4)
            if isinstance(pct, (int, float)):
                fw = inner.width() * max(0.0, min(1.0, pct / 100.0))
                p.setBrush(col("gauge_fill", _SECTION))
                p.drawRoundedRect(QRectF(inner.left(), inner.top(), fw, inner.height()),
                                  4, 4)
            lbl = hy.fmt_kj(d.get("battery_j")) if d.get("have_hybrid") else "-- kJ"
            if isinstance(pct, (int, float)):
                lbl = f"{pct:.0f}%  {lbl}"
            draw_metric_row(p, bar.adjusted(8, 0, -8, 0),
                            str(cfg.get("label_battery", "ERS")), lbl, _SECTION,
                            data_bold=data_bold)
            y += bar_h + pad * 0.4
        if cfg.get("show_lap_energy", True):
            used = d.get("used_lap")
            budget = d.get("budget_lap")
            line = "\u2014"
            if isinstance(used, (int, float)) and isinstance(budget, (int, float)):
                line = f"{hy.fmt_kj(used)} / {hy.fmt_kj(budget)} lap"
            elif isinstance(used, (int, float)):
                line = f"{hy.fmt_kj(used)} this lap"
            elif edit:
                line = "-- / -- lap"
            row_h = max(20.0, h * 0.14)
            row = QRectF(card.left() + pad, y, card.width() - 2 * pad, row_h)
            draw_dark_cell(p, row, _SECTION, radius=cell_radius(row_h))
            draw_metric_row(p, row.adjusted(8, 0, -8, 0),
                            str(cfg.get("label_lap", "LAP")), line, _SECTION)
            y += row_h + pad * 0.35
        chip_h = max(18.0, h * 0.12)
        chip_w = (card.width() - 2 * pad - pad * 0.5) / 2
        x = card.left() + pad
        if cfg.get("show_boost", True):
            active = bool(d.get("boost_active"))
            draw_status_chip(p, QRectF(x, y, chip_w, chip_h),
                             str(cfg.get("label_boost", "BOOST")), _SECTION,
                             active=active)
            x += chip_w + pad * 0.5
        if cfg.get("show_p2p", True):
            active = bool(d.get("p2p_active"))
            draw_status_chip(p, QRectF(x, y, chip_w, chip_h),
                             str(cfg.get("label_p2p", "P2P")), _SECTION,
                             active=active)
