"""7-segment scoreboard digit drawing for IMS-style LED displays."""

from __future__ import annotations

import math

from PyQt6.QtCore import QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QPainter

# Segments a–g in standard 7-segment layout:
#   a
# f   b
#   g
# e   c
#   d
_DIGITS: dict[str, tuple[int, ...]] = {
    "0": (1, 1, 1, 1, 1, 1, 0),
    "1": (0, 1, 1, 0, 0, 0, 0),
    "2": (1, 1, 0, 1, 1, 0, 1),
    "3": (1, 1, 1, 1, 0, 0, 1),
    "4": (0, 1, 1, 0, 0, 1, 1),
    "5": (1, 0, 1, 1, 0, 1, 1),
    "6": (1, 0, 1, 1, 1, 1, 1),
    "7": (1, 1, 1, 0, 0, 0, 0),
    "8": (1, 1, 1, 1, 1, 1, 1),
    "9": (1, 1, 1, 1, 0, 1, 1),
}

# Horizontal segment endpoints as fractions of digit box (x0,y0,x1,y1).
_H_SEGS = (
    (0.12, 0.06, 0.88, 0.06),   # a top
    (0.12, 0.50, 0.88, 0.50),   # g middle
    (0.12, 0.94, 0.88, 0.94),   # d bottom
)
# Vertical segments (x, y0, y1).
_V_SEGS = (
    (0.06, 0.10, 0.46),   # f upper-left
    (0.94, 0.10, 0.46),   # b upper-right
    (0.06, 0.54, 0.90),   # e lower-left
    (0.94, 0.54, 0.90),   # c lower-right
)
_SEG_ORDER = ("a", "b", "c", "d", "e", "f", "g")


def _digit_size(digit_h: float) -> tuple[float, float]:
    w = digit_h * 0.62
    return w, digit_h


def scoreboard_text_width(text: str, digit_h: float, *,
                          min_digits: int = 0) -> float:
    """Total width for a scoreboard string (right-aligned block)."""
    digits = _normalize_digits(text)
    n = max(len(digits), min_digits, 1 if digits else 0)
    if n == 0:
        return 0.0
    dw, _ = _digit_size(digit_h)
    gap = dw * 0.10
    return n * dw + max(0, n - 1) * gap


def _normalize_digits(text: str) -> str:
    return "".join(ch for ch in str(text or "").strip() if ch.isdigit())


def _segment_specs(x: float, y: float, w: float, h: float,
                   mask: tuple[int, ...]) -> list[tuple[QPointF, QPointF, bool]]:
    """Return lit segments as (start, end, horizontal)."""
    specs: list[tuple[QPointF, QPointF, bool]] = []
    for on, seg in zip(mask, _SEG_ORDER):
        if not on:
            continue
        if seg in ("a", "g", "d"):
            idx = {"a": 0, "g": 1, "d": 2}[seg]
            x0, y0, x1, y1 = _H_SEGS[idx]
            specs.append((
                QPointF(x + x0 * w, y + y0 * h),
                QPointF(x + x1 * w, y + y1 * h),
                True,
            ))
        else:
            idx = {"f": 0, "b": 1, "e": 2, "c": 3}[seg]
            sx, y0f, y1f = _V_SEGS[idx]
            specs.append((
                QPointF(x + sx * w, y + y0f * h),
                QPointF(x + sx * w, y + y1f * h),
                False,
            ))
    return specs


def _bulb_count(length: float, stroke: float, *, horizontal: bool) -> int:
    pitch = stroke * (1.55 if horizontal else 1.40)
    target = 7 if horizontal else 4
    return max(3, min(target, int(length / pitch)))


def _draw_segment_bulbs(p: QPainter, p0: QPointF, p1: QPointF, color: QColor,
                        stroke: float, *, horizontal: bool,
                        glow: bool = False) -> None:
    """Draw discrete LED bulbs along one segment."""
    dx = p1.x() - p0.x()
    dy = p1.y() - p0.y()
    length = math.hypot(dx, dy)
    if length < 1e-3:
        return

    n = _bulb_count(length, stroke, horizontal=horizontal)
    slot = length / n
    if horizontal:
        bulb_w = slot * 0.62
        bulb_h = stroke * (1.05 if not glow else 1.25)
    else:
        bulb_w = stroke * (0.82 if not glow else 1.02)
        bulb_h = slot * 0.62

    radius = min(bulb_w, bulb_h) * 0.38
    p.setPen(Qt.PenStyle.NoPen)
    p.setBrush(color)

    for i in range(n):
        t = (i + 0.5) / n
        cx = p0.x() + dx * t
        cy = p0.y() + dy * t
        rect = QRectF(cx - bulb_w / 2, cy - bulb_h / 2, bulb_w, bulb_h)
        p.drawRoundedRect(rect, radius, radius)


def draw_scoreboard_digit(p: QPainter, x: float, y: float, w: float, h: float,
                          ch: str, color: QColor) -> None:
    """Draw one 7-segment digit at (x,y) with size (w,h)."""
    mask = _DIGITS.get(ch)
    if mask is None:
        return
    specs = _segment_specs(x, y, w, h, mask)
    if not specs:
        return
    stroke = max(1.4, h * 0.11)
    glow = QColor(color)
    glow.setAlpha(80)
    for seg_color, is_glow in ((glow, True), (color, False)):
        for p0, p1, horizontal in specs:
            _draw_segment_bulbs(
                p, p0, p1, seg_color, stroke,
                horizontal=horizontal, glow=is_glow)


def draw_scoreboard_text(p: QPainter, rect: QRectF, text: str, color: QColor,
                         *, min_digits: int = 0) -> None:
    """Draw digits right-aligned inside rect (IMS pylon car-number style)."""
    digits = _normalize_digits(text)
    if not digits and min_digits <= 0:
        return
    digit_h = rect.height() * 0.88
    dw, dh = _digit_size(digit_h)
    gap = dw * 0.10
    n = max(len(digits), min_digits)
    total_w = n * dw + max(0, n - 1) * gap
    x = rect.right() - total_w
    y = rect.center().y() - dh / 2.0
    # Pad with leading blanks when min_digits > len(digits).
    padded = digits.rjust(n, " ") if digits else " " * n
    for i, ch in enumerate(padded):
        if ch == " ":
            x += dw + gap
            continue
        draw_scoreboard_digit(p, x, y, dw, dh, ch, color)
        x += dw + gap
