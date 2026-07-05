"""Shared telemetry read helpers."""

from overlay import telemetry as tele


class FakeIR:
    def __init__(self, values: dict):
        self._values = values

    def __getitem__(self, key):
        if key not in self._values:
            raise KeyError(key)
        return self._values[key]


def test_tire_corners_constant_has_four_corners():
    assert len(tele.TIRE_CORNERS) == 4
    keys = {c[0] for c in tele.TIRE_CORNERS}
    assert keys == {"lf", "rf", "lr", "rr"}


def test_read_tire_corners_wear_and_temp():
    ir = FakeIR({
        "LFwearM": 0.91, "LFtempCL": 85.0,
        "RFwearM": 0.88, "RFtempCL": 86.0,
        "LRwearM": 0.90, "LRtempCL": 84.0,
        "RRwearM": 0.87, "RRtempCL": 83.0,
    })
    corners = tele.read_tire_corners(ir, wear=True, temp=True, pressure=False)
    assert corners["lf"]["wear"] == 0.91
    assert corners["rf"]["temp"] == 86.0
    assert "pressure" not in corners["lf"]


def test_read_track_temp_falls_back_to_crew():
    ir = FakeIR({"TrackTemp": None, "TrackTempCrew": 42.5})
    assert tele.read_track_temp(ir) == 42.5


def test_weather_snapshot_respects_config_toggles():
    ir = FakeIR({
        "TrackWetness": 3, "RainIntensity": 1,
        "TrackTemp": 30.0, "AirTemp": 22.0,
        "WindDir": 180.0, "WindVel": 5.0,
    })
    wk = {"Skies": "Partly Cloudy", "RelativeHumidity": 55, "FogLevel": 0.1}
    snap = tele.weather_snapshot(
        ir, wk, cfg={"show_skies": True, "show_rain": False,
                     "show_temps": True, "show_wind": False})
    assert snap["skies"] == "Partly Cloudy"
    assert "track_wetness" not in snap
    assert snap["track_temp"] == 30.0
    assert "wind_dir" not in snap
