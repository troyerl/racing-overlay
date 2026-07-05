"""Tire panel — 4-corner wear, temp, and optional cold pressure."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import cell_radius, col, draw_card, draw_dark_cell, panel_pad
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "tire_panel"
_CORNERS = (("FL", "lf"), ("FR", "rf"), ("RL", "lr"), ("RR", "rr"))


class TirePanelWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(180, 140)
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
        corners = d.get("corners") or {}
        edit = d.get("edit")
        if not corners and not edit:
            return
        card, _radius = draw_card(p, w, h, _SECTION)
        pad = panel_pad(h)
        iw, ih = card.width() - 2 * pad, card.height() - 2 * pad
        gap = max(4.0, iw * 0.04)
        cw = (iw - gap) / 2
        ch = (ih - gap) / 2
        warn = float(cfg.get("warn_wear_pct", 30.0) or 30.0)
        data_bold = data_font_bold(_SECTION)
        rad = cell_radius(min(cw, ch) * 0.4)
        for i, (lbl, key) in enumerate(_CORNERS):
            col_i, row_i = i % 2, i // 2
            x = card.left() + pad + col_i * (cw + gap)
            y = card.top() + pad + row_i * (ch + gap)
            rect = QRectF(x, y, cw, ch)
            draw_dark_cell(p, rect, _SECTION, radius=rad)
            cdata = corners.get(key) or {}
            p.setFont(tfont(ch * 0.22, bold=True))
            p.setPen(col("header", _SECTION))
            p.drawText(QRectF(x + 6, y + 4, cw - 12, ch * 0.22),
                       Qt.AlignmentFlag.AlignLeft, lbl)
            ty = y + ch * 0.26
            if cfg.get("show_wear", True):
                wear = cdata.get("wear")
                bar = QRectF(x + 8, y + ch - ch * 0.22, cw - 16, ch * 0.14)
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(col("bar_bg", _SECTION))
                p.drawRoundedRect(bar, 3, 3)
                if isinstance(wear, (int, float)):
                    pct = max(0.0, min(100.0, wear * 100.0))
                    fill_w = bar.width() * pct / 100.0
                    fcol = (col("warn", _SECTION) if pct <= warn
                            else col("wear", _SECTION))
                    p.setBrush(fcol)
                    p.drawRoundedRect(QRectF(bar.left(), bar.top(), fill_w, bar.height()),
                                      3, 3)
                    val = f"{pct:.0f}%"
                else:
                    val = "\u2014" if edit else "--"
                p.setFont(tabfont(ch * 0.24, bold=data_bold))
                p.setPen(col("text", _SECTION))
                p.drawText(QRectF(x + 6, ty, cw - 12, ch * 0.28),
                           Qt.AlignmentFlag.AlignLeft, val)
                ty += ch * 0.30
            if cfg.get("show_temp", True):
                temp = cdata.get("temp")
                if isinstance(temp, (int, float)):
                    t = config.conv_temp(temp)
                    ts = f"{t:.0f}\u00b0" if t is not None else "\u2014"
                else:
                    ts = "\u2014" if edit else "--"
                p.setFont(tabfont(ch * 0.22, bold=False))
                p.setPen(col("muted", _SECTION))
                p.drawText(QRectF(x + 6, ty, cw - 12, ch * 0.22),
                           Qt.AlignmentFlag.AlignLeft, ts)
                ty += ch * 0.24
            if cfg.get("show_pressure", False):
                pr = cdata.get("pressure")
                ps = f"{pr:.0f} kPa" if isinstance(pr, (int, float)) else (
                    "\u2014" if edit else "--")
                p.setFont(tabfont(ch * 0.20, bold=False))
                p.setPen(col("muted", _SECTION))
                p.drawText(QRectF(x + 6, ty, cw - 12, ch * 0.20),
                           Qt.AlignmentFlag.AlignLeft, ps)
