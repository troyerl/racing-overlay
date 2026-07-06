"""Pit edit: joint point sync and handle deduplication."""

from __future__ import annotations

import math

import pytest
from PyQt6.QtCore import QPoint, QPointF, Qt, QRectF, QEvent
from PyQt6.QtGui import QMouseEvent, QWheelEvent
from PyQt6.QtWidgets import QApplication

from overlay.widgets.track_map import TrackMapWidget, _PIT_JOINT_EPS


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def _widget():
    w = TrackMapWidget()
    loop = [(x, 0.5) for x in [i / 10 for i in range(11)]]
    w.set_track(loop, start_finish=0.0, corners=[])
    return w


def _pit_edit_ready(qapp):
    w = _widget()
    w.pit_edit_mode = True
    w.resize(400, 300)
    w.show()
    qapp.processEvents()
    w._layout_scale = 200.0
    w._layout_ox = 50.0
    w._layout_oy = 40.0
    w._pit_edit_base_scale = 200.0
    w._pit_edit_base_ox = 50.0
    w._pit_edit_base_oy = 40.0
    return w


def _wheel_event(pos: QPointF, *, angle_delta_y: int = 0,
                 pixel_delta_y: int = 0) -> QWheelEvent:
    return QWheelEvent(
        pos,
        pos,
        QPoint(0, pixel_delta_y),
        QPoint(0, angle_delta_y),
        Qt.MouseButton.NoButton,
        Qt.KeyboardModifier.NoModifier,
        Qt.ScrollPhase.ScrollUpdate,
        False,
    )


def _mouse_event(event_type, pos: QPointF, button: Qt.MouseButton,
                 buttons: Qt.MouseButton | None = None) -> QMouseEvent:
    btns = buttons if buttons is not None else button
    return QMouseEvent(
        event_type,
        pos,
        pos,
        button,
        btns,
        Qt.KeyboardModifier.NoModifier,
    )


def test_sync_pit_joint_on_load(qapp):
    w = _widget()
    w.load_pit_edit(
        [(0.2, 0.4), (0.5, 0.42), (0.8, 0.44)],
        [(0.1, 0.5), (0.9, 0.48)],
    )
    assert w._pit_edit_merge[0] == w._pit_edit_road[-1]


def test_set_pit_edit_point_road_last_moves_merge_start(qapp):
    w = _widget()
    w.load_pit_edit([(0.2, 0.4), (0.5, 0.42)], [(0.5, 0.42), (0.9, 0.48)])
    w._set_pit_edit_point("road", 1, 0.55, 0.43)
    assert w._pit_edit_road[-1] == (0.55, 0.43)
    assert w._pit_edit_merge[0] == (0.55, 0.43)


def test_set_pit_edit_point_merge_first_moves_road_end(qapp):
    w = _widget()
    w.load_pit_edit([(0.2, 0.4), (0.5, 0.42)], [(0.5, 0.42), (0.9, 0.48)])
    w._set_pit_edit_point("merge", 0, 0.52, 0.41)
    assert w._pit_edit_merge[0] == (0.52, 0.41)
    assert w._pit_edit_road[-1] == (0.52, 0.41)


def test_set_pit_edit_point_joint_moves_both(qapp):
    w = _widget()
    w.load_pit_edit([(0.2, 0.4), (0.5, 0.42)], [(0.5, 0.42), (0.9, 0.48)])
    w._set_pit_edit_point("joint", 0, 0.6, 0.45)
    assert w._pit_edit_road[-1] == (0.6, 0.45)
    assert w._pit_edit_merge[0] == (0.6, 0.45)


def test_pit_has_joint_when_coincident(qapp):
    w = _widget()
    w.load_pit_edit([(0.2, 0.4), (0.5, 0.42)], [(0.5, 0.42), (0.9, 0.48)])
    assert w._pit_has_joint()
    w._pit_edit_merge[0] = (0.5 + _PIT_JOINT_EPS * 10, 0.42)
    assert not w._pit_has_joint()


def test_pop_road_reties_merge_start(qapp):
    w = _widget()
    w.load_pit_edit([(0.2, 0.4), (0.5, 0.42), (0.8, 0.44)],
                    [(0.8, 0.44), (0.9, 0.48)])
    w.pit_edit_phase = "road"
    w.pop_last_pit_edit_point()
    assert w._pit_edit_merge[0] == w._pit_edit_road[-1]


def test_joint_handle_single_hit(qapp):
    w = _widget()
    w.load_pit_edit([(0.2, 0.4), (0.5, 0.42)], [(0.5, 0.42), (0.9, 0.48)])
    w.pit_edit_mode = True
    w._pit_hit = []
    # Simulate handle layout at origin for hit test wiring.
    w._pit_hit.append((QRectF(-10, -10, 20, 20), "joint", 0))
    assert w._pit_handle_at(QPointF(0, 0)) == ("joint", 0)


def test_reset_pit_edit_view(qapp):
    w = _widget()
    w.pit_edit_mode = True
    w._pit_edit_zoom = 4.0
    w._pit_edit_pan = (50.0, -30.0)
    w.reset_pit_edit_view()
    assert w._pit_edit_zoom == 1.0
    assert w._pit_edit_pan == (0.0, 0.0)


def test_wheel_angle_delta_zooms(qapp):
    w = _pit_edit_ready(qapp)
    w.wheelEvent(_wheel_event(QPointF(200, 150), angle_delta_y=120))
    assert w._pit_edit_zoom > 1.0


def test_wheel_pixel_delta_zooms(qapp):
    w = _pit_edit_ready(qapp)
    w.wheelEvent(_wheel_event(QPointF(200, 150), pixel_delta_y=10))
    assert w._pit_edit_zoom > 1.0


def test_wheel_zoom_respects_layout_coords(qapp):
    w = _pit_edit_ready(qapp)
    w._layout_mirror = True
    w._layout_rot = 90
    w.wheelEvent(_wheel_event(QPointF(200, 150), angle_delta_y=120))
    assert w._pit_edit_zoom > 1.0
    assert all(math.isfinite(v) for v in w._pit_edit_pan)


def test_middle_click_drag_pans_without_adding_points(qapp):
    w = _pit_edit_ready(qapp)
    w.load_pit_edit([(0.2, 0.4), (0.5, 0.42)], [(0.5, 0.42), (0.9, 0.48)])
    n_road = len(w._pit_edit_road)
    n_merge = len(w._pit_edit_merge)
    start_pan = w._pit_edit_pan
    pos = QPointF(200, 150)
    w.mousePressEvent(_mouse_event(
        QEvent.Type.MouseButtonPress, pos, Qt.MouseButton.MiddleButton))
    assert w._pit_pan_active
    w.mouseMoveEvent(_mouse_event(
        QEvent.Type.MouseMove, QPointF(230, 170), Qt.MouseButton.NoButton,
        Qt.MouseButton.MiddleButton))
    assert w._pit_edit_pan != start_pan
    w.mouseReleaseEvent(_mouse_event(
        QEvent.Type.MouseButtonRelease, QPointF(230, 170),
        Qt.MouseButton.MiddleButton))
    assert not w._pit_pan_active
    assert len(w._pit_edit_road) == n_road
    assert len(w._pit_edit_merge) == n_merge
