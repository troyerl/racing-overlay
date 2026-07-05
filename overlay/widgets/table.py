"""
Shared base for the styled timing tables (Relative, Standings).

All colors, fonts, column visibility, sizing and easing come from config.CFG
(the "table" section), so the look is fully customizable via overlay_config.json.
"""

from __future__ import annotations

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QFontMetricsF, QLinearGradient, QPainter, QPen
from PyQt6.QtWidgets import QWidget

from .. import config
from . import icons
from .chrome import contrast_text, draw_card, draw_edge_band, ease, resolve_row_height
from .chrome import draw_row_divider as _draw_row_divider_chrome
from .chrome import soften_color as _soften_color
from .fonts import data_font_bold as _data_font_bold, tabfont, tfont

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
    "laps_remain":    ("LEFT", "laps_remain"),
    "incident_limit": ("INC",  "incident_limit"),
    "fast_repairs":   ("FR",   "fast_repairs"),
    "weather":        ("WX",   "weather"),
    "track_wetness":  ("WET",  "track_wetness"),
    "session_type":   ("",     "session_type"),
}


# When a row jumps farther than this many slots, snap instead of sliding through
# intermediate rows (avoids cars crossing each other on big reorder).
_ROW_SNAP_SLOTS = 1.25


def _tcfg() -> dict:
    # Each table (relative / standings) now carries its own styling, so resolve
    # against the section currently painting. Falls back to whichever table
    # section exists if called outside a paint pass.
    sec = config.active_section()
    cfg = config.CFG
    if sec in ("relative", "standings") and isinstance(cfg.get(sec), dict):
        return cfg[sec]
    return cfg.get("relative") or cfg.get("standings") or {}


def col(key: str) -> QColor:
    colors = _tcfg().get("colors", {})
    raw = colors.get(key)
    if raw is None:
        for sec in ("relative", "standings"):
            raw = config.DEFAULTS.get(sec, {}).get("colors", {}).get(key)
            if raw is not None:
                break
    return config.qcolor(raw or "#ffffff")


