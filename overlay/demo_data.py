"""
Demo telemetry source.

FakeIRSDK mimics the small slice of the pyirsdk interface the overlays use
(is_connected, startup(), and dict-style variable access) so you can see the
HUDs render with simulated traffic without iRacing running.

Cars are advanced using wall-clock time, so motion is smooth regardless of how
often the overlay polls.
"""

from __future__ import annotations

import math
import random
import time

from . import common as oc

# (car number, name, iRating, license string, class color)
_DEMO_DRIVERS = [
    ("11", "Denny Hamlin", 4600, "B 2.34", "#ff5bac"),
    ("19", "Martin Truex", 1800, "A 3.71", "#d6c44a"),
    ("5", "Kyle Larson", 1900, "B 2.22", "#ff5bac"),
    ("24", "William Byron", 2200, "B 2.31", "#b06bff"),
    ("48", "L Troyer", 2500, "A 3.32", "#3aa0ff"),
    ("8", "Kyle Busch", 8000, "A 3.34", "#3aa0ff"),
    ("9", "Chase Elliott", 1700, "B 2.61", "#ff5bac"),
    ("22", "Joey Logano", 1950, "A 3.50", "#46d27a"),
    ("12", "Ryan Blaney", 1900, "A 3.55", "#3aa0ff"),
    ("20", "Christopher Bell", 2100, "C 1.99", "#ffa23a"),
    ("45", "Tyler Reddick", 1750, "A 3.10", "#46d27a"),
    ("23", "Bubba Wallace", 1900, "A 3.50", "#d6c44a"),
]


