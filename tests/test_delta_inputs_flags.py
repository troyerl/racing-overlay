"""Delta bar modes, inputs history, and flag context formatters."""

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
        delta_bar_widget=type("W", (), {"set_data": lambda s, d: setattr(s, "d", d)})(),
    )
    hud._update_delta_bar(player=0, positions=[1, 2, 3, 4])
    assert abs(hud.delta_bar_widget.d["delta"] - 3.5) < 0.01


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
