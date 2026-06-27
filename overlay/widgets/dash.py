"""
Dash / RPM widget.

A dark pill that shows, left-to-right:
  * a gear number inside a green ring (the ring fills with RPM vs redline),
  * a row of shift-light dots across the top,
  * two big labeled readouts (configurable),
  * a large position number on the right,
  * a bottom strip with three configurable items.

The two center readouts (config.dash.center: left/right) and the three bottom
items (config.dash.bottom: left/center/right) are each chosen from a metric by
name, so they can be swapped from the settings editor without touching code.

All colors, sizes and toggles come from config.CFG["dash"].
"""

from __future__ import annotations

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import (QColor, QFont, QFontMetricsF, QLinearGradient,
                         QPainter, QPen)
from PyQt6.QtWidgets import QWidget

from .. import config
from . import icons
from .table import ease


def _dcfg() -> dict:
    return config.CFG["dash"]


def _dcol(key: str) -> QColor:
    return config.qcolor(_dcfg()["colors"][key])


_DFONT_CACHE: dict = {}


def _dfont(px: float, bold: bool = True) -> QFont:
    fam = config.CFG.get("font_family", "Segoe UI")
    pxi = max(6, int(round(px * config.text_scale_for())))
    key = (fam, pxi, bold)
    f = _DFONT_CACHE.get(key)
    if f is None:
        f = QFont(fam)
        f.setStyleHint(QFont.StyleHint.SansSerif)
        f.setPixelSize(pxi)
        f.setBold(bold)
        if len(_DFONT_CACHE) > 512:
            _DFONT_CACHE.clear()
        _DFONT_CACHE[key] = f
    return f


def _gear_str(g) -> str:
    if g is None:
        return "N"
    g = int(g)
    if g < 0:
        return "R"
    if g == 0:
        return "N"
    return str(g)


