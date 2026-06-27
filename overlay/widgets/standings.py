"""
Standings (Timing Tower): the running order, styled like the Relative table.

Uses the same row layout (status badge, position + class stripe, name, license,
iRating) but the right-hand value is the gap to the leader. Header shows a title
and the shown/total count.
"""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPen

from .. import config
from . import icons
from . import table as tw
from .table import BaseTable

_VA_LEFT = Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft


class StandingsWidget(BaseTable):
    section = "standings"

    def draw_header(self, p, x, y, w, h):
        d = self.data or {}
        cfg = config.CFG["standings"]
        hcfg = cfg.get("header", {})
        icfg = cfg.get("header_icons", {})
        fs = h * 0.42
        title = str(d.get("title", cfg.get("title", "Standings")))
        builders = {
            "order_pill": lambda al, ic: self._pill_item(
                p, "ORDER", "order_pill", fs, h, al, ic),
            "title": lambda al, ic: self._label_item(
                p, title, "title", fs, h, al, False, ic),
            "count": lambda al, ic: self._label_item(
                p, str(d.get("header_right", "")), "count", fs, h, al, True, ic),
        }
        items = []
        for sec in ("left", "center", "right"):
            b = builders.get(hcfg.get(sec, "none"))
            if b:
                items.append(b(sec, bool(icfg.get(sec))))
        self._layout_items(p, x, y, w, h, items)

    def _pill_item(self, p, text, icon_key, fs, h, align, use_icon):
        icon_on = use_icon and icons.has(icon_key)
        if icon_on:
            p.setFont(icons.icon_font(fs * 0.9))
            glyph = icons.glyph(icon_key)
            gw = p.fontMetrics().horizontalAdvance(glyph)

            def draw(p, ax, y, hh):
                p.setFont(icons.icon_font(fs * 0.9))
                p.setPen(tw.col("muted"))
                p.drawText(QRectF(ax, y, gw + 2, hh), _VA_LEFT, glyph)

            return {"align": align, "w": gw, "draw": draw}

        pill_w = h * 1.3

        def draw(p, ax, y, hh):
            cy = y + hh / 2
            pill = QRectF(ax, cy - hh * 0.22, pill_w, hh * 0.44)
            p.setPen(QPen(tw.col("muted"), 1))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(pill, 3, 3)
            p.setFont(tw.tfont(fs * 0.6))
            p.setPen(tw.col("muted"))
            p.drawText(pill, Qt.AlignmentFlag.AlignCenter, text)

        return {"align": align, "w": pill_w, "draw": draw}

    def _label_item(self, p, text, icon_key, fs, h, align, muted, use_icon):
        icon_on = use_icon and icons.has(icon_key)
        if icon_on:
            p.setFont(icons.icon_font(fs * 0.85))
            text = icons.glyph(icon_key)
        else:
            p.setFont(tw.tfont(fs))
        w = p.fontMetrics().horizontalAdvance(text)

        def draw(p, ax, y, hh):
            p.setFont(icons.icon_font(fs * 0.85) if icon_on else tw.tfont(fs))
            p.setPen(tw.col("muted") if muted else tw.col("text"))
            p.drawText(QRectF(ax, y, w + 4, hh), _VA_LEFT, text)

        return {"align": align, "w": w, "draw": draw}