def license_color(letter: str) -> QColor:
    return config.qcolor(_tcfg()["license_colors"].get(letter, "#666666"))


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

        def gwidth(grp):
            return sum(it["w"] for it in grp) + spacing * max(0, len(grp) - 1)

        lw = gwidth(groups["left"])
        rw = gwidth(groups["right"])
        cw = gwidth(groups["center"])

        # Left hugs the left edge, right hugs the right edge. The center group is
        # centered in the *gap between* the two side groups rather than on the
        # whole row -- so as the table narrows the empty space shrinks first and
        # the far-right element stays put instead of being overrun. Only once the
        # gap is fully closed do the elements start to touch.
        left_end = x + lw
        right_start = x + w - rw
        min_gap = spacing * 0.35
        slot = right_start - left_end
        cx_center = left_end + (slot - cw) / 2.0
        cx_center = max(left_end + min_gap,
                        min(cx_center, right_start - min_gap - cw))

        cx = x
        for it in groups["left"]:
            it["draw"](p, cx, y, h)
            cx += it["w"] + spacing

        cx = right_start
        for it in groups["right"]:
            it["draw"](p, cx, y, h)
            cx += it["w"] + spacing

        cx = cx_center
        for it in groups["center"]:
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

    def draw_header(self, p, card: QRectF, radius: float, pad: float, h: float):
        band = QRectF(card.left(), card.top(), card.width(), pad + h)
        draw_edge_band(p, band, "header_bg", self.section, bottom_line=True,
                       radius_top=radius)
        self._draw_slots(p, card.left() + pad, card.top() + pad,
                         card.width() - 2 * pad, h,
                         "header", "header_icons", line=False)

    def draw_footer(self, p, card: QRectF, radius: float, pad: float, h: float):
        band_top = card.bottom() - pad * 0.5 - h
        band = QRectF(card.left(), band_top, card.width(), card.bottom() - band_top)
        draw_edge_band(p, band, "footer_bg", self.section, top_line=True,
                       radius_bottom=radius, opaque=True)
        self._draw_slots(p, card.left() + pad, band_top,
                         card.width() - 2 * pad, h,
                         "footer", "footer_icons", line=False)

    def _draw_edge_band(self, p, rect: QRectF, bg_key: str, *,
                        top_line: bool = False, bottom_line: bool = False,
                        radius_top: float = 0.0,
                        radius_bottom: float = 0.0,
                        opaque: bool = False) -> None:
        draw_edge_band(p, rect, bg_key, self.section,
                       top_line=top_line, bottom_line=bottom_line,
                       radius_top=radius_top, radius_bottom=radius_bottom,
                       opaque=opaque)

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
        # The band height (h) already scales with this group's font scale, so the
        # text size follows it and stays independent of the row text size.
        fs = h * 0.42
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
        card, radius = draw_card(p, float(w), float(h), self.section)

        rows = (self.data or {}).get("rows", [])
        n = max(1, len(rows))
        self._paint_tc = tc
        self._paint_cols = self._columns()
        self._paint_has_footer = self.has_footer()
        # Header / footer band heights scale with their own font scale so larger
        # header/footer text grows the band (instead of clipping) and is fully
        # independent of the row text size.
        hscale = max(0.3, tc.get("header_font_scale", 1.0) or 1.0)
        fscale = max(0.3, tc.get("footer_font_scale", 1.0) or 1.0)
        fixed_rh = float(tc.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            # Fixed pixel sizing: rows, text and header keep their size no matter
            # how big the panel is -- dragging it just adds empty space below.
            row_h = fixed_rh
            pad = 8.0
            header_h = round(fixed_rh * 1.25 * hscale)
            footer_h = round(fixed_rh * 1.1 * fscale) if self._paint_has_footer else 0.0
            body_top = card.top() + pad + header_h
        else:
            pad = max(8.0, h * 0.025)
            header_h = max(26.0, h * 0.12) * hscale
            footer_h = (max(24.0, h * 0.11) * fscale) if self._paint_has_footer else 0.0
            body_top = card.top() + pad + header_h
            body_h = h - body_top - footer_h - pad
            row_h = resolve_row_height(body_h=body_h, row_count=n,
                                       panel_h=h, cfg=tc)

        self.draw_header(p, card, radius, pad, header_h)

        dt = self._dt()
        keys_now = set()
        animating = False
        tau = float(tc.get("row_ease_tau", 0.16) or 0.16)
        for tgt, row in enumerate(rows):
            key = row.get("key", tgt)
            keys_now.add(key)
            if row.get("empty"):
                st = self._anim.get(key)
                if st is None:
                    st = {"idx": float(tgt)}
                    self._anim[key] = st
                st["idx"] = float(tgt)
                continue
            st = self._anim.get(key)
            if st is None:
                st = {"idx": float(tgt)}
                self._anim[key] = st
            target = float(tgt)
            if abs(st["idx"] - target) > _ROW_SNAP_SLOTS:
                st["idx"] = target
            else:
                st["idx"] = ease(st["idx"], target, dt, tau)
            if abs(st["idx"] - target) > 0.02:
                animating = True
        for dead in [k for k in self._anim if k not in keys_now]:
            del self._anim[dead]
        self._animating = animating

        draw_order = sorted(
            range(len(rows)),
            key=lambda i: self._anim[rows[i].get("key", i)]["idx"],
        )
        prev_draw_idx = None
        for tgt in draw_order:
            row = rows[tgt]
            st = self._anim[row.get("key", tgt)]
            y = body_top + st["idx"] * row_h
            self._draw_row(p, row, tgt, pad, y, w - 2 * pad, row_h)
            if (tc.get("row_dividers", True) and prev_draw_idx is not None
                    and abs(st["idx"] - prev_draw_idx) <= 1.05
                    and not row.get("empty")):
                self._draw_row_divider(p, pad, y, w - 2 * pad)
            if not row.get("empty"):
                prev_draw_idx = st["idx"]

        if self._paint_has_footer:
            self.draw_footer(p, card, radius, pad, footer_h)

        if animating:
            self.update()

    # --- row + cells --------------------------------------------------------

    def _draw_row_divider(self, p, x, y, w) -> None:
        _draw_row_divider_chrome(p, x, y, w, self.section)

    def _draw_row(self, p, row, i, x, y, w, h):
        if row.get("empty"):  # blank placeholder used to keep the player centered
            return
        tc = getattr(self, "_paint_tc", None) or _tcfg()
        cols = getattr(self, "_paint_cols", None) or self._columns()
        wf = tc["widths"]
        fs = h * tc["font_scale"]
        gutter = h * wf["gutter"]

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
                w = h * wf["irating"]
                if _tcfg().get("show_irating_projection"):
                    w *= 1.50
                if _tcfg().get("irating_show_icon", True) and icons.has("irating"):
                    w *= 1.12
                return w
            if k == "pit":
                return h * wf.get("pit", 2.1)
            if k == "gap":
                return h * wf["gap"]
            if k == "last_lap":
                return h * wf.get("last_lap", 2.9)
            if k == "best_lap":
                return h * wf.get("best_lap", 2.9)
            wkey = wf.get(k)
            if wkey is not None:
                return h * wkey
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

        # Row backgrounds span the full row width (badge included).
        row_rect = QRectF(x, y, w, h)
        is_player = row.get("is_player")
        if is_player:
            self._draw_row_tint(p, row_rect, "player_row")
        elif row.get("lapping"):
            self._draw_row_tint(
                p, row_rect, "threat" if row.get("lap_ahead") else "lapped")
        elif row.get("in_pit"):
            self._draw_row_tint(p, row_rect, "pit_row")
        elif row.get("speaking"):
            self._draw_row_tint(p, row_rect, "speaking_row")
        elif i % 2 == 1 and tc["alt_row_shading"]:
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("row_alt"))
            p.drawRect(row_rect)

        if row.get("speaking"):
            self._draw_speaking_accent(p, row_rect)

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
            name_bold = tc.get("name_font_bold", True)
            if row.get("speaking"):
                p.setPen(col("badge_speaking_bg"))
            elif row.get("in_pit"):
                p.setPen(col("muted"))
            else:
                p.setPen(col("text"))
            p.setFont(tfont(fs, bold=name_bold))
            name = str(row.get("name", ""))
            elided = QFontMetricsF(p.font()).elidedText(
                name, Qt.TextElideMode.ElideRight, max(10.0, nw))
            p.drawText(QRectF(nx, y, max(10.0, nw), h),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                       elided)
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
            gap = row.get("gap")
            gtxt = row.get("gap_text")
            if gtxt is None:
                if gap is None:
                    gtxt = "--"
                elif isinstance(gap, (int, float)) and gap == 0:
                    gtxt = "0.0"
                elif isinstance(gap, (int, float)):
                    gtxt = f"{abs(gap):.1f}"
                else:
                    gtxt = str(gap)
            gpen = col("text")
            if (self.section == "relative" and isinstance(gap, (int, float))
                    and not row.get("is_player")):
                if gap > 0:
                    gpen = col("irating_delta_down")
                elif gap < 0:
                    gpen = col("irating_delta_up")
            p.setPen(gpen)
            p.setFont(tabfont(fs * tc["gap_font_scale"], bold=_data_font_bold()))
            p.drawText(QRectF(gx, y, max(10.0, gw - gutter), h),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                       gtxt)
        if "last_lap" in slots:
            lx, lw = slots["last_lap"]
            self._draw_laptime(p, row, "last_lap", lx, y, lw, h, fs)
        if "best_lap" in slots:
            bx, bw = slots["best_lap"]
            self._draw_laptime(p, row, "best_lap", bx, y, bw, h, fs)
        for tkey in ("class_pos", "status", "laps", "closing", "qual_pos",
                     "team", "nickname"):
            if tkey in slots:
                tx, tw = slots[tkey]
                self._draw_text_cell(p, row, tkey, tx, y, tw, h, fs)
        if "car_flag" in slots:
            fx, fw = slots["car_flag"]
            self._draw_car_flag(p, row, fx, y, fw, h, fs)
        for gkey, field in (("gap_ahead", "gap_ahead_text"),
                            ("gap_leader", "gap_leader_text")):
            if gkey in slots:
                gx, gw = slots[gkey]
                self._draw_gap_text(p, row, field, gx, y, gw, h, fs, gutter)
        if "qual_best" in slots:
            qx, qw = slots["qual_best"]
            self._draw_laptime(p, row, "qual_best", qx, y, qw, h, fs)
        if "gap_pole" in slots:
            px, pw = slots["gap_pole"]
            self._draw_gap_text(p, row, "gap_pole", px, y, pw, h, fs, gutter)

    def _draw_row_tint(self, p, rect: QRectF, color_key: str) -> None:
        """Soft horizontal wash for player / lapped-traffic row highlights."""
        accent = col(color_key)
        h = rect.height()
        stripe_w = max(2.5, h * 0.07)

        edge = QColor(accent)
        edge.setAlpha(min(255, edge.alpha() + 50))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(edge)
        p.drawRoundedRect(QRectF(rect.left(), rect.top() + h * 0.12,
                                 stripe_w, h * 0.76), 1.5, 1.5)

        grad = QLinearGradient(rect.topLeft(), rect.topRight())
        for stop, scale in ((0.0, 0.42), (0.35, 0.22), (0.72, 0.08), (1.0, 0.0)):
            c = QColor(accent)
            c.setAlpha(int(accent.alpha() * scale))
            grad.setColorAt(stop, c)
        p.setBrush(grad)
        p.drawRect(rect)

        rim = QColor(accent)
        rim.setAlpha(min(255, int(accent.alpha() * 0.55)))
        p.setPen(QPen(rim, 1))
        p.setBrush(Qt.BrushStyle.NoBrush)
        inset = rect.adjusted(0.5, 0.5, -0.5, -0.5)
        p.drawLine(inset.topLeft(), inset.topRight())
        p.drawLine(inset.bottomLeft(), inset.bottomRight())

    def _draw_speaking_accent(self, p, rect: QRectF) -> None:
        """Bright green stripe + wash so radio traffic reads at a glance."""
        accent = col("badge_speaking_bg")
        h = rect.height()
        stripe_w = max(3.5, h * 0.09)

        edge = QColor(accent)
        edge.setAlpha(255)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(edge)
        p.drawRoundedRect(QRectF(rect.left(), rect.top() + h * 0.10,
                                 stripe_w, h * 0.80), 2.0, 2.0)

        wash = QColor(accent)
        wash.setAlpha(38)
        p.setBrush(wash)
        p.drawRect(rect)

    def _draw_badge(self, p, row, x, y, bw, h):
        cx, cy = x + bw / 2, y + h / 2
        size = min(bw, h) * 0.62
        box = QRectF(cx - size / 2, cy - size / 2, size, size)
        if row.get("speaking"):
            self._draw_speaker_badge(p, box)
            return
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
            p.drawEllipse(box)

    def _draw_speaker_badge(self, p, box: QRectF) -> None:
        g = icons.glyph("speaking")
        if not g:
            return
        pad = box.width() * 0.06
        pill = box.adjusted(-pad, -pad, pad, pad)
        p.setPen(QPen(col("badge_speaking_border"), max(1.2, box.width() * 0.08)))
        p.setBrush(col("badge_speaking_bg"))
        p.drawEllipse(pill)
        p.setPen(col("badge_speaking_text"))
        p.setFont(icons.icon_font(pill.height() * 0.58))
        p.drawText(pill, Qt.AlignmentFlag.AlignCenter, g)

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
        p.setFont(tfont(fs, bold=_data_font_bold()))
        p.drawText(QRectF(x + h * 0.2, y, pw - h * 0.2, h),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   str(row.get("position", "")))

    def _draw_license(self, p, row, x, y, lw, h, fs):
        # Class-colored pill showing license class + safety rating (e.g. "R 3.34").
        letter = str(row.get("lic_class", "")).strip()
        sr = str(row.get("sr", "")).strip()
        if letter and sr:
            text = f"{letter} {sr}"
        else:
            text = sr or letter or "\u2014"
        bg = _soften_color(license_color(letter))
        p.setFont(tfont(fs * 0.84, bold=_data_font_bold()))
        tw = p.fontMetrics().horizontalAdvance(text)
        pad_x = fs * 0.28
        pill_h = h * 0.54
        pill_w = min(lw, tw + 2 * pad_x)
        cell = QRectF(x, y + (h - pill_h) / 2.0, pill_w, pill_h)
        edge = QColor(bg)
        edge.setAlpha(min(255, int(bg.alpha() * 0.55) + 60))
        p.setPen(QPen(edge, 1))
        p.setBrush(bg)
        p.drawRoundedRect(cell, 4, 4)
        p.setPen(contrast_text(bg))
        p.drawText(cell, Qt.AlignmentFlag.AlignCenter, text)

    def _baseline_y(self, fm: QFontMetricsF, rect: QRectF) -> float:
        """Baseline that vertically centers text ink inside *rect*."""
        return rect.center().y() + (fm.ascent() - fm.descent()) / 2.0

    def _draw_irating(self, p, row, x, y, iw, h, fs):
        cell = QRectF(x, y + h * 0.2, iw, h * 0.6)
        tc = _tcfg()
        show_icon = tc.get("irating_show_icon", True) and icons.has("irating")
        icon_gap = fs * 0.10
        icon_outer = 0.0
        pill_left = cell.left()
        if show_icon:
            glyph = icons.glyph("irating")
            ic_px = cell.height() * 0.48
            ic_f = icons.icon_font(ic_px)
            ic_w = QFontMetricsF(ic_f).horizontalAdvance(glyph)
            icon_outer = ic_w + icon_gap
            p.setFont(ic_f)
            p.setPen(col("muted"))
            p.drawText(QRectF(cell.left(), cell.top(), ic_w, cell.height()),
                       _VA_LEFT, glyph)
            pill_left = cell.left() + icon_outer

        pill = QRectF(pill_left, cell.top(), max(4.0, cell.right() - pill_left),
                      cell.height())
        p.setPen(QPen(col("irating_border"), 1))
        p.setBrush(col("irating_bg"))
        p.drawRoundedRect(pill, 4, 4)

        ir_txt = str(row.get("irating", ""))
        delta = row.get("irating_delta")
        show_delta = (tc.get("show_irating_projection")
                      and delta is not None and ir_txt and ir_txt != "--")

        p.setFont(tfont(fs * 0.82, bold=_data_font_bold()))
        if show_delta:
            ir_font = p.font()
            ir_fm = QFontMetricsF(ir_font)
            ir_w = ir_fm.horizontalAdvance(ir_txt)
            gap = fs * 0.50
            dcol = col("irating_delta_up") if delta > 0 else (
                col("irating_delta_down") if delta < 0 else col("muted"))

            use_icons = icons.has("irating_up") and delta != 0
            if use_icons:
                iglyph = icons.glyph("irating_up" if delta > 0 else "irating_down")
                ifont = icons.icon_font(fs * 0.55)
                nfont = tfont(fs * 0.78, bold=False)
                dtxt = str(abs(delta))
                i_fm = QFontMetricsF(ifont)
                n_fm = QFontMetricsF(nfont)
                i_w = i_fm.horizontalAdvance(iglyph)
                n_w = n_fm.horizontalAdvance(dtxt)
                icon_slot = fs * 0.42
                icon_gap_d = fs * 0.10
                d_w = icon_slot + icon_gap_d + n_w
            else:
                dtxt = f"{delta:+d}" if delta else "0"
                nfont = p.font()
                n_fm = QFontMetricsF(nfont)
                d_w = n_fm.horizontalAdvance(dtxt)

            total = ir_w + gap + d_w
            pad_x = fs * 0.18
            left = pill.left() + max(pad_x, (pill.width() - total) / 2.0)
            ir_baseline = self._baseline_y(ir_fm, pill)
            p.setPen(col("irating_text"))
            p.drawText(QPointF(left, ir_baseline), ir_txt)

            dx = left + ir_w + gap
            if use_icons:
                num_baseline = self._baseline_y(n_fm, pill)
                num_mid = num_baseline - (n_fm.ascent() - n_fm.descent()) / 2.0
                icon_baseline = num_mid + (i_fm.ascent() - i_fm.descent()) / 2.0
                icon_x = dx + max(0.0, (icon_slot - i_w) / 2.0)
                p.setFont(ifont)
                p.setPen(dcol)
                p.drawText(QPointF(icon_x, icon_baseline), iglyph)
                p.setFont(nfont)
                p.drawText(QPointF(dx + icon_slot + icon_gap_d, num_baseline), dtxt)
            else:
                p.setPen(dcol)
                p.drawText(QPointF(dx, self._baseline_y(n_fm, pill)), dtxt)
        else:
            p.setPen(col("irating_text"))
            p.drawText(pill, Qt.AlignmentFlag.AlignCenter, ir_txt)

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
        p.setFont(tabfont(fs * 0.9, bold=_data_font_bold()))
        p.drawText(cell, Qt.AlignmentFlag.AlignCenter, f"#{num}" if num else "")

    def _draw_laptime(self, p, row, key, x, y, lw, h, fs):
        p.setPen(col("muted") if row.get("in_pit") else col("text"))
        p.setFont(tabfont(fs * 0.92, bold=_data_font_bold()))
        p.drawText(QRectF(x, y, lw, h), Qt.AlignmentFlag.AlignCenter,
                   row.get(key) or "\u2014")

    def _draw_text_cell(self, p, row, key, x, y, w, h, fs):
        p.setPen(col("muted") if row.get("in_pit") else col("text"))
        p.setFont(tfont(fs * 0.88, bold=_data_font_bold()))
        p.drawText(QRectF(x, y, max(10.0, w), h),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                   str(row.get(key) or "\u2014"))

    def _draw_gap_text(self, p, row, field, x, y, w, h, fs, gutter):
        gtxt = row.get(field) or "\u2014"
        p.setPen(col("text"))
        p.setFont(tabfont(fs * _tcfg().get("gap_font_scale", 1.12),
                          bold=_data_font_bold()))
        p.drawText(QRectF(x, y, max(10.0, w - gutter), h),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                   gtxt)

    def _draw_car_flag(self, p, row, x, y, w, h, fs):
        txt = row.get("car_flag") or "\u2014"
        kind = row.get("car_flag_kind")
        if not kind or txt == "\u2014":
            self._draw_text_cell(p, row, "car_flag", x, y, w, h, fs)
            return
        bg_key = {
            "black": "flag_black",
            "meatball": "flag_meatball",
            "dq": "flag_dq",
            "furled": "flag_furled",
        }.get(kind, "flag_black")
        fg_key = bg_key + "_text"
        cell = QRectF(x + w * 0.05, y + h * 0.22, w * 0.9, h * 0.56)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(col(bg_key))
        p.drawRoundedRect(cell, cell.height() * 0.35, cell.height() * 0.35)
        p.setPen(col(fg_key))
        p.setFont(tfont(fs * 0.72, bold=True))
        p.drawText(cell, Qt.AlignmentFlag.AlignCenter, txt)
