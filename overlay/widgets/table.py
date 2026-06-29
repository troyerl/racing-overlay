"""
Shared base for the styled timing tables (Relative, Standings).

All colors, fonts, column visibility, sizing and easing come from config.CFG
(the "table" section), so the look is fully customizable via overlay_config.json.
"""

from __future__ import annotations

import math

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QFont, QLinearGradient, QPainter, QPen
from PyQt6.QtWidgets import QWidget

from .. import config
from . import icons

_VA_LEFT = Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft

# Edge-slot items that can be mapped to any header/footer section of either
# table. Each maps to (short text label, icon name). The displayed value is a
# preformatted string the app puts in data["slots"][key]; "title", "count" and
# "order_pill" are rendered specially (see _slot_item).
SLOT_ITEMS: dict[str, tuple[str, str]] = {
    "sof":            ("SOF",  "sof"),
    "class_sof":      ("CSOF", "class_sof"),
    "position":       ("POS",  "position"),
    "class_position": ("CPOS", "class_position"),
    "session_time":   ("TIME", "session_time"),
    "race_time":      ("RACE", "race_time"),
    "lap":            ("LAP",  "lap"),
    "incidents":      ("INC",  "incidents"),
    "track_name":     ("",     "track_name"),
    "track_temp":     ("TRK",  "track_temp"),
    "air_temp":       ("AIR",  "air_temp"),
    "best_lap":       ("BEST", "best_lap"),
    "session_best":   ("SBEST", "session_best"),
    "local_time":     ("CLK",  "local_time"),
    "sim_time":       ("SIM",  "sim_time"),
    "cpu":            ("CPU",  "cpu"),
    "mem":            ("MEM",  "mem"),
}


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


def _contrast_text(bg: QColor) -> QColor:
    """Black or white, whichever reads better on the given background."""
    lum = (0.299 * bg.red() + 0.587 * bg.green() + 0.114 * bg.blue()) / 255.0
    return QColor(20, 22, 26) if lum > 0.6 else QColor(255, 255, 255)


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


_ALL_COLUMNS = {"stripe": True}


