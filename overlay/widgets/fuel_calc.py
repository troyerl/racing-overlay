"""
Fuel Calculator: current fuel, projected laps/time until empty, how much to add
to finish, a pit window, and avg/max/min usage scenarios -- styled to match the
dash and timing tables.

The app does all the math in app._update_fuel_calc() and feeds a single dict via
set_data(); this widget is pure rendering. Expected dict (any value may be None):

    {
      "level": float,            # current fuel (L)
      "cap": float,              # tank capacity (L)
      "add": float,              # litres to add to reach the finish
      "window": (int, int)|None, # recommended pit-window lap range
      "window_open": bool,       # are we within that window now?
      "rows": {                  # avg / high / low burn scenarios (keys max/min)
         "avg": {"usage","laps","pits","refuel"},
         "max": {...}, "min": {...},
      },
      "time_empty": float,       # seconds until empty (avg usage)
      "time_margin": float,      # time_empty - race time remaining (<0 = short)
      "laps_empty": float,       # laps until empty (avg usage)
      "laps_margin": float,      # laps_empty - laps remaining (<0 = short)
      "strip": {"total": int, "window": (int,int)|None, "now": int|None},
    }
"""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QFontMetricsF, QLinearGradient, QPainter, QPen
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .chrome import col as _chrome_col
from .chrome import draw_accent_bar, draw_card, draw_dark_cell, draw_edge_band
from .chrome import draw_section_header
from .chrome import draw_row_divider, resolve_row_height
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "fuel_calc"

_VC = Qt.AlignmentFlag.AlignVCenter
_VC_LEFT = _VC | Qt.AlignmentFlag.AlignLeft
_VC_RIGHT = _VC | Qt.AlignmentFlag.AlignRight
_CENTER = Qt.AlignmentFlag.AlignCenter

_STAT_COLS = ("usage", "laps", "pits", "refuel")
_STAT_ROWS = ("avg", "max", "min")
_STAT_ROW_LABELS = {"avg": "AVG", "max": "MAX", "min": "MIN"}


def _cfg() -> dict:
    return config.CFG["fuel_calc"]


def _col(key: str) -> QColor:
    return _chrome_col(key, _SECTION)


def _fmt1(x) -> str:
    return f"{x:.1f}" if isinstance(x, (int, float)) else "\u2013"


def _stat_headers() -> dict[str, str]:
    return {
        "usage": "USAGE",
        "laps": "LAPS",
        "pits": "PITS",
        "refuel": "REFUEL",
    }


def _fmt_fuel(x) -> str:
    """Stats-grid fuel amount (converted units, no unit suffix)."""
    v = config.conv_fuel(x)
    if v is None:
        return "\u2013"
    return f"{v:.1f}"


def _fmt_stat_cell(col: str, val) -> str:
    if col in ("usage", "refuel"):
        return _fmt_fuel(val)
    return _fmt1(val)


def _fmt_hms(sec) -> str:
    if not isinstance(sec, (int, float)):
        return "--:--:--"
    sec = int(max(0, sec))
    h, rem = divmod(sec, 3600)
    m, s = divmod(rem, 60)
    return f"{h:02d}:{m:02d}:{s:02d}"


def _fmt_signed_hms(sec) -> str:
    if not isinstance(sec, (int, float)):
        return "--:--:--"
    sign = "-" if sec < 0 else "+"
    return sign + _fmt_hms(abs(sec))


class FuelCalcWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(360, 320)
        self.setSizePolicy(QSizePolicy.Policy.Expanding, QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        data = data or {}
        if data == self.data:
            return
        self.data = data
        self.update()

    def paintEvent(self, event) -> None:  # noqa: N802 (Qt naming)
        config.use_section("fuel_calc")
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        cfg = _cfg()
        d = self.data or {}
        card, radius = draw_card(p, w, h, _SECTION)

        m = w * 0.04
        inner = w - 2 * m

        bar_h = max(3.0, h * 0.018)
        draw_accent_bar(p, QRectF(m, h * 0.012, inner, bar_h), _SECTION)

        # Build the stack of enabled blocks (each a relative weight), then place
        # them top-to-bottom so hidden features collapse and the rest reflow.
        show_pill = cfg.get("show_pill", True)
        show_add = cfg.get("show_add", True)
        show_gauge = cfg.get("show_gauge", True)
        top_on = show_pill or show_add or show_gauge

        blocks: list[tuple[str, float]] = []
        if cfg.get("show_title", True):
            blocks.append(("title", 0.55))
        if top_on:
            blocks.append(("top", 1.15))
        if cfg.get("show_stats", True):
            blocks.append(("stats", 2.6))
        if cfg.get("show_strip", True):
            blocks.append(("strip", 0.6))
        if cfg.get("show_time", True):
            blocks.append(("time", 0.95))
        if cfg.get("show_laps", True):
            blocks.append(("laps", 0.95))
        if not blocks:
            return

        content_top = h * 0.012 + bar_h + h * 0.015
        content_bottom = h * 0.985
        gap = h * 0.02
        sumw = sum(wt for _k, wt in blocks)
        avail = (content_bottom - content_top) - gap * (len(blocks) - 1)
        cy = content_top
        for key, wt in blocks:
            bh = avail * wt / sumw
            if key == "title":
                title_h = bh
                band = QRectF(card.left() + m, cy, inner, title_h)
                draw_section_header(
                    p, band, str(cfg.get("title", "FUEL CALCULATOR")),
                    _SECTION, radius_top=radius)
            elif key == "top":
                self._draw_top(p, d, m, cy, inner, bh,
                               show_pill, show_add, show_gauge)
            elif key == "stats":
                self._draw_stats(p, d, m, cy, inner, bh)
            elif key == "strip":
                self._draw_strip(p, d, m, cy, inner, bh)
            elif key == "time":
                self._draw_box(p, "TIME UNTIL EMPTY", _fmt_hms(d.get("time_empty")),
                               _fmt_signed_hms(d.get("time_margin")),
                               d.get("time_margin"), m, cy, inner, bh)
            elif key == "laps":
                self._draw_box(p, "LAPS UNTIL EMPTY", _fmt1(d.get("laps_empty")),
                               self._signed1(d.get("laps_margin")),
                               d.get("laps_margin"), m, cy, inner, bh)
            cy += bh + gap

    @staticmethod
    def _signed1(x) -> str:
        if not isinstance(x, (int, float)):
            return "\u2013"
        return f"{'+' if x >= 0 else '-'}{abs(x):.1f}"

    @staticmethod
    def _fit_font(text, max_w, max_px):
        """A bold font at most max_px tall, shrunk so text fits within max_w."""
        px = max(6.0, max_px)
        f = tfont(px, True)
        if max_w <= 0 or not text:
            return f
        for _ in range(12):
            if QFontMetricsF(f).horizontalAdvance(text) <= max_w:
                break
            px *= 0.9
            if px < 6.0:
                f = tfont(6.0, True)
                break
            f = tfont(px, True)
        return f

    # -- top: status pill + add box, and the fuel gauge ---------------------
    def _draw_top(self, p, d, x, y, w, h, show_pill, show_add, show_gauge) -> None:
        left_on = show_pill or show_add
        # Gauge alone takes the full width; otherwise it shares with the left
        # group (pill / add), and the left group fills the width if no gauge.
        if show_gauge and not left_on:
            self._draw_gauge(p, d, x, y, w, h)
            return
        left_w = w * 0.46 if show_gauge else w

        if show_pill and show_add:
            pill_w = left_w * 0.46
            self._draw_pill(p, d, x, y, pill_w, h)
            ax = x + pill_w + w * 0.015
            self._draw_add(p, d, ax, y, x + left_w - ax, h)
        elif show_pill:
            self._draw_pill(p, d, x, y, left_w, h)
        elif show_add:
            self._draw_add(p, d, x, y, left_w, h)

        if show_gauge:
            g_x = x + left_w + w * 0.04
            self._draw_gauge(p, d, g_x, y, w - left_w - w * 0.04, h)

    def _draw_pill(self, p, d, x, y, w, h) -> None:
        window = d.get("window")
        open_ = bool(d.get("window_open"))
        pill = QRectF(x, y, w, h)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(_col("pill_open") if open_ else _col("pill_closed"))
        p.drawRoundedRect(pill, 8, 8)
        p.setPen(_col("pill_text"))
        p.setFont(tfont(h * 0.34, True))
        p.drawText(QRectF(pill.left(), pill.top() + h * 0.10, pill.width(),
                          h * 0.46), _CENTER, "OPEN" if open_ else "CLOSED")
        sub = f"L{window[0]}-{window[1]}" if window else "\u2014"
        p.setFont(tfont(h * 0.22, True))
        p.drawText(QRectF(pill.left(), pill.top() + h * 0.52, pill.width(),
                          h * 0.4), _CENTER, sub)

    def _draw_add(self, p, d, x, y, w, h) -> None:
        add = d.get("add")
        rect = QRectF(x, y, w, h)
        draw_dark_cell(p, rect, _SECTION, radius=8)
        p.setPen(_col("add_text"))
        p.setFont(tabfont(h * 0.46, bold=True))
        if isinstance(add, (int, float)):
            add_c = config.conv_fuel(add)
            txt = f"+{add_c:.1f}{config.fuel_unit()}" if add_c is not None else "\u2014"
        else:
            txt = "\u2014"
        p.drawText(rect, _CENTER, txt)

    def _draw_gauge(self, p, d, x, y, w, h) -> None:
        level = d.get("level")
        cap = d.get("cap")
        bar = QRectF(x, y + h * 0.18, w, h * 0.36)
        p.setBrush(_col("gauge_bg"))
        p.setPen(QPen(_col("cell_border"), 1))
        p.drawRoundedRect(bar, 4, 4)
        if isinstance(level, (int, float)) and isinstance(cap, (int, float)) and cap > 0:
            frac = max(0.0, min(1.0, level / cap))
            fill = QRectF(bar.left() + 1, bar.top() + 1,
                          (bar.width() - 2) * frac, bar.height() - 2)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(_col("gauge_fill"))
            p.drawRoundedRect(fill, 3, 3)
        # E ... current ... max labels under the bar.
        p.setFont(tfont(h * 0.26, True))
        p.setPen(_col("muted"))
        p.drawText(QRectF(x, bar.bottom() + 1, w * 0.3, h * 0.4), _VC_LEFT, "E")
        cur = config.conv_fuel(level)
        pct = d.get("fuel_pct")
        cur_txt = f"{cur:.1f} {config.fuel_unit()}" if cur is not None else "\u2014"
        if isinstance(pct, (int, float)):
            cur_txt = f"{cur_txt} ({pct:.0f}%)"
        p.drawText(QRectF(x, bar.bottom() + 1, w, h * 0.4), _CENTER, cur_txt)
        if d.get("alert"):
            p.setPen(_col("box_warn"))
            p.setFont(tfont(h * 0.22, True))
            p.drawText(QRectF(x, bar.bottom() + h * 0.38, w, h * 0.22),
                       _CENTER, "LOW FUEL")
        elif d.get("pit_hint"):
            p.setPen(_col("muted"))
            p.setFont(tfont(h * 0.20, False))
            p.drawText(QRectF(x, bar.bottom() + h * 0.38, w, h * 0.22),
                       _CENTER, str(d.get("pit_hint")))
        elif isinstance(d.get("live_burn"), (int, float)):
            p.setPen(_col("muted"))
            p.setFont(tfont(h * 0.20, False))
            burn = config.conv_fuel(d["live_burn"])
            p.drawText(QRectF(x, bar.bottom() + h * 0.38, w, h * 0.22),
                       _CENTER, f"{burn:.2f}{config.fuel_unit()}/lap")
        capc = config.conv_fuel(cap)
        p.setPen(_col("muted"))
        cap_txt = f"{capc:.0f}{config.fuel_unit()}" if capc is not None else "\u2014"
        p.drawText(QRectF(x, bar.bottom() + 1, w, h * 0.4), _VC_RIGHT, cap_txt)

    # -- the avg/max/min burn grid ------------------------------------------
    def _draw_stats(self, p, d, x, y, w, h) -> None:
        rows = d.get("rows") or {}
        headers = _stat_headers()
        label_w = w * 0.13
        col_w = (w - label_w) / len(_STAT_COLS)
        cfg = _cfg()
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            row_h = fixed_rh
            head_h = round(fixed_rh * 1.1)
        else:
            head_h = h * 0.22
            row_h = resolve_row_height(
                body_h=h - head_h, row_count=len(_STAT_ROWS),
                panel_h=self.height(), cfg=cfg)
        data_bold = data_font_bold(_SECTION)

        band = QRectF(x, y, w, head_h)
        draw_edge_band(p, band, "header_bg", _SECTION, bottom_line=True)
        hscale = max(0.3, float(cfg.get("stats_header_font_scale", 1.0) or 1.0))
        p.setFont(tfont(head_h * 0.5 * hscale, bold=False, widget_scale=False))
        p.setPen(_col("header"))
        for i, k in enumerate(_STAT_COLS):
            cx = x + label_w + i * col_w
            p.drawText(QRectF(cx, y, col_w, head_h), _CENTER, headers[k])

        rscale = max(0.3, float(cfg.get("stats_row_font_scale", 1.0) or 1.0))
        for r, rk in enumerate(_STAT_ROWS):
            ry = y + head_h + r * row_h
            if r % 2 == 1:
                p.setPen(Qt.PenStyle.NoPen)
                p.setBrush(_col("row_alt"))
                p.drawRoundedRect(QRectF(x, ry, w, row_h), 4, 4)
            p.setFont(tfont(row_h * 0.4 * rscale, True, widget_scale=False))
            p.setPen(_col("muted"))
            p.drawText(QRectF(x + 2, ry, label_w, row_h), _VC_LEFT,
                       _STAT_ROW_LABELS.get(rk, rk.upper()))
            data = rows.get(rk) or {}
            p.setPen(_col("text"))
            p.setFont(tabfont(row_h * 0.46 * rscale, bold=data_bold, widget_scale=False))
            for i, k in enumerate(_STAT_COLS):
                cx = x + label_w + i * col_w
                p.drawText(QRectF(cx, ry, col_w, row_h), _CENTER,
                           _fmt_stat_cell(k, data.get(k)))
            if cfg.get("row_dividers", True) and r < len(_STAT_ROWS) - 1:
                draw_row_divider(p, x, ry + row_h, w, _SECTION)

    # -- PIT lap-timeline strip ---------------------------------------------
    def _draw_strip(self, p, d, x, y, w, h) -> None:
        p.setFont(tfont(h * 0.5, True))
        p.setPen(_col("muted"))
        lbl_w = w * 0.10
        p.drawText(QRectF(x, y, lbl_w, h), _VC_LEFT, "PIT")

        strip = d.get("strip") or {}
        total = int(strip.get("total") or 0)
        sx = x + lbl_w
        sw = w - lbl_w
        if total <= 0:
            return
        win = strip.get("window")
        now = strip.get("now")
        gap = sw / total * 0.22
        seg_w = sw / total - gap
        bar_h = h * 0.6
        by = y + (h - bar_h) / 2
        for i in range(total):
            cx = sx + i * (seg_w + gap)
            if now is not None and i == now:
                color = _col("strip_now")
            elif win and win[0] <= i <= win[1]:
                color = _col("strip_window")
            else:
                color = _col("strip_none")
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(color)
            p.drawRoundedRect(QRectF(cx, by, max(2.0, seg_w), bar_h), 2, 2)

    # -- a summary box (TIME / LAPS until empty) ----------------------------
    def _draw_box(self, p, label, value, margin_txt, margin_val, x, y, w, h) -> None:
        rect = QRectF(x, y, w, h)
        draw_dark_cell(p, rect, _SECTION, radius=6)
        pad = w * 0.02
        # Three non-overlapping zones: label | value | margin. Fonts scale with
        # height but are capped by zone width so a tall box can't overlap/clip.
        gap = w * 0.02
        label_w = w * 0.34
        margin_w = w * 0.22
        value_w = w - label_w - margin_w - pad * 2 - gap * 2

        p.setFont(self._fit_font(label, label_w - pad, h * 0.30))
        p.setPen(_col("text"))
        p.drawText(QRectF(x + pad, y, label_w - pad, h), _VC_LEFT, label)
        p.setFont(tabfont(min(h * 0.52, 48), bold=data_font_bold(_SECTION)))
        p.setPen(_col("box_value"))
        p.drawText(QRectF(x + label_w + gap, y, value_w, h), _VC_RIGHT, value)
        warn = isinstance(margin_val, (int, float)) and margin_val < 0
        p.setFont(tabfont(min(h * 0.32, 32), bold=False))
        p.setPen(_col("box_warn") if warn else _col("muted"))
        p.drawText(QRectF(x + label_w + gap * 2 + value_w, y, margin_w - pad, h),
                   _VC_RIGHT, margin_txt)
