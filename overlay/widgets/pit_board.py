"""Pit board — requested pit services and fast-repair status."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import col, draw_card, draw_dark_cell
from .fonts import data_font_bold, tfont

_SECTION = "pit_board"


class PitBoardWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(200, 140)
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
        services = d.get("services") or []
        if not services and not d.get("edit") and not d.get("pit_active"):
            return
        draw_card(p, w, h, _SECTION)
        pad = max(6.0, h * 0.08)
        y = pad
        data_bold = data_font_bold(_SECTION)
        if d.get("pit_active"):
            banner_h = h * 0.14
            rect = QRectF(pad, y, w - 2 * pad, banner_h)
            p.fillRect(rect, col("active_bg", _SECTION))
            p.setFont(tfont(banner_h * 0.55, bold=True))
            p.setPen(col("active_text", _SECTION))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, "PIT STOP ACTIVE")
            y += banner_h + pad * 0.5
        if cfg.get("show_title", True):
            p.setFont(tfont(h * 0.12, bold=True))
            p.setPen(col("title", _SECTION))
            p.drawText(QRectF(pad, y, w - 2 * pad, h * 0.10),
                       Qt.AlignmentFlag.AlignLeft, "PIT SERVICES")
            y += h * 0.11
        n = max(1, len(services))
        row_h = max(16.0, (h - y - pad) / (n + 1))
        for svc in services:
            rect = QRectF(pad, y, w - 2 * pad, row_h * 0.85)
            draw_dark_cell(p, rect, _SECTION, radius=6.0)
            checked = svc.get("checked")
            mark = "\u2713" if checked else "\u2013"
            p.setFont(tfont(row_h * 0.42, bold=data_bold))
            p.setPen(col("checked", _SECTION) if checked else col("muted", _SECTION))
            p.drawText(QRectF(rect.left() + 6, rect.top(), 20, rect.height()),
                       Qt.AlignmentFlag.AlignVCenter, mark)
            p.setPen(col("text", _SECTION) if checked else col("muted", _SECTION))
            p.drawText(QRectF(rect.left() + 24, rect.top(), rect.width() - 30,
                              rect.height()),
                       Qt.AlignmentFlag.AlignVCenter, svc.get("label", ""))
            y += row_h
        extras = []
        if cfg.get("show_compound", True) and d.get("compound") is not None:
            extras.append(f"Set {d['compound']}")
        if d.get("fuel_l") is not None:
            extras.append(f"+{d['fuel_l']:.1f} {config.fuel_unit()}")
        if cfg.get("show_fast_repairs", True) and d.get("repairs") is not None:
            extras.append(f"Repairs {d['repairs']}")
        if cfg.get("show_pressures", False) and d.get("pressures"):
            pr = d["pressures"]
            parts = []
            for lbl, key in (("LF", "lf"), ("RF", "rf"), ("LR", "lr"), ("RR", "rr")):
                v = pr.get(key)
                if isinstance(v, (int, float)):
                    parts.append(f"{lbl} {v:.0f}")
            if parts:
                extras.append(" ".join(parts))
        if extras:
            p.setFont(tfont(row_h * 0.38, bold=False))
            p.setPen(col("muted", _SECTION))
            p.drawText(QRectF(pad, y, w - 2 * pad, row_h),
                       Qt.AlignmentFlag.AlignLeft, "  \u2022  ".join(extras))
