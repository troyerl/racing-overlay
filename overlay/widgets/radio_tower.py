"""Radio tower — current team-radio speaker with race position."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QColor, QFontMetricsF, QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from . import icons
from .chrome import col, draw_card, draw_section_header, panel_pad, resolve_row_height
from .fonts import data_font_bold, tfont

_SECTION = "radio_tower"
_PREVIEW_ROW = {
    "position": 2,
    "car_number": "10",
    "name": "Preview Driver",
    "active": True,
    "is_player": False,
    "is_pro": False,
    "group_icon": "league",
    "group_color": "#5bb8ff",
}


def _driver_part(row, *, show_name: bool, show_car_number: bool) -> str:
    name = str(row.get("name", "")).strip()
    num = str(row.get("car_number", "")).strip()
    if show_name and show_car_number and name and num:
        return f"{name} #{num}"
    if show_name and name:
        return name
    if show_car_number and num:
        return f"#{num}" if not num.startswith("#") else num
    return ""


def _row_text(row, *, show_position: bool, show_name: bool,
              show_car_number: bool) -> str:
    driver = _driver_part(row, show_name=show_name,
                          show_car_number=show_car_number)
    pos = row.get("position", "")
    has_pos = show_position and pos != ""
    if has_pos and driver:
        return f"{pos} - {driver}"
    if has_pos:
        return str(pos)
    return driver


def _draw_speaking_accent(p: QPainter, rect: QRectF) -> None:
    accent = col("badge_speaking_bg", _SECTION, "#22c55e")
    h = rect.height()
    stripe_w = max(3.5, h * 0.09)
    edge = QColor(accent)
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(edge)
    p.drawRoundedRect(QRectF(rect.left(), rect.top() + h * 0.10,
                             stripe_w, h * 0.80), 2.0, 2.0)
    wash = QColor(accent)
    wash.setAlpha(38)
    p.setBrush(wash)
    p.drawRect(rect)


def _badge_glyph(row: dict) -> tuple[str, QColor] | tuple[None, None]:
    """Pro star wins; else group icon. Returns (glyph, pen_color) or (None, None)."""
    if bool(row.get("is_pro")):
        g = icons.glyph("pro_driver")
        if g:
            return g, col("pro_badge", _SECTION, "#f5c542")
    group_icon = str(row.get("group_icon") or "")
    if group_icon and icons.has(group_icon):
        g = icons.glyph(group_icon)
        if g:
            return g, config.qcolor(str(row.get("group_color") or "#5bb8ff"))
    return None, None


class RadioTowerWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(100, 44)
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
        p.setRenderHint(QPainter.RenderHint.TextAntialiasing)
        w, h = float(self.width()), float(self.height())
        d = self.data or {}
        cfg = config.CFG.get(_SECTION, {})
        rows = d.get("rows") or []
        if not rows and d.get("edit"):
            rows = [_PREVIEW_ROW]
        if not rows:
            return

        card, radius = draw_card(p, w, h, _SECTION)
        pad = panel_pad(h)
        y = card.top() + pad
        show_title = cfg.get("show_title", True)
        if show_title:
            hh = max(18.0, h * 0.22)
            hdr = QRectF(card.left() + pad, y, card.width() - 2 * pad, hh)
            draw_section_header(p, hdr, str(cfg.get("title", "RADIO")), _SECTION,
                                radius_top=radius)
            y += hh + pad * 0.2

        show_pos = cfg.get("show_position", True)
        show_num = cfg.get("show_car_number", True)
        show_name = cfg.get("show_name", True)
        highlight = cfg.get("highlight_player", True)
        row = rows[0]

        body_h = max(card.bottom() - pad - y, 22.0)
        fixed_rh = float(cfg.get("row_height_px", 0) or 0)
        if fixed_rh > 0:
            row_h = fixed_rh
        else:
            row_h = resolve_row_height(body_h=body_h, row_count=1, panel_h=h, cfg=cfg)
        row_h = max(22.0, row_h)

        content_w = card.width() - 2 * pad
        text_size = row_h * 0.46

        x0 = card.left() + pad
        row_top = y
        row_rect = QRectF(x0, row_top, content_w, row_h - 2)

        if row.get("active"):
            _draw_speaking_accent(p, row_rect)
        elif row.get("is_player") and highlight:
            p.fillRect(row_rect, col("player_row", _SECTION, "#ffffff14"))

        text = _row_text(row, show_position=show_pos, show_name=show_name,
                         show_car_number=show_num)
        if not text:
            return

        stripe_w = max(3.5, (row_h - 2) * 0.09)
        text_inset = stripe_w + max(6.0, row_h * 0.12)
        text_x = x0 + text_inset
        text_w = max(0.0, content_w - text_inset)

        glyph, badge_color = _badge_glyph(row)
        if glyph and badge_color is not None:
            ic_px = (row_h - 2) * 0.32
            ic_f = icons.icon_font(ic_px)
            gw = QFontMetricsF(ic_f).horizontalAdvance(glyph)
            gap = max(2.0, (row_h - 2) * 0.08)
            p.setFont(ic_f)
            p.setPen(badge_color)
            p.drawText(QRectF(text_x, row_top, gw + 2, row_h - 2),
                       Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft,
                       glyph)
            text_x = text_x + gw + gap
            text_w = max(0.0, text_w - (gw + gap))

        is_pro = bool(row.get("is_pro"))
        p.setFont(tfont(text_size, bold=data_font_bold(_SECTION) or is_pro))
        if is_pro and not row.get("active"):
            p.setPen(col("pro_name", _SECTION, "#f5c542"))
        else:
            p.setPen(col("text", _SECTION, "#d8d8d8"))
        text_rect = QRectF(text_x, row_top, text_w, row_h - 2)
        p.drawText(text_rect,
                   Qt.AlignmentFlag.AlignVCenter | Qt.AlignmentFlag.AlignLeft
                   | Qt.TextFlag.TextSingleLine,
                   p.fontMetrics().elidedText(
                       text, Qt.TextElideMode.ElideRight,
                       int(text_rect.width())))
