"""Pit board — requested pit services and fast-repair status."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import (cell_radius, col, draw_card, draw_dark_cell, draw_section_header,
                     draw_status_chip, panel_pad, resolve_row_height)
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "pit_board"
_PREVIEW_SERVICES = (
    {"key": "lf_tire", "label": "LF tire", "checked": True},
    {"key": "fuel", "label": "Fuel", "checked": True},
    {"key": "rf_tire", "label": "RF tire", "checked": False},
)


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
        if not services and d.get("edit"):
            services = _PREVIEW_SERVICES
        if not services and not d.get("edit") and not d.get("pit_active"):
            return
        card, radius = draw_card(p, w, h, _SECTION)
        pad = panel_pad(h)
        y = card.top() + pad
        data_bold = data_font_bold(_SECTION)
        if d.get("pit_active") and cfg.get("show_pit_banner", True):
            banner_h = max(22.0, h * 0.12)
            draw_status_chip(p, QRectF(card.left() + pad, y,
                                       card.width() - 2 * pad, banner_h),
                             str(cfg.get("pit_banner_text", "PIT STOP ACTIVE")),
                             _SECTION, active=True)
            y += banner_h + pad * 0.4
        if cfg.get("show_title", True):
            hh = max(20.0, h * 0.10)
            hdr = QRectF(card.left() + pad, y, card.width() - 2 * pad, hh)
            draw_section_header(p, hdr, str(cfg.get("title", "PIT SERVICES")), _SECTION,
                                radius_top=radius if not d.get("pit_active") else 0)
            y += hh + pad * 0.25
        n = max(1, len(services))
        extras_h = h * 0.14
        body_h = max(card.bottom() - pad - y - extras_h, float(n) * 18.0)
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            row_h = fixed_rh
        else:
            row_h = resolve_row_height(body_h=body_h, row_count=n, panel_h=h, cfg=cfg)
        row_h = max(18.0, row_h)
        rad = cell_radius(row_h)
        mark_w = max(18.0, row_h * 0.45)
        for svc in services:
            rect = QRectF(card.left() + pad, y, card.width() - 2 * pad, row_h - 2)
            draw_dark_cell(p, rect, _SECTION, radius=rad)
            checked = svc.get("checked")
            mark = "\u2713" if checked else "\u2013"
            p.setFont(tfont(row_h * 0.42, bold=data_bold))
            p.setPen(col("checked", _SECTION) if checked else col("muted", _SECTION))
            p.drawText(QRectF(rect.left() + 8, rect.top(), mark_w, rect.height()),
                       Qt.AlignmentFlag.AlignVCenter, mark)
            p.setPen(col("text", _SECTION) if checked else col("muted", _SECTION))
            p.drawText(QRectF(rect.left() + mark_w + 4, rect.top(),
                              rect.width() - mark_w - 8, rect.height()),
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
            p.setFont(tabfont(row_h * 0.38, bold=False))
            p.setPen(col("muted", _SECTION))
            p.drawText(QRectF(card.left() + pad, y, card.width() - 2 * pad, extras_h),
                       Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                       "  \u2022  ".join(extras))
