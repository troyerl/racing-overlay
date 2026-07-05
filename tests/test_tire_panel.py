"""Tire panel payload tests."""

from overlay.app import AdvancedSimHUD


class _FakeIR:
    def __getitem__(self, key):
        data = {
            "LFwearM": 0.85, "RFwearM": 0.82,
            "LRwearM": 0.90, "RRwearM": 0.88,
            "LFtempCL": 85.0, "RFtempCL": 87.0,
            "LRtempCL": 82.0, "RRtempCL": 84.0,
        }
        return data[key]


def test_update_tire_panel_corners(monkeypatch):
    from overlay import config

    monkeypatch.setitem(config.CFG.setdefault("tire_panel", {}),
                        "show_pressure", False)
    hud = object.__new__(AdvancedSimHUD)
    hud.ir = _FakeIR()
    hud.tire_panel_widget = type(
        "W", (), {"set_data": lambda s, d: setattr(s, "d", d)})()
    hud.edit_mode_enabled = lambda: False
    hud._update_tire_panel()
    corners = hud.tire_panel_widget.d["corners"]
    assert corners["lf"]["wear"] == 0.85
    assert corners["rr"]["temp"] == 84.0
