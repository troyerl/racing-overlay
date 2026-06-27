"""
Dash -- a multi-container racing dashboard.

The dash composes several distinct rounded containers in a fixed layout:

  * Top container:      shift / RPM bar (left) + a status readout (right)
  * Bottom container:   a small + a big readout (left) + two stat cells (right)
  * Position container:  the position box, on its own to the right
  * Strip container:     three small readouts in their own floating pill
  * Center medallion:    either a gear + input ring, or throttle/brake/clutch
                         pedal bars (with an ABS highlight) -- see center_mode
  * Delta bar:           an optional thin bar across the top (faster vs slower)

The container *layout* is fixed, but the *content* of every slot is fully
configurable from config.CFG["dash"]: top_right, primary_left, primary_right,
stat_left, stat_right and strip_left/center/right each pick any metric from
METRICS (or "none" to hide it), so you can e.g. swap speed and laps, or show
laps remaining instead of the lap count. Colors, a per-widget text scale and
the shift-bar / ring / position toggles come from the same section. Units
follow the global config.units setting.

Expected data dict (all optional; missing values render as "--"):
    rpm, redline, sl_first, sl_last   shift bar
    throttle, brake, clutch           ring fill + pedal bars (0..1 each)
    abs_active                        flashes the brake pedal bar when True
    gear                              gear number ("R"/"N"/1..)
    speed_ms                          speed in m/s (converted to mph/kph)
    position                          race position (int)
    lap, laps_total                   current lap / total laps
    incidents                         incident count
    tire_l, tire_r                    front tire wear as 0..1 fractions
    fuel, fuel_laps                   fuel level (litres) + laps remaining
    air_temp, track_temp              temperatures in Celsius
    last_lap, best_lap, cur_lap       lap times in seconds
    delta                             delta to session best
"""

from __future__ import annotations

import math

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import (QColor, QFont, QFontMetricsF, QLinearGradient,
                         QPainter, QPen)
from PyQt6.QtWidgets import QWidget

from .. import config
from . import icons

_VC_LEFT = Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft


def _ease(cur: float, tgt: float, dt: float, tau: float) -> float:
    if tau <= 0:
        return tgt
    return cur + (tgt - cur) * (1.0 - math.exp(-dt / tau))


def _num(d: dict, key: str):
    v = d.get(key)
    return v if isinstance(v, (int, float)) else None


def _gear_str(g) -> str:
    if g is None:
        return "N"
    g = int(g)
    return "R" if g < 0 else ("N" if g == 0 else str(g))


