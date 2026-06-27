"""
Directional proximity-warning radar.

You are the white car in the center. When a car is alongside, a red bar fades
outward on that side (from CarLeftRight). When a car is within range ahead or
behind, a colored glow appears above/below, shading yellow -> red as it gets
closer. The widget background is transparent so it floats over the game.

Honest limitation: iRacing reports only an aggregate CarLeftRight for the player
(left / right / both), not each rival's lateral offset, so the side bars are
on/off (with a stronger tint for 2-car situations). Front/rear closeness is real,
derived from CarIdxLapDistPct deltas.

All colors, sizes, easing and toggles come from config.CFG["radar"].
"""

from __future__ import annotations

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QLinearGradient, QPainter, QPen, QPixmap
from PyQt6.QtWidgets import QWidget

from .. import config
from .table import ease


def _rcfg() -> dict:
    return config.CFG["radar"]


def _rcol(key: str) -> QColor:
    return config.qcolor(_rcfg()["colors"][key])


def _prox_color(closeness: float, alpha: int = 200) -> QColor:
    """Yellow (far) -> red (touching)."""
    c = max(0.0, min(1.0, closeness))
    y = _rcol("yellow")
    r = _rcol("red")
    return QColor(
        int(y.red() + (r.red() - y.red()) * c),
        int(y.green() + (r.green() - y.green()) * c),
        int(y.blue() + (r.blue() - y.blue()) * c),
        alpha,
    )


