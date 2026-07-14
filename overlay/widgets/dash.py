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
    car_number                        player car number (str)
    lap, laps_total                   current lap / total laps
    incidents                         incident count
    tire_l, tire_r                    front tire wear as 0..1 fractions
    fuel, fuel_laps                   fuel level (litres) + laps remaining
    air_temp, track_temp              temperatures in Celsius
    last_lap, best_lap, cur_lap       lap times in seconds
    delta                             delta to session best
    irating                           player iRating (int)
    irating_delta                     projected iRating change (int, race only;
                                      shown inline when dash.show_irating_projection)
"""

from __future__ import annotations

import math

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QFont, QFontMetricsF, QPainter, QPainterPath, QPen
from PyQt6.QtWidgets import QWidget

from .. import config
from .. import telemetry as tele
from .. import traffic as tr
from . import icons
from .chrome import col, draw_dark_cell, draw_panel_rect, draw_row_divider
from .fonts import data_font_bold, tabfont, tfont
from .formats import clock, signed_delta

_SECTION = "dash"

_VC_LEFT = Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft
_VC_CENTER = Qt.AlignmentFlag.AlignCenter


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


def _f_car_number(d):
    num = str(d.get("car_number", "")).strip()
    return num if num else "--"


def _f_lap_count(d):
    lap, total = _num(d, "lap"), _num(d, "laps_total")
    if lap is not None and total and total > 0:
        return f"{int(lap)}/{int(total)}"
    return f"{int(lap)}" if lap is not None else "--"


def _f_laps_left(d):
    total = _num(d, "laps_total")
    lead = _num(d, "lead_lap")
    lap = _num(d, "lap")
    if total is not None and total > 0:
        base = lead if lead is not None and lead > 0 else lap
        if base is not None:
            return f"{max(0, int(total) - int(base))}"
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


def _f_tires_4(d):
    keys = (("FL", "tire_lf"), ("FR", "tire_rf"),
            ("RL", "tire_lr"), ("RR", "tire_rr"))
    rows = []
    for lbl, k in keys:
        v = _num(d, k)
        rows.append((lbl, f"{v * 100:.0f}%" if v is not None else "--"))
    return rows


def _f_tire_temp(d):
    v = _num(d, "tire_temp_max")
    if v is not None:
        t = config.conv_temp(v)
        return f"{t:.0f}\u00b0" if t is not None else "--"
    keys = (("FL", "tire_temp_lf"), ("FR", "tire_temp_rf"),
            ("RL", "tire_temp_lr"), ("RR", "tire_temp_rr"))
    rows = []
    for lbl, k in keys:
        tv = _num(d, k)
        if tv is not None:
            t = config.conv_temp(tv)
            rows.append((lbl, f"{t:.0f}\u00b0" if t is not None else "--"))
        else:
            rows.append((lbl, "--"))
    if rows and all(r[1] == "--" for r in rows):
        return "--"
    return rows


def _f_fuel_pct(d):
    v = _num(d, "fuel_pct")
    return f"{v * 100:.0f}%" if v is not None else "--"


def _f_fuel_burn(d):
    v = _num(d, "fuel_burn")
    if v is None:
        return "--"
    cv = config.conv_fuel(v)
    return f"{cv:.1f} {config.fuel_unit()}/h" if cv is not None else "--"


def _f_delta_key(key):
    def fmt(d):
        v = _num(d, key)
        return signed_delta(v, places=2) if v is not None else "--"
    return fmt


def _f_time_remain(d):
    v = _num(d, "time_remain")
    return clock(v) if v is not None else "--"


def _f_class_pos(d):
    pos, total = _num(d, "class_pos"), _num(d, "class_total")
    if pos is not None and total and total > 0:
        return f"P{int(pos)}/{int(total)}"
    return f"P{int(pos)}" if pos is not None else "--"


def _f_inc_team(d):
    v = _num(d, "incidents_team")
    return f"{int(v)}x" if v is not None else "--"


def _f_inc_limit(d):
    count = _num(d, "incidents")
    limit = _num(d, "incident_limit")
    if count is not None and limit is not None and limit > 0:
        return f"{int(count)}/{int(limit)}x"
    if count is not None:
        return f"{int(count)}x"
    return "--"


def _f_dc(key, suffix=""):
    def fmt(d):
        v = _num(d, key)
        if v is None:
            return "--"
        if suffix == "%":
            return f"{v:.0f}%"
        return f"{v:.0f}"
    return fmt


def _f_engine_warn(d):
    w = d.get("engine_warnings")
    return tr.engine_warning_text(w)


def _f_voltage(d):
    v = _num(d, "voltage")
    return f"{v:.1f}V" if v is not None else "--"


def _f_gap_ahead(d):
    v = _num(d, "gap_ahead")
    return f"+{v:.1f}" if v is not None else "--"


def _f_gap_behind(d):
    v = _num(d, "gap_behind")
    return f"+{v:.1f}" if v is not None else "--"


def _f_lap_corners(d):
    return str(d.get("lap_corners") or "--")


def _f_inc(d):
    v = _num(d, "incidents")
    return f"{int(v)}x" if v is not None else "--"


def _f_gear(d):
    return _gear_str(d.get("gear"))


def _f_delta(d):
    v = _num(d, "delta")
    return signed_delta(v, places=2) if v is not None else "--"


def _f_clock(key):
    return lambda d: clock(_num(d, key))


def _fmt_irating_val(v) -> str:
    if v is None:
        return "--"
    ir = int(round(v))
    dc = config.CFG.get("dash", {})
    if dc.get("irating_abbreviate", True) and ir >= 1000:
        return f"{ir / 1000:.1f}k"
    return str(ir)


def _f_irating(d):
    return _fmt_irating_val(_num(d, "irating"))


def _dash_show_irating_delta(d: dict) -> bool:
    dc = config.CFG.get("dash", {})
    if not dc.get("show_irating_projection"):
        return False
    if d.get("irating_delta") is None:
        return False
    return _num(d, "irating") is not None


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
    "car_number": ("#", _f_car_number),
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
    "my_session_best": ("SBEST", _f_clock("my_session_best")),
    "cur_lap": ("TIME", _f_clock("cur_lap")),
    "delta": ("DELTA", _f_delta),
    "irating": ("iR", _f_irating),
    "air_temp": ("A", _f_temp("air_temp")),
    "track_temp": ("T", _f_temp("track_temp")),
    "tires_4": ("TIRE", _f_tires_4),
    "tire_temp": ("TEMP", _f_tire_temp),
    "fuel_pct": ("FUEL%", _f_fuel_pct),
    "fuel_burn": ("BURN", _f_fuel_burn),
    "delta_best": ("BEST", _f_delta_key("delta_best")),
    "delta_optimal": ("OPT", _f_delta_key("delta_optimal")),
    "time_remain": ("TGO", _f_time_remain),
    "class_pos": ("CPOS", _f_class_pos),
    "incidents_team": ("TEAM", _f_inc_team),
    "incidents_limit": ("INC", _f_inc_limit),
    "dc_brake_bias": ("BB", _f_dc("dc_brake_bias", "%")),
    "dc_tc": ("TC", _f_dc("dc_traction_control")),
    "dc_abs": ("ABS", _f_dc("dc_abs")),
    "dc_fuel_mix": ("MIX", _f_dc("dc_fuel_mixture")),
    "dc_tire_set": ("SET", _f_dc("dc_tire_set")),
    "engine_warn": ("ENG", _f_engine_warn),
    "oil_temp": ("OIL", _f_temp("oil_temp")),
    "water_temp": ("H2O", _f_temp("water_temp")),
    "voltage": ("V", _f_voltage),
    "gap_ahead": ("AHD", _f_gap_ahead),
    "gap_behind": ("BHD", _f_gap_behind),
    "lap_corners": ("COR", _f_lap_corners),
}

# Order used to populate dropdowns in the settings editor.
METRIC_KEYS: list = list(METRICS.keys())


def _m_label(key: str) -> str:
    lbl = METRICS.get(key, METRICS["none"])[0]
    return lbl() if callable(lbl) else lbl


def _display_label(key: str) -> str:
    """Text label for a metric, or empty when an icon replaces it."""
    if icons.glyph(key):
        return ""
    return _m_label(key)


def _m_value(key: str, d: dict):
    return METRICS.get(key, METRICS["none"])[1](d)


def _m_lines(key: str, d: dict) -> list:
    """Stacked rows for cell rendering: [(sub-label, value), ...]."""
    v = _m_value(key, d)
    if isinstance(v, list):
        # Drop a redundant metric-name row label when the icon already names it.
        metric_lbl = _m_label(key)
        if (icons.glyph(key) and v and metric_lbl
                and str(v[0][0]).upper() == str(metric_lbl).upper()):
            return [("", v[0][1]), *v[1:]]
        return v
    return [(_display_label(key), v)]


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
        self._shift_blink_since_ms: int | None = None
        self._shift_blink_suppressed = False
        self._ped = {"t": 0.0, "b": 0.0, "c": 0.0}  # eased throttle/brake/clutch
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0
        self._animating = False
        # Flag bar: which flag is showing and when it first appeared, so it can
        # pulse briefly on arrival and then hold steady.
        self._flag_shown = None
        self._flag_since_ms = 0
        self.setMinimumSize(480, 150)

    # -- data / animation --------------------------------------------------
    def set_data(self, data: dict) -> None:
        data = data or {}
        flag = data.get("flag")
        if flag != self._flag_shown:  # a new flag (re)starts the pulse window
            self._flag_shown = flag
            self._flag_since_ms = self._clock.elapsed()
        prev = self.data
        self.data = data
        discrete = tele.dash_discrete_key(data)
        prev_discrete = tele.dash_discrete_key(prev) if prev else None
        if (discrete != prev_discrete
                or tele.dash_easing_moved(prev, data)
                or tele.dash_live_moved(prev, data)
                or self._animating):
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
        return col(key, _SECTION)

    def _lbl_font(self, px: float) -> QFont:
        return tfont(px, bold=True)

    def _val_font(self, px: float) -> QFont:
        return tabfont(px, bold=data_font_bold(_SECTION))

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

    def _should_blink(self, d, c) -> bool:
        """True when the shift light should flash: at the shift RPM and not in
        top gear (no upshift available there)."""
        if not c.get("shift_blink", True):
            return False
        rpm = _num(d, "rpm")
        if rpm is None:
            return False
        # No point flashing in (or above) the highest forward gear.
        gear = _num(d, "gear")
        top = _num(d, "top_gear")
        if top and gear is not None and gear >= top:
            return False
        # Blink at a fraction of the car's redline (configurable). This is more
        # predictable than iRacing's shift-light RPMs, which are often set well
        # below the limiter and made the bar flash too early; iRacing's blink /
        # shift RPM is only a fallback when the redline isn't reported.
        pct = float(c.get("shift_blink_pct", 0.99) or 0.99)
        redline = _num(d, "redline")
        if redline:
            blink_rpm = redline * pct
        else:
            blink_rpm = _num(d, "sl_blink") or _num(d, "sl_shift") or 8000.0 * pct
        return rpm >= blink_rpm

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

        # Shift-light blink: at the shift RPM, flash the whole bar to say "shift
        # now" (but never in top gear, where there's nothing to shift up to).
        # After shift_blink_max_sec of continuous eligibility, stop flashing until
        # RPM drops below the threshold (then blink again on the next climb).
        # Forces continuous repaints while it's flashing.
        self._shift_blink = False
        if c.get("show_shift_bar", True) and self._should_blink(d, c):
            now_ms = self._clock.elapsed()
            if self._shift_blink_since_ms is None:
                self._shift_blink_since_ms = now_ms
                self._shift_blink_suppressed = False
            max_sec = float(c.get("shift_blink_max_sec", 3.0) or 3.0)
            if (not self._shift_blink_suppressed
                    and max_sec > 0
                    and (now_ms - self._shift_blink_since_ms) / 1000.0 >= max_sec):
                self._shift_blink_suppressed = True
            if not self._shift_blink_suppressed:
                hz = float(c.get("shift_blink_hz", 7.0) or 7.0)
                if (now_ms * hz / 1000.0) % 1.0 >= 0.5:
                    self._shift_blink = True
                self._animating = True
        else:
            self._shift_blink_since_ms = None
            self._shift_blink_suppressed = False

        # --- container geometry ------------------------------------------
        m = h * 0.045
        gp = h * 0.022             # vertical gap between top/bottom containers
        hg = w * 0.007             # horizontal gap before the position container
        show_pos = c.get("show_position", True)
        panels_top = m
        panels_bottom = h * 0.80   # the strip pill straddles below this line

        # Thin flag bar across the top; always reserve its strip (+ a small gap)
        # so the layout stays put whether or not a flag is showing. Drawn full
        # width over the position box at the end of paintEvent.
        flag_top = None
        flag_bar_h = 0.0
        if c.get("show_flags", True):
            ctx = d.get("flag_context") if d.get("flag") else None
            flag_bar_h = max(8.0 if ctx else 6.0, h * (0.165 if ctx else 0.105))
            flag_top = panels_top
            panels_top += flag_bar_h + h * 0.03   # small gap below the bar

        # Optional delta bar across the top; reserve space above the panels.
        delta_bar_geom = None
        if c.get("show_delta_bar", False):
            db_h = h * 0.05
            delta_bar_geom = (panels_top, db_h * 0.7)
            panels_top += db_h
        left_left = m
        right_edge = w - m
        bar_w = right_edge - left_left

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

        # Flag bar spans the full dash width (including over the position box).
        flag_rect = None
        if flag_top is not None:
            flag_rect = QRectF(left_left, flag_top, bar_w, flag_bar_h)

        top_rect = QRectF(left_left, panels_top, top_right - left_left, top_h)
        bot_rect = QRectF(left_left, panels_top + top_h + gp,
                          right_edge - left_left, bot_h)

        draw_panel_rect(p, top_rect, _SECTION)
        draw_panel_rect(p, bot_rect, _SECTION)
        if show_pos:
            self._draw_position(p, p9_rect, d)

        # Ring medallion: centered on the full dash panel area (includes position box).
        ring_cx = (left_left + right_edge) / 2
        ring_cy = panels_top + total / 2
        ring_d = total * 0.80
        ring_half = ring_d / 2
        bpad = bot_rect.height() * 0.14
        stat_h = bot_rect.height() - bpad * 2
        base_pad = h * 0.035
        # Size left content against the normal ring pad; extra iRating clearance
        # (gapL) only adds visual mph–ring whitespace and must not shrink fonts.
        base_gapL = ring_cx - ring_half - base_pad
        left_pad = self._ring_left_clearance(ring_cx, ring_half, base_pad,
                                             bot_rect, bpad, stat_h, c, d)
        gapL = ring_cx - ring_half - left_pad
        gapR = ring_cx + ring_half + base_pad

        # --- top container contents (shift bar | status) -----------------
        ipad = top_rect.height() * 0.22
        inc_right = top_rect.right() - ipad

        if c.get("show_shift_bar", True):
            self._draw_shift(p, QRectF(top_rect.left() + ipad,
                                       top_rect.center().y() - top_rect.height() * 0.20,
                                       base_gapL - (top_rect.left() + ipad),
                                       top_rect.height() * 0.40), c)
        if c.get("top_right", "incidents") not in (None, "none"):
            self._draw_status(p, QRectF(gapR, top_rect.top(),
                                        inc_right - gapR,
                                        top_rect.height()),
                              c.get("top_right", "incidents"), d)

        # --- bottom container contents (primary | stats) -----------------
        if c.get("primary_left", "lap_count") not in (None, "none") \
                or c.get("primary_right", "speed") not in (None, "none"):
            # Two equal columns in the left strip; each metric left-aligned.
            primary_left = bot_rect.left() + bpad
            self._draw_primary(
                p, QRectF(primary_left, bot_rect.top() + bpad,
                          max(0.0, gapL - primary_left),
                          bot_rect.height() - bpad * 2),
                c, d, fit_width=max(0.0, base_gapL - primary_left))
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
            pill_h = h * 0.22
            pill = QRectF(ring_cx - pill_w / 2,
                          panels_bottom - pill_h * 0.28 + h * 0.02,
                          pill_w, pill_h)
            self._draw_strip(p, pill, strip_keys, d)

        # --- center medallion (floats on top of everything) --------------
        if c.get("show_ring", True):
            if c.get("center_mode", "ring") == "pedals":
                self._draw_pedals(p, ring_cx, ring_cy, ring_d, c, d)
            else:
                self._draw_ring(p, ring_cx, ring_cy, ring_d, c, d)

        # --- flag bar (yellow / black / green) on top of everything ------
        if flag_rect is not None:
            self._draw_flag(p, flag_rect, c, d.get("flag"), ring_cx,
                            d.get("flag_context"))

        if delta_bar_geom is not None:
            delta_top, delta_bar_h = delta_bar_geom
            self._draw_delta_bar(
                p, QRectF(left_left, delta_top, bar_w, delta_bar_h), c, d)

        if self._animating:
            self.update()

    # -- flag bar ----------------------------------------------------------
    def _draw_flag(self, p, rect, c, flag, center_x=None, context=None) -> None:
        """A thin colored bar across the top of the dash with a label. Color +
        text convey the flag (yellow / black / green); it waves by flashing."""
        spec = {
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
        }.get(flag)
        if spec is None:
            return
        label, bg_key, fg_key = spec
        # Pulse (flash) only for the first couple of seconds after the flag
        # appears, then hold steady so it doesn't flash for the whole stint.
        hz = float(c.get("flag_blink_hz", 2.5) or 2.5)
        pulse_on = bool(c.get("flag_pulse", True))
        pulse_s = float(c.get("flag_pulse_seconds", 1.5) or 0.0)
        since = (self._clock.elapsed() - self._flag_since_ms) / 1000.0
        if pulse_on and since < pulse_s:
            on = (self._clock.elapsed() * hz / 1000.0) % 1.0 < 0.5
            self._animating = True  # keep repainting through the pulse window
        else:
            on = True  # solid once the pulse window ends (or if pulsing is off)

        bg = self._col(bg_key)
        if not on:  # dim half of the wave
            bg = bg.darker(180)
        fg = self._col(fg_key)
        r = rect.height() * 0.5

        # base bar
        p.setBrush(bg)
        # A faint light edge defines the bar (and makes the black flag visible).
        p.setPen(QPen(QColor(255, 255, 255, 45), 1))
        p.drawRoundedRect(rect, r, r)

        # fill pattern (clipped to the bar's rounded shape): a black/white weave
        # for the checkered flag, diagonal slashes for everything else.
        p.save()
        clip = QPainterPath()
        clip.addRoundedRect(rect, r, r)
        p.setClipPath(clip)
        if flag == "checkered":
            self._draw_checker(p, rect, fg)
        else:
            hatch = QColor(fg)
            hatch.setAlpha(70)
            p.setPen(QPen(hatch, max(2.0, rect.height() * 0.16)))
            step = rect.height() * 0.6
            x = rect.left() - rect.height()
            while x < rect.right() + rect.height():
                p.drawLine(QPointF(x, rect.bottom()),
                           QPointF(x + rect.height(), rect.top()))
                x += step
        p.restore()

        context = str(context).strip() if context else ""
        cx = center_x if center_x is not None else rect.center().x()
        if context:
            title_font = self._lbl_font(rect.height() * 0.36)
            sub_font = tfont(rect.height() * 0.24, bold=False)
            tw = max(QFontMetricsF(title_font).horizontalAdvance(label),
                     QFontMetricsF(sub_font).horizontalAdvance(context))
            pad = rect.height() * 0.55
            gap = QRectF(cx - tw / 2 - pad, rect.top(), tw + pad * 2, rect.height())
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(bg)
            p.drawRoundedRect(gap, gap.height() * 0.5, gap.height() * 0.5)
            title_y = rect.center().y() - rect.height() * 0.26
            sub_y = rect.center().y() + rect.height() * 0.28
            self._text_centered(p, QPointF(cx, title_y), title_font, label, fg)
            sub_fg = QColor(fg)
            sub_fg.setAlpha(min(255, int(fg.alpha() * 0.88)))
            self._text_centered(p, QPointF(cx, sub_y), sub_font, context, sub_fg)
            return

        # clear a rounded gap behind the text so the slashes frame it
        font = self._lbl_font(rect.height() * 0.52)
        tw = QFontMetricsF(font).horizontalAdvance(label)
        pad = rect.height() * 0.6
        gap = QRectF(cx - tw / 2 - pad, rect.top(), tw + pad * 2, rect.height())
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(bg)
        p.drawRoundedRect(gap, gap.height() * 0.5, gap.height() * 0.5)

        self._text_centered(p, QPointF(cx, rect.center().y()), font, label, fg)

    @staticmethod
    def _draw_checker(p, rect, color) -> None:
        """A two-row black/white checkerboard filling the (already-clipped) bar."""
        sq = rect.height() / 2.0
        cols = int(rect.width() / sq) + 2
        cell = QColor(color)
        cell.setAlpha(180)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(cell)
        for ri in range(2):
            for ci in range(cols):
                if (ri + ci) % 2 == 0:
                    p.drawRect(QRectF(rect.left() + ci * sq,
                                      rect.top() + ri * sq, sq, sq))

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
        if key == "irating":
            h = rect.height()
            val_px = h * 0.34
            pair_w = self._irating_pair_width(d, val_px, h)
            if pair_w > rect.width() and pair_w > 0:
                val_px *= rect.width() / pair_w
            self._draw_irating_pair(p, rect, d, val_px)
            return
        val = _m_str(key, d)
        h = rect.height()
        glyph = icons.glyph(key)
        ic_f = icons.icon_font(h * 0.46)
        val_f = self._val_font(h * 0.46)
        iw = QFontMetricsF(ic_f).horizontalAdvance(glyph) if glyph else 0.0
        gap = h * 0.14
        vw = QFontMetricsF(val_f).horizontalAdvance(val)
        total = iw + (gap if glyph else 0.0) + vw
        # Auto-shrink so wide values (clock metrics) never clip the container.
        if total > rect.width() and total > 0:
            s = rect.width() / total
            ic_f = icons.icon_font(h * 0.46 * s)
            val_f = self._val_font(h * 0.46 * s)
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

    # -- primary (lower-left): two equal columns when both set; else centered --
    def _draw_primary(self, p, rect, c, d, fit_width=None):
        h = rect.height()
        left_key = c.get("primary_left", "lap_count")
        right_key = c.get("primary_right", "speed")
        show_l = left_key not in (None, "none")
        show_r = right_key not in (None, "none")
        if not show_l and not show_r:
            return

        def sizes(s):
            val = h * 0.58 * s
            return {
                "flag": h * 0.32 * s, "lbl": h * 0.28 * s, "val": val,
                "gauge": h * 0.30 * s,
                "g_icon": h * 0.12 * s, "g_lbl": h * 0.10 * s,
                "g_spd": h * 0.12 * s,
            }

        def metric_width(key, glyph, lbl, val, z, icon_key="flag", gap_key="g_icon"):
            tot = 0.0
            if glyph:
                tot += (QFontMetricsF(icons.icon_font(z[icon_key]))
                        .horizontalAdvance(glyph) + z[gap_key])
            if lbl:
                tot += (QFontMetricsF(self._lbl_font(z["lbl"]))
                        .horizontalAdvance(lbl) + z["g_lbl"])
            if key == "irating":
                tot += self._irating_pair_width(d, z["val"], h)
            else:
                tot += QFontMetricsF(self._val_font(z["val"])).horizontalAdvance(val)
            return tot

        def draw_metric(col: QRectF, key, glyph, lbl, val, *,
                        icon_key="flag", gap_key="g_icon",
                        align_right: bool = False):
            fit = col.width()
            need = metric_width(key, glyph, lbl, val, sizes(1.0),
                                icon_key=icon_key, gap_key=gap_key)
            s = fit / need if need > fit and need > 0 else 1.0
            z = sizes(s)
            total_w = metric_width(key, glyph, lbl, val, z,
                                   icon_key=icon_key, gap_key=gap_key)
            x = (col.right() - total_w) if align_right else col.left()

            def draw(font, text, color):
                nonlocal x
                p.setFont(font)
                p.setPen(color)
                wte = QFontMetricsF(font).horizontalAdvance(text)
                p.drawText(QRectF(x, col.top(), wte + 6, h), _VC_LEFT, text)
                return wte

            if glyph:
                x += draw(icons.icon_font(z[icon_key]), glyph,
                          self._col("label")) + z[gap_key]
            if lbl:
                x += draw(self._lbl_font(z["lbl"]), lbl, self._col("label")) + z["g_lbl"]
            if key == "irating":
                pair_w = self._irating_pair_width(d, z["val"], h)
                self._draw_irating_pair(
                    p, QRectF(x, col.top(), pair_w, h), d, z["val"])
            else:
                draw(self._val_font(z["val"]), val, self._col("value"))

        l_lbl = _display_label(left_key) if show_l else ""
        l_val = _m_str(left_key, d) if show_l and left_key != "irating" else ""
        l_glyph = icons.glyph(left_key) if show_l else ""
        r_lbl = _display_label(right_key) if show_r else ""
        r_val = _m_str(right_key, d) if show_r and right_key != "irating" else ""
        r_glyph = icons.glyph(right_key) if show_r else ""

        if show_l and show_r:
            # Two evenly spaced columns; each metric right-aligned in its column.
            col_w = rect.width() * 0.5
            draw_metric(QRectF(rect.left(), rect.top(), col_w, h),
                        left_key, l_glyph, l_lbl, l_val, align_right=True)
            draw_metric(QRectF(rect.left() + col_w, rect.top(), col_w, h),
                        right_key, r_glyph, r_lbl, r_val,
                        icon_key="gauge", gap_key="g_spd", align_right=True)
            return

        # Single active metric: center the group in the strip.
        key = left_key if show_l else right_key
        glyph = l_glyph if show_l else r_glyph
        lbl = l_lbl if show_l else r_lbl
        val = l_val if show_l else r_val
        icon_key = "flag" if show_l else "gauge"
        gap_key = "g_icon" if show_l else "g_spd"
        fit = rect.width() if fit_width is None else fit_width
        need = metric_width(key, glyph, lbl, val, sizes(1.0),
                            icon_key=icon_key, gap_key=gap_key)
        s = fit / need if need > fit and need > 0 else 1.0
        z = sizes(s)
        total_w = metric_width(key, glyph, lbl, val, z,
                               icon_key=icon_key, gap_key=gap_key)
        x0 = rect.left() + max(0.0, (rect.width() - total_w) / 2)
        draw_metric(QRectF(x0, rect.top(), max(total_w, fit), h),
                    key, glyph, lbl, val, icon_key=icon_key, gap_key=gap_key)

    # -- stats (two configurable stacked cells) ----------------------------
    def _draw_stats(self, p, rect, c, d):
        keys = [c.get("stat_left", "tires"), c.get("stat_right", "fuel_stack")]
        cells = [(k, _m_lines(k, d)) for k in keys if k not in (None, "none")]
        if not cells:
            return
        gap = rect.width() * 0.06
        cw = (rect.width() - gap * (len(cells) - 1)) / len(cells)
        x = rect.left()
        center = len(cells) == 1
        for key, lines in cells:
            self._draw_stat_cell(p, QRectF(x, rect.top(), cw, rect.height()),
                                 key, lines, d, center=center)
            x += cw + gap

    def _delta_color(self, delta: int | float) -> QColor:
        if delta > 0:
            return self._col("irating_delta_up")
        if delta < 0:
            return self._col("irating_delta_down")
        return self._col("muted")

    def _ring_left_clearance(self, ring_cx, ring_half, base_pad,
                             bot_rect, bpad, stat_h, c, d) -> float:
        """Match mph clearance to the ring-to-iRating gap on the stats side."""
        keys = [c.get("stat_left", "tires"), c.get("stat_right", "fuel_stack")]
        keys = [k for k in keys if k not in (None, "none")]
        if "irating" not in keys:
            return base_pad

        gapR = ring_cx + ring_half + base_pad
        stats_w = max(0.0, bot_rect.right() - bpad - gapR)
        if stats_w <= 0:
            return base_pad

        n = len(keys)
        cell_gap = stats_w * 0.06
        cw = (stats_w - cell_gap * (n - 1)) / n
        idx = keys.index("irating")
        val_px = stat_h * 0.24
        ir_w = self._irating_pair_width(d, val_px, stat_h)
        offset_in_stats = idx * (cw + cell_gap) + max(0.0, (cw - ir_w) / 2)
        return base_pad + offset_in_stats

    def _stat_icon_gap(self, h: float) -> float:
        return h * 0.20

    def _irating_layout(self, val_px: float) -> tuple[float, float, float]:
        pad_x = val_px * 0.58
        ir_delta_gap = val_px * 0.50
        delta_icon_gap = val_px * 0.10
        return pad_x, ir_delta_gap, delta_icon_gap

    def _irating_outer_icon_px(self, rect_h: float) -> float:
        return rect_h * 0.40

    def _irating_icon_outer_width(self, rect_h: float) -> float:
        glyph = icons.glyph("irating")
        if not glyph:
            return 0.0
        ic_px = self._irating_outer_icon_px(rect_h)
        ic_f = icons.icon_font(ic_px)
        return QFontMetricsF(ic_f).horizontalAdvance(glyph) + self._stat_icon_gap(rect_h)

    def _irating_delta_width(self, delta: int, val_px: float) -> float:
        _, _, delta_icon_gap = self._irating_layout(val_px)
        use_icons = icons.has("irating_up") and delta != 0
        if use_icons:
            iglyph = icons.glyph("irating_up" if delta > 0 else "irating_down")
            ic_f = icons.icon_font(val_px * 0.55)
            val_f = self._val_font(val_px * 0.78)
            ic_w = QFontMetricsF(ic_f).horizontalAdvance(iglyph)
            num_w = QFontMetricsF(val_f).horizontalAdvance(str(abs(delta)))
            return ic_w + delta_icon_gap + num_w
        return QFontMetricsF(self._val_font(val_px)).horizontalAdvance(f"{delta:+d}")

    def _irating_pill_content_width(self, d: dict, val_px: float) -> float:
        val_f = self._val_font(val_px)
        ir_w = QFontMetricsF(val_f).horizontalAdvance(_f_irating(d))
        if not _dash_show_irating_delta(d):
            return ir_w
        _, ir_delta_gap, _ = self._irating_layout(val_px)
        return ir_w + ir_delta_gap + self._irating_delta_width(int(d["irating_delta"]), val_px)

    def _irating_pill_width(self, d: dict, val_px: float) -> float:
        pad_x, _, _ = self._irating_layout(val_px)
        return self._irating_pill_content_width(d, val_px) + 2 * pad_x

    def _irating_pair_width(self, d: dict, val_px: float, rect_h: float) -> float:
        return self._irating_icon_outer_width(rect_h) + self._irating_pill_width(d, val_px)

    def _irating_pill_rect(self, pill_left: float, rect: QRectF,
                           d: dict, val_px: float) -> QRectF:
        pill_w = min(self._irating_pill_width(d, val_px),
                     max(0.0, rect.width() - (pill_left - rect.left())))
        pill_h = rect.height() * 0.68
        return QRectF(pill_left, rect.top() + (rect.height() - pill_h) / 2,
                      pill_w, pill_h)

    def _draw_irating_pair(self, p, rect, d: dict, val_px: float) -> None:
        rect_h = rect.height()
        pill_w = self._irating_pill_width(d, val_px)
        icon_outer = self._irating_icon_outer_width(rect_h)
        total_w = icon_outer + pill_w
        block_left = rect.left() + max(0.0, (rect.width() - total_w) / 2)
        pad_x, ir_delta_gap, delta_icon_gap = self._irating_layout(val_px)

        glyph = icons.glyph("irating")
        pill_left = block_left
        if glyph:
            ic_f = icons.icon_font(self._irating_outer_icon_px(rect_h))
            ic_w = QFontMetricsF(ic_f).horizontalAdvance(glyph)
            p.setFont(ic_f)
            p.setPen(self._col("label"))
            p.drawText(QRectF(block_left, rect.top(), ic_w, rect_h), _VC_LEFT, glyph)
            pill_left = block_left + ic_w + self._stat_icon_gap(rect_h)

        cell = self._irating_pill_rect(pill_left, rect, d, val_px)
        p.setPen(QPen(self._col("irating_border"), 1))
        p.setBrush(self._col("irating_bg"))
        p.drawRoundedRect(cell, 4, 4)

        val_f = self._val_font(val_px)
        ir_txt = _f_irating(d)
        ir_w = QFontMetricsF(val_f).horizontalAdvance(ir_txt)
        content_w = self._irating_pill_content_width(d, val_px)
        x = cell.left() + max(pad_x, (cell.width() - content_w) / 2)

        p.setFont(val_f)
        p.setPen(self._col("irating_text"))
        p.drawText(QRectF(x, cell.top(), ir_w, cell.height()),
                   _VC_LEFT, ir_txt)

        if not _dash_show_irating_delta(d):
            return

        delta = int(d["irating_delta"])
        dx = x + ir_w + ir_delta_gap
        dcol = self._delta_color(delta)
        use_icons = icons.has("irating_up") and delta != 0
        if use_icons:
            iglyph = icons.glyph("irating_up" if delta > 0 else "irating_down")
            ifont = icons.icon_font(val_px * 0.55)
            nfont = self._val_font(val_px * 0.78)
            dtxt = str(abs(delta))
            i_w = QFontMetricsF(ifont).horizontalAdvance(iglyph)
            n_w = QFontMetricsF(nfont).horizontalAdvance(dtxt)
            p.setFont(ifont)
            p.setPen(dcol)
            p.drawText(QRectF(dx, cell.top(), i_w, cell.height()), _VC_LEFT, iglyph)
            p.setFont(nfont)
            p.setPen(dcol)
            p.drawText(QRectF(dx + i_w + delta_icon_gap, cell.top(), n_w, cell.height()),
                       _VC_LEFT, dtxt)
        else:
            dtxt = f"{delta:+d}" if delta else "0"
            p.setFont(val_f)
            p.setPen(dcol)
            d_w = QFontMetricsF(val_f).horizontalAdvance(dtxt)
            p.drawText(QRectF(dx, cell.top(), d_w, cell.height()), _VC_LEFT, dtxt)

    def _draw_stat_cell(self, p, rect, key, lines, d: dict, *, center: bool = False):
        h = rect.height()
        glyph = "" if key == "irating" else icons.glyph(key)
        ic_px, lbl_px, val_px = h * 0.40, h * 0.20, h * 0.24
        icon_gap, lbl_gap = self._stat_icon_gap(h), h * 0.08
        icon_w = (QFontMetricsF(icons.icon_font(ic_px)).horizontalAdvance(glyph)
                  + icon_gap) if glyph else 0.0
        lbl_fm = QFontMetricsF(self._lbl_font(lbl_px))
        val_fm = QFontMetricsF(self._val_font(val_px))
        widest = 0.0
        for lbl, val in lines:
            if key == "irating":
                widest = max(widest, self._irating_pair_width(d, val_px, h))
                continue
            lw = (lbl_fm.horizontalAdvance(lbl) + lbl_gap) if lbl else 0.0
            widest = max(widest, lw + val_fm.horizontalAdvance(val))
        need = icon_w + widest + h * 0.08
        if need > rect.width() and need > 0:
            s = rect.width() / need
            ic_px, lbl_px, val_px = ic_px * s, lbl_px * s, val_px * s
            icon_gap, lbl_gap = icon_gap * s, lbl_gap * s
            need = rect.width()

        x = rect.left()
        if center and need < rect.width():
            x = rect.left() + (rect.width() - need) / 2
        if glyph:
            p.setFont(icons.icon_font(ic_px))
            p.setPen(self._col("label"))
            gw = p.fontMetrics().horizontalAdvance(glyph)
            p.drawText(QRectF(x, rect.top(), gw + 4, h), _VC_LEFT, glyph)
            x += gw + icon_gap
        lbl_f, val_f = self._lbl_font(lbl_px), self._val_font(val_px)
        n = max(1, len(lines))
        c = self._cfg()
        for i, (lbl, val) in enumerate(lines):
            cy = rect.top() + (i + 0.5) / n * h
            row = QRectF(x, cy - h * 0.5 / n, rect.right() - x, h / n)
            if i > 0 and c.get("row_dividers", True) and len(lines) > 1:
                draw_row_divider(p, row.left(), row.top(), row.width(), _SECTION)
            if key == "irating":
                self._draw_irating_pair(p, row, d, val_px)
                continue
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
        orange = self._col("orange")
        draw_panel_rect(p, box, _SECTION)
        p.setPen(QPen(orange, max(1.6, box.height() * 0.022)))
        p.setBrush(Qt.BrushStyle.NoBrush)
        r = min(box.width(), box.height()) * self._cfg().get("corner_radius_frac", 0.0)
        p.drawRoundedRect(box, r, r)
        fs = box.height() * 0.40
        f = self._val_font(fs)
        tw = QFontMetricsF(f).horizontalAdvance(text)
        max_w = box.width() * 0.74
        if tw > max_w and tw > 0:
            f = self._val_font(fs * max_w / tw)
        self._text_centered(p, box.center(), f, text, orange)

    # -- gear + input ring medallion (drawn on top) ------------------------
    def _draw_ring(self, p, cx, cy, ring_d, c, d):
        # Dark medallion behind the ring so it reads as floating above panels,
        # with its own border so it stands out from the containers.
        mr = ring_d / 2 + ring_d * 0.06
        border = QColor(self._col("cell_border"))
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

        self._text_centered(p, QPointF(cx, cy), self._val_font(gear_px),
                             _gear_str(d.get("gear")), self._col("gear"))

    def _draw_ring_arc(self, p, arc, pen_w, frac, on_color, c):
        n = max(1, int(c.get("ring_segments", 16)))
        seg = 360.0 / n
        span = seg * 0.72
        frac = max(0.0, min(1.0, frac))
        # Deadzone: residual easing / sensor noise leaves a tiny non-zero value
        # when a pedal is released. Treat that as fully off so no segment lights.
        if frac < 0.02:
            frac = 0.0
        lit = frac * n
        off = self._col("ring_track")
        glow = QColor(on_color)
        glow.setAlpha(75)
        p.setPen(QPen(glow, pen_w * 2.0, cap=Qt.PenCapStyle.FlatCap))
        for i in range(n):
            if i < lit:
                ang = 90.0 + (i + 0.5) * seg  # sweep counter-clockwise (L->R)
                p.drawArc(arc, int((ang + span / 2) * 16), int(-span * 16))
        for i in range(n):
            ang = 90.0 + (i + 0.5) * seg
            p.setPen(QPen(on_color if i < lit else off, pen_w,
                          cap=Qt.PenCapStyle.FlatCap))
            p.drawArc(arc, int((ang + span / 2) * 16), int(-span * 16))

    # -- pedal-bar medallion (throttle / brake / clutch, drawn on top) ------
    def _draw_pedals(self, p, cx, cy, ring_d, c, d):
        mr = ring_d / 2 + ring_d * 0.06
        border = QColor(self._col("cell_border"))
        border.setAlpha(150)
        p.setPen(QPen(border, max(1.5, ring_d * 0.022)))
        p.setBrush(self._col("bg_bottom"))
        p.drawEllipse(QPointF(cx, cy), mr, mr)

        bars = self._selected_inputs(c)
        if not bars:
            # No inputs selected: just show the gear, centered.
            self._text_centered(p, QPointF(cx, cy), self._val_font(ring_d * 0.50),
                                _gear_str(d.get("gear")), self._col("gear"))
            return

        # Gear stays prominent at the top of the medallion.
        self._text_centered(p, QPointF(cx, cy - ring_d * 0.28),
                            self._val_font(ring_d * 0.26),
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
        p.setPen(QPen(self._col("cell_border"), 1.0))
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
        p.setPen(QPen(self._col("border"), 1.0))
        p.drawLine(QPointF(cx, rect.top() + 1), QPointF(cx, rect.bottom() - 1))

    # -- strip pill (own container) ----------------------------------------
    def _draw_strip(self, p, pill, keys, d):
        items = [k for k in keys if k not in (None, "none")]
        sh = pill.height()
        draw_dark_cell(p, pill, _SECTION, radius=sh / 2)
        if not items:
            return

        pad = sh * 0.55
        cx0 = pill.left() + pad
        content_w = pill.width() - 2 * pad
        cell = content_w / len(items)
        val_f = self._val_font(sh * 0.40)
        lbl_f = self._lbl_font(sh * 0.34)
        gap = sh * 0.18
        for i, key in enumerate(items):
            label = _display_label(key)
            glyph = icons.glyph(key)
            if key == "irating":
                val_px = sh * 0.34
                pair_w = self._irating_pair_width(d, val_px, sh)
                tx = cx0 + i * cell + (cell - pair_w) / 2
                self._draw_irating_pair(
                    p, QRectF(tx, pill.top(), pair_w, sh), d, val_px)
                continue
            val = _m_str(key, d)
            parts = []
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
