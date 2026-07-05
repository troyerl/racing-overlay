"""ERS / hybrid energy gauge widget."""

from __future__ import annotations

from PyQt6.QtCore import QRectF, Qt
from PyQt6.QtGui import QPainter
from PyQt6.QtWidgets import QSizePolicy, QWidget

from .. import config
from .. import hybrid as hy
from .chrome import col, draw_card, draw_dark_cell
from .fonts import data_font_bold, tabfont, tfont

_SECTION = "ers_hybrid"


class ErsHybridWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data: dict = {}
        self.setMinimumSize(180, 100)
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
        draw_card(p, w, h, _SECTION)
        pad = max(6.0, h * 0.10)
        if not d.get("have_hybrid") and not d.get("edit"):
            p.setFont(tfont(h * 0.22, bold=False))
            p.setPen(col("muted", _SECTION))
            p.drawText(QRectF(pad, pad, w - 2 * pad, h - 2 * pad),
                       Qt.AlignmentFlag.AlignCenter, "No hybrid data")
            return
        data_bold = data_font_bold(_SECTION)
        y = pad
        if cfg.get("show_battery", True):
            pct = d.get("battery_pct")
            bar = QRectF(pad, y, w - 2 * pad, h * 0.28)
            draw_dark_cell(p, bar, _SECTION, radius=8.0)
            inner = bar.adjusted(4, 4, -4, -4)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(col("gauge_bg", _SECTION))
            p.drawRoundedRect(inner, 4, 4)
            if isinstance(pct, (int, float)):
                fw = inner.width() * max(0.0, min(1.0, pct / 100.0))
                p.setBrush(col("gauge_fill", _SECTION))
                p.drawRoundedRect(QRectF(inner.left(), inner.top(), fw, inner.height()),
                                  4, 4)
            p.setFont(tabfont(h * 0.22, bold=data_bold))
            p.setPen(col("text", _SECTION))
            lbl = hy.fmt_kj(d.get("battery_j"))
            if isinstance(pct, (int, float)):
                lbl = f"{pct:.0f}%  {lbl}"
            p.drawText(bar, Qt.AlignmentFlag.AlignCenter, lbl)
            y += bar.height() + pad * 0.5
        if cfg.get("show_lap_energy", True):
            used = d.get("used_lap")
            budget = d.get("budget_lap")
            line = "\u2014"
            if isinstance(used, (int, float)) and isinstance(budget, (int, float)):
                line = f"{hy.fmt_kj(used)} / {hy.fmt_kj(budget)} lap"
            elif isinstance(used, (int, float)):
                line = f"{hy.fmt_kj(used)} this lap"
            p.setFont(tfont(h * 0.18, bold=False))
            p.setPen(col("muted", _SECTION))
            p.drawText(QRectF(pad, y, w - 2 * pad, h * 0.14),
                       Qt.AlignmentFlag.AlignCenter, line)
            y += h * 0.15
        pills = []
        if cfg.get("show_boost", True) and d.get("boost_active"):
            pills.append("BOOST")
        if cfg.get("show_p2p", True) and d.get("p2p_active"):
            pills.append("P2P")
        if pills:
            p.setFont(tfont(h * 0.16, bold=True))
            p.setPen(col("pill", _SECTION))
            p.drawText(QRectF(pad, y, w - 2 * pad, h * 0.12),
                       Qt.AlignmentFlag.AlignCenter, "  ".join(pills))
