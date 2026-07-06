"""Scoreboard 7-segment digit drawing tests."""

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtCore import QRectF
from PyQt6.QtGui import QColor, QImage, QPainter

from overlay.widgets.scoreboard_digits import (
    draw_scoreboard_digit,
    draw_scoreboard_text,
    scoreboard_text_width,
)


def test_scoreboard_text_width_two_digits():
    h = 24.0
    w_two = scoreboard_text_width("45", h, min_digits=2)
    w_pad = scoreboard_text_width("3", h, min_digits=2)
    assert w_two == w_pad
    assert w_two > 0


def test_scoreboard_text_width_min_digits_padding():
    h = 20.0
    one = scoreboard_text_width("7", h, min_digits=0)
    two = scoreboard_text_width("7", h, min_digits=2)
    assert two > one


def test_draw_scoreboard_text_paint_smoke():
    img = QImage(80, 32, QImage.Format.Format_ARGB32)
    img.fill(0)
    p = QPainter(img)
    try:
        draw_scoreboard_text(
            p, QRectF(0, 0, 80, 32), "45", QColor("#ff8c00"), min_digits=2)
        draw_scoreboard_digit(p, 4, 4, 20, 24, "8", QColor("#ff8c00"))
    finally:
        p.end()


@pytest.fixture(scope="module")
def qapp():
    from PyQt6.QtWidgets import QApplication

    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_draw_scoreboard_text_requires_qapp(qapp):
    img = QImage(60, 28, QImage.Format.Format_ARGB32)
    img.fill(0)
    p = QPainter(img)
    try:
        draw_scoreboard_text(
            p, QRectF(0, 0, 60, 28), "02", QColor(255, 140, 0), min_digits=2)
    finally:
        p.end()
