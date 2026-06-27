"""
Shared base for the styled timing tables (Relative, Standings).

All colors, fonts, column visibility, sizing and easing come from config.CFG
(the "table" section), so the look is fully customizable via overlay_config.json.
"""

from __future__ import annotations

import math

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QFont, QPainter, QPen
from PyQt6.QtWidgets import QWidget

from .. import config


def ease(current: float, target: float, dt: float, tau: float = 0.12) -> float:
    """Frame-rate-independent exponential smoothing toward a target."""
    if tau <= 0:
        return target
    return current + (target - current) * (1.0 - math.exp(-dt / tau))


def _tcfg() -> dict:
    return config.CFG["table"]


def col(key: str) -> QColor:
    return config.qcolor(_tcfg()["colors"][key])


def license_color(letter: str) -> QColor:
    return config.qcolor(_tcfg()["license_colors"].get(letter, "#666666"))


# Cache fonts by their resolved parameters; building a QFont (and the implicit
# metrics work) every frame is wasteful. A new size/scale/family is just a new
# key, so no invalidation is needed. Returned fonts must not be mutated.
_FONT_CACHE: dict = {}


def tfont(size: float, bold: bool = True) -> QFont:
    fam = config.CFG.get("font_family", "Segoe UI")
    pt = round(max(5.0, size * config.text_scale_for()), 1)
    key = (fam, pt, bold)
    f = _FONT_CACHE.get(key)
    if f is None:
        f = QFont(fam)
        f.setStyleHint(QFont.StyleHint.SansSerif)
        f.setPointSizeF(pt)
        f.setBold(bold)
        if len(_FONT_CACHE) > 512:
            _FONT_CACHE.clear()
        _FONT_CACHE[key] = f
    return f


_ALL_COLUMNS = {
    "badge": True, "position": True, "stripe": True, "name": True,
    "license": True, "irating": True, "gap": True,
}


