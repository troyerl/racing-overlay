"""
Directional proximity-warning radar.

You are the white car in the center. When a car is alongside, a red marker
appears on that side (from CarLeftRight) and slides fore/aft: low (level with
your rear bumper) when the car is behind, rising to the top as it pulls up to
your front bumper. When a car is within range ahead or behind, a colored glow
appears above/below, shading yellow -> red as it gets closer. Front and rear
sensing can each be turned off. The background is transparent so it floats over
the game.

Honest limitation: iRacing reports only an aggregate CarLeftRight for the player
(left / right / both), not each rival's lateral offset, so the side is on/off
(with a stronger tint for 2-car situations). The fore/aft marker position and
front/rear closeness are real, derived from CarIdxLapDistPct deltas.

All colors, sizes, easing and toggles come from config.CFG["radar"].
"""

from __future__ import annotations

from PyQt6.QtCore import QElapsedTimer, QPointF, QRectF, Qt
from PyQt6.QtGui import QColor, QFont, QLinearGradient, QPainter, QPen, QPixmap
from PyQt6.QtWidgets import QWidget

from .. import config
from .chrome import draw_card, ease


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
                     "right2": False, "left_pos": 0.0, "right_pos": 0.0,
                     "ahead": None, "behind": None,
                     "left_label": "", "right_label": "",
                     "left_closing": None, "right_closing": None,
                     "clear_secs": None}
        self._a = {"left": 0.0, "right": 0.0, "ahead": 0.0, "behind": 0.0,
                   "left_pos": 0.0, "right_pos": 0.0}
        self._clock = QElapsedTimer()
        self._clock.start()
        self._last_ms = 0
        self._animating = False
        self._pix_cache: dict[tuple, QPixmap] = {}
        self.setMinimumSize(140, 180)

    def _cached_pixmap(self, key: tuple, build) -> QPixmap:
        pm = self._pix_cache.get(key)
        if pm is not None:
            return pm
        pm = build()
        if len(self._pix_cache) > 48:
            self._pix_cache.clear()
        self._pix_cache[key] = pm
        return pm

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
            draw_card(p, w, h, "radar")

        car_w = max(12.0, w * sz["car_w"])
        car_h = max(24.0, h * sz["car_h"])
        bar_h = car_h * sz["bar_h"]
        inner = car_w * 0.75

        dt = self._dt()
        a = self._a
        side_tau = rc["ease_side_tau"]
        glow_tau = rc["ease_glow_tau"]
        show_front = rc.get("show_front", True)
        show_rear = rc.get("show_rear", True)
        t_left = 1.0 if d.get("left") else 0.0
        t_right = 1.0 if d.get("right") else 0.0
        t_ahead = (d.get("ahead") or 0.0) if show_front else 0.0
        t_behind = (d.get("behind") or 0.0) if show_rear else 0.0
        t_lpos = float(d.get("left_pos") or 0.0)
        t_rpos = float(d.get("right_pos") or 0.0)
        a["left"] = ease(a["left"], t_left, dt, side_tau)
        a["right"] = ease(a["right"], t_right, dt, side_tau)
        a["ahead"] = ease(a["ahead"], t_ahead, dt, glow_tau)
        a["behind"] = ease(a["behind"], t_behind, dt, glow_tau)
        a["left_pos"] = ease(a["left_pos"], t_lpos, dt, side_tau)
        a["right_pos"] = ease(a["right_pos"], t_rpos, dt, side_tau)
        self._animating = (abs(a["left"] - t_left) > 0.004
                           or abs(a["right"] - t_right) > 0.004
                           or abs(a["ahead"] - t_ahead) > 0.004
                           or abs(a["behind"] - t_behind) > 0.004
                           or (a["left"] > 0.01 and abs(a["left_pos"] - t_lpos) > 0.004)
                           or (a["right"] > 0.01 and abs(a["right_pos"] - t_rpos) > 0.004))

        if show_front and a["ahead"] > 0.01:
            self._v_glow(p, cx, cy - car_h * 0.45, h * 0.06, a["ahead"], up=True)
        if show_rear and a["behind"] > 0.01:
            self._v_glow(p, cx, cy + car_h * 0.45, h * 0.94, a["behind"], up=False)

        # Vertical travel of the side marker: +pos toward the top (front bumper),
        # -pos toward the bottom (rear bumper), kept within the card. The marker
        # height (~a car length) is the configurable sizes.bar_h fraction.
        marker_h = max(18.0, bar_h)
        travel = max(0.0, h * 0.5 - marker_h * 0.5 - h * 0.06)
        # Optional: tint the side marker by fore/aft overlap (red dead-alongside,
        # yellow toward your bumpers) as a proxy for closeness. None == plain red.
        prox = rc.get("side_proximity_color", False)
        if a["left"] > 0.01:
            p.save()
            p.setOpacity(a["left"])
            self._side_marker(p, w * 0.07, cx - inner, cy - a["left_pos"] * travel,
                              marker_h, strong=d.get("left2"), to_left=True,
                              closeness=(1.0 - abs(a["left_pos"])) if prox else None,
                              closing=d.get("left_closing"),
                              label=str(d.get("left_label") or ""))
            p.restore()
        if a["right"] > 0.01:
            p.save()
            p.setOpacity(a["right"])
            self._side_marker(p, cx + inner, w * 0.93, cy - a["right_pos"] * travel,
                              marker_h, strong=d.get("right2"), to_left=False,
                              closeness=(1.0 - abs(a["right_pos"])) if prox else None,
                              closing=d.get("right_closing"),
                              label=str(d.get("right_label") or ""))
            p.restore()

        clear_secs = d.get("clear_secs")
        if rc.get("show_clear_timer") and clear_secs is not None and clear_secs >= 0:
            txt = f"Clear {clear_secs:.0f}s"
            csz = max(6, round(7 * config.text_scale_for("radar")))
            p.setFont(QFont(config.CFG.get("font_family", "Arial"), csz,
                            QFont.Weight.Bold))
            fm = p.fontMetrics()
            tw = fm.horizontalAdvance(txt) + 8
            th = fm.height() + 4
            rect = QRectF(cx - tw / 2, h * 0.92 - th, tw, th)
            p.setPen(Qt.PenStyle.NoPen)
            p.setBrush(QColor(10, 13, 17, 170))
            p.drawRoundedRect(rect, 3, 3)
            p.setPen(_rcol("nose"))
            p.drawText(rect, Qt.AlignmentFlag.AlignCenter, txt)

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

    def _side_marker(self, p, x0, x1, yc, marker_h, strong, to_left,
                     closeness=None, closing=None, label=""):
        left, right = min(x0, x1), max(x0, x1)
        w = max(1, int(round(right - left)))
        h = max(1, int(round(marker_h)))
        alpha = 235 if strong else 195
        c_bucket = -1 if closeness is None else int(max(0.0, min(1.0, closeness)) * 20)
        close_bucket = -1 if closing is None else int(max(0.0, min(1.0, closing)) * 20)
        key = ("side", w, h, to_left, alpha, c_bucket, close_bucket)
        pm = self._cached_pixmap(key, lambda: self._build_side_pixmap(
            w, h, to_left, alpha, closeness, closing))
        p.drawPixmap(int(round(left)), int(round(yc - marker_h / 2.0)), pm)
        if label:
            lsz = max(6, round(min(w, h) * 0.38))
            p.setFont(QFont(config.CFG.get("font_family", "Arial"), lsz,
                            QFont.Weight.Bold))
            p.setPen(QColor(255, 255, 255))
            p.drawText(QRectF(left, yc - marker_h / 2.0, w, marker_h),
                       Qt.AlignmentFlag.AlignCenter, label)

    def _build_side_pixmap(self, w, h, to_left, alpha, closeness, closing=None):
        if closing is not None and closing > 0:
            base = _prox_color(closing, 255)
        elif closeness is not None:
            base = _prox_color(closeness, 255)
        else:
            base = _rcol("red")
        pm = QPixmap(w, h)
        pm.fill(Qt.GlobalColor.transparent)
        pp = QPainter(pm)
        pp.setRenderHint(QPainter.RenderHint.Antialiasing, True)
        if to_left:
            grad = QLinearGradient(float(w), 0.0, 0.0, 0.0)
        else:
            grad = QLinearGradient(0.0, 0.0, float(w), 0.0)
        grad.setColorAt(0.0, QColor(base.red(), base.green(), base.blue(), alpha))
        grad.setColorAt(1.0, QColor(base.red(), base.green(), base.blue(), 0))
        pp.fillRect(0, 0, w, h, grad)
        self._feather_mask(pp, w, h, vertical=True)
        pp.end()
        return pm

    def _v_glow(self, p, cx, y_inner, y_outer, closeness, up):
        half_w = self.width() * _rcfg()["sizes"]["glow_w"]
        top, bottom = min(y_inner, y_outer), max(y_inner, y_outer)
        w = max(1, int(round(half_w * 2.0)))
        h = max(1, int(round(bottom - top)))
        inner_local = float(y_inner - top)
        outer_local = float(y_outer - top)
        c_bucket = int(max(0.0, min(1.0, closeness)) * 20)
        peak = int(80 + 130 * max(0.0, min(1.0, closeness)))
        key = ("glow", w, h, c_bucket, peak,
               round(inner_local, 1), round(outer_local, 1))
        pm = self._cached_pixmap(key, lambda: self._build_glow_pixmap(
            w, h, peak, closeness, inner_local, outer_local))
        p.drawPixmap(int(round(cx - half_w)), int(round(top)), pm)

    def _build_glow_pixmap(self, w, h, peak, closeness, inner_local, outer_local):
        col = _prox_color(closeness, peak)
        fade = QColor(col.red(), col.green(), col.blue(), 0)
        pm = QPixmap(w, h)
        pm.fill(Qt.GlobalColor.transparent)
        pp = QPainter(pm)
        pp.setRenderHint(QPainter.RenderHint.Antialiasing, True)
        grad = QLinearGradient(0.0, inner_local, 0.0, outer_local)
        grad.setColorAt(0.0, col)
        grad.setColorAt(1.0, fade)
        pp.fillRect(0, 0, w, h, grad)
        self._feather_mask(pp, w, h, vertical=False)
        pp.end()
        return pm
