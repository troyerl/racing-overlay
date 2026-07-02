"""Session clock helpers (grid / pre-green)."""

from overlay.app import AdvancedSimHUD


def test_session_time_remain_from_total_minus_elapsed_on_grid():
    class FakeIR:
        def __getitem__(self, key):
            if key == "SessionTimeRemain":
                return -1.0
            if key == "SessionTime":
                return 45.0
            if key == "SessionTimeTotal":
                return 3600.0
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.ir = FakeIR()
    assert hud._session_time_remain() == 3555.0


def test_session_time_remain_at_grid_start():
    class FakeIR:
        def __getitem__(self, key):
            if key == "SessionTimeRemain":
                return -1.0
            if key == "SessionTime":
                return 0.0
            if key == "SessionTimeTotal":
                return 1800.0
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.ir = FakeIR()
    assert hud._session_time_remain() == 1800.0


def test_session_time_remain_rejects_unlimited_placeholder_total():
    class FakeIR:
        def __getitem__(self, key):
            if key == "SessionTimeRemain":
                return -1.0
            if key == "SessionTime":
                return 120.0
            if key == "SessionTimeTotal":
                return 604800.0
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.ir = FakeIR()
    assert hud._session_time_remain() is None


def test_session_time_remain_uses_live_remain_when_available():
    class FakeIR:
        def __getitem__(self, key):
            if key == "SessionTimeRemain":
                return 900.0
            raise KeyError(key)

    hud = object.__new__(AdvancedSimHUD)
    hud.ir = FakeIR()
    assert hud._session_time_remain() == 900.0