class BaseTable(QWidget):
    # Subclasses set this to their config section ("relative" / "standings") so
    # column visibility can be toggled independently per table.
    section: str | None = None

    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict | None = None
        self.setMinimumSize(360, 200)
        self._ir_has_delta = False
        self._anim: dict = {}
        self._animating = False
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0

    def set_data(self, data: dict) -> None:
        changed = data != self.data
        self.data = data
        # Skip repaints when rows are unchanged and no slide/fade is in flight.
        if changed or self._animating:
            self.update()

    def _dt(self) -> float:
        now = self._clock.elapsed()
        dt = (now - self._last_ms) / 1000.0
        self._last_ms = now
        return max(0.0, min(0.1, dt))

    def _columns(self) -> dict:
        cols = dict(_ALL_COLUMNS)
        if self.section:
            cols.update(config.CFG.get(self.section, {}).get("columns", {}))
        return cols

    def _layout_items(self, p, x, y, w, h, items) -> None:
        """Place header/footer items into left / center / right slots.

        Each item is {"align": str, "w": float, "draw": fn(p, ax, y, h)} where
        the draw callback renders starting at left edge ax.
        """
        spacing = h * 0.45
        groups = {"left": [], "center": [], "right": []}
        for it in items:
            groups.get(it.get("align", "left"), groups["left"]).append(it)

        cx = x
        for it in groups["left"]:
            it["draw"](p, cx, y, h)
            cx += it["w"] + spacing

        grp = groups["right"]
        total = sum(it["w"] for it in grp) + spacing * max(0, len(grp) - 1)
        cx = x + w - total
        for it in grp:
            it["draw"](p, cx, y, h)
            cx += it["w"] + spacing

        grp = groups["center"]
        total = sum(it["w"] for it in grp) + spacing * max(0, len(grp) - 1)
        cx = x + w / 2 - total / 2
        for it in grp:
            it["draw"](p, cx, y, h)
            cx += it["w"] + spacing

    # Subclasses override these.
    def draw_header(self, p, x, y, w, h):
        d = self.data or {}
        fs = h * 0.44
        p.setFont(tfont(fs))
        p.setPen(col("text"))
        p.drawText(QRectF(x, y, w, h),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   str(d.get("title", "")))
        p.setPen(col("muted"))
        p.drawText(QRectF(x, y, w, h),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                   str(d.get("header_right", "")))

    def draw_footer(self, p, x, y, w, h):
        pass

    def has_footer(self) -> bool:
        return False

    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section(self.section)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        tc = _tcfg()
        radius = max(6.0, h * tc["corner_radius_frac"])

        p.setBrush(col("bg"))
        p.setPen(QPen(col("border"), 1))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        rows = (self.data or {}).get("rows", [])
        # The iRating pill shrinks when no row is showing a change arrow.
        self._ir_has_delta = any(r.get("ir_delta") is not None for r in rows)
        pad = max(8.0, h * 0.025)
        header_h = max(26.0, h * 0.12)
        footer_h = max(24.0, h * 0.11) if self.has_footer() else 0.0
        body_top = pad + header_h
        body_h = h - body_top - footer_h - pad
        n = max(1, len(rows))
        row_h = body_h / n

        self.draw_header(p, pad, pad, w - 2 * pad, header_h)

        dt = self._dt()
        keys_now = set()
        animating = False
        for tgt, row in enumerate(rows):
            key = row.get("key", tgt)
            keys_now.add(key)
            st = self._anim.get(key)
            if st is None:
                st = {"idx": float(tgt), "alpha": 0.0}
                self._anim[key] = st
            st["idx"] = ease(st["idx"], float(tgt), dt, tc["row_ease_tau"])
            st["alpha"] = ease(st["alpha"], 1.0, dt, tc["fade_ease_tau"])
            if abs(st["idx"] - tgt) > 0.01 or st["alpha"] < 0.99:
                animating = True
        for dead in [k for k in self._anim if k not in keys_now]:
            del self._anim[dead]
        self._animating = animating

        for tgt, row in enumerate(rows):
            st = self._anim[row.get("key", tgt)]
            y = body_top + st["idx"] * row_h
            p.save()
            p.setOpacity(max(0.0, min(1.0, st["alpha"])))
            self._draw_row(p, row, tgt, pad, y, w - 2 * pad, row_h)
            p.restore()

        if self.has_footer():
            self.draw_footer(p, pad, h - footer_h - pad * 0.5, w - 2 * pad, footer_h)

    # --- row + cells --------------------------------------------------------

    def _draw_row(self, p, row, i, x, y, w, h):
        if row.get("empty"):  # blank placeholder used to keep the player centered
            return
        tc = _tcfg()
        cols = self._columns()
        wf = tc["widths"]
        fs = h * tc["font_scale"]
        gutter = h * wf["gutter"]

        show_pit = cols.get("pit")
        badge_w = h * wf["badge"] if cols["badge"] else 0.0
        pos_w = h * wf["position"] if cols["position"] else 0.0
        gap_w = h * wf["gap"] if cols["gap"] else 0.0
        ir_key = "irating" if self._ir_has_delta else "irating_narrow"
        ir_w = h * wf.get(ir_key, wf["irating"]) if cols["irating"] else 0.0
        lic_w = h * wf["license"] if cols["license"] else 0.0
        pit_w = h * wf.get("pit", 2.1) if show_pit else 0.0

        # Right-to-left order: gap | pit | irating | license | (name fills rest).
        right = x + w
        x_pos = x + badge_w + (gutter if cols["position"] else 0.0)
        x_name = x_pos + pos_w + gutter
        gap_l = right - gap_w
        pit_r = gap_l - (gutter if cols["gap"] else 0.0)
        pit_l = pit_r - pit_w
        ir_r = pit_l - (gutter if show_pit else 0.0)
        ir_l = ir_r - ir_w
        lic_r = ir_l - (gutter if cols["irating"] else 0.0)
        lic_l = lic_r - lic_w
        any_right = cols["license"] or cols["irating"] or show_pit
        name_r = lic_l - (gutter if any_right else 0.0)

        is_player = row.get("is_player")
        bg_left = x_pos - gutter
        if is_player:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("player_row"))
            p.drawRect(QRectF(bg_left, y, right - bg_left, h))
        elif i % 2 == 1 and tc["alt_row_shading"]:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("row_alt"))
            p.drawRect(QRectF(bg_left, y, right - bg_left, h))
        if row.get("lapping"):
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("threat"))
            thr_left = name_r if any_right else gap_l
            p.drawRect(QRectF(thr_left, y, right - thr_left, h))

        if cols["badge"]:
            self._draw_badge(p, row, x, y, badge_w, h)
        if cols["position"]:
            self._draw_position(p, row, x_pos, y, pos_w, h, fs, cols["stripe"])

        if cols["name"]:
            p.setPen(col("muted") if row.get("in_pit") else col("text"))
            p.setFont(tfont(fs))
            p.drawText(QRectF(x_name, y, max(10.0, name_r - x_name), h),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                       row.get("name", ""))

        if cols["license"]:
            self._draw_license(p, row, lic_l, y, lic_w, h, fs)
        if cols["irating"]:
            self._draw_irating(p, row, ir_l, y, ir_w, h, fs)
        if show_pit:
            self._draw_pit(p, row, pit_l, y, pit_w, h, fs)

        if cols["gap"]:
            p.setPen(col("text"))
            p.setFont(tfont(fs * tc["gap_font_scale"]))
            gap = row.get("gap")
            gtxt = row.get("gap_text")
            if gtxt is None:
                gtxt = f"{gap:.1f}" if gap is not None else "--"
            p.drawText(QRectF(gap_l, y, gap_w - gutter, h),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight, gtxt)

    def _draw_badge(self, p, row, x, y, bw, h):
        cx, cy = x + bw / 2, y + h / 2
        size = min(bw, h) * 0.62
        box = QRectF(cx - size / 2, cy - size / 2, size, size)
        if row.get("is_player"):
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("badge_player"))
            p.drawEllipse(box)
        elif row.get("in_pit"):
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("badge_pit_bg"))
            p.drawRoundedRect(box, 3, 3)
            p.setPen(col("badge_pit_text"))
            p.setFont(tfont(size * 0.5))
            p.drawText(box, Qt.AlignmentFlag.AlignCenter, "PIT")
        elif row.get("lapping"):
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("badge_lap"))
            p.drawRoundedRect(box, 3, 3)
            self._draw_clock(p, box)
        else:
            p.setPen(QPen(col("badge_empty_border"), 1))
            p.setBrush(col("badge_empty_fill"))
            p.drawRoundedRect(box, 3, 3)

    def _draw_clock(self, p, box):
        p.setPen(QPen(QColor(255, 255, 255), max(1.0, box.width() * 0.08)))
        p.setBrush(Qt.BrushStyle.NoBrush)
        inner = box.adjusted(box.width() * 0.22, box.height() * 0.22,
                             -box.width() * 0.22, -box.height() * 0.22)
        p.drawEllipse(inner)
        c = inner.center()
        p.drawLine(c, QPointF(c.x(), c.y() - inner.height() * 0.32))
        p.drawLine(c, QPointF(c.x() + inner.width() * 0.26, c.y()))

    def _draw_position(self, p, row, x, y, pw, h, fs, stripe_on):
        if stripe_on:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(config.qcolor(row.get("class_color", "#888888")))
            p.drawRoundedRect(QRectF(x, y + h * 0.18, h * 0.12, h * 0.64), 2, 2)
        p.setPen(col("text"))
        p.setFont(tfont(fs))
        p.drawText(QRectF(x + h * 0.2, y, pw - h * 0.2, h),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   str(row.get("position", "")))

    def _draw_license(self, p, row, x, y, lw, h, fs):
        cell = QRectF(x, y + h * 0.2, lw, h * 0.6)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(col("cell_dark"))
        p.drawRoundedRect(cell, 4, 4)
        p.setPen(col("text"))
        p.setFont(tfont(fs * 0.78))
        p.drawText(QRectF(cell.left() + 6, cell.top(), cell.width() * 0.55, cell.height()),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   str(row.get("sr", "")))
        letter = str(row.get("lic_class", ""))
        sq = QRectF(cell.right() - cell.height() - 3, cell.top() + 3,
                    cell.height() - 6, cell.height() - 6)
        p.setBrush(license_color(letter))
        p.setPen(Qt.PenStyle.NoPen)
        p.drawRoundedRect(sq, 3, 3)
        p.setPen(col("text"))
        p.setFont(tfont(fs * 0.72))
        p.drawText(sq, Qt.AlignmentFlag.AlignCenter, letter)

    def _draw_irating(self, p, row, x, y, iw, h, fs):
        cell = QRectF(x, y + h * 0.2, iw, h * 0.6)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(col("irating_bg"))
        p.drawRoundedRect(cell, 4, 4)
        p.setPen(col("irating_text"))
        p.setFont(tfont(fs * 0.82))
        delta = row.get("ir_delta")
        if delta is None:
            # No change arrow: center the value in the (narrower) pill.
            p.drawText(cell, Qt.AlignmentFlag.AlignCenter, str(row.get("irating", "")))
            return
        p.drawText(QRectF(cell.left() + 8, cell.top(), cell.width() * 0.55, cell.height()),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   str(row.get("irating", "")))
        up = delta >= 0
        p.setPen(col("ir_up") if up else col("ir_down"))
        p.setFont(tfont(fs * 0.74))
        arrow = "\u25B2" if up else "\u25BC"
        p.drawText(QRectF(cell.left(), cell.top(), cell.width() - 8, cell.height()),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                   f"{arrow}{abs(delta)}")

    def _draw_pit(self, p, row, x, y, pw, h, fs):
        cell = QRectF(x, y + h * 0.2, pw, h * 0.6)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(col("cell_dark"))
        p.drawRoundedRect(cell, 4, 4)
        in_pit = row.get("in_pit")
        p.setPen(col("badge_player") if in_pit else col("text"))
        p.setFont(tfont(fs * 0.8))
        txt = "PIT" if in_pit else (row.get("pit") or "\u2014")
        p.drawText(cell.adjusted(5, 0, -5, 0), Qt.AlignmentFlag.AlignCenter, txt)
