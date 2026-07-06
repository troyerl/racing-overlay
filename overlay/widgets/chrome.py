"""Shared panel chrome: cards, bands, cells, dividers."""

from __future__ import annotations

import math

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import (QColor, QLinearGradient, QPainter, QPainterPath, QPen)

from .. import config
from .fonts import data_font_bold, tabfont, tfont

_BORDER_ALIASES = {"border": "panel_border", "panel_border": "border"}


def section_cfg(section: str | None = None) -> dict:
    sec = section or config.active_section()
    if sec and isinstance(config.CFG.get(sec), dict):
        return config.CFG[sec]
    return {}


def col(key: str, section: str | None = None,
        fallback: str | None = None) -> QColor:
    colors = section_cfg(section).get("colors", {})
    if key in colors:
        return config.qcolor(colors[key])
    alias = _BORDER_ALIASES.get(key)
    if alias and alias in colors:
        return config.qcolor(colors[alias])
    if fallback is not None:
        return config.qcolor(fallback)
    return config.qcolor(colors.get(key, "#ff00ff"))


def soften_color(c: QColor, toward: str = "#1b1f26", mix: float = 0.20) -> QColor:
    t = config.qcolor(toward)
    m = max(0.0, min(1.0, mix))
    return QColor(
        int(c.red() * (1.0 - m) + t.red() * m),
        int(c.green() * (1.0 - m) + t.green() * m),
        int(c.blue() * (1.0 - m) + t.blue() * m),
        c.alpha(),
    )


def contrast_text(bg: QColor) -> QColor:
    lum = (0.299 * bg.red() + 0.587 * bg.green() + 0.114 * bg.blue()) / 255.0
    return QColor(20, 22, 26) if lum > 0.6 else QColor(255, 255, 255)


def band_path(rect: QRectF, radius_top: float = 0.0,
              radius_bottom: float = 0.0) -> QPainterPath:
    x, y, w, h = rect.x(), rect.y(), rect.width(), rect.height()
    rt = min(max(0.0, radius_top), w / 2.0, h / 2.0)
    rb = min(max(0.0, radius_bottom), w / 2.0, h / 2.0)
    path = QPainterPath()
    path.moveTo(x + rt, y)
    path.lineTo(x + w - rt, y)
    if rt > 0:
        path.arcTo(x + w - 2 * rt, y, 2 * rt, 2 * rt, 90, -90)
    else:
        path.lineTo(x + w, y)
    path.lineTo(x + w, y + h - rb)
    if rb > 0:
        path.arcTo(x + w - 2 * rb, y + h - 2 * rb, 2 * rb, 2 * rb, 0, -90)
    else:
        path.lineTo(x + w, y + h)
    path.lineTo(x + rb, y + h)
    if rb > 0:
        path.arcTo(x, y + h - 2 * rb, 2 * rb, 2 * rb, 270, -90)
    else:
        path.lineTo(x, y + h)
    path.lineTo(x, y + rt)
    if rt > 0:
        path.arcTo(x, y, 2 * rt, 2 * rt, 180, -90)
    else:
        path.lineTo(x, y)
    path.closeSubpath()
    return path


def draw_card(p: QPainter, w: float, h: float, section: str | None = None,
              *, radius_frac: float | None = None) -> tuple[QRectF, float]:
    """Paint gradient card shell; returns (card rect, corner radius)."""
    cfg = section_cfg(section)
    radius = max(8.0, h * (radius_frac if radius_frac is not None
                           else cfg.get("corner_radius_frac", 0.0)))
    card = QRectF(0.5, 0.5, w - 1, h - 1)
    colors = cfg.get("colors", {})
    if "bg_top" in colors and "bg_bottom" in colors:
        grad = QLinearGradient(0, 0, 0, h)
        grad.setColorAt(0.0, col("bg_top", section))
        grad.setColorAt(1.0, col("bg_bottom", section))
        p.setBrush(grad)
    else:
        p.setBrush(col("bg", section, "#1b1f26"))
    p.setPen(QPen(col("border", section, "#ffffff28"), 1))
    p.drawRoundedRect(card, radius, radius)
    return card, radius