class RadarWidget(QWidget):
    def __init__(self, parent=None):
        super().__init__(parent)
        self.data = {"left": False, "right": False, "left2": False,
                     "right2": False, "ahead": None, "behind": None}
        self._a = {"left": 0.0, "right": 0.0, "ahead": 0.0, "behind": 0.0}
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0
        self._animating = False
        self.setMinimumSize(140, 180)

    def set_data(self, data: dict) -> None:
        changed = data != self.data
        self.data = data
        if changed or self._animating:
            self.update()

    def _dt(self) -> float:
        now = self._clock.elapsed()
        dt = (now - self._last_ms) / 1000.0
        self._last_ms = now
        return max(0.0, min(0.1, dt))

    def paintEvent(self, event) -> None:  # noqa: N802
        p = QPainter(self)
        p.setRenderHint(QPainter.RenderHint.Antialiasing)
        w, h = self.width(), self.height()
        cx, cy = w / 2.0, h / 2.0
        d = self.data or {}
        rc = _rcfg()
        sz = rc["sizes"]

        # Rounded card behind the radar so it matches the dash/table panels.
        if rc.get("show_panel", True) and "bg_top" in rc["colors"]:
            radius = max(8.0, min(w, h) * rc.get("corner_radius_frac", 0.12))
            grad = QLinearGradient(0, 0, 0, h)
            grad.setColorAt(0.0, _rcol("bg_top"))
            grad.setColorAt(1.0, _rcol("bg_bottom"))
            p.setBrush(grad)
            p.setPen(QPen(_rcol("panel_border"), 1))
            p.drawRoundedRect(QRectF(0.5, 0.5, w - 1, h - 1), radius, radius)

        car_w = max(12.0, w * sz["car_w"])
        car_h = max(24.0, h * sz["car_h"])
        bar_h = car_h * sz["bar_h"]
        inner = car_w * 0.75

        dt = self._dt()
        a = self._a
        side_tau = rc["ease_side_tau"]
        glow_tau = rc["ease_glow_tau"]
        t_left = 1.0 if d.get("left") else 0.0
        t_right = 1.0 if d.get("right") else 0.0
        t_ahead = d.get("ahead") or 0.0
        t_behind = d.get("behind") or 0.0
        a["left"] = ease(a["left"], t_left, dt, side_tau)
        a["right"] = ease(a["right"], t_right, dt, side_tau)
        a["ahead"] = ease(a["ahead"], t_ahead, dt, glow_tau)
        a["behind"] = ease(a["behind"], t_behind, dt, glow_tau)
        self._animating = (abs(a["left"] - t_left) > 0.004
                           or abs(a["right"] - t_right) > 0.004
                           or abs(a["ahead"] - t_ahead) > 0.004
                           or abs(a["behind"] - t_behind) > 0.004)

        if a["ahead"] > 0.01:
            self._v_glow(p, cx, cy - car_h * 0.45, h * 0.06, a["ahead"], up=True)
        if a["behind"] > 0.01:
            self._v_glow(p, cx, cy + car_h * 0.45, h * 0.94, a["behind"], up=False)

        if a["left"] > 0.01:
            p.save()
            p.setOpacity(a["left"])
            self._side_bar(p, w * 0.07, cx - inner, cy, bar_h,
                           strong=d.get("left2"), to_left=True)
            p.restore()
        if a["right"] > 0.01:
            p.save()
            p.setOpacity(a["right"])
            self._side_bar(p, cx + inner, w * 0.93, cy, bar_h,
                           strong=d.get("right2"), to_left=False)
            p.restore()

        if rc.get("show_axis", True):
            p.setPen(QPen(_rcol("axis"), max(1.0, w * 0.006)))
            p.drawLine(QPointF(w * 0.08, cy), QPointF(w * 0.92, cy))
            p.drawLine(QPointF(cx, h * 0.10), QPointF(cx, h * 0.90))
        if rc.get("show_nose", True):
            p.setPen(QPen(_rcol("nose"), max(1.5, w * 0.012)))
            p.drawLine(QPointF(cx, cy - car_h * 0.5),
                       QPointF(cx, cy - car_h * 0.5 - h * sz["nose_len"]))

        p.setPen(Qt.PenStyle.NoPen)
        p.setBrush(_rcol("car"))
        car = QRectF(cx - car_w / 2, cy - car_h / 2, car_w, car_h)
        p.drawRoundedRect(car, car_w * 0.4, car_w * 0.4)

    @staticmethod
    def _feather_mask(pp, w, h, vertical):
        # Multiply a perpendicular fade onto whatever is already drawn so the
        # long edges of the bar dissolve instead of ending in a hard line.
        pp.setCompositionMode(QPainter.CompositionMode.CompositionMode_DestinationIn)
        if vertical:
            m = QLinearGradient(0.0, 0.0, 0.0, float(h))
        else:
            m = QLinearGradient(0.0, 0.0, float(w), 0.0)
        m.setColorAt(0.0, QColor(0, 0, 0, 0))
        m.setColorAt(0.5, QColor(0, 0, 0, 255))
        m.setColorAt(1.0, QColor(0, 0, 0, 0))
        pp.fillRect(0, 0, w, h, m)

    def _side_bar(self, p, x0, x1, cy, bar_h, strong, to_left):
        # A soft bar beside the car: strong at the inner edge next to you and
        # fading outward, with the top/bottom edges feathered so there's no box.
        left, right = min(x0, x1), max(x0, x1)
        w = max(1, int(round(right - left)))
        h = max(1, int(round(bar_h)))
        base = _rcol("red")
        alpha = 235 if strong else 195
        pm = QPixmap(w, h)
        pm.fill(Qt.GlobalColor.transparent)
        pp = QPainter(pm)
        pp.setRenderHint(QPainter.RenderHint.Antialiasing, True)
        # Horizontal fade: strongest at the inner edge (nearest the car).
        if to_left:
            grad = QLinearGradient(float(w), 0.0, 0.0, 0.0)
        else:
            grad = QLinearGradient(0.0, 0.0, float(w), 0.0)
        grad.setColorAt(0.0, QColor(base.red(), base.green(), base.blue(), alpha))
        grad.setColorAt(1.0, QColor(base.red(), base.green(), base.blue(), 0))
        pp.fillRect(0, 0, w, h, grad)
        self._feather_mask(pp, w, h, vertical=True)
        pp.end()
        p.drawPixmap(int(round(left)), int(round(cy - bar_h / 2.0)), pm)

    def _v_glow(self, p, cx, y_inner, y_outer, closeness, up):
        # A soft bar ahead/behind the car: strong near the car and fading toward
        # the edge, with the side edges feathered so there's no box.
        half_w = self.width() * _rcfg()["sizes"]["glow_w"]
        top, bottom = min(y_inner, y_outer), max(y_inner, y_outer)
        w = max(1, int(round(half_w * 2.0)))
        h = max(1, int(round(bottom - top)))
        peak = int(80 + 130 * max(0.0, min(1.0, closeness)))
        col = _prox_color(closeness, peak)
        fade = QColor(col.red(), col.green(), col.blue(), 0)
        pm = QPixmap(w, h)
        pm.fill(Qt.GlobalColor.transparent)
        pp = QPainter(pm)
        pp.setRenderHint(QPainter.RenderHint.Antialiasing, True)
        # Vertical fade: strongest at the inner edge (nearest the car).
        inner_local = float(y_inner - top)
        outer_local = float(y_outer - top)
        grad = QLinearGradient(0.0, inner_local, 0.0, outer_local)
        grad.setColorAt(0.0, col)
        grad.setColorAt(1.0, fade)
        pp.fillRect(0, 0, w, h, grad)
        self._feather_mask(pp, w, h, vertical=False)
        pp.end()
        p.drawPixmap(int(round(cx - half_w)), int(round(top)), pm)
