"""Pit engineer — caution and green-flag pit strategy recommendations."""

from __future__ import annotations

import math
from dataclasses import dataclass

from PyQt6.QtCore import QRect, QRectF, QSize, Qt
from PyQt6.QtGui import QFontMetrics, QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from ..pit_strategy import PitRec
from .chrome import col, draw_card, draw_section_header, draw_status_chip, panel_pad
from .fonts import tfont

_SECTION = "pit_advisor"
_REF_H = 100.0
_GAP_TITLE_CHIP = 6.0
_GAP_CHIP_RATIONALE = 8.0
_GAP_RATIONALE_SECONDARY = 6.0
_WRAP_ALIGN = (Qt.AlignmentFlag.AlignLeft | Qt.AlignmentFlag.AlignTop
               | Qt.TextFlag.TextWordWrap)
_PREVIEW = {
    "rec": "pit_next_lap",
    "label": "PIT NEXT LAP",
    "rationale": "Pit next lap to pass #12 \u2014 6.2s ahead, stop costs ~28s",
    "secondary": "Best stop: laps 24\u201326",
    "actionable": True,
    "edit": True,
}


@dataclass
class _PitLayout:
    total_content_h: float
    visible: bool
    show_title: bool
    title_rect: QRectF | None
    chip: QRectF | None
    rationale_rect: QRectF | None
    secondary_rect: QRectF | None
    rationale_font_size: float
    secondary_font_size: float
    rec: str | None
    label: str | None
    rationale: str | None
    secondary: str | None
    active: bool


def _wrap_height(fm: QFontMetrics, text: str, text_w: float) -> float:
    if not text:
        return 0.0
    br = fm.boundingRect(
        QRect(0, 0, int(text_w), 10000), int(_WRAP_ALIGN), text)
    return float(br.height())


def _resolve_fields(d: dict, cfg: dict) -> tuple:
    rec = d.get("rec")
    label = d.get("label")
    rationale = d.get("rationale")
    secondary = d.get("secondary")
    actionable = bool(d.get("actionable"))
    edit = bool(d.get("edit"))

    if not label and edit:
        label = _PREVIEW["label"]
        rationale = _PREVIEW["rationale"]
        secondary = _PREVIEW["secondary"]
        rec = _PREVIEW["rec"]
        actionable = True

    visible = bool(label)
    if visible and cfg.get("show_only_when_actionable", True) and not actionable and not edit:
        visible = False

    active = False
    if rec is not None:
        active = rec not in (PitRec.STAY_OUT.value, PitRec.HOLD.value, PitRec.MARGINAL.value)
        if rec == PitRec.MARGINAL.value:
            active = True

    return rec, label, rationale, secondary, actionable, edit, visible, active