def draw_panel_rect(p: QPainter, rect: QRectF, section: str | None = None, *,
                    radius_frac: float | None = None,
                    radius_basis: str = "min") -> float:
    """Paint gradient panel in an arbitrary rect; returns corner radius."""
    cfg = section_cfg(section)
    frac = (radius_frac if radius_frac is not None
            else cfg.get("corner_radius_frac", 0.0))
    if radius_basis == "min":
        radius = min(rect.width(), rect.height()) * frac
    else:
        radius = max(8.0, rect.height() * frac)
    colors = cfg.get("colors", {})
    if "bg_top" in colors and "bg_bottom" in colors:
        grad = QLinearGradient(0, rect.top(), 0, rect.bottom())
        grad.setColorAt(0.0, col("bg_top", section))
        grad.setColorAt(1.0, col("bg_bottom", section))
        p.setBrush(grad)
    else:
        p.setBrush(col("bg", section, "#1b1f26"))
    p.setPen(QPen(col("border", section, "#ffffff28"), 1))
    p.drawRoundedRect(rect, radius, radius)
    return radius


def draw_edge_band(p: QPainter, rect: QRectF, bg_key: str,
                   section: str | None = None, *,
                   top_line: bool = False, bottom_line: bool = False,
                   radius_top: float = 0.0, radius_bottom: float = 0.0,
                   opaque: bool = False) -> None:
    bg = col(bg_key, section, "#0b0e12bb")
    if opaque:
        bg.setAlpha(255)
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(bg)
    p.drawPath(band_path(rect, radius_top, radius_bottom))
    edge = col("border", section, "#ffffff28")
    edge.setAlpha(max(edge.alpha(), 40))
    p.setPen(QPen(edge, 1))
    x, y, bw, bh = rect.x(), rect.y(), rect.width(), rect.height()
    if top_line:
        inset = radius_top if radius_top > 0 else 0.0
        p.drawLine(int(x + inset), int(y), int(x + bw - inset), int(y))
    if bottom_line:
        inset = radius_bottom if radius_bottom > 0 else 0.0
        p.drawLine(int(x + inset), int(y + bh), int(x + bw - inset), int(y + bh))


def draw_row_divider(p: QPainter, x: float, y: float, w: float,
                     section: str | None = None) -> None:
    edge = col("border", section, "#ffffff28")
    edge.setAlpha(max(30, int(edge.alpha() * 0.55)))
    p.setPen(QPen(edge, 1))
    p.drawLine(int(x), int(y), int(x + w), int(y))


def draw_dark_cell(p: QPainter, rect: QRectF, section: str | None = None,
                   *, radius: float = 4.0) -> None:
    p.setPen(QPen(col("cell_border", section, "#ffffff20"), 1))
    p.setBrush(col("cell_dark", section, "#0b0e12"))
    p.drawRoundedRect(rect, radius, radius)


def draw_accent_bar(p: QPainter, rect: QRectF, section: str | None = None) -> None:
    h = rect.height()
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(col("accent", section, "#e23b3b"))
    p.drawRoundedRect(rect, h / 2, h / 2)


def resolve_row_height(*, body_h: float, row_count: int, panel_h: float,
                       cfg: dict) -> float:
    """Scale-to-fit row height, optionally capped by max_row_height_frac.

    When ``row_height_px`` > 0 the caller should use that fixed value instead;
    this helper is for the ``row_height_px == 0`` path shared by all tables.
    """
    n = max(1, int(row_count))
    row_h = body_h / n
    max_rh_frac = float(cfg.get("max_row_height_frac", 0) or 0)
    if max_rh_frac > 0:
        row_h = min(row_h, panel_h * max_rh_frac)
    return row_h


