"""
Flags -- a big, standalone flag banner.

Draws the current race flag (caution, black, blue, white, checkered, ...) as a
single bold banner inside the same dark rounded panel the other widgets use, with
the diagonal-slash (or checkerboard) texture and label plate borrowed from the
dash flag so it sits naturally alongside the rest of the overlay. When nothing is
flying it shows a calm muted state.

Expected data dict:
    flag   flag name from the app's session-flag logic, or None
"""

from __future__ import annotations

from PyQt6.QtCore import QPointF, QRectF, Qt
from PyQt6.QtGui import (QColor, QFont, QFontMetricsF, QLinearGradient, QPainter,
                         QPainterPath, QPen)
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config

# flag name -> (label, background color key, text color key).
_SPEC = {
    "yellow": ("CAUTION", "flag_yellow", "flag_yellow_text"),
    "black": ("BLACK FLAG", "flag_black", "flag_black_text"),
    "meatball": ("MEATBALL", "flag_meatball", "flag_meatball_text"),
    "furled": ("WARNING", "flag_furled", "flag_furled_text"),
    "dq": ("DISQUALIFIED", "flag_dq", "flag_dq_text"),
    "green": ("GREEN", "flag_green", "flag_green_text"),
    "white": ("LAST LAP", "flag_white_bg", "flag_white_text"),
    "red": ("RED FLAG", "flag_red", "flag_red_text"),
    "blue": ("LET BY", "flag_blue", "flag_blue_text"),
    "debris": ("DEBRIS", "flag_debris", "flag_debris_text"),
    "crossed": ("HALFWAY", "flag_crossed", "flag_crossed_text"),
    "checkered": ("FINISH", "flag_checker_bg", "flag_checker_text"),
}


class FlagsWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self._font_cache: dict = {}
        self.setMinimumSize(180, 80)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        data = data or {}
        if data == self.data:
            return
        self.data = data
        self.update()

    def _cfg(self) -> dict:
        return config.CFG["flags"]

    def _col(self, key: str) -> QColor:
        return config.qcolor(self._cfg()["colors"].get(key, "#ff00ff"))

    def _font(self, px: float, bold: bool = True) -> QFont:
        fam = config.CFG.get("font_family", "Segoe UI")
        pxi = max(6, int(round(px * config.text_scale_for("flags"))))
        key = (fam, pxi, bold)
        f = self._font_cache.get(key)
        if f is None:
            f = QFont(fam)
            f.setStyleHint(QFont.StyleHint.SansSerif)
            f.setPixelSize(pxi)
            f.setBold(bold)
            if len(self._font_cache) > 64:
                self._font_cache.clear()
            self._font_cache[key] = f
        return f

    def paintEvent(self, event) -> None:  # noqa: N802
        flag = (self.data or {}).get("flag")
        # Nothing flying: stay invisible unless we're in layout-edit mode, where
        # a placeholder keeps the panel findable and draggable.
        if flag is None and not (self.data or {}).get("edit"):
            return
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())

        radius = max(8.0, min(w, h) * 0.16)
        grad = QLinearGradient(0, 0, 0, h)
        grad.setColorAt(0.0, self._col("bg_top"))
        grad.setColorAt(1.0, self._col("bg_bottom"))
        p.setBrush(grad)
        p.setPen(QPen(self._col("panel_border"), 1.2))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        pad = max(6.0, min(w, h) * 0.12)
        self._draw_banner(p, QRectF(pad, pad, w - 2 * pad, h - 2 * pad), flag)

    def _draw_banner(self, p, rect: QRectF, flag) -> None:
        spec = _SPEC.get(flag)
        r = min(rect.height() * 0.34, 22.0)
        if spec is None:
            p.setBrush(self._col("idle_bg"))
            p.setPen(QPen(QColor(255, 255, 255, 26), 1))
            p.drawRoundedRect(rect, r, r)
            p.setFont(self._font(rect.height() * 0.26))
            p.setPen(self._col("idle_text"))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter,
                       str(self._cfg().get("idle_text", "TRACK CLEAR")))
            return

        label, bg_key, fg_key = spec
        bg = self._col(bg_key)
        fg = self._col(fg_key)
        p.setBrush(bg)
        p.setPen(QPen(QColor(255, 255, 255, 45), 1))
        p.drawRoundedRect(rect, r, r)

        # Texture, clipped to the banner: checkerboard for the finish flag,
        # diagonal slashes otherwise (matches the dash flag treatment).
        p.save()
        clip = QPainterPath()
        clip.addRoundedRect(rect, r, r)
        p.setClipPath(clip)
        if flag == "checkered":
            self._draw_checker(p, rect, fg)
        else:
            hatch = QColor(fg)
            hatch.setAlpha(64)
            p.setPen(QPen(hatch, max(2.0, rect.height() * 0.10)))
            step = rect.height() * 0.5
            x = rect.left() - rect.height()
            while x < rect.right() + rect.height():
                p.drawLine(QPointF(x, rect.bottom()),
                           QPointF(x + rect.height(), rect.top()))
                x += step
        p.restore()

        # Rounded plate behind the label so the texture frames it.
        font = self._font(rect.height() * 0.30)
        avail = rect.width() * 0.82
        tw = QFontMetricsF(font).horizontalAdvance(label)
        if tw > avail and tw > 0:
            font = self._font(rect.height() * 0.30 * avail / tw)
            tw = QFontMetricsF(font).horizontalAdvance(label)
        plate_pad = rect.height() * 0.28
        plate_h = min(rect.height() * 0.62, font.pixelSize() * 1.9)
        plate = QRectF(rect.center().x() - tw / 2 - plate_pad,
                       rect.center().y() - plate_h / 2,
                       tw + plate_pad * 2, plate_h)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(bg)
        p.drawRoundedRect(plate, plate.height() / 2, plate.height() / 2)
        p.setFont(font)
        p.setPen(fg)
        p.drawText(rect, Qt.AlignmentFlag.AlignCenter, label)

    @staticmethod
    def _draw_checker(p, rect: QRectF, color) -> None:
        rows = 3
        sq = rect.height() / rows
        cols = int(rect.width() / sq) + 2
        cell = QColor(color)
        cell.setAlpha(190)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(cell)
        for ri in range(rows):
            for ci in range(cols):
                if (ri + ci) % 2 == 0:
                    p.drawRect(QRectF(rect.left() + ci * sq,
                                      rect.top() + ri * sq, sq, sq))
