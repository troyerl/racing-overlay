"""CPU/memory optimization: repaint gating, map cache, config diff."""

from __future__ import annotations

import pytest

pytest.importorskip("PyQt6.QtWidgets")

from PyQt6.QtWidgets import QApplication

from overlay import config
from overlay import telemetry as tele
from overlay.app import AdvancedSimHUD, _WIDGET_KEYS
from overlay.widgets.dash import DashWidget
from overlay.widgets.inputs import InputTraceWidget
from overlay.widgets.track_map import TrackMapWidget


@pytest.fixture(scope="module")
def qapp():
    app = QApplication.instance()
    if app is None:
        app = QApplication([])
    return app


def test_dash_discrete_key_ignores_easing_fields():
    a = {"gear": 3, "rpm": 5000.0, "throttle": 0.5, "brake": 0.0, "clutch": 0.0}
    b = {"gear": 3, "rpm": 7000.0, "throttle": 0.9, "brake": 0.1, "clutch": 0.2}
    assert tele.dash_discrete_key(a) == tele.dash_discrete_key(b)


def test_dash_easing_moved_detects_pedal_change():
    prev = {"rpm": 5000, "throttle": 0.5, "brake": 0.0, "clutch": 0.0}
    nxt = {"rpm": 5000, "throttle": 0.6, "brake": 0.0, "clutch": 0.0}
    assert tele.dash_easing_moved(prev, nxt)


def test_dash_set_data_skips_when_only_easing_within_epsilon(qapp):
    w = DashWidget()
    updates: list[int] = []
    orig = w.update

    def counted_update():
        updates.append(1)
        orig()

    w.update = counted_update  # type: ignore[method-assign]
    base = {
        "gear": 4, "rpm": 6000.0, "throttle": 0.5, "brake": 0.0, "clutch": 0.0,
        "position": 3, "lap": 5,
    }
    w.set_data(base)
    assert len(updates) == 1
    w.set_data(dict(base, rpm=6000.0001, throttle=0.50001))
    assert len(updates) == 1


def test_dash_animating_schedules_repaint(qapp, monkeypatch):
    w = DashWidget()
    w.data = {
        "gear": 4, "rpm": 6000.0, "throttle": 0.2, "brake": 0.0, "clutch": 0.0,
    }
    w._ped["t"] = 0.0
    updates: list[int] = []
    orig = w.update

    def counted_update():
        updates.append(1)
        orig()

    w.update = counted_update  # type: ignore[method-assign]
    w.paintEvent(None)  # type: ignore[arg-type]
    assert w._animating
    assert len(updates) >= 1


def test_inputs_unchanged_sample_skips_update(qapp):
    w = InputTraceWidget()
    updates: list[int] = []
    orig = w.update

    def counted_update():
        updates.append(1)
        orig()

    w.update = counted_update  # type: ignore[method-assign]
    payload = {
        "throttle": 0.5, "brake": 0.0, "clutch": 1.0, "steer": 0.5,
        "abs_active": False, "gear": 3,
    }
    w.set_data(payload)
    assert len(updates) == 1
    w.set_data(dict(payload))
    assert len(updates) == 1


def test_map_car_targets_moved_respects_epsilon():
    prev = [(0, 0.5, "1", "#ff0000", True, False, False)]
    tiny = [(0, 0.5000001, "1", "#ff0000", True, False, False)]
    big = [(0, 0.502, "1", "#ff0000", True, False, False)]
    assert not TrackMapWidget._car_targets_moved(prev, tiny)
    assert TrackMapWidget._car_targets_moved(prev, big)


def test_map_static_cache_key_changes_on_resize(qapp):
    w = TrackMapWidget()
    w.set_track([(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)], 0.0, [])
    w.resize(400, 300)
    k1 = w._static_cache_key()
    w.resize(500, 300)
    k2 = w._static_cache_key()
    assert k1 and k2 and k1 != k2


def test_map_static_cache_invalidates_on_path_change(qapp):
    w = TrackMapWidget()
    w.set_track([(0.0, 0.0), (1.0, 0.0), (1.0, 1.0)], 0.0, [])
    w.resize(300, 300)
    w._static_pix = object()  # type: ignore[assignment]
    w._static_key = ("fake",)
    w.set_track([(0.0, 0.0), (2.0, 0.0), (2.0, 1.0)], 0.0, [])
    assert w._static_pix is None


def test_repaint_config_sections_only_touches_changed_widget():
    hud = object.__new__(AdvancedSimHUD)
    hud._cfg_section_snap = {}
    painted: list[str] = []

    class _W:
        def __init__(self, name):
            self.name = name

        def update(self):
            painted.append(self.name)

    widgets = {k: _W(k) for k in _WIDGET_KEYS}
    hud._widget_by_key = lambda: widgets  # type: ignore[method-assign]
    hud._repaint_all = lambda: painted.append("__all__")  # type: ignore[method-assign]
    hud.map_widget = type("M", (), {  # type: ignore[attr-defined]
        "_invalidate_static_cache": lambda self: None,
    })()

    cfg = {k: {"show": True} for k in _WIDGET_KEYS}
    hud._repaint_config_sections(cfg)
    assert "__all__" in painted

    painted.clear()
    cfg2 = dict(cfg)
    cfg2["map"] = {"show": True, "mirror": True}
    hud._repaint_config_sections(cfg2)
    assert painted == ["map"]


def test_demo_arrays_reused_within_tick():
    from overlay import demo_data

    ir = demo_data.FakeIRSDK(num_cars=4)
    ir.begin_tick()
    a = ir["CarIdxLapDistPct"]
    b = ir["CarIdxLapDistPct"]
    assert a is b
    c = ir["CarIdxEstTime"]
    d = ir["CarIdxEstTime"]
    assert c is d
