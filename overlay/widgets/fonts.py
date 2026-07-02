"""Shared font helpers for overlay widgets."""

from __future__ import annotations

import sys

from PyQt6.QtGui import QFont

from .. import config

_FONT_CACHE: dict = {}


def _tabular_family() -> str:
    fam = config.CFG.get("tabular_font_family", "") or ""
    if fam:
        return fam
    return "SF Mono" if sys.platform == "darwin" else "Consolas"


def tfont(size: float, bold: bool = True) -> QFont:
    fam = config.CFG.get("font_family", "Segoe UI")
    pt = round(max(5.0, size * config.text_scale_for()), 1)
    key = (fam, pt, bold, False)
    f = _FONT_CACHE.get(key)
    if f is None:
        f = QFont(fam)
        f.setStyleHint(QFont.StyleHint.SansSerif)
        f.setPointSizeF(pt)
        f.setBold(bold)
        if len(_FONT_CACHE) > 512:
            _FONT_CACHE.clear()
        _FONT_CACHE[key] = f
    return f


def tabfont(size: float, bold: bool = False) -> QFont:
    fam = _tabular_family()
    pt = round(max(5.0, size * config.text_scale_for()), 1)
    key = (fam, pt, bold, True)
    f = _FONT_CACHE.get(key)
    if f is None:
        f = QFont(fam)
        f.setStyleHint(QFont.StyleHint.Monospace)
        f.setPointSizeF(pt)
        f.setBold(bold)
        if len(_FONT_CACHE) > 512:
            _FONT_CACHE.clear()
        _FONT_CACHE[key] = f
    return f


def data_font_bold(section: str | None = None) -> bool:
    sec = section or config.active_section()
    if sec and isinstance(config.CFG.get(sec), dict):
        return bool(config.CFG[sec].get("data_font_bold", False))
    return False