class BaseTable(QWidget):
    # Subclasses set this to their config section ("relative" / "standings") so
    # column visibility can be toggled independently per table.
    section: str | None = None

    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict | None = None
        self.setMinimumSize(360, 200)
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

    # --- header / footer slots ---------------------------------------------
    # Both edges share one mappable system: each of the three sections
    # (left/center/right) picks any item from SLOT_ITEMS (or "none"). The app
    # pre-formats each configured item's value into data["slots"].

    def has_footer(self) -> bool:
        if not self.section:
            return False
        return bool(config.CFG.get(self.section, {}).get("show_footer", False))

    def draw_header(self, p, x, y, w, h):
        self._draw_slots(p, x, y, w, h, "header", "header_icons", line=False)

    def draw_footer(self, p, x, y, w, h):
        self._draw_slots(p, x, y, w, h, "footer", "footer_icons", line=True)

    def _draw_slots(self, p, x, y, w, h, group, icons_key, line):
        if not self.section:
            return
        cfg = config.CFG.get(self.section, {})
        gcfg = cfg.get(group, {})
        icfg = cfg.get(icons_key, {})
        if line:
            p.setPen(QPen(col("border"), 1))
            p.drawLine(int(x), int(y), int(x + w), int(y))
        d = self.data or {}
        slots = d.get("slots", {})
        # Header and footer text size are configured independently of the rows.
        mult = _tcfg().get(f"{group}_font_scale", 1.0) or 1.0
        fs = h * 0.42 * mult
        items = []
        for pos in ("left", "center", "right"):
            key = gcfg.get(pos, "none")
            if not key or key == "none":
                continue
            it = self._slot_item(p, key, slots, d, fs, h, pos,
                                 bool(icfg.get(pos)))
            if it:
                items.append(it)
        self._layout_items(p, x, y, w, h, items)

    def _slot_item(self, p, key, slots, d, fs, h, align, use_icon):
        if key == "title":
            text = str(d.get("title", ""))
            p.setFont(tfont(fs))
            w = p.fontMetrics().horizontalAdvance(text)

            def draw(p, ax, y, hh):
                p.setFont(tfont(fs))
                p.setPen(col("text"))
                p.drawText(QRectF(ax, y, w + 4, hh), _VA_LEFT, text)

            return {"align": align, "w": w, "draw": draw}
        if key == "order_pill":
            return self._pill_item(p, "ORDER", fs, h, align)
        spec = SLOT_ITEMS.get(key)
        if spec is None:
            # "count" and any unknown key fall through to a value-only readout.
            spec = ("", key)
        label, icon_key = spec
        value = str(slots.get(key, "\u2014"))
        return self._slot_text(p, fs, h, align, value, label, icon_key,
                              use_icon, muted_value=(key == "count"))

    def _slot_text(self, p, fs, h, align, value, label, icon_key, use_icon,
                  muted_value=False):
        icon_on = use_icon and icon_key and icons.has(icon_key)
        if icon_on:
            lead = icons.glyph(icon_key)
            p.setFont(icons.icon_font(fs * 0.82))
        elif label:
            lead = label
            p.setFont(tfont(fs * 0.62))
        else:
            lead = ""
        lead_w = p.fontMetrics().horizontalAdvance(lead) if lead else 0.0
        gap = fs * 0.35 if lead else 0.0
        p.setFont(tfont(fs * 0.9))
        val_w = p.fontMetrics().horizontalAdvance(value)

        def draw(p, ax, y, hh):
            if lead:
                p.setPen(col("muted"))
                p.setFont(icons.icon_font(fs * 0.82) if icon_on
                          else tfont(fs * 0.62))
                p.drawText(QRectF(ax, y, lead_w + 2, hh), _VA_LEFT, lead)
            p.setFont(tfont(fs * 0.9))
            p.setPen(col("muted") if muted_value else col("text"))
            p.drawText(QRectF(ax + lead_w + gap, y, val_w + 4, hh),
                       _VA_LEFT, value)

        return {"align": align, "w": lead_w + gap + val_w, "draw": draw}

    def _pill_item(self, p, text, fs, h, align):
        pill_w = h * 1.3

        def draw(p, ax, y, hh):
            cy = y + hh / 2
            pill = QRectF(ax, cy - hh * 0.22, pill_w, hh * 0.44)
            p.setPen(QPen(col("muted"), 1))
            p.setBrush(Qt.BrushStyle.NoBrush)
            p.drawRoundedRect(pill, 3, 3)
            p.setFont(tfont(fs * 0.6))
            p.setPen(col("muted"))
            p.drawText(pill, Qt.AlignmentFlag.AlignCenter, text)

        return {"align": align, "w": pill_w, "draw": draw}

    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section(self.section)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        tc = _tcfg()
        radius = max(10.0, h * tc["corner_radius_frac"])

        # Vertical gradient card to match the dash (falls back to flat "bg").
        cols = tc["colors"]
        if "bg_top" in cols and "bg_bottom" in cols:
            grad = QLinearGradient(0, 0, 0, h)
            grad.setColorAt(0.0, col("bg_top"))
            grad.setColorAt(1.0, col("bg_bottom"))
            p.setBrush(grad)
        else:
            p.setBrush(col("bg"))
        p.setPen(QPen(col("border"), 1))
        p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        rows = (self.data or {}).get("rows", [])
        n = max(1, len(rows))
        fixed_rh = float(tc.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            # Fixed pixel sizing: rows, text and header keep their size no matter
            # how big the panel is -- dragging it just adds empty space below.
            row_h = fixed_rh
            pad = 8.0
            header_h = round(fixed_rh * 1.25)
            footer_h = round(fixed_rh * 1.1) if self.has_footer() else 0.0
            body_top = pad + header_h
            body_h = h - body_top - footer_h - pad
            if row_h * n > body_h:  # panel too short: shrink so nothing clips
                row_h = max(1.0, body_h / n)
        else:
            pad = max(8.0, h * 0.025)
            header_h = max(26.0, h * 0.12)
            footer_h = max(24.0, h * 0.11) if self.has_footer() else 0.0
            body_top = pad + header_h
            body_h = h - body_top - footer_h - pad
            row_h = body_h / n
            # With only a few rows, don't let them stretch (and the text balloon)
            # to fill the panel: cap row height and leave the extra space empty.
            max_rh_frac = tc.get("max_row_height_frac", 0.0) or 0.0
            if max_rh_frac > 0:
                row_h = min(row_h, h * max_rh_frac)

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

        right = x + w

        def fixed_width(k: str) -> float:
            if k == "badge":
                return h * wf["badge"]
            if k == "position":
                return h * wf["position"]
            if k == "car_number":
                return h * wf.get("car_number", 1.6)
            if k == "license":
                return h * wf["license"]
            if k == "irating":
                return h * wf["irating"]
            if k == "pit":
                return h * wf.get("pit", 2.1)
            if k == "gap":
                return h * wf["gap"]
            if k == "last_lap":
                return h * wf.get("last_lap", 2.9)
            if k == "best_lap":
                return h * wf.get("best_lap", 2.9)
            return 0.0  # "name" is flexible

        # Build the visible column run in the configured order, then assign each
        # an x position. The "name" column soaks up whatever width is left over.
        order = config.table_column_order(self.section) if self.section else []
        n_gut = max(0, len(order) - 1)
        fixed_total = sum(fixed_width(k) for k in order if k != "name")
        name_w = max(10.0, w - fixed_total - n_gut * gutter) if "name" in order else 0.0

        slots: dict = {}
        cx = x
        for k in order:
            cw = name_w if k == "name" else fixed_width(k)
            slots[k] = (cx, cw)
            cx += cw + gutter

        # Row backgrounds. A leading badge sits in the gutter outside the
        # highlight (matching the original look); otherwise highlight the row.
        if order and order[0] == "badge":
            bx, bw = slots["badge"]
            bg_left = bx + bw
        else:
            bg_left = x
        is_player = row.get("is_player")
        if is_player:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("player_row"))
            p.drawRect(QRectF(bg_left, y, right - bg_left, h))
        elif i % 2 == 1 and tc["alt_row_shading"]:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("row_alt"))
            p.drawRect(QRectF(bg_left, y, right - bg_left, h))
        if row.get("lapping"):
            # Tint the whole row (red for a car a lap ahead, blue for a car a
            # lap down) all the way to the right edge.
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("threat") if row.get("lap_ahead") else col("lapped"))
            p.drawRect(QRectF(bg_left, y, right - bg_left, h))

        if "badge" in slots:
            bx, bw = slots["badge"]
            self._draw_badge(p, row, bx, y, bw, h)
        if "position" in slots:
            px, pw = slots["position"]
            self._draw_position(p, row, px, y, pw, h, fs, cols["stripe"])
        if "car_number" in slots:
            cnx, cnw = slots["car_number"]
            self._draw_number(p, row, cnx, y, cnw, h, fs)
        if "name" in slots:
            nx, nw = slots["name"]
            p.setPen(col("muted") if row.get("in_pit") else col("text"))
            p.setFont(tfont(fs))
            p.drawText(QRectF(nx, y, max(10.0, nw), h),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                       row.get("name", ""))
        if "license" in slots:
            lx, lw = slots["license"]
            self._draw_license(p, row, lx, y, lw, h, fs)
        if "irating" in slots:
            ix, iw = slots["irating"]
            self._draw_irating(p, row, ix, y, iw, h, fs)
        if "pit" in slots:
            qx, qw = slots["pit"]
            self._draw_pit(p, row, qx, y, qw, h, fs)
        if "gap" in slots:
            gx, gw = slots["gap"]
            p.setPen(col("text"))
            p.setFont(tfont(fs * tc["gap_font_scale"]))
            gap = row.get("gap")
            gtxt = row.get("gap_text")
            if gtxt is None:
                gtxt = f"{gap:.1f}" if gap is not None else "--"
            p.drawText(QRectF(gx, y, max(10.0, gw - gutter), h),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight, gtxt)
        if "last_lap" in slots:
            lx, lw = slots["last_lap"]
            self._draw_laptime(p, row, "last_lap", lx, y, lw, h, fs)
        if "best_lap" in slots:
            bx, bw = slots["best_lap"]
            self._draw_laptime(p, row, "best_lap", bx, y, bw, h, fs)

    def _draw_badge(self, p, row, x, y, bw, h):
        cx, cy = x + bw / 2, y + h / 2
        size = min(bw, h) * 0.62
        box = QRectF(cx - size / 2, cy - size / 2, size, size)
        if row.get("is_player"):
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("badge_player"))
            p.drawEllipse(box)
        elif row.get("in_pit"):
            # Widen the badge into a pill and ease the font so "PIT" has a bit
            # of padding around it instead of filling the square edge-to-edge.
            pill_w = min(bw, size * 1.55)
            pill_h = size * 0.92
            pill = QRectF(cx - pill_w / 2, cy - pill_h / 2, pill_w, pill_h)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("badge_pit_bg"))
            p.drawRoundedRect(pill, 4, 4)
            p.setPen(col("badge_pit_text"))
            p.setFont(tfont(pill_h * 0.46))
            p.drawText(pill, Qt.AlignmentFlag.AlignCenter, "PIT")
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
        # The whole pill takes the license-class color; text sits on top with a
        # contrasting color. Shows the class letter + safety rating (e.g. "A 3.45").
        cell = QRectF(x, y + h * 0.2, lw, h * 0.6)
        letter = str(row.get("lic_class", ""))
        bg = license_color(letter)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(bg)
        p.drawRoundedRect(cell, 4, 4)
        sr = str(row.get("sr", "")).strip()
        text = (f"{letter} {sr}".strip() if letter else sr)
        p.setPen(_contrast_text(bg))
        p.setFont(tfont(fs * 0.78))
        p.drawText(cell, Qt.AlignmentFlag.AlignCenter, text)

    def _draw_irating(self, p, row, x, y, iw, h, fs):
        cell = QRectF(x, y + h * 0.2, iw, h * 0.6)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(col("irating_bg"))
        p.drawRoundedRect(cell, 4, 4)
        p.setPen(col("irating_text"))
        p.setFont(tfont(fs * 0.82))
        p.drawText(cell, Qt.AlignmentFlag.AlignCenter, str(row.get("irating", "")))

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

    def _draw_number(self, p, row, x, y, nw, h, fs):
        # No background pill -- just the car number, prefixed with '#'.
        cell = QRectF(x, y + h * 0.2, nw, h * 0.6)
        num = str(row.get("car_number", "")).strip()
        p.setPen(col("muted") if row.get("in_pit") else col("text"))
        p.setFont(tfont(fs * 0.9))
        p.drawText(cell, Qt.AlignmentFlag.AlignCenter, f"#{num}" if num else "")

    def _draw_laptime(self, p, row, key, x, y, lw, h, fs):
        p.setPen(col("muted") if row.get("in_pit") else col("text"))
        p.setFont(tfont(fs * 0.92))
        p.drawText(QRectF(x, y, lw, h), Qt.AlignmentFlag.AlignCenter,
                   row.get(key) or "\u2014")
