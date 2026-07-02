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

# Default demo map: Chicagoland Speedway (iRacing TrackID 123).
DEMO_TRACK_ID = 123

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

# Lap-% extent (entry -> exit, wrapping start/finish) of the demo pit route.
# overlay/app.py:_demo_pit_geometry builds the matching geometry from these, and
# the three "always pit" demo cars ride this span every lap so the pit route
# (entry / exit blends + lane) is visible without a live scan. Keep in sync.
DEMO_PIT_IN_PCT = 0.90
DEMO_PIT_OUT_PCT = 0.12
# Pit-lane-only sub-span inside the full route (entry/exit blends sit outside).
DEMO_PIT_LANE_LO = 0.95
DEMO_PIT_LANE_HI = 0.06
# Number of cars (never the player) that occasionally visit the pit route.
DEMO_PIT_CARS = 1
# Lap-% rate on the pit route vs racing line (pit speed limit ~40% of race pace).
_DEMO_PIT_PACE = 0.38
def configure_pit_extents(
    in_pct: float | None,
    out_pct: float | None,
    lane_lo: float | None,
    lane_hi: float | None,
) -> None:
    """Sync demo pit-car simulation with loaded track pit lap-% extents."""
    global DEMO_PIT_IN_PCT, DEMO_PIT_OUT_PCT, DEMO_PIT_LANE_LO, DEMO_PIT_LANE_HI
    if in_pct is not None:
        DEMO_PIT_IN_PCT = float(in_pct)
    if out_pct is not None:
        DEMO_PIT_OUT_PCT = float(out_pct)
    if lane_lo is not None:
        DEMO_PIT_LANE_LO = float(lane_lo)
    if lane_hi is not None:
        DEMO_PIT_LANE_HI = float(lane_hi)


def _in_demo_pit(frac: float) -> bool:
    """Whether a lap fraction lies on the demo pit route (circular interval)."""
    span = (DEMO_PIT_OUT_PCT - DEMO_PIT_IN_PCT) % 1.0
    return ((frac - DEMO_PIT_IN_PCT) % 1.0) <= span


def _on_demo_pit_lane(frac: float) -> bool:
    """True on the pit lane itself (not entry/exit blends) — matches iRacing OnPitRoad."""
    span = (DEMO_PIT_LANE_HI - DEMO_PIT_LANE_LO) % 1.0
    return ((frac - DEMO_PIT_LANE_LO) % 1.0) <= span