def ease(current: float, target: float, dt: float, tau: float = 0.12) -> float:
    """Frame-rate-independent exponential smoothing toward a target."""
    if tau <= 0:
        return target
    return current + (target - current) * (1.0 - math.exp(-dt / tau))


def panel_pad(h: float) -> float:
    """Standard inner padding for card panels."""
    return max(8.0, h * 0.08)


def cell_radius(row_h: float) -> float:
    """Standard corner radius for dark cells."""
    return min(8.0, max(4.0, row_h * 0.22))


def draw_section_header(p: QPainter, rect: QRectF, title: str,
                        section: str | None = None, *,
                        radius_top: float = 0.0) -> None:
    """Title band clipped to card top corners."""
    draw_edge_band(p, rect, "header_bg", section,
                   top_line=True, bottom_line=True,
                   radius_top=radius_top, opaque=True)
    p.setFont(tfont(rect.height() * 0.55, bold=True))
    p.setPen(col("title", section))
    p.drawText(rect.adjusted(10, 0, -8, 0),
               Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
               title)


def draw_metric_row(p: QPainter, rect: QRectF, label: str, value: str,
                    section: str | None = None, *,
                    sub: str = "", data_bold: bool = False) -> None:
    """Label + tabular value + optional muted sub-column inside a row rect."""
    lh = rect.height()
    lw = rect.width()
    p.setFont(tfont(lh * 0.38, bold=True))
    p.setPen(col("header", section))
    p.drawText(QRectF(rect.left(), rect.top(), lw * 0.22, lh),
               Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
               label)
    sub_w = lw * 0.18 if sub else 0.0
    p.setFont(tabfont(lh * 0.42, bold=data_bold))
    p.setPen(col("text", section))
    p.drawText(QRectF(rect.left() + lw * 0.22, rect.top(),
                      lw * 0.60 - sub_w, lh),
               Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
               value)
    if sub:
        p.setFont(tfont(lh * 0.34, bold=False))
        p.setPen(col("muted", section))
        p.drawText(QRectF(rect.right() - sub_w, rect.top(), sub_w, lh),
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignRight,
                   sub)


def draw_status_chip(p: QPainter, rect: QRectF, text: str,
                     section: str | None = None, *, active: bool = True) -> None:
    """Rounded status pill (e.g. PIT STOP ACTIVE, BOOST)."""
    r = min(rect.height() * 0.35, 10.0)
    bg = col("active_bg" if active else "cell_dark", section)
    fg = col("active_text" if active else "muted", section)
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(bg)
    p.drawRoundedRect(rect, r, r)
    p.setFont(tfont(rect.height() * 0.48, bold=True))
    p.setPen(fg)
    p.drawText(rect, Qt.AlignmentFlag.AlignCenter, text)


def draw_player_tint(p: QPainter, rect: QRectF,
                     section: str | None = None) -> None:
    """Semi-transparent player-row highlight under a cell."""
    tint = col("player_row", section)
    tint.setAlpha(min(180, max(60, tint.alpha())))
    p.fillRect(rect, tint)


def draw_edit_preview_rows(p: QPainter, rect: QRectF, section: str | None,
                           labels: list[str], *, row_count: int | None = None) -> None:
    """Skeleton metric rows for layout-edit preview."""
    n = row_count if row_count is not None else len(labels)
    n = max(1, n)
    row_h = rect.height() / n
    rad = cell_radius(row_h)
    for i in range(n):
        row_rect = QRectF(rect.left(), rect.top() + i * row_h,
                          rect.width(), row_h - 2)
        draw_dark_cell(p, row_rect, section, radius=rad)
        lbl = labels[i] if i < len(labels) else labels[-1]
        draw_metric_row(p, row_rect.adjusted(8, 0, -8, 0), lbl, "\u2014",
                        section, sub="")
