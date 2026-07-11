"""Shared font helpers for overlay widgets."""

from __future__ import annotations

from PyQt6.QtGui import QFont

from .. import config

_FONT_CACHE: dict = {}


def clear_font_cache() -> None:
    """Drop cached QFont objects (call after config changes)."""
    _FONT_CACHE.clear()


def _tabular_family() -> str:
    fam = config.CFG.get("tabular_font_family", "") or ""
    if fam:
        return fam
    # Empty Tabular inherits the global Font so one setting drives all text.
    return str(config.CFG.get("font_family", "Segoe UI") or "Segoe UI")


def tfont(size: float, bold: bool = True, *, widget_scale: bool = True) -> QFont:
    fam = config.CFG.get("font_family", "Segoe UI")
    if widget_scale:
        scale = config.text_scale_for()
    else:
        scale = float(config.CFG.get("text_scale", 1.0) or 1.0)
    pt = round(max(5.0, size * scale), 1)
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


def tabfont(size: float, bold: bool = False, *, widget_scale: bool = True) -> QFont:
    fam = _tabular_family()
    if widget_scale:
        scale = config.text_scale_for()
    else:
        scale = float(config.CFG.get("text_scale", 1.0) or 1.0)
    pt = round(max(5.0, size * scale), 1)
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
