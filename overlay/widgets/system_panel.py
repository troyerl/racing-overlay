"""System panel — CPU, memory, GPU, FPS, and network/WiFi readouts."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from . import icons
from .chrome import (cell_radius, col, draw_card, draw_dark_cell, draw_row_divider,
                     draw_section_header, panel_pad, resolve_row_height)
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "system_panel"

# show_* config key -> (label, icon name, optional pct field in data dict)
_ROW_SPECS: dict[str, tuple[str, str, str | None]] = {
    "show_cpu": ("CPU", "cpu", "cpu_pct"),
    "show_mem": ("Memory", "mem", "mem_pct"),
    "show_gpu": ("GPU", "gpu", "gpu_pct"),
    "show_fps": ("FPS", "fps", None),
    "show_network": ("Network", "network", None),
}


def format_network_value(chan_quality, chan_latency, wifi: dict | None, *,
                         compact: bool = False) -> str:
    """Format the NET row: iRacing channel when available, else OS WiFi."""
    parts: list[str] = []
    if chan_quality is not None:
        parts.append(f"{int(round(float(chan_quality)))}%")
    if chan_latency is not None:
        parts.append(f"{int(round(float(chan_latency)))} ms")
    if parts:
        return " \u00b7 ".join(parts)
    if wifi:
        pct = wifi.get("quality_pct")
        if pct is not None:
            return f"{int(pct)}%" if compact else f"WiFi {int(pct)}%"
        rssi = wifi.get("rssi_dbm")
        if rssi is not None:
            return f"{int(rssi)} dBm" if compact else f"WiFi {int(rssi)} dBm"
    return "\u2014"


def _row_value(key: str, d: dict, *, compact_net: bool) -> str:
    if key == "show_fps":
        fps = d.get("fps")
        return str(fps) if fps is not None else "\u2014"
    if key == "show_network":
        return format_network_value(
            d.get("chan_quality"), d.get("chan_latency"), d.get("wifi"),
            compact=compact_net)
    if key == "show_cpu":
        return str(d.get("cpu") or "\u2014")
    if key == "show_mem":
        return str(d.get("mem") or "\u2014")
    if key == "show_gpu":
        return str(d.get("gpu") or "\u2014")
    return "\u2014"


def draw_system_row(
    p: QPainter,
    rect: QRectF,
    *,
    label: str,
    icon_key: str,
    value: str,
    pct: float | None,
    show_icons: bool,
    data_bold: bool,
) -> None:
    """Icon or label (left), optional usage bar (middle), value right-aligned."""
    lh = rect.height()
    pad_x = max(10.0, lh * 0.22)
    inner = rect.adjusted(pad_x, lh * 0.18, -pad_x, -lh * 0.18)
    label_w = lh * 0.95
    value_w = max(lh * 2.2, inner.width() * 0.28)
    bar_left = inner.left() + label_w + lh * 0.12
    bar_right = inner.right() - value_w - lh * 0.08
    bar_h = max(4.0, inner.height() * 0.42)
    bar_y = inner.top() + (inner.height() - bar_h) / 2.0

    icon_on = show_icons and icons.has(icon_key)
    if icon_on:
        p.setFont(icons.icon_font(lh * 0.42))
        p.setPen(col("header", _SECTION))
        p.drawText(QRectF(inner.left(), inner.top(), label_w, inner.height()),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   icons.glyph(icon_key))
    else:
        p.setFont(tfont(lh * 0.36, bold=True))
        p.setPen(col("muted", _SECTION))
        p.drawText(QRectF(inner.left(), inner.top(), label_w, inner.height()),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                   label)

    if pct is not None and bar_right > bar_left + 8:
        track = QRectF(bar_left, bar_y, bar_right - bar_left, bar_h)
        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(col("gauge_bg", _SECTION))
        p.drawRoundedRect(track, bar_h * 0.35, bar_h * 0.35)
        fill_w = track.width() * max(0.0, min(1.0, float(pct) / 100.0))
        if fill_w > 0.5:
            p.setBrush(col("gauge_fill", _SECTION))
            p.drawRoundedRect(QRectF(track.left(), track.top(), fill_w, track.height()),
                              bar_h * 0.35, bar_h * 0.35)

    p.setFont(tabfont(lh * 0.40, bold=data_bold))
    p.setPen(col("text", _SECTION))
    p.drawText(QRectF(inner.right() - value_w, inner.top(), value_w, inner.height()),
               Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
               value)


class SystemPanelWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(140, 100)
        self.setSizePolicy(QSizePolicy.Policy.Expanding,
                           QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        data = data or {}
        if data == self.data:
            return
        self.data = data
        self.update()

    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section(_SECTION)
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = float(self.width()), float(self.height())
        d = self.data or {}
        cfg = config.CFG.get(_SECTION, {})
        card, radius = draw_card(p, w, h, _SECTION)
        pad = panel_pad(h)
        y = card.top() + pad
        data_bold = data_font_bold(_SECTION)
        show_icons = bool(cfg.get("show_icons", False))
        compact_net = show_icons

        if cfg.get("show_title", True):
            hh = max(22.0, h * 0.12)
            hdr = QRectF(card.left() + pad, y, card.width() - 2 * pad, hh)
            draw_section_header(p, hdr, str(cfg.get("title", "PERFORMANCE")), _SECTION,
                                radius_top=radius)
            y += hh + pad * 0.35

        rows: list[tuple[str, str, float | None, str]] = []
        for cfg_key, (label, icon_key, pct_field) in _ROW_SPECS.items():
            if not cfg.get(cfg_key, True):
                continue
            pct = d.get(pct_field) if pct_field else None
            if not isinstance(pct, (int, float)):
                pct = None
            val = _row_value(cfg_key, d, compact_net=compact_net)
            rows.append((label, icon_key, pct, val))

        if not rows and d.get("edit"):
            for _cfg_key, (label, icon_key, _pct_field) in _ROW_SPECS.items():
                rows.append((label, icon_key, None, "\u2014"))

        body_h = max(card.bottom() - pad - y, 40.0)
        n = max(1, len(rows))
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            row_h = fixed_rh
        else:
            row_h = resolve_row_height(body_h=body_h, row_count=n, panel_h=h, cfg=cfg)
        row_h = max(18.0, row_h)
        rad = cell_radius(row_h)
        for i, (label, icon_key, pct, val) in enumerate(rows[:5]):
            rect = QRectF(card.left() + pad, y, card.width() - 2 * pad, row_h - 3)
            draw_dark_cell(p, rect, _SECTION, radius=rad)
            draw_system_row(
                p, rect,
                label=label,
                icon_key=icon_key,
                value=val,
                pct=pct,
                show_icons=show_icons,
                data_bold=data_bold,
            )
            y += row_h
            if cfg.get("row_dividers", True) and i + 1 < len(rows[:5]):
                draw_row_divider(p, card.left() + pad, y - 3,
                                 card.width() - 2 * pad, _SECTION)