def measure_pit_advisor_layout(width: float, data: dict,
                               cfg: dict | None = None) -> _PitLayout:
    """Compute wrapped text layout and total content height for a panel width."""
    cfg = cfg or config.CFG.get(_SECTION, {})
    d = data or {}
    w = max(1.0, float(width))
    ref_h = _REF_H
    pad = panel_pad(ref_h)
    text_w = max(1.0, w - 1 - 2 * pad)

    y = 0.5 + pad
    show_title = cfg.get("show_title", True)
    title_rect = None
    if show_title:
        hh = max(20.0, ref_h * 0.14)
        title_rect = QRectF(0.5 + pad, y, text_w, hh)
        y += hh + _GAP_TITLE_CHIP

    rec, label, rationale, secondary, _actionable, _edit, visible, active = _resolve_fields(
        d, cfg)

    if not visible:
        return _PitLayout(
            total_content_h=0.0,
            visible=False,
            show_title=False,
            title_rect=None,
            chip=None,
            rationale_rect=None,
            secondary_rect=None,
            rationale_font_size=11.0,
            secondary_font_size=10.0,
            rec=rec,
            label=label,
            rationale=rationale,
            secondary=secondary,
            active=active,
        )

    body_h = max((ref_h - 1) - pad - y, 40.0)
    chip_h = max(24.0, body_h * 0.38)
    chip = QRectF(0.5 + pad, y, text_w, chip_h)
    y += chip_h + _GAP_CHIP_RATIONALE

    rationale_font_size = max(11.0, chip_h * 0.38)
    secondary_font_size = max(10.0, chip_h * 0.34)
    rationale_rect = None
    secondary_rect = None

    if rationale:
        fm = QFontMetrics(tfont(rationale_font_size, bold=False))
        rh = _wrap_height(fm, str(rationale), text_w)
        rationale_rect = QRectF(0.5 + pad, y, text_w, rh)
        y += rh
        if secondary:
            y += _GAP_RATIONALE_SECONDARY

    if secondary:
        fm = QFontMetrics(tfont(secondary_font_size, bold=False))
        sh = _wrap_height(fm, str(secondary), text_w)
        secondary_rect = QRectF(0.5 + pad, y, text_w, sh)
        y += sh

    return _PitLayout(
        total_content_h=max(72.0, y + pad),
        visible=True,
        show_title=show_title,
        title_rect=title_rect,
        chip=chip,
        rationale_rect=rationale_rect,
        secondary_rect=secondary_rect,
        rationale_font_size=rationale_font_size,
        secondary_font_size=secondary_font_size,
        rec=rec,
        label=label,
        rationale=rationale,
        secondary=secondary,
        active=active,
    )


class PitAdvisorWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(180, 72)
        self.setSizePolicy(QSizePolicy.Policy.Expanding,
                           QSizePolicy.Policy.Expanding)

    def set_data(self, data: dict) -> None:
        data = data or {}
        if data != self.data:
            self.data = data
        self.update()
        self._sync_panel_height()

    def resizeEvent(self, event) -> None:  # noqa: N802
        super().resizeEvent(event)
        self._sync_panel_height()

    def sizeHint(self):  # noqa: N802
        return self._measured_size()

    def minimumSizeHint(self):  # noqa: N802
        return self._measured_size()

    def _measured_size(self) -> QSize:
        w = max(self.width(), self.minimumWidth())
        layout = measure_pit_advisor_layout(w, self.data)
        return QSize(w, int(math.ceil(layout.total_content_h)))

    def _sync_panel_height(self) -> None:
        layout = measure_pit_advisor_layout(self.width(), self.data)
        self.updateGeometry()
        win = self.window()
        from ..panel import PanelWindow
        if isinstance(win, PanelWindow):
            win.fit_content_height(int(math.ceil(layout.total_content_h)))

    def paintEvent(self, event) -> None:  # noqa: N802
        config.use_section(_SECTION)
        w, h = float(self.width()), float(self.height())
        cfg = config.CFG.get(_SECTION, {})
        layout = measure_pit_advisor_layout(w, self.data, cfg)

        if not layout.visible or not layout.chip or not layout.label:
            return

        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        card, radius = draw_card(p, w, h, _SECTION)

        if layout.show_title and layout.title_rect is not None:
            draw_section_header(
                p, layout.title_rect,
                str(cfg.get("title", "PIT ENGINEER")),
                _SECTION, radius_top=radius)

        draw_status_chip(p, layout.chip, str(layout.label), _SECTION,
                         active=layout.active)

        if layout.rationale_rect is not None and layout.rationale:
            p.setFont(tfont(layout.rationale_font_size, bold=False))
            p.setPen(col("text", _SECTION))
            p.drawText(layout.rationale_rect, _WRAP_ALIGN, str(layout.rationale))

        if layout.secondary_rect is not None and layout.secondary:
            p.setFont(tfont(layout.secondary_font_size, bold=False))
            p.setPen(col("muted", _SECTION))
            p.drawText(layout.secondary_rect, _WRAP_ALIGN, str(layout.secondary))
