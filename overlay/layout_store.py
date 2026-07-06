"""Persist per-panel window geometry to a JSON file next to the scripts."""

from __future__ import annotations

import json

from PyQt6.QtCore import QPoint, QRect
from PyQt6.QtGui import QGuiApplication

from . import paths

LAYOUT_FILE = paths.data_file("overlay_layout.json")

_MIN_PANEL_W = 90
_MIN_PANEL_H = 44
_MIN_VISIBLE = 48


def _overlap(start: int, size: int, lo: int, hi: int) -> int:
    """Pixels of [start, start+size] overlapping [lo, hi]."""
    return max(0, min(start + size, hi) - max(start, lo))


def geometry_mostly_off_screen(x: int, y: int, w: int, h: int,
                               avail: QRect) -> bool:
    """True when too little of the panel is visible on the desktop."""
    v_vis = _overlap(y, h, avail.top(), avail.bottom())
    h_vis = _overlap(x, w, avail.left(), avail.right())
    if v_vis < max(_MIN_VISIBLE, h // 2):
        return True
    if h_vis < max(_MIN_VISIBLE, w // 2):
        return True
    return False


def clamp_panel_geometry(x, y, w, h, *,
                         avail: QRect | None = None) -> tuple[int, int, int, int]:
    """Ensure at least most of the panel is visible on the available desktop."""
    x, y, w, h = int(x), int(y), int(w), int(h)
    w = max(_MIN_PANEL_W, w)
    h = max(_MIN_PANEL_H, h)

    if avail is None:
        app = QGuiApplication.instance()
        if app is None:
            return x, y, w, h
        screen = (QGuiApplication.screenAt(QPoint(x, y))
                  or QGuiApplication.primaryScreen())
        if screen is None:
            return x, y, w, h
        avail = screen.availableGeometry()

    if not geometry_mostly_off_screen(x, y, w, h, avail):
        return x, y, w, h

    w = min(w, avail.width())
    h = min(h, avail.height())

    if x + _MIN_VISIBLE <= avail.left():
        x = avail.left()
    if x + w - _MIN_VISIBLE >= avail.right():
        x = max(avail.left(), avail.right() - w)

    if y + h > avail.bottom():
        y = max(avail.top(), avail.bottom() - h)
    if y + _MIN_VISIBLE > avail.bottom():
        y = max(avail.top(), avail.bottom() - _MIN_VISIBLE)
    if y < avail.top():
        y = avail.top()

    if x + w > avail.right():
        x = max(avail.left(), avail.right() - w)
    if x < avail.left():
        x = avail.left()

    return x, y, w, h


def load_layout() -> dict:
    try:
        with open(LAYOUT_FILE, "r", encoding="utf-8") as fh:
            data = json.load(fh)
        return data if isinstance(data, dict) else {}
    except (OSError, ValueError):
        return {}


def save_layout(layout: dict) -> None:
    try:
        with open(LAYOUT_FILE, "w", encoding="utf-8") as fh:
            json.dump(layout, fh, indent=2)
    except OSError:
        pass
