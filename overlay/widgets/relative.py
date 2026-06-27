"""
Relative table: cars nearest the player on track, styled after modern overlays.

Rows are painted by BaseTable; this subclass supplies the SOF/position header
and the RACE time / lap / incidents footer. Threat rows (different lap) and the
player row are highlighted by the base class from per-row flags.
"""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPen

from .. import config
from . import icons
from . import table as tw
from .table import BaseTable

_VA_LEFT = Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft


class RelativeWidget(BaseTable):
    section = "relative"

    def has_footer(self) -> bool:
        return config.CFG["relative"].get("show_footer", True)

    def draw_header(self, p, x, y, w, h):
        d = self.data or {}
        rcfg = config.CFG["relative"]
        hcfg = rcfg.get("header", {})
        icfg = rcfg.get("header_icons", {})
        fs = h * 0.42
        builders = {"sof": self._sof_item, "position": self._pos_item}
        items = []
        for sec in ("left", "center", "right"):
            b = builders.get(hcfg.get(sec, "none"))
            if b:
                items.append(b(p, d, fs, h, sec, bool(icfg.get(sec))))
        self._layout_items(p, x, y, w, h, items)

    def _sof_item(self, p, d, fs, h, align, use_icon):
        val = str(d.get("sof", "--"))
        pad = 8
        p.setFont(tw.tfont(fs))
        val_w = p.fontMetrics().horizontalAdvance(val)
        icon_on = use_icon and icons.has("sof")
        if icon_on:
            p.setFont(icons.icon_font(fs * 0.9))
            lead_w = p.fontMetrics().horizontalAdvance(icons.glyph("sof"))
        else:
            lead_w = h * 0.95  # the SOF pill

        def draw(p, ax, y, hh):
            cy = y + hh / 2
            p.setPen(tw.col("muted"))
            if icon_on:
                p.setFont(icons.icon_font(fs * 0.9))
                p.drawText(QRectF(ax, y, lead_w + 2, hh), _VA_LEFT, icons.glyph("sof"))
            else:
                pill = QRectF(ax, cy - hh * 0.22, lead_w, hh * 0.44)
                p.setBrush(Qt.BrushStyle.NoBrush)
                p.drawRoundedRect(pill, 3, 3)
                p.setFont(tw.tfont(fs * 0.62))
                p.drawText(pill, Qt.AlignmentFlag.AlignCenter, "SOF")
            p.setFont(tw.tfont(fs))
            p.setPen(tw.col("text"))
            p.drawText(QRectF(ax + lead_w + pad, y, val_w + 4, hh), _VA_LEFT, val)

        return {"align": align, "w": lead_w + pad + val_w, "draw": draw}

    def _pos_item(self, p, d, fs, h, align, use_icon):
        pos, total = d.get("pos"), d.get("total")
        txt = f"{pos}/{total}" if pos and total else "--"
        icon_on = use_icon and icons.has("position")
        if icon_on:
            p.setFont(icons.icon_font(fs * 0.85))
            lead = icons.glyph("position")
        else:
            p.setFont(tw.tfont(fs))
            lead = "\u2715 "
        lead_w = p.fontMetrics().horizontalAdvance(lead)
        p.setFont(tw.tfont(fs))
        txt_w = p.fontMetrics().horizontalAdvance(txt)
        gap = fs * 0.4 if icon_on else 0

        def draw(p, ax, y, hh):
            p.setPen(tw.col("muted"))
            p.setFont(icons.icon_font(fs * 0.85) if icon_on else tw.tfont(fs))
            p.drawText(QRectF(ax, y, lead_w + 2, hh), _VA_LEFT, lead)
            p.setFont(tw.tfont(fs))
            p.setPen(tw.col("text"))
            p.drawText(QRectF(ax + lead_w + gap, y, txt_w + 4, hh), _VA_LEFT, txt)

        return {"align": align, "w": lead_w + gap + txt_w, "draw": draw}

    def draw_footer(self, p, x, y, w, h):
        d = (self.data or {}).get("footer", {})
        rcfg = config.CFG["relative"]
        fcfg = rcfg.get("footer", {})
        icfg = rcfg.get("footer_icons", {})
        fs = h * 0.4
        p.setPen(QPen(tw.col("border"), 1))
        p.drawLine(int(x), int(y), int(x + w), int(y))

        specs = {
            "race_time": ("race_time", "RACE ",
                          f"{d.get('race_time', '--')} / {d.get('race_total', '--')}"),
            "lap": ("lap", "Lap ", f"{d.get('lap', '-')}/~{d.get('lap_est', '-')}"),
            "incidents": ("incidents", "\u2298 ", f"{d.get('incidents', 0)}"),
        }
        items = []
        for sec in ("left", "center", "right"):
            spec = specs.get(fcfg.get(sec, "none"))
            if spec:
                icon_key, label, value = spec
                items.append(self._text_item(
                    p, fs, h, sec, value, label, icon_key, bool(icfg.get(sec))))
        self._layout_items(p, x, y, w, h, items)

    def _text_item(self, p, fs, h, align, value, label, icon_key, use_icon):
        icon_on = use_icon and icons.has(icon_key)
        if icon_on:
            p.setFont(icons.icon_font(fs * 0.78))
            lead = icons.glyph(icon_key)
        else:
            p.setFont(tw.tfont(fs * 0.85))
            lead = label
        lead_w = p.fontMetrics().horizontalAdvance(lead) if lead else 0
        gap = fs * 0.4 if icon_on else 0
        p.setFont(tw.tfont(fs * 0.85))
        val_w = p.fontMetrics().horizontalAdvance(value)

        def draw(p, ax, y, hh):
            if lead:
                p.setPen(tw.col("muted"))
                p.setFont(icons.icon_font(fs * 0.78) if icon_on else tw.tfont(fs * 0.85))
                p.drawText(QRectF(ax, y, lead_w + 2, hh), _VA_LEFT, lead)
            p.setFont(tw.tfont(fs * 0.85))
            p.setPen(tw.col("text"))
            p.drawText(QRectF(ax + lead_w + gap, y, val_w + 4, hh), _VA_LEFT, value)

        return {"align": align, "w": lead_w + gap + val_w, "draw": draw}