class FakeIRSDK:
    def __init__(self, num_cars: int = len(_DEMO_DRIVERS), player_idx: int = 4):
        self.num_cars = min(num_cars, len(_DEMO_DRIVERS))
        self.player_idx = min(player_idx, self.num_cars - 1)
        self.lap_time = 32.0  # Chicagoland oval-ish; also DriverCarEstLapTime
        self._rate = 1.0 / self.lap_time  # laps per second (baseline)

        rng = random.Random(7)
        # Per-car pace: small steady offset + gentle oscillation so order shuffles
        # without teleporting dots. Cars spread evenly around the lap at t=0.
        self._speed = [0.0] * self.num_cars
        self._amp = [0.0] * self.num_cars
        self._w = [0.0] * self.num_cars
        self._phase = [rng.uniform(0, 6.2832) for _ in range(self.num_cars)]
        self._offset = [0.0] * self.num_cars
        # Cars with a lower index than the player sit to the left, higher to the
        # right -- a demo heuristic for which side they appear on when alongside.
        self._lane = ["left" if i < self.player_idx else "right"
                      for i in range(self.num_cars)]
        # One car (never the player) visits the pit route on staggered laps so
        # the pit geometry is visible without cluttering the map every lap.
        others = [i for i in range(self.num_cars) if i != self.player_idx]
        self._pit_cars = frozenset(others[-DEMO_PIT_CARS:])

        for i in range(self.num_cars):
            self._offset[i] = i / self.num_cars + rng.uniform(-0.008, 0.008)
            if abs(i - self.player_idx) <= 2:
                self._speed[i] = 1.0 + rng.uniform(-0.003, 0.003)
                self._amp[i] = rng.uniform(0.004, 0.012)
                self._w[i] = rng.uniform(0.05, 0.11)
            else:
                self._speed[i] = 1.0 + rng.uniform(-0.015, 0.015)
                self._amp[i] = rng.uniform(0.008, 0.018)
                self._w[i] = rng.uniform(0.03, 0.07)

        self._start = time.time()

    def _fuel_burn_per_sec(self) -> float:
        """Litres/sec consumed on track, matched to FuelUsePerHour and lap_time."""
        return 176.0 * (self.lap_time / 3600.0) / self.lap_time

    @property
    def pace_idx(self) -> int:
        return self.num_cars

    def _pace_lap_pct(self) -> float:
        return ((time.time() - self._start) * 0.012) % 1.0

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

    def _pit_visit_lap(self, i: int) -> bool:
        """Whether car i is on a staggered pit-stop lap (not every lap)."""
        return (int(self._total_laps()[i]) + i) % 4 == 0

    def _pit_adjusted_tot(self, i: int, tot: float) -> float:
        """Stretch time on the demo pit route so lap-% matches pit speed."""
        if i not in self._pit_cars or not self._pit_visit_lap(i):
            return tot
        in_pt = DEMO_PIT_IN_PCT
        out_pt = DEMO_PIT_OUT_PCT
        span = (out_pt - in_pt) % 1.0
        if span <= 1e-6:
            return tot
        pace = _DEMO_PIT_PACE
        track_frac = 1.0 - span
        lap = int(tot)
        frac = tot % 1.0
        whole = lap * (span * pace + track_frac)
        pit_pos = (frac - in_pt) % 1.0
        if pit_pos <= span:
            whole += pit_pos * pace
        else:
            whole += span * pace + (frac - out_pt) % 1.0
        return whole

    def _lap_frac(self, i: int) -> float:
        return self._pit_adjusted_tot(i, self._total_laps()[i]) % 1.0

    def _lap_pct(self) -> list[float]:
        return [self._lap_frac(i) for i in range(self.num_cars)]

    def _lap_counts(self) -> list[int]:
        laps = [int(d) + 1 for d in self._total_laps()]
        if self.player_idx < len(laps) and laps[self.player_idx] < 2:
            laps = [lap + 1 for lap in laps]
        return laps

    def _on_pit_list(self) -> list[bool]:
        """Which cars are on pit road this tick."""
        out = []
        for i in range(self.num_cars):
            frac = self._lap_frac(i)
            if i in self._pit_cars and self._pit_visit_lap(i):
                out.append(_on_demo_pit_lane(frac))
            else:
                lap = int(self._total_laps()[i]) + 1
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
                ] + [{
                    "CarIdx": self.pace_idx,
                    "CarNumber": "00",
                    "UserName": "Pace Car",
                    "IRating": 0,
                    "LicString": "P",
                    "CarClassColor": "#111111",
                    "CarClassID": 0,
                    "CarIsPaceCar": True,
                }],
            }

        if key == "PlayerCarIdx":
            return self.player_idx

        if key == "CarIdxLapDistPct":
            pcts = self._lap_pct()
            return pcts + [self._pace_lap_pct()]

        if key == "CarIdxEstTime":
            return [p * self.lap_time for p in self._lap_pct()] + [0.0]

        if key == "CarIdxOnPitRoad":
            return self._on_pit_list() + [False]

        if key == "OnPitRoad":
            return self._on_pit_list()[self.player_idx]

        if key == "EngineWarnings":
            # Pit speed limiter bit (0x10) on while the player is on pit road.
            return 0x10 if self._on_pit_list()[self.player_idx] else 0

        if key == "CarIdxTrackSurface":
            base = [oc.TRK_IN_PIT_STALL if on else oc.TRK_ON_TRACK
                    for on in self._on_pit_list()]
            return base + [oc.TRK_ON_TRACK]

        if key == "CarIdxPosition":
            order = sorted(
                range(self.num_cars), key=lambda i: self._total_laps()[i], reverse=True
            )
            positions = [0] * self.num_cars
            for rank, idx in enumerate(order, start=1):
                positions[idx] = rank
            return positions + [0]

        if key == "CarIdxLap":
            return self._lap_counts()

        if key == "CarIdxF2Time":
            totals = self._total_laps()
            leader = max(totals)
            return [(leader - t) * self.lap_time for t in totals]

        if key == "CarIdxClassPosition":
            return self["CarIdxPosition"]  # single-class demo field

        if key == "SessionState":
            return 4  # racing — enables iRating projection in demo

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

        if key == "RadioTransmitCarIdx":
            # Cycle through a few cars so the speaking badge is visible in demo.
            return int((time.time() - self._start) // 3.0) % self.num_cars

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
            return 176.0 * (self.lap_time / 32.0)

        if key == "SessionTime":
            return time.time() - self._start

        if key == "SessionTimeTotal":
            return 45 * 60.0

        if key == "SessionLapsRemainEx":
            return max(0, 50 - int(self._total_laps()[self.player_idx]))

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
            if cyc < 2.0:
                return 0x00004000 | 0x00008000   # caution waving
            if cyc < 4.0:
                return 0x00004000 | 0x00000200   # caution + 1 lap to green
            if cyc < 6.0:
                return 0x00004000 | 0x00001000   # caution + 5 to go
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
            return {"TrackID": DEMO_TRACK_ID,
                    "TrackDisplayName": "Chicagoland Speedway",
                    "TrackConfigName": "Oval", "TrackLength": "2.25 km",
                    "TrackType": "oval course", "Category": "Oval",
                    "TrackNumTurns": 4,
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
            return max(2.0, 60.0 - t * self._fuel_burn_per_sec())

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
            jit = self._lap_jitter() * 0.12
            return self.lap_time + jit

        if key == "LapBestLapTime":
            return self.lap_time - 0.35

        return None


def make_demo_irsdk() -> FakeIRSDK:
    return FakeIRSDK()