def _clock(sec) -> str:
    if not sec or sec <= 0:
        return "--:--"
    m = int(sec // 60)
    return f"{m}:{sec - m * 60:06.3f}"


# --------------------------------------------------------------------------
# Metric registry: every content slot in the dash picks one of these keys.
# A formatter returns either a plain string (single value) or a list of
# (sub-label, value) rows for stacked cells (tires, fuel+laps).
# --------------------------------------------------------------------------
def _f_speed(d):
    v = config.conv_speed(_num(d, "speed_ms"))
    return f"{v:.0f}" if v is not None else "--"


def _f_rpm(d):
    v = _num(d, "rpm")
    return f"{v:.0f}" if v is not None else "--"


def _f_pos(d):
    v = d.get("position")
    return f"P{int(v)}" if isinstance(v, (int, float)) and v else "--"


def _f_lap_count(d):
    lap, total = _num(d, "lap"), _num(d, "laps_total")
    if lap is not None and total and total > 0:
        return f"{int(lap)}/{int(total)}"
    return f"{int(lap)}" if lap is not None else "--"


def _f_laps_left(d):
    lap, total = _num(d, "lap"), _num(d, "laps_total")
    if lap is not None and total and total > 0:
        return f"{max(0, int(total) - int(lap))}"
    return "--"


def _f_lap(d):
    v = _num(d, "lap")
    return f"{int(v)}" if v is not None else "--"


def _f_fuel(d):
    v = config.conv_fuel(_num(d, "fuel"))
    return f"{v:.1f} {config.fuel_unit()}" if v is not None else "--"


def _f_fuel_laps(d):
    v = _num(d, "fuel_laps")
    return f"{v:.1f} Laps" if v is not None else "-- Laps"


def _f_fuel_stack(d):
    v = config.conv_fuel(_num(d, "fuel"))
    laps = _num(d, "fuel_laps")
    top = f"{v:.1f} {config.fuel_unit()}" if v is not None else "--"
    bot = f"{laps:.1f} Laps" if laps is not None else "-- Laps"
    return [("FUEL", top), ("", bot)]


def _f_tires(d):
    l, r = _num(d, "tire_l"), _num(d, "tire_r")
    ls = f"{l * 100:.0f}%" if l is not None else "--"
    rs = f"{r * 100:.0f}%" if r is not None else "--"
    return [("L", ls), ("R", rs)]


def _f_inc(d):
    v = _num(d, "incidents")
    return f"{int(v)}x" if v is not None else "--"


def _f_gear(d):
    return _gear_str(d.get("gear"))


def _f_delta(d):
    v = _num(d, "delta")
    return f"{v:+.2f}" if v is not None else "--"


def _f_clock(key):
    return lambda d: _clock(_num(d, key))


def _f_temp(key):
    def fmt(d):
        v = config.conv_temp(_num(d, key))
        return f"{v:.0f}\u00b0" if v is not None else "--"
    return fmt


# key -> (label, formatter). The label is used wherever the slot's render
# style shows one (small primary, stat cells, strip); big readouts hide it.
METRICS: dict = {
    "none": ("", lambda d: ""),
    "speed": (lambda: config.speed_unit(), _f_speed),
    "rpm": ("RPM", _f_rpm),
    "gear": ("GEAR", _f_gear),
    "position": ("POS", _f_pos),
    "lap_count": ("LAP", _f_lap_count),
    "laps_left": ("LEFT", _f_laps_left),
    "lap": ("LAP", _f_lap),
    "fuel": (lambda: config.fuel_unit(), _f_fuel),
    "fuel_stack": ("FUEL", _f_fuel_stack),
    "fuel_laps": ("FUEL", _f_fuel_laps),
    "tires": ("TIRE", _f_tires),
    "incidents": ("INC", _f_inc),
    "last_lap": ("LAST", _f_clock("last_lap")),
    "best_lap": ("BEST", _f_clock("best_lap")),
    "cur_lap": ("TIME", _f_clock("cur_lap")),
    "delta": ("DELTA", _f_delta),
    "air_temp": ("A", _f_temp("air_temp")),
    "track_temp": ("T", _f_temp("track_temp")),
}

# Order used to populate dropdowns in the settings editor.
METRIC_KEYS: list = list(METRICS.keys())

# Time-style metrics hide their text label in the strip (the icon is enough).
_TIME_KEYS = {"last_lap", "best_lap", "cur_lap"}


def _m_label(key: str) -> str:
    lbl = METRICS.get(key, METRICS["none"])[0]
    return lbl() if callable(lbl) else lbl


def _m_value(key: str, d: dict):
    return METRICS.get(key, METRICS["none"])[1](d)


def _m_lines(key: str, d: dict) -> list:
    """Stacked rows for cell rendering: [(sub-label, value), ...]."""
    v = _m_value(key, d)
    return v if isinstance(v, list) else [(_m_label(key), v)]


def _m_str(key: str, d: dict) -> str:
    """Single value string (first row of a stacked metric)."""
    v = _m_value(key, d)
    if isinstance(v, list):
        return v[0][1] if v else "--"
    return v


class DashWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self._shift = 0.0
        self._shift_blink = False  # dark half of the shift-light blink
        self._ped = {"t": 0.0, "b": 0.0, "c": 0.0}  # eased throttle/brake/clutch
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0
        self._animating = False
        self._font_cache: dict = {}
        self.setMinimumSize(480, 150)

    # -- data / animation --------------------------------------------------
    def set_data(self, data: dict) -> None:
        data = data or {}
        changed = data != self.data
        self.data = data
        if changed or self._animating:
            self.update()

    def _dt(self) -> float:
        now = self._clock.elapsed()
        dt = (now - self._last_ms) / 1000.0
        self._last_ms = now
        return max(0.0, min(0.1, dt))

    # -- helpers -----------------------------------------------------------
    def _cfg(self) -> dict:
        return config.CFG["dash"]

    def _col(self, key: str) -> QColor:
        cols = self._cfg()["colors"]
        return config.qcolor(cols.get(key, "#ff00ff"))

    def _font(self, px: float, bold: bool = True) -> QFont:
        fam = config.CFG.get("font_family", "Segoe UI")
        pxi = max(6, int(round(px * config.text_scale_for("dash"))))
        key = (fam, pxi, bold)
        f = self._font_cache.get(key)
        if f is None:
            f = QFont(fam)
            f.setStyleHint(QFont.StyleHint.SansSerif)
            f.setPixelSize(pxi)
            f.setBold(bold)
            if len(self._font_cache) > 256:
                self._font_cache.clear()
            self._font_cache[key] = f
        return f

    def _text_centered(self, p, center, font, text, color) -> None:
        p.setFont(font)
        p.setPen(color)
        br = QFontMetricsF(font).tightBoundingRect(text)
        x = center.x() - (br.left() + br.width() / 2)
        y = center.y() - (br.top() + br.height() / 2)
        p.drawText(QPointF(x, y), text)

    def _shift_frac(self, d) -> float:
        rpm = _num(d, "rpm")
        if rpm is None:
            return 0.0
        first, last = _num(d, "sl_first"), _num(d, "sl_last")
        if first is None or last is None or last <= first:
            redline = _num(d, "redline") or 8000.0
            first, last = redline * 0.78, redline * 0.995
        return max(0.0, min(1.0, (rpm - first) / (last - first)))

    def _selected_inputs(self, c) -> list:
        """Inputs to display, in order. Each is (eased_value, color_key, abs_on).

        Shared by both center modes so the ring and the pedal bars always show
        the same set of selected inputs.
        """
        abs_on = bool(self.data.get("abs_active"))
        spec = [
            ("show_throttle", True, "t", "pedal_throttle", False),
            ("show_brake", True, "b", "pedal_brake", abs_on),
            ("show_clutch", False, "c", "pedal_clutch", False),
        ]
        return [(self._ped[pk], colk, a)
                for cfgk, dflt, pk, colk, a in spec if c.get(cfgk, dflt)]

    # -- panel helper ------------------------------------------------------
    def _panel(self, p, rect, radius=None) -> None:
        if radius is None:
            radius = min(rect.width(), rect.height()) * 0.22
        grad = QLinearGradient(0, rect.top(), 0, rect.bottom())
        grad.setColorAt(0.0, self._col("bg_top"))
        grad.setColorAt(1.0, self._col("bg_bottom"))
        p.setPen(QPen(self._col("panel_border"), 1.2))
        p.setBrush(grad)
        p.drawRoundedRect(rect, radius, radius)

    # -- paint -------------------------------------------------------------
    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section("dash")
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        d = self.data or {}
        c = self._cfg()

        dt = self._dt()
        st = self._shift_frac(d)
        self._shift = _ease(self._shift, st, dt, 0.06)
        # Eased driver inputs, shared by the ring and the pedal bars.
        pt = max(0.0, min(1.0, _num(d, "throttle") or 0.0))
        pb = max(0.0, min(1.0, _num(d, "brake") or 0.0))
        pc = max(0.0, min(1.0, _num(d, "clutch") or 0.0))
        self._ped["t"] = _ease(self._ped["t"], pt, dt, 0.05)
        self._ped["b"] = _ease(self._ped["b"], pb, dt, 0.05)
        self._ped["c"] = _ease(self._ped["c"], pc, dt, 0.05)
        self._animating = (abs(self._shift - st) > 0.003
                           or abs(self._ped["t"] - pt) > 0.003
                           or abs(self._ped["b"] - pb) > 0.003
                           or abs(self._ped["c"] - pc) > 0.003)

        # Shift-light blink: once RPM tops out, flash the whole bar to say
        # "shift now". Forces continuous repaints while it's flashing.
        self._shift_blink = False
        if (c.get("show_shift_bar", True) and c.get("shift_blink", True)
                and self._shift >= 0.999):
            hz = float(c.get("shift_blink_hz", 7.0) or 7.0)
            if (self._clock.elapsed() * hz / 1000.0) % 1.0 >= 0.5:
                self._shift_blink = True
            self._animating = True

        # --- container geometry ------------------------------------------
        m = h * 0.045
        gp = h * 0.022             # vertical gap between top/bottom containers
        hg = w * 0.007             # horizontal gap before the position container
        show_pos = c.get("show_position", True)
        panels_top = m
        panels_bottom = h * 0.80   # the strip pill straddles below this line

        # Optional delta bar across the very top; reserve space above the panels.
        if c.get("show_delta_bar", False):
            db_h = h * 0.05
            self._draw_delta_bar(p, QRectF(m, m, (w - m) - m, db_h * 0.7), c, d)
            panels_top = m + db_h
        left_left = m
        right_edge = w - m

        total = panels_bottom - panels_top
        top_h = (total - gp) * 0.42
        bot_h = (total - gp) * 0.58

        # The position box is its OWN container at the top-right; the top row's
        # shift/status container ends before it. The bottom container spans
        # the full width underneath both.
        top_right = right_edge
        if show_pos:
            p9_w = top_h * 1.30
            p9_rect = QRectF(right_edge - p9_w, panels_top, p9_w, top_h)
            top_right = p9_rect.left() - hg

        top_rect = QRectF(left_left, panels_top, top_right - left_left, top_h)
        bot_rect = QRectF(left_left, panels_top + top_h + gp,
                          right_edge - left_left, bot_h)

        self._panel(p, top_rect)
        self._panel(p, bot_rect)
        if show_pos:
            self._draw_position(p, p9_rect, d)

        # Ring medallion: sits on the seam between the two panels (biased upward)
        # so it doesn't crowd the strip pill below.
        ring_cx = left_left + (right_edge - left_left) * 0.46
        ring_cy = panels_top + top_h + gp / 2
        ring_d = total * 0.80
        ring_half = ring_d / 2
        pad = h * 0.035
        gapL = ring_cx - ring_half - pad
        gapR = ring_cx + ring_half + pad

        # --- top container contents (shift bar | status) -----------------
        ipad = top_rect.height() * 0.22
        inc_right = top_rect.right() - ipad

        if c.get("show_shift_bar", True):
            self._draw_shift(p, QRectF(top_rect.left() + ipad,
                                       top_rect.center().y() - top_rect.height() * 0.20,
                                       gapL - (top_rect.left() + ipad),
                                       top_rect.height() * 0.40), c)
        if c.get("top_right", "incidents") not in (None, "none"):
            self._draw_status(p, QRectF(gapR, top_rect.top(),
                                        inc_right - gapR,
                                        top_rect.height()),
                              c.get("top_right", "incidents"), d)

        # --- bottom container contents (primary | stats) -----------------
        bpad = bot_rect.height() * 0.14
        if c.get("primary_left", "lap_count") not in (None, "none") \
                or c.get("primary_right", "speed") not in (None, "none"):
            self._draw_primary(p, QRectF(bot_rect.left() + bpad,
                                         bot_rect.top() + bpad,
                                         gapL - (bot_rect.left() + bpad),
                                         bot_rect.height() - bpad * 2), c, d)
        if c.get("stat_left", "tires") not in (None, "none") \
                or c.get("stat_right", "fuel_stack") not in (None, "none"):
            self._draw_stats(p, QRectF(gapR, bot_rect.top() + bpad,
                                       bot_rect.right() - bpad - gapR,
                                       bot_rect.height() - bpad * 2), c, d)

        # --- strip pill (own container) ----------------------------------
        strip_keys = [c.get("strip_left", "air_temp"),
                      c.get("strip_center", "track_temp"),
                      c.get("strip_right", "last_lap")]
        if any(k not in (None, "none") for k in strip_keys):
            pill_w = (right_edge - left_left) * 0.66
            pill_h = h * 0.26
            pill = QRectF(ring_cx - pill_w / 2, panels_bottom - pill_h * 0.28,
                          pill_w, pill_h)
            self._draw_strip(p, pill, strip_keys, d)

        # --- center medallion (floats on top of everything) --------------
        if c.get("show_ring", True):
            if c.get("center_mode", "ring") == "pedals":
                self._draw_pedals(p, ring_cx, ring_cy, ring_d, c, d)
            else:
                self._draw_ring(p, ring_cx, ring_cy, ring_d, c, d)

    # -- shift / RPM bar (segmented) ---------------------------------------
    def _draw_shift(self, p, rect, c):
        n = max(1, int(c.get("shift_segments", 20)))
        gap = rect.width() / n * 0.30
        bw = rect.width() / n - gap
        lit = self._shift * n
        red_f = max(0.0, min(1.0, c.get("shift_red_frac", 0.16)))
        yel_f = max(0.0, min(1.0 - red_f, c.get("shift_yellow_frac", 0.24)))
        red0 = n * (1.0 - red_f)
        yel0 = n * (1.0 - red_f - yel_f)
        green, off = self._col("shift_green"), self._col("shift_off")
        yel, red = self._col("shift_yellow"), self._col("shift_red")
        full_h = rect.height()
        tick_h = rect.height() * 0.5
        # During the dark half of a blink, drop every segment to the "off" tick.
        blink_dark = getattr(self, "_shift_blink", False)
        p.setPen(Qt.PenStyle.NoPen)
        for i in range(n):
            x = rect.left() + i * (bw + gap)
            if not blink_dark and i < lit:
                cc = red if i >= red0 else yel if i >= yel0 else green
                y, bh = rect.top(), full_h
            else:
                cc, y, bh = off, rect.top() + (full_h - tick_h) / 2, tick_h
            p.setBrush(cc)
            p.drawRoundedRect(QRectF(x, y, bw, bh), bw * 0.4, bw * 0.4)

    # -- status readout (icon + big value, e.g. incidents) -----------------
    def _draw_status(self, p, rect, key, d):
        val = _m_str(key, d)
        h = rect.height()
        glyph = icons.glyph(key)
        ic_f = icons.icon_font(h * 0.46)
        val_f = self._font(h * 0.46)
        iw = QFontMetricsF(ic_f).horizontalAdvance(glyph) if glyph else 0.0
        gap = h * 0.14
        vw = QFontMetricsF(val_f).horizontalAdvance(val)
        total = iw + (gap if glyph else 0.0) + vw
        # Auto-shrink so wide values (clock metrics) never clip the container.
        if total > rect.width() and total > 0:
            s = rect.width() / total
            ic_f = icons.icon_font(h * 0.46 * s)
            val_f = self._font(h * 0.46 * s)
            iw = QFontMetricsF(ic_f).horizontalAdvance(glyph) if glyph else 0.0
            vw = QFontMetricsF(val_f).horizontalAdvance(val)
            gap *= s
            total = iw + (gap if glyph else 0.0) + vw
        x = rect.left() + max(0.0, (rect.width() - total) / 2)
        if glyph:
            p.setFont(ic_f)
            # Incidents keep the warning-amber icon; other metrics use the label tint.
            p.setPen(self._col("warn") if key == "incidents" else self._col("label"))
            p.drawText(QRectF(x, rect.top(), iw + 4, h), _VC_LEFT, glyph)
            x += iw + gap
        p.setFont(val_f)
        p.setPen(self._col("value"))
        p.drawText(QRectF(x, rect.top(), vw + 6, h), _VC_LEFT, val)

    # -- primary (lower-left): a small readout + a big readout on one row ---
    def _draw_primary(self, p, rect, c, d):
        h = rect.height()
        left_key = c.get("primary_left", "lap_count")
        right_key = c.get("primary_right", "speed")
        show_l = left_key not in (None, "none")
        show_r = right_key not in (None, "none")

        # left = small group (icon + label + value); right = big value + icon.
        l_lbl = _m_label(left_key) if show_l else ""
        l_val = _m_str(left_key, d) if show_l else ""
        l_glyph = icons.glyph(left_key) if show_l else ""
        r_val = _m_str(right_key, d) if show_r else ""
        r_glyph = icons.glyph(right_key) if show_r else ""

        def sizes(s):
            return {
                "flag": h * 0.32 * s, "lbl": h * 0.28 * s, "lapv": h * 0.38 * s,
                "spd": h * 0.66 * s, "gauge": h * 0.30 * s,
                "g_icon": h * 0.12 * s, "g_lbl": h * 0.10 * s,
                "g_grp": h * 0.34 * s, "g_spd": h * 0.12 * s,
            }

        def measure(s):
            z = sizes(s)
            tot = 0.0
            if show_l:
                if l_glyph:
                    tot += (QFontMetricsF(icons.icon_font(z["flag"]))
                            .horizontalAdvance(l_glyph) + z["g_icon"])
                if l_lbl:
                    tot += (QFontMetricsF(self._font(z["lbl"]))
                            .horizontalAdvance(l_lbl) + z["g_lbl"])
                tot += QFontMetricsF(self._font(z["lapv"])).horizontalAdvance(l_val)
                if show_r:
                    tot += z["g_grp"]
            if show_r:
                tot += QFontMetricsF(self._font(z["spd"])).horizontalAdvance(r_val)
                if r_glyph:
                    tot += (z["g_spd"] + QFontMetricsF(icons.icon_font(z["gauge"]))
                            .horizontalAdvance(r_glyph))
            return tot

        need = measure(1.0)
        s = rect.width() / need if need > rect.width() and need > 0 else 1.0
        z = sizes(s)
        x = rect.left()

        def draw(font, text, color):
            nonlocal x
            p.setFont(font)
            p.setPen(color)
            wte = QFontMetricsF(font).horizontalAdvance(text)
            p.drawText(QRectF(x, rect.top(), wte + 6, h), _VC_LEFT, text)
            return wte

        if show_l:
            if l_glyph:
                x += draw(icons.icon_font(z["flag"]), l_glyph,
                          self._col("label")) + z["g_icon"]
            if l_lbl:
                x += draw(self._font(z["lbl"]), l_lbl, self._col("label")) + z["g_lbl"]
            x += draw(self._font(z["lapv"]), l_val, self._col("value"))
            if show_r:
                x += z["g_grp"]
        if show_r:
            x += draw(self._font(z["spd"]), r_val, self._col("value"))
            if r_glyph:
                x += z["g_spd"]
                draw(icons.icon_font(z["gauge"]), r_glyph, self._col("label"))

    # -- stats (two configurable stacked cells) ----------------------------
    def _draw_stats(self, p, rect, c, d):
        keys = [c.get("stat_left", "tires"), c.get("stat_right", "fuel_stack")]
        cells = [(k, _m_lines(k, d)) for k in keys if k not in (None, "none")]
        if not cells:
            return
        gap = rect.width() * 0.06
        cw = (rect.width() - gap * (len(cells) - 1)) / len(cells)
        x = rect.left()
        for key, lines in cells:
            self._draw_stat_cell(p, QRectF(x, rect.top(), cw, rect.height()),
                                 key, lines)
            x += cw + gap

    def _draw_stat_cell(self, p, rect, key, lines):
        h = rect.height()
        glyph = icons.glyph(key)
        ic_px, lbl_px, val_px = h * 0.40, h * 0.20, h * 0.24
        icon_gap, lbl_gap = h * 0.12, h * 0.08
        icon_w = (QFontMetricsF(icons.icon_font(ic_px)).horizontalAdvance(glyph)
                  + icon_gap) if glyph else 0.0
        lbl_fm = QFontMetricsF(self._font(lbl_px))
        val_fm = QFontMetricsF(self._font(val_px))
        widest = 0.0
        for lbl, val in lines:
            lw = (lbl_fm.horizontalAdvance(lbl) + lbl_gap) if lbl else 0.0
            widest = max(widest, lw + val_fm.horizontalAdvance(val))
        need = icon_w + widest + h * 0.08
        if need > rect.width() and need > 0:
            s = rect.width() / need
            ic_px, lbl_px, val_px = ic_px * s, lbl_px * s, val_px * s
            icon_gap, lbl_gap = icon_gap * s, lbl_gap * s

        x = rect.left()
        if glyph:
            p.setFont(icons.icon_font(ic_px))
            p.setPen(self._col("label"))
            gw = p.fontMetrics().horizontalAdvance(glyph)
            p.drawText(QRectF(x, rect.top(), gw + 4, h), _VC_LEFT, glyph)
            x += gw + icon_gap
        lbl_f, val_f = self._font(lbl_px), self._font(val_px)
        n = max(1, len(lines))
        for i, (lbl, val) in enumerate(lines):
            cy = rect.top() + (i + 0.5) / n * h
            row = QRectF(x, cy - h * 0.5 / n, rect.right() - x, h / n)
            tx = x
            if lbl:
                p.setFont(lbl_f)
                p.setPen(self._col("label"))
                lw = p.fontMetrics().horizontalAdvance(lbl)
                p.drawText(QRectF(tx, row.top(), lw + 4, row.height()),
                           _VC_LEFT, lbl)
                tx += lw + lbl_gap
            p.setFont(val_f)
            p.setPen(self._col("value"))
            p.drawText(QRectF(tx, row.top(), max(10.0, rect.right() - tx),
                              row.height()), _VC_LEFT, val)

    # -- position box (own container) --------------------------------------
    def _draw_position(self, p, box, d):
        v = d.get("position")
        text = f"P{int(v)}" if v else "--"
        col = self._col("orange")
        r = min(box.width(), box.height()) * 0.20
        grad = QLinearGradient(0, box.top(), 0, box.bottom())
        grad.setColorAt(0.0, self._col("bg_top"))
        grad.setColorAt(1.0, self._col("bg_bottom"))
        p.setPen(QPen(col, max(1.6, box.height() * 0.022)))
        p.setBrush(grad)
        p.drawRoundedRect(box, r, r)
        fs = box.height() * 0.40
        f = self._font(fs)
        tw = QFontMetricsF(f).horizontalAdvance(text)
        max_w = box.width() * 0.74
        if tw > max_w and tw > 0:
            f = self._font(fs * max_w / tw)
        self._text_centered(p, box.center(), f, text, col)

    # -- gear + input ring medallion (drawn on top) ------------------------
    def _draw_ring(self, p, cx, cy, ring_d, c, d):
        # Dark medallion behind the ring so it reads as floating above panels,
        # with its own border so it stands out from the containers.
        mr = ring_d / 2 + ring_d * 0.06
        border = QColor(self._col("medallion_border"))
        border.setAlpha(150)
        p.setPen(QPen(border, max(1.5, ring_d * 0.022)))
        p.setBrush(self._col("bg_bottom"))
        p.drawEllipse(QPointF(cx, cy), mr, mr)

        # One concentric arc-ring per selected input (outer -> inner).
        inputs = self._selected_inputs(c)
        n = len(inputs)
        if n:
            pen_w = ring_d * (0.11 if n == 1 else 0.075 if n == 2 else 0.055)
            gap = pen_w * 0.55
            r_out = ring_d / 2 - pen_w / 2 - ring_d * 0.015
            for i, (val, colkey, abs_on) in enumerate(inputs):
                r = r_out - i * (pen_w + gap)
                arc = QRectF(cx - r, cy - r, 2 * r, 2 * r)
                on = self._col("abs") if abs_on else self._col(colkey)
                self._draw_ring_arc(p, arc, pen_w, val, on, c)
            gear_px = ring_d * (0.50 if n == 1 else 0.40 if n == 2 else 0.32)
        else:
            gear_px = ring_d * 0.50

        self._text_centered(p, QPointF(cx, cy), self._font(gear_px),
                             _gear_str(d.get("gear")), self._col("gear"))

    def _draw_ring_arc(self, p, arc, pen_w, frac, on_color, c):
        n = max(1, int(c.get("ring_segments", 16)))
        seg = 360.0 / n
        span = seg * 0.72
        lit = max(0.0, min(1.0, frac)) * n
        off = self._col("ring_track")
        glow = QColor(on_color)
        glow.setAlpha(75)
        p.setPen(QPen(glow, pen_w * 2.0, cap=Qt.PenCapStyle.FlatCap))
        for i in range(n):
            if i < lit:
                ang = 90.0 - (i + 0.5) * seg
                p.drawArc(arc, int((ang + span / 2) * 16), int(-span * 16))
        for i in range(n):
            ang = 90.0 - (i + 0.5) * seg
            p.setPen(QPen(on_color if i < lit else off, pen_w,
                          cap=Qt.PenCapStyle.FlatCap))
            p.drawArc(arc, int((ang + span / 2) * 16), int(-span * 16))

    # -- pedal-bar medallion (throttle / brake / clutch, drawn on top) ------
    def _draw_pedals(self, p, cx, cy, ring_d, c, d):
        mr = ring_d / 2 + ring_d * 0.06
        border = QColor(self._col("medallion_border"))
        border.setAlpha(150)
        p.setPen(QPen(border, max(1.5, ring_d * 0.022)))
        p.setBrush(self._col("bg_bottom"))
        p.drawEllipse(QPointF(cx, cy), mr, mr)

        bars = self._selected_inputs(c)
        if not bars:
            # No inputs selected: just show the gear, centered.
            self._text_centered(p, QPointF(cx, cy), self._font(ring_d * 0.50),
                                _gear_str(d.get("gear")), self._col("gear"))
            return

        # Gear stays prominent at the top of the medallion.
        self._text_centered(p, QPointF(cx, cy - ring_d * 0.28),
                            self._font(ring_d * 0.26),
                            _gear_str(d.get("gear")), self._col("gear"))

        n = len(bars)
        area_w = ring_d * (0.26 if n == 1 else 0.46 if n == 2 else 0.60)
        area_h = ring_d * 0.44
        top = cy - ring_d * 0.14
        bottom = top + area_h
        bar_w = area_w / (n + (n - 1) * 0.6)
        gap = bar_w * 0.6
        x0 = cx - area_w / 2
        rad = bar_w * 0.30
        p.setPen(Qt.PenStyle.NoPen)
        for i, (val, ckey, abs_on) in enumerate(bars):
            x = x0 + i * (bar_w + gap)
            p.setBrush(self._col("pedal_track"))
            p.drawRoundedRect(QRectF(x, top, bar_w, area_h), rad, rad)
            fh = area_h * max(0.0, min(1.0, val))
            if fh > 0.5:
                p.setBrush(self._col("abs") if abs_on else self._col(ckey))
                p.drawRoundedRect(QRectF(x, bottom - fh, bar_w, fh), rad, rad)

    # -- delta bar (thin horizontal: faster green right / slower red left) --
    def _draw_delta_bar(self, p, rect, c, d):
        r = rect.height() / 2
        p.setPen(QPen(self._col("pill_border"), 1.0))
        p.setBrush(self._col("delta_bar_track"))
        p.drawRoundedRect(rect, r, r)
        cx = rect.center().x()
        delta = _num(d, "delta")
        if delta is not None:
            rng = float(c.get("delta_bar_range", 1.0)) or 1.0
            frac = max(-1.0, min(1.0, delta / rng))
            fill_w = abs(frac) * (rect.width() / 2)
            faster = delta < 0  # negative delta = ahead of best = faster
            p.setPen(Qt.PenStyle.NoPen)
            if faster:
                p.setBrush(self._col("delta_faster"))
                p.drawRoundedRect(QRectF(cx, rect.top(), fill_w, rect.height()), r, r)
            elif fill_w > 0:
                p.setBrush(self._col("delta_slower"))
                p.drawRoundedRect(QRectF(cx - fill_w, rect.top(), fill_w,
                                         rect.height()), r, r)
        # Center reference tick.
        p.setPen(QPen(self._col("label"), 1.2))
        p.drawLine(QPointF(cx, rect.top() + 1), QPointF(cx, rect.bottom() - 1))

    # -- strip pill (own container) ----------------------------------------
    def _draw_strip(self, p, pill, keys, d):
        items = [k for k in keys if k not in (None, "none")]
        sh = pill.height()
        p.setPen(QPen(self._col("pill_border"), 1.4))
        p.setBrush(self._col("pill_bg"))
        p.drawRoundedRect(pill, sh / 2, sh / 2)
        if not items:
            return

        pad = sh * 0.55
        cx0 = pill.left() + pad
        content_w = pill.width() - 2 * pad
        cell = content_w / len(items)
        val_f = self._font(sh * 0.40)
        lbl_f = self._font(sh * 0.34)
        gap = sh * 0.18
        for i, key in enumerate(items):
            label = "" if key in _TIME_KEYS else _m_label(key)
            val = _m_str(key, d)
            parts = []
            glyph = icons.glyph(key)
            if glyph:
                parts.append((icons.icon_font(sh * 0.42), glyph, self._col("label")))
            if label:
                parts.append((lbl_f, label, self._col("label")))
            parts.append((val_f, val, self._col("value")))
            widths = [QFontMetricsF(f).horizontalAdvance(t) for f, t, _ in parts]
            total = sum(widths) + gap * (len(parts) - 1)
            tx = cx0 + i * cell + (cell - total) / 2
            for (f, t, col), wp in zip(parts, widths):
                p.setFont(f)
                p.setPen(col)
                p.drawText(QRectF(tx, pill.top(), wp + 4, sh), _VC_LEFT, t)
                tx += wp + gap