def _clock(sec) -> str:
    if not sec or sec <= 0:
        return "--:--"
    m = int(sec // 60)
    s = sec - m * 60
    return f"{m}:{s:06.3f}"


def _num(d: dict, key: str):
    v = d.get(key)
    return v if isinstance(v, (int, float)) else None


# Metric registry: key -> (label, value-formatter(raw dict) -> str). A label may
# be a string or a zero-arg callable (used for unit-aware labels like KPH/MPH).
def _fmt_speed(d):  # unit-aware (follows config.units)
    v = config.conv_speed(_num(d, "speed_ms"))
    return f"{v:.0f}" if v is not None else "--"


def _fmt_speed_kph(d):
    v = _num(d, "speed_ms")
    return f"{v * 3.6:.0f}" if v is not None else "--"


def _fmt_speed_mph(d):
    v = _num(d, "speed_ms")
    return f"{v * 2.236936:.0f}" if v is not None else "--"


def _fmt_rpm(d):
    v = _num(d, "rpm")
    return f"{v:.0f}" if v is not None else "--"


def _fmt_pos(d):
    v = d.get("position")
    return f"P{int(v)}" if v else "--"


def _fmt_lap(d):
    v = d.get("lap")
    return f"{int(v)}" if isinstance(v, (int, float)) else "--"


def _fmt_fuel(d):  # unit-aware (litres or gallons)
    v = config.conv_fuel(_num(d, "fuel"))
    return f"{v:.1f}" if v is not None else "--"


def _fmt_delta(d):
    v = _num(d, "delta")
    return f"{v:+.2f}" if v is not None else "--"


def _fmt_inc(d):
    v = d.get("incidents")
    return f"{int(v)}x" if isinstance(v, (int, float)) else "--"


def _fmt_temp(key):  # unit-aware (C or F)
    def fmt(d):
        v = config.conv_temp(_num(d, key))
        return f"{v:.0f}\u00b0" if v is not None else "--"
    return fmt


METRICS: dict = {
    "none": ("", lambda d: ""),
    "speed": (config.speed_unit, _fmt_speed),
    "speed_kph": ("KPH", _fmt_speed_kph),
    "speed_mph": ("MPH", _fmt_speed_mph),
    "rpm": ("RPM", _fmt_rpm),
    "gear": ("GEAR", lambda d: _gear_str(d.get("gear"))),
    "position": ("POS", _fmt_pos),
    "lap": ("LAP", _fmt_lap),
    "fuel": (config.fuel_unit, _fmt_fuel),
    "last_lap": ("LAST", lambda d: _clock(_num(d, "last_lap"))),
    "best_lap": ("BEST", lambda d: _clock(_num(d, "best_lap"))),
    "cur_lap": ("TIME", lambda d: _clock(_num(d, "cur_lap"))),
    "delta": ("DELTA", _fmt_delta),
    "incidents": ("INC", _fmt_inc),
    "track_temp": ("TRACK", _fmt_temp("track_temp")),
    "air_temp": ("AIR", _fmt_temp("air_temp")),
}

# Ordered keys used to populate dropdowns in the settings editor.
METRIC_KEYS = [
    "none", "speed", "speed_kph", "speed_mph", "rpm", "gear", "position", "lap",
    "fuel", "last_lap", "best_lap", "cur_lap", "delta", "incidents",
    "track_temp", "air_temp",
]


def _metric(key: str):
    return METRICS.get(key, METRICS["none"])


def _label_text(key: str) -> str:
    """Resolve a metric's label (string or callable) to display text."""
    label = _metric(key)[0]
    return label() if callable(label) else label


class DashWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self._ring = 0.0
        self._shift = 0.0
        self._clock_t = QElapsedTimer()
        self._clock_t.start()
        self._last_ms = 0
        self._animating = False
        self.setMinimumSize(320, 110)

    def set_data(self, data: dict) -> None:
        data = data or {}
        changed = data != self.data
        self.data = data
        # Repaint only when something changed or an animation is still settling,
        # so a steady dash drops to 0 FPS instead of redrawing 60x/sec.
        if changed or self._animating:
            self.update()

    def _dt(self) -> float:
        now = self._clock_t.elapsed()
        dt = (now - self._last_ms) / 1000.0
        self._last_ms = now
        return max(0.0, min(0.1, dt))

    # -- geometry-independent helpers -------------------------------------
    def _shift_fraction(self, d) -> float:
        rpm = _num(d, "rpm")
        if rpm is None:
            return 0.0
        first = _num(d, "sl_first")
        last = _num(d, "sl_last")
        if first is None or last is None or last <= first:
            redline = _num(d, "redline") or 8000.0
            first, last = redline * 0.80, redline * 0.99
        return max(0.0, min(1.0, (rpm - first) / (last - first)))

    def _ring_fraction(self, d) -> float:
        src = _dcfg().get("ring_source", "rpm")
        if src == "throttle":
            return max(0.0, min(1.0, _num(d, "throttle") or 0.0))
        if src == "brake":
            return max(0.0, min(1.0, _num(d, "brake") or 0.0))
        rpm = _num(d, "rpm")
        redline = _num(d, "redline")
        if rpm is None or not redline:
            return 0.0
        return max(0.0, min(1.0, rpm / redline))

    # -- painting ----------------------------------------------------------
    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section("dash")
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        d = self.data or {}
        dc = _dcfg()

        dt = self._dt()
        ring_t = self._ring_fraction(d)
        shift_t = self._shift_fraction(d)
        self._ring = ease(self._ring, ring_t, dt, 0.10)
        self._shift = ease(self._shift, shift_t, dt, 0.06)
        self._animating = (abs(self._ring - ring_t) > 0.003
                           or abs(self._shift - shift_t) > 0.003)

        main_h = h * 0.80
        rad = main_h * 0.30

        # Gear geometry sits at the fixed left edge, independent of pill width.
        show_ring = dc.get("show_gear_ring", True)
        gear_right = 1 + main_h * (0.92 if show_ring else 0.80)
        content_left = gear_right + main_h * 0.16

        # Lay out the (tightly packed) center readouts so we know how wide the
        # content is before deciding the pill width.
        items, center_right = self._center_layout(content_left, main_h, dc, d)

        show_pos = dc.get("show_position", True)
        if show_pos:
            pw = float(w)
        else:
            # No position: end the pill just past the content (no empty space).
            pw = min(float(w), max(center_right + main_h * 0.40,
                                   content_left + main_h * 1.20))
        main = QRectF(1, 1, pw - 2, main_h - 1)

        grad = QLinearGradient(0, 0, 0, main_h)
        grad.setColorAt(0.0, _dcol("bg_top"))
        grad.setColorAt(1.0, _dcol("bg_bottom"))
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(grad)
        p.drawRoundedRect(main, rad, rad)

        if show_ring:
            self._draw_gear(p, main, main_h, d)
        else:
            self._draw_gear_plain(p, main, main_h, d)

        pos_left = w * 0.74
        if dc.get("show_shift_lights", True):
            x1 = pos_left if show_pos else center_right
            self._draw_shift_lights(p, content_left, x1, main, main_h, dc)

        self._paint_center(p, items, main, main_h)

        if show_pos:
            self._draw_position(p, pos_left, w, main, main_h, d)

        if show_pos:
            sx, sright = w * 0.16, w * 0.82
        else:
            sx = max(main_h * 0.20, content_left - main_h * 0.30)
            sright = pw - main_h * 0.18
        self._draw_bottom(p, sx, sright, h, main_h, dc, d)

    def _draw_gear(self, p, main, main_h, d):
        ring_d = main_h * 0.82
        cy = main.center().y()
        left = main.left() + main_h * 0.10
        rect = QRectF(left, cy - ring_d / 2, ring_d, ring_d)
        pen_w = ring_d * 0.10
        arc = rect.adjusted(pen_w / 2, pen_w / 2, -pen_w / 2, -pen_w / 2)

        p.setBrush(Qt.BrushStyle.NoBrush)
        p.setPen(QPen(_dcol("ring_track"), pen_w, cap=Qt.PenCapStyle.RoundCap))
        p.drawArc(arc, 0, 360 * 16)
        frac = self._ring
        if frac > 0.001:
            p.setPen(QPen(_dcol("ring_active"), pen_w, cap=Qt.PenCapStyle.RoundCap))
            p.drawArc(arc, 90 * 16, -int(360 * 16 * frac))

        inner = rect.adjusted(pen_w * 1.5, pen_w * 1.5, -pen_w * 1.5, -pen_w * 1.5)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(_dcol("bg_bottom"))
        p.drawEllipse(inner)
        p.setPen(_dcol("gear_text"))
        self._draw_glyph_centered(p, inner.center(), _dfont(ring_d * 0.50),
                                  _gear_str(d.get("gear")))
        return rect.right()

    def _draw_gear_plain(self, p, main, main_h, d):
        rect = QRectF(main.left() + main_h * 0.10, main.top(), main_h * 0.7, main_h)
        p.setPen(_dcol("gear_text"))
        self._draw_glyph_centered(p, rect.center(), _dfont(main_h * 0.50),
                                  _gear_str(d.get("gear")))
        return rect.right()

    @staticmethod
    def _draw_glyph_centered(p, center, font, text) -> None:
        """Draw text centered on its tight glyph bounds (not the font line box)."""
        p.setFont(font)
        br = QFontMetricsF(font).tightBoundingRect(text)
        x = center.x() - (br.left() + br.width() / 2)
        y = center.y() - (br.top() + br.height() / 2)
        p.drawText(QPointF(x, y), text)

    def _draw_shift_lights(self, p, x0, x1, main, main_h, dc):
        n = max(1, int(dc.get("shift_lights", 15)))
        y = main.top() + main_h * 0.20
        dot_r = main_h * 0.055
        x1 = x1 - dot_r
        x0 = x0 + dot_r
        lit = self._shift * n
        red_frac = max(0.0, min(1.0, dc.get("shift_red_frac", 0.20)))
        yel_frac = max(0.0, min(1.0 - red_frac, dc.get("shift_yellow_frac", 0.30)))
        red_start = n * (1.0 - red_frac)
        yel_start = n * (1.0 - red_frac - yel_frac)
        on_c = _dcol("shift_on")
        off_c = _dcol("shift_off")
        yel_c = _dcol("shift_yellow")
        red_c = _dcol("shift_red")
        p.setPen(Qt.PenStyle.NoPen)
        for i in range(n):
            cx = x0 + (x1 - x0) * (i / (n - 1)) if n > 1 else (x0 + x1) / 2
            if i < lit:
                if i >= red_start:
                    p.setBrush(red_c)
                elif i >= yel_start:
                    p.setBrush(yel_c)
                else:
                    p.setBrush(on_c)
            else:
                p.setBrush(off_c)
            p.drawEllipse(QRectF(cx - dot_r, y - dot_r, dot_r * 2, dot_r * 2))

    def _center_layout(self, x0, main_h, dc, d):
        """Measure the center readouts, packed left-to-right; return (items, right)."""
        center = dc.get("center", {})
        icon_cfg = dc.get("center_icons", {})
        slots = [(s, center.get(s, "none")) for s in ("left", "right")]
        slots = [(s, k) for s, k in slots if k and k != "none"]
        items = []
        x = x0
        gap = main_h * 0.55
        val_fm = QFontMetricsF(_dfont(main_h * 0.30, bold=True))
        for slot, key in slots:
            val = _metric(key)[1](d)
            if icon_cfg.get(slot) and icons.has(key):
                lf = icons.icon_font(main_h * 0.15)
                lead = icons.glyph(key)
            else:
                lf = _dfont(main_h * 0.14, bold=True)
                lead = _label_text(key)
            lw = QFontMetricsF(lf).horizontalAdvance(lead)
            vw = val_fm.horizontalAdvance(val)
            cw = max(lw, vw)
            items.append({"x": x, "lead": lead, "lead_font": lf, "val": val, "w": cw})
            x += cw + gap
        right = (x - gap) if items else x0
        return items, right

    def _paint_center(self, p, items, main, main_h):
        if not items:
            return
        align = Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter
        lbl_y = main.top() + main_h * 0.30
        val_y = main.top() + main_h * 0.47
        val_f = _dfont(main_h * 0.30, bold=True)
        for it in items:
            p.setPen(_dcol("label"))
            p.setFont(it["lead_font"])
            p.drawText(QRectF(it["x"], lbl_y, it["w"] + 6, main_h * 0.16),
                       align, it["lead"])
            p.setPen(_dcol("value"))
            p.setFont(val_f)
            p.drawText(QRectF(it["x"], val_y, it["w"] + 10, main_h * 0.34),
                       align, it["val"])

    def _draw_position(self, p, x0, w, main, main_h, d):
        v = d.get("position")
        text = f"P{int(v)}" if v else "--"
        p.setFont(_dfont(main_h * 0.38, bold=True))
        p.setPen(_dcol("position"))
        p.drawText(QRectF(x0, main.top(), w - x0 - main_h * 0.12, main_h),
                   Qt.AlignmentFlag.AlignCenter, text)

    def _draw_bottom(self, p, sx, sright, h, main_h, dc, d):
        bottom = dc.get("bottom", {})
        icon_cfg = dc.get("bottom_icons", {})
        names = ("left", "center", "right")
        slots = [(s, bottom.get(s, "none")) for s in names]
        if all(k in (None, "none") for _, k in slots):
            return
        strip_h = h * 0.26
        strip_w = max(main_h * 1.5, sright - sx)
        sy = main_h - strip_h * 0.45
        strip = QRectF(sx, sy, strip_w, strip_h)
        p.setPen(QPen(_dcol("bottom_border"), 1.4))
        p.setBrush(_dcol("bottom_bg"))
        p.drawRoundedRect(strip, strip_h / 2, strip_h / 2)

        third = strip_w / 3.0
        lbl_px = strip_h * 0.34
        icon_px = strip_h * 0.40
        val_f = _dfont(strip_h * 0.40, bold=True)
        gap = strip_h * 0.22
        for i, (slot, key) in enumerate(slots):
            if key in (None, "none"):
                continue
            cell = QRectF(sx + i * third, sy, third, strip_h)
            val = _metric(key)[1](d)
            use_icon = bool(icon_cfg.get(slot)) and icons.has(key)
            if use_icon:
                p.setFont(icons.icon_font(icon_px))
                lead = icons.glyph(key)
            else:
                p.setFont(_dfont(lbl_px, bold=True))
                lead = _label_text(key)
            lw = p.fontMetrics().horizontalAdvance(lead)
            p.setFont(val_f)
            vw = p.fontMetrics().horizontalAdvance(val)
            total = lw + gap + vw
            tx = cell.left() + (cell.width() - total) / 2
            p.setPen(_dcol("bottom_label"))
            p.setFont(icons.icon_font(icon_px) if use_icon
                      else _dfont(lbl_px, bold=True))
            p.drawText(QRectF(tx, cell.top(), lw + 2, strip_h),
                       Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                       lead)
            p.setFont(val_f)
            p.setPen(_dcol("bottom_value"))
            p.drawText(QRectF(tx + lw + gap, cell.top(), vw + 4, strip_h),
                       Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignVCenter,
                       val)
