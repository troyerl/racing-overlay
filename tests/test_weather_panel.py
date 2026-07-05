"""Weather panel trend buffer tests."""

import time

from overlay.app import AdvancedSimHUD


def test_weather_track_trend_delta():
    hud = object.__new__(AdvancedSimHUD)
    hud._weather_track_hist = __import__("collections").deque(maxlen=600)
    hud._weather_sample_counter = 59
    hud._weather_track_hist.append((time.time() - 200, 25.0))
    hud._weather_sample_counter = 59
    trend = hud._weather_track_trend(27.0, 300.0)
    assert trend is not None
    assert trend > 0
