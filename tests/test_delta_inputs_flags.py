"""Delta bar modes, inputs history, and flag context formatters."""

from overlay import telemetry as tele
from overlay import traffic as tr
from overlay.app import AdvancedSimHUD


class _FakeIR:
    def __init__(self, mapping):
        self._m = mapping

    def __getitem__(self, key):
        if key not in self._m:
            raise KeyError(key)
        return self._m[key]


def _hud(**kwargs) -> AdvancedSimHUD:
    hud = object.__new__(AdvancedSimHUD)
    for k, v in kwargs.items():
        setattr(hud, k, v)
    return hud


def test_delta_bar_last_lap_mode(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("delta_bar", {}), "mode", "last_lap")
    hud = _hud(
        ir=_FakeIR({"LapCurrentLapTime": 35.0}),
        _delta_last_lap_time=32.5,
        _delta_pit_hold=False,
        delta_bar_widget=type("W", (), {"set_data": lambda s, d: setattr(s, "d", d)})(),
    )
    hud._update_delta_bar()
    assert abs(hud.delta_bar_widget.d["delta"] - 2.5) < 0.01


def test_delta_bar_leader_last_mode(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("delta_bar", {}),
                        "mode", "leader_last")
    hud = _hud(
        ir=_FakeIR({"LapCurrentLapTime": 34.0}),
        _car_last=[30.5, 32.0, 33.0, 31.0],
        _delta_pit_hold=False,
        delta_bar_widget=type("W", (), {"set_data": lambda s, d: setattr(s, "d", d)})(),
    )
    hud._update_delta_bar(player=0, positions=[1, 2, 3, 4])
    assert abs(hud.delta_bar_widget.d["delta"] - 3.5) < 0.01


def test_delta_pit_hold_while_on_pit(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("delta_bar", {}), "mode", "session_best")
    hud = _hud(
        ir=_FakeIR({"LapDeltaToSessionBest": 0.5}),
        _delta_pit_hold=False,
        _delta_was_on_pit=False,
    )
    hud._track_delta_pit_hold(True, 0, 0)
    assert hud._delta_pit_hold
    assert hud._delta_bar_value() is None


def test_delta_pit_hold_suppresses_until_sector(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("delta_bar", {}), "mode", "session_best")
    hud = _hud(
        ir=_FakeIR({"LapDeltaToSessionBest": 0.5}),
        _delta_pit_hold=False,
        _delta_was_on_pit=True,
    )
    hud._track_delta_pit_hold(False, 0, 0)
    assert hud._delta_pit_hold
    assert hud._delta_bar_value() is None
    hud._track_delta_pit_hold(False, 0, 1)
    assert not hud._delta_pit_hold
    assert abs(hud._delta_bar_value() - 0.5) < 0.01


def test_delta_bar_pit_hold_returns_none(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("delta_bar", {}), "mode", "session_best")
    hud = _hud(
        ir=_FakeIR({"LapDeltaToSessionBest": 2.0}),
        _delta_pit_hold=True,
        delta_bar_widget=type("W", (), {
            "data": {"delta": 2.0},
            "_animating": False,
            "set_data": lambda s, d: setattr(s, "d", d),
        })(),
    )
    hud._update_delta_bar()
    assert hud.delta_bar_widget.d["delta"] is None


def test_needs_sector_timer_dash_and_delta_bar(monkeypatch):
    from overlay import config
    from overlay.app import AdvancedSimHUD

    monkeypatch.setitem(config.CFG.setdefault("dash", {}), "show_delta_bar", True)
    en = {"sector_timing": False, "laptime_log": False, "map": False, "delta_bar": False}
    assert AdvancedSimHUD._needs_sector_timer(en)
    en["delta_bar"] = True
    assert AdvancedSimHUD._needs_sector_timer(en)


def test_dash_delta_bar_without_strip_metric(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("dash", {}), "show_delta_bar", True)
    monkeypatch.setitem(config.CFG["dash"], "show_flags", False)
    monkeypatch.setitem(config.CFG.setdefault("delta_bar", {}), "mode", "session_best")
    monkeypatch.setattr(config, "dash_metric_in_use", lambda key: False)
    widget = type("W", (), {"set_data": lambda s, d: setattr(s, "d", d)})()
    hud = _hud(
        ir=_FakeIR({
            "PlayerCarIdx": 0,
            "SessionLapsTotal": 0,
            "Clutch": 1.0,
            "BrakeABSactive": False,
            "Gear": 3,
            "RPM": 5000,
            "Throttle": 0.5,
            "Brake": 0.0,
            "Speed": 50.0,
            "Lap": 5,
            "LapDeltaToSessionBest": 1.23,
        }),
        _delta_pit_hold=False,
        _demo_active=False,
        _driver_cache={},
        _car_info={},
        dash_widget=widget,
    )
    hud._update_dash(0, [1], None)
    assert abs(widget.d["delta"] - 1.23) < 0.01


def test_flag_context_yellow_includes_sector():
    from overlay.widgets.sector_timing import SectorTimer

    hud = _hud(
        ir=_FakeIR({}),
        _sector_timer=SectorTimer(),
    )
    hud._sector_timer.starts = [0.0, 0.34, 0.68]
    hud._sector_timer.idx = 1
    ctx = hud._flag_context("yellow", hud._FLAG_YELLOW_BASE)
    assert "Sector S2" in ctx


def test_flag_context_checkered_shows_position():
    from overlay import config

    cfg = config.CFG.setdefault("flags", {})
    old = cfg.get("show_finish_position")
    cfg["show_finish_position"] = True
    try:
        hud = _hud(ir=_FakeIR({}))
        ctx = hud._flag_context("checkered", 0, player=2,
                                positions=[3, 2, 1, 4])
        assert "P1" in ctx
    finally:
        if old is None:
            cfg.pop("show_finish_position", None)
        else:
            cfg["show_finish_position"] = old


def test_sector_timing_snap_key_rounds_clock():
    snap = {
        "cur_lap": 32.4567,
        "last_lap": 32.1,
        "best_lap": 31.9,
        "predicted_lap": 32.44,
        "active_idx": 1,
        "sectors": [{"time": 10.123, "status": "running", "active": True,
                     "delta": 0.001}],
    }
    k1 = tele.sector_timing_snap_key(snap)
    k2 = tele.sector_timing_snap_key(dict(snap, cur_lap=32.4554))
    assert k1 == k2


def test_inputs_shift_gear_changes_in_history():
    hist = [
        (0.0, 0.5, 0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 3),
        (0.1, 0.5, 0.0, 1.0, 0.5, 0.0, 0.0, 0.0, 4),
    ]
    shifts = []
    prev_g = None
    for entry in hist:
        g = entry[8]
        if prev_g is not None and g is not None and g != prev_g:
            shifts.append(g)
        prev_g = g
    assert shifts == [4]


def test_engine_warning_text_limiter():
    assert "LIM" in tr.engine_warning_text(tr.ENGINE_LIM)