class FakeIRSDK:
    def __init__(self, num_cars: int = len(_DEMO_DRIVERS), player_idx: int = 4):
        self.num_cars = min(num_cars, len(_DEMO_DRIVERS))
        self.player_idx = min(player_idx, self.num_cars - 1)
        self.lap_time = 92.0  # seconds, also used as DriverCarEstLapTime
        self._rate = 1.0 / self.lap_time  # laps per second (baseline)

        rng = random.Random(7)
        # Per-car pace: a steady speed offset plus an oscillating surge so the
        # running order keeps shuffling (overtakes happen as the sines cross).
        # amp * w stays below the baseline rate so cars never travel backwards.
        #
        # A tight battle pack around the player keeps cars beside you so the
        # radar regularly shows left / right / both; the rest of the field is
        # spread around the lap so the track map stays populated. Pack cars run
        # near the player's pace with small surges so they weave fore/aft and
        # cross sides instead of streaming away.
        self._speed = [0.0] * self.num_cars
        self._amp = [0.0] * self.num_cars
        self._w = [0.0] * self.num_cars
        self._phase = [rng.uniform(0, 6.2832) for _ in range(self.num_cars)]
        self._offset = [0.0] * self.num_cars
        # Cars with a lower index than the player sit to the left, higher to the
        # right -- a demo heuristic for which side they appear on when alongside.
        self._lane = ["left" if i < self.player_idx else "right"
                      for i in range(self.num_cars)]

        for i in range(self.num_cars):
            if abs(i - self.player_idx) <= 2:
                self._speed[i] = 1.0 + rng.uniform(-0.004, 0.004)
                self._amp[i] = rng.uniform(0.010, 0.022)
                self._w[i] = rng.uniform(0.06, 0.13)
                self._offset[i] = 0.5 + (i - self.player_idx) * 0.006
            else:
                self._speed[i] = 1.0 + rng.uniform(-0.03, 0.03)
                self._amp[i] = rng.uniform(0.05, 0.09)
                self._w[i] = rng.uniform(0.04, 0.09)
                self._offset[i] = i / self.num_cars + rng.uniform(-0.01, 0.01)

        self._start = time.time()

    # --- pyirsdk-compatible surface ----------------------------------------

    @property
    def is_connected(self) -> bool:
        return True

    def startup(self, *args, **kwargs) -> bool:
        return True

    def shutdown(self) -> None:  # pragma: no cover - parity with real SDK
        pass

    # --- simulation ---------------------------------------------------------

    def _total_laps(self) -> list[float]:
        t = time.time() - self._start
        return [
            self._rate * self._speed[i] * t
            + self._amp[i] * math.sin(self._w[i] * t + self._phase[i])
            + self._offset[i]
            for i in range(self.num_cars)
        ]

    def _lap_pct(self) -> list[float]:
        return [d % 1.0 for d in self._total_laps()]

    def _on_pit_list(self) -> list[bool]:
        """Cars periodically duck onto pit road (staggered, ~6% of a lap)."""
        totals = self._total_laps()
        out = []
        for i, tot in enumerate(totals):
            lap = int(tot) + 1
            frac = tot % 1.0
            out.append(((lap + i * 3) % 8 == 0) and frac < 0.06)
        return out

    def _lap_jitter(self) -> float:
        """A small deterministic per-lap wobble (-1..1) so consecutive laps differ
        slightly -- lets the lap-compare demo show believable corner deltas."""
        lap = int(self._total_laps()[self.player_idx])
        return (((lap * 37) % 7) - 3) / 3.0

    def _drive_profile(self, frac: float):
        """(throttle, brake, steer_angle, speed_factor) for the player at a given
        lap fraction -- three corners a lap, shifted a touch each lap."""
        jit = self._lap_jitter()
        base = 0.5 + 0.5 * math.sin((frac + 0.012 * jit) * 2 * math.pi * 3 - 1.2)
        corner = max(0.0, 1.0 - base)
        throttle = max(0.0, min(1.0, 0.12 + base - max(0.0, 0.08 * jit)))
        brake = max(0.0, min(1.0, corner * 1.5 - 0.45))
        steer = 3.6 * corner * math.sin(frac * 2 * math.pi * 1.5)
        return throttle, brake, steer, base

    def _engine(self):
        """Plausible (speed_ms, rpm, gear) for the player from lap position."""
        frac = self._lap_pct()[self.player_idx]
        # Three "corners" a lap: speed dips and recovers around the loop, with a
        # small per-lap variation in how deep the corners are.
        base = 0.5 + 0.5 * math.sin(frac * 2 * math.pi * 3 - 1.2)
        spd = 90.0 + 150.0 * base + 6.0 * self._lap_jitter() * (1.0 - base)
        spd = max(65.0, min(258.0, spd))
        bands = [(0, 60), (60, 100), (100, 140), (140, 180), (180, 220), (220, 320)]
        gear, rpm = 1, 6000.0
        for gi, (lo, hi) in enumerate(bands, start=1):
            if lo <= spd < hi:
                gear = gi
                rpm = 5200.0 + (spd - lo) / (hi - lo) * 2700.0
                break
        return spd / 3.6, rpm, gear

    @staticmethod
    def _wrap(delta: float) -> float:
        if delta > 0.5:
            return delta - 1.0
        if delta < -0.5:
            return delta + 1.0
        return delta

    def __getitem__(self, key: str):
        if key == "DriverInfo":
            return {
                "DriverCarEstLapTime": self.lap_time,
                "DriverCarRedLine": 8000.0,
                "DriverCarSLFirstRPM": 6200.0,
                "DriverCarSLShiftRPM": 7600.0,
                "DriverCarSLLastRPM": 7850.0,
                "DriverCarSLBlinkRPM": 7950.0,
                "DriverCarGearNumForward": 6,
                "DriverCarFuelMaxLtr": 60.0,
                "DriverCarMaxFuelPct": 1.0,
                "Drivers": [
                    {
                        "CarIdx": i,
                        "CarNumber": _DEMO_DRIVERS[i][0],
                        "UserName": _DEMO_DRIVERS[i][1],
                        "IRating": _DEMO_DRIVERS[i][2],
                        "LicString": _DEMO_DRIVERS[i][3],
                        "CarClassColor": _DEMO_DRIVERS[i][4],
                        "CarClassID": 0,
                    }
                    for i in range(self.num_cars)
                ],
            }

        if key == "PlayerCarIdx":
            return self.player_idx

        if key == "CarIdxLapDistPct":
            return self._lap_pct()

        if key == "CarIdxEstTime":
            return [p * self.lap_time for p in self._lap_pct()]

        if key == "CarIdxOnPitRoad":
            return self._on_pit_list()

        if key == "OnPitRoad":
            return self._on_pit_list()[self.player_idx]

        if key == "EngineWarnings":
            # Pit speed limiter bit (0x10) on while the player is on pit road.
            return 0x10 if self._on_pit_list()[self.player_idx] else 0

        if key == "CarIdxTrackSurface":
            return [oc.TRK_IN_PIT_STALL if on else oc.TRK_ON_TRACK
                    for on in self._on_pit_list()]

        if key == "CarIdxPosition":
            order = sorted(
                range(self.num_cars), key=lambda i: self._total_laps()[i], reverse=True
            )
            positions = [0] * self.num_cars
            for rank, idx in enumerate(order, start=1):
                positions[idx] = rank
            return positions

        if key == "CarIdxLap":
            return [int(d) + 1 for d in self._total_laps()]

        if key == "CarIdxF2Time":
            totals = self._total_laps()
            leader = max(totals)
            return [(leader - t) * self.lap_time for t in totals]

        if key == "CarIdxClassPosition":
            return self["CarIdxPosition"]  # single-class demo field

        if key == "CarIdxLastLapTime":
            # A plausible last lap per car: the demo pace scaled around lap_time,
            # nudged by a slow wobble so the values tick over time.
            t = time.time() - self._start
            return [self.lap_time / self._speed[i]
                    + 0.6 * math.sin(t * 0.2 + self._phase[i])
                    for i in range(self.num_cars)]

        if key == "CarIdxBestLapTime":
            return [self.lap_time / self._speed[i] - 0.45
                    for i in range(self.num_cars)]

        if key == "Lap":
            return int(self._total_laps()[self.player_idx]) + 1

        if key == "SessionLapsTotal":
            return 50

        if key == "LFwearM":
            t = time.time() - self._start
            return max(0.0, min(1.0, 0.92 - 0.02 * (0.5 + 0.5 * math.sin(t * 0.05))))

        if key == "RFwearM":
            t = time.time() - self._start
            return max(0.0, min(1.0, 0.88 - 0.02 * (0.5 + 0.5 * math.sin(t * 0.05 + 1))))

        if key == "FuelUsePerHour":
            return 176.0  # L/hr -> ~12 laps remaining at the demo fuel level

        if key == "SessionTime":
            return time.time() - self._start

        if key == "SessionTimeTotal":
            return 45 * 60.0

        if key == "SessionTimeRemain":
            return max(0.0, 45 * 60.0 - (time.time() - self._start))

        if key == "SessionTimeOfDay":
            # Start at 14:00 and let the sim clock advance with real time.
            return (14 * 3600.0 + (time.time() - self._start)) % 86400.0

        if key == "SessionFlags":
            # Cycle through the states so the dash flag indicator is visible in
            # demo: caution -> red -> blue -> debris -> white -> black ->
            # meatball -> furled -> DQ -> crossed -> checkered -> green.
            cyc = (time.time() - self._start) % 60.0
            if cyc < 6.0:
                return 0x00004000 | 0x00008000   # caution + caution waving
            if 9.0 <= cyc < 12.0:
                return 0x00000010                # red flag (session stopped)
            if 14.0 <= cyc < 17.0:
                return 0x00000020                # blue flag (let faster car by)
            if 19.0 <= cyc < 22.0:
                return 0x00000040                # debris
            if 24.0 <= cyc < 27.0:
                return 0x00000002                # white flag (final lap)
            if 30.0 <= cyc < 33.0:
                return 0x00010000                # black flag
            if 36.0 <= cyc < 39.0:
                return 0x00100000                # meatball (repair)
            if 42.0 <= cyc < 45.0:
                return 0x00080000                # furled (warning)
            if 48.0 <= cyc < 51.0:
                return 0x00020000                # disqualified
            if 51.0 <= cyc < 54.0:
                return 0x00000080                # crossed (race halfway)
            if 54.0 <= cyc < 57.0:
                return 0x00000001                # checkered (finish)
            return 0x00000004                    # green

        if key == "IsInGarage":
            # Spend the first ~10s of each 60s cycle "in the garage" so the
            # garage-vs-track profile switch is observable in the demo.
            return ((time.time() - self._start) % 60.0) < 10.0

        if key == "PlayerCarMyIncidentCount":
            return 11

        if key == "WeekendInfo":
            return {"TrackID": "_demo", "TrackDisplayName": "Demo Speedpark",
                    "TrackConfigName": "Oval", "TrackLength": "4.00 km",
                    "WeekendOptions": {"IncidentLimit": 17}}

        if key == "SplitTimeInfo":
            return {"Sectors": [{"SectorNum": 0, "SectorStartPct": 0.0},
                                {"SectorNum": 1, "SectorStartPct": 0.34},
                                {"SectorNum": 2, "SectorStartPct": 0.68}]}

        if key == "CarLeftRight":
            pct = self._lap_pct()
            me = pct[self.player_idx]
            left = right = False
            for i, p in enumerate(pct):
                if i == self.player_idx:
                    continue
                if abs(self._wrap(p - me)) < 0.006:
                    if self._lane[i] == "left":
                        left = True
                    else:
                        right = True
            if left and right:
                return oc.LR_CAR_LEFT_RIGHT
            if left:
                return oc.LR_CAR_LEFT
            if right:
                return oc.LR_CAR_RIGHT
            return oc.LR_CLEAR

        if key == "FuelLevel":
            t = time.time() - self._start
            return max(2.0, 60.0 - t * 0.18)  # liters, slowly burning down

        if key == "FuelLevelPct":
            return self["FuelLevel"] / 60.0

        if key == "LapDeltaToSessionBest":
            t = time.time() - self._start
            return math.sin(t * 0.6) * 0.45  # gentle +/- swing

        if key == "Speed":
            # Held near the pit limit (~55 km/h) while on pit road.
            if self._on_pit_list()[self.player_idx]:
                return 15.3
            return self._engine()[0]

        if key == "WindDir":
            # Slowly rotating wind so the compass visibly moves in the demo.
            return (time.time() - self._start) * 0.05 % (2 * math.pi)

        if key == "WindVel":
            return 4.2  # m/s

        if key == "RPM":
            return self._engine()[1]

        if key == "Gear":
            return self._engine()[2]

        if key == "Throttle":
            if self._on_pit_list()[self.player_idx]:
                return 0.2
            return self._drive_profile(self._lap_pct()[self.player_idx])[0]

        if key == "Brake":
            if self._on_pit_list()[self.player_idx]:
                return 0.0
            return self._drive_profile(self._lap_pct()[self.player_idx])[1]

        if key == "Clutch":
            # iRacing reports 1.0 = fully engaged (pedal up); blip toward 0
            # (disengaged) briefly, as if shifting.
            t = time.time() - self._start
            return 0.12 if (t % 7.0) < 0.4 else 1.0

        if key == "BrakeABSactive":
            return self["Brake"] > 0.55  # ABS "kicks in" under hard braking

        if key == "SteeringWheelAngle":
            return self._drive_profile(self._lap_pct()[self.player_idx])[2]
        if key == "SteeringWheelAngleMax":
            return 5.0                        # radians of lock (for normalizing)

        if key == "LapDist":
            return self._lap_pct()[self.player_idx] * 4000.0  # meters along lap

        if key == "PlayerTrackSurface":
            return (oc.TRK_IN_PIT_STALL if self._on_pit_list()[self.player_idx]
                    else oc.TRK_ON_TRACK)

        if key == "LatAccel":
            # Cornering load: peaks through the three corners a lap.
            frac = self._lap_pct()[self.player_idx]
            base = 0.5 + 0.5 * math.sin(frac * 2 * math.pi * 3 - 1.2)
            return max(0.0, 1.0 - base) * 16.0 * math.sin(frac * 2 * math.pi * 1.5)

        if key == "LongAccel":
            prof = self._drive_profile(self._lap_pct()[self.player_idx])
            return (prof[0] - prof[1]) * 9.0  # throttle minus brake, ~m/s^2

        if key == "PlayerCarPosition":
            return self["CarIdxPosition"][self.player_idx]

        if key in ("TrackTemp", "TrackTempCrew"):
            t = time.time() - self._start
            return 27.0 + 2.0 * math.sin(t * 0.03)

        if key == "AirTemp":
            t = time.time() - self._start
            return 22.0 + 1.5 * math.sin(t * 0.025)

        if key == "LapCurrentLapTime":
            return (time.time() - self._start) % self.lap_time

        if key == "LapLastLapTime":
            return 105.035

        if key == "LapBestLapTime":
            return 104.512

        return None


def make_demo_irsdk() -> FakeIRSDK:
    return FakeIRSDK()
