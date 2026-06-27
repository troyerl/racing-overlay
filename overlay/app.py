"""
AdvancedSimHUD - a multi-widget iRacing overlay (PyQt6 + pyirsdk).

Four independent, individually movable overlay windows:
  * Timing Tower (overall standings, from CarIdxPosition + DriverInfo)
  * Relative HUD (nearest cars on track, from CarIdxEstTime)
  * Proximity Radar (left/right blind spot, from CarLeftRight)
  * Track Map (2D shape, from a track file or learned GPS; cars by lap pct)

Each panel is its own frameless top-level window and remembers its position in
overlay_layout.json. Drag them in edit mode (--no-clickthrough) to arrange.

Corrections vs. the common template:
  * The blind-spot variable is CarLeftRight, an ENUM, not "TrackToPlayerCarIdx"
    and not a bitfield (0x02/0x04/0x06). See overlay/common.py for the enum.
  * Standings and Relative are actually computed here (the template left them
    hardcoded). Relative uses CarIdxEstTime, the standard approximation.
  * DriverInfo (session YAML) is cached and refreshed periodically instead of
    being re-parsed every 16 ms tick.
"""

from __future__ import annotations

import os
import sys

from PyQt6.QtCore import QTimer
from PyQt6.QtWidgets import QApplication

from . import common as oc
from . import config
from . import layout_store
from .panel import PanelWindow
from .widgets import track_map
from .widgets.dash import DashWidget
from .widgets.radar import RadarWidget
from .widgets.relative import RelativeWidget
from .widgets.standings import StandingsWidget

# Default window geometry per panel: (x, y, w, h). Overridden by saved layout.
DEFAULT_GEOMS = {
    "standings": (40, 120, 560, 360),
    "relative": (620, 120, 600, 360),
    "radar": (40, 500, 200, 260),
    "map": (620, 500, 480, 320),
    "dash": (260, 800, 660, 190),
}


class AdvancedSimHUD:
    """Owns the telemetry connection and drives the independent panel windows."""

    def __init__(self, click_through: bool = True, demo: bool = False):
        self.click_through = click_through
        self.demo = demo
        self.ir = oc.make_irsdk(demo=demo)
        self._settings_window = None

        # Repaint every panel immediately when the config changes (editor UI).
        config.on_change(lambda _cfg: self._repaint_all())

        self._driver_cache: dict[int, dict] = {}
        # Engine/shift-light params from the session YAML (cached with drivers).
        self._car_info: dict = {}
        self._driver_refresh_counter = 0
        # Per-car pit history: idx -> {"on": bool, "lap": int, "time": float}.
        # We track this ourselves because iRacing exposes no per-car "last pit".
        self._pit: dict[int, dict] = {}
        # Per-car lap-time arrays, refreshed per tick only if a column needs them.
        self._car_last = None
        self._car_best = None
        self._path_builder = track_map.TrackPathBuilder()
        self._track_loaded = False
        self.tracks_dir = os.path.join(
            os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "tracks"
        )

        self._layout_state = layout_store.load_layout()
        self.panels: list[PanelWindow] = []
        self._build_panels()

        # 16 ms ~= iRacing's 60 Hz telemetry tick (a software timer, not vsync).
        self.timer = QTimer()
        self.timer.timeout.connect(self.process_telemetry_tick)
        self.timer.start(16)

    # --- UI -----------------------------------------------------------------

    def _wrap(self, key: str, content) -> PanelWindow:
        win = PanelWindow(
            key,
            content,
            DEFAULT_GEOMS[key],
            self._layout_state,
            click_through=self.click_through,
        )
        self.panels.append(win)
        return win

    def _build_panels(self) -> None:
        self.standings_widget = StandingsWidget()
        self.relative_widget = RelativeWidget()
        self.radar_widget = RadarWidget()
        self.map_widget = track_map.TrackMapWidget()
        self.dash_widget = DashWidget()
        if self.demo:
            self._load_demo_track()

        self._wrap("standings", self.standings_widget)
        self._wrap("relative", self.relative_widget)
        self._wrap("radar", self.radar_widget)
        self._wrap("map", self.map_widget)
        self._wrap("dash", self.dash_widget)

    def show(self) -> None:
        for win in self.panels:
            win.show()

    def _repaint_all(self) -> None:
        for w in (self.standings_widget, self.relative_widget,
                  self.radar_widget, self.map_widget, self.dash_widget):
            w.update()

    def open_settings(self) -> None:
        from .config_editor import ConfigEditor

        if self._settings_window is None:
            self._settings_window = ConfigEditor()
        self._settings_window.show()
        self._settings_window.raise_()
        self._settings_window.activateWindow()

    # --- Telemetry helpers --------------------------------------------------

    def _drivers(self) -> dict[int, dict]:
        """CarIdx -> driver dict, cached and refreshed about twice a second."""
        self._driver_refresh_counter += 1
        if self._driver_cache and self._driver_refresh_counter < 30:
            return self._driver_cache
        self._driver_refresh_counter = 0

        info = self.ir["DriverInfo"] if self.ir else None
        if not info:
            return self._driver_cache
        self._driver_cache = {
            d["CarIdx"]: d for d in info.get("Drivers", [])
        }
        self._car_info = {
            "redline": info.get("DriverCarRedLine"),
            "sl_first": info.get("DriverCarSLFirstRPM"),
            "sl_last": info.get("DriverCarSLLastRPM"),
            "sl_shift": info.get("DriverCarSLShiftRPM"),
            "est_lap": float(info.get("DriverCarEstLapTime", 0.0) or 0.0),
        }
        return self._driver_cache

    # --- Per-tick update ----------------------------------------------------

    def process_telemetry_tick(self) -> None:
        if self.ir is None or (not self.ir.is_connected and not self.ir.startup()):
            return

        player = self.ir["PlayerCarIdx"]
        positions = self.ir["CarIdxPosition"]
        lap_pct = self.ir["CarIdxLapDistPct"]
        surface = self.ir["CarIdxTrackSurface"]
        est_time = self.ir["CarIdxEstTime"]
        car_left_right = self.ir["CarLeftRight"]
        car_lap = self.ir["CarIdxLap"]
        car_f2 = self.ir["CarIdxF2Time"]
        drivers = self._drivers()
        lap_est = self._lap_est(est_time)

        # Track pit stops only if a table actually shows the pit column.
        sess_time = None
        if (config.has_column("relative", "pit")
                or config.has_column("standings", "pit")):
            sess_time = self.ir["SessionTime"]
            self._update_pit_tracking(surface, car_lap, sess_time)

        # Per-car lap times only if a table shows that column (else skip the read).
        self._car_last = self._car_best = None
        if config.has_column("relative", "last_lap") or \
                config.has_column("standings", "last_lap"):
            self._car_last = self.ir["CarIdxLastLapTime"]
        if config.has_column("relative", "best_lap") or \
                config.has_column("standings", "best_lap"):
            self._car_best = self.ir["CarIdxBestLapTime"]

        self._update_radar(player, lap_pct, surface, car_left_right)
        self._update_standings(positions, drivers, surface, car_f2, player, lap_est,
                               car_lap, sess_time)
        self._update_relative(player, est_time, surface, drivers, positions,
                              car_lap, lap_est, sess_time)
        self._update_map(player, lap_pct, surface, drivers)
        self._update_dash(player, positions, car_lap)

    @staticmethod
    def _empty_row(tag: str) -> dict:
        """A blank placeholder row (keeps the player centered when padding)."""
        return {"key": f"_empty_{tag}", "empty": True}

    @staticmethod
    def _visible_cols(section: str) -> dict:
        """Per-column visibility for a table, driven by its column_order list.

        Returned as a dict so the row builders can keep using cols.get(key) to
        skip computing fields whose column isn't shown. "stripe" is a separate
        sub-toggle of the position cell.
        """
        order = set(config.table_column_order(section))
        cols = {k: (k in order) for k in config.TABLE_COLUMNS}
        cols["stripe"] = config.CFG[section]["columns"].get("stripe", True)
        return cols

    def _update_pit_tracking(self, surface, car_lap, sess_time) -> None:
        """Record each car's last pit (lap + race time) on pit-road entry."""
        on_pit = self.ir["CarIdxOnPitRoad"]
        n = len(surface) if surface else (len(on_pit) if on_pit else 0)
        for idx in range(n):
            if on_pit is not None:
                now_on = bool(on_pit[idx])
            else:  # fall back to the track-surface enum
                now_on = surface[idx] in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
            st = self._pit.setdefault(idx, {"on": False, "lap": None, "time": None})
            if now_on and not st["on"]:  # rising edge = just entered the pits
                st["lap"] = car_lap[idx] if car_lap else None
                st["time"] = sess_time
            st["on"] = now_on

    def _pit_text(self, idx, car_lap, sess_time, mode) -> str:
        st = self._pit.get(idx)
        if not st or (st["lap"] is None and st["time"] is None):
            return "\u2014"  # no stop observed yet
        if mode == "laps_since":
            if car_lap and st["lap"] is not None and car_lap[idx] is not None:
                return f"{max(0, car_lap[idx] - st['lap'])}L"
            return "\u2014"
        if mode == "time_since":
            if st["time"] is not None and sess_time is not None:
                return self._fmt_clock(max(0.0, sess_time - st["time"]))
            return "\u2014"
        if mode == "at_lap":
            return f"L{st['lap']}" if st["lap"] is not None else "\u2014"
        if mode == "at_time":
            return self._fmt_clock(st["time"]) if st["time"] is not None else "\u2014"
        return "\u2014"

    def _lap_est(self, est_time) -> float:
        # Use the cached est-lap-time (refreshed with drivers) instead of
        # re-parsing the DriverInfo session YAML every single tick.
        lap_est = float(self._car_info.get("est_lap", 0.0) or 0.0)
        if lap_est <= 0:
            lap_est = max((t for t in est_time if t and t > 0), default=0.0)
        return lap_est

    def _update_radar(self, player, lap_pct, surface, car_left_right) -> None:
        # Nearest car ahead / behind within range (excluding the alongside zone,
        # which is shown by the side bars instead).
        nearest_ahead = nearest_behind = None
        alongside_zone = 0.004
        radar_range = config.CFG["radar"]["range_pct"]
        if player is not None and lap_pct:
            me = lap_pct[player]
            for idx, pct in enumerate(lap_pct):
                if idx == player or pct is None or pct < 0:
                    continue
                if surface and surface[idx] != oc.TRK_ON_TRACK:
                    continue
                delta = pct - me
                if delta > 0.5:
                    delta -= 1.0
                elif delta < -0.5:
                    delta += 1.0
                if alongside_zone < delta <= radar_range:
                    nearest_ahead = delta if nearest_ahead is None else min(nearest_ahead, delta)
                elif -radar_range <= delta < -alongside_zone:
                    nearest_behind = delta if nearest_behind is None else max(nearest_behind, delta)

        def closeness(delta):
            if delta is None:
                return None
            return max(0.0, min(1.0, 1.0 - abs(delta) / radar_range))

        self.radar_widget.set_data({
            "left": car_left_right in oc.CAR_ON_LEFT,
            "right": car_left_right in oc.CAR_ON_RIGHT,
            "left2": car_left_right == oc.LR_2_CARS_LEFT,
            "right2": car_left_right == oc.LR_2_CARS_RIGHT,
            "ahead": closeness(nearest_ahead),
            "behind": closeness(nearest_behind),
        })

    def _build_standings_row(self, idx, drivers, positions, surface, car_f2,
                             player, lap_est, cols, car_lap, sess_time,
                             pit_mode) -> dict:
        # Only compute fields whose column is actually shown.
        d = drivers.get(idx, {})
        cls = sr = ""
        if cols.get("license"):
            cls, sr = self._parse_license(d.get("LicString"))

        pit = ""
        if cols.get("pit"):
            pit = self._pit_text(idx, car_lap, sess_time, pit_mode)

        gap_text = ""
        if cols.get("gap"):
            f2 = car_f2[idx] if car_f2 else None
            pos = positions[idx] if positions else None
            if pos == 1 or not f2:
                gap_text = "\u2014"
            elif lap_est and f2 >= lap_est:
                gap_text = f"-{int(f2 // lap_est)}L"
            else:
                gap_text = f"+{f2:.1f}"

        return {
            "key": idx,
            "position": (positions[idx] if positions else "") if cols.get("position") else "",
            "car_number": str(d.get("CarNumber", "")) if cols.get("car_number") else "",
            "name": d.get("UserName", f"Car {idx}") if cols.get("name") else "",
            "class_color": self._class_color(d, idx) if cols.get("stripe") else "#888888",
            "sr": sr,
            "lic_class": cls,
            "irating": self._fmt_irating(d.get("IRating")) if cols.get("irating") else "",
            "pit": pit,
            "gap_text": gap_text,
            "last_lap": self._lap_for(idx, self._car_last) if cols.get("last_lap") else "",
            "best_lap": self._lap_for(idx, self._car_best) if cols.get("best_lap") else "",
            "is_player": idx == player,
            "in_pit": surface[idx] in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS),
            "lapping": False,
        }

    def _update_standings(self, positions, drivers, surface, car_f2,
                          player, lap_est, car_lap, sess_time) -> None:
        if not positions:
            return
        scfg = config.CFG["standings"]
        cols = self._visible_cols("standings")
        pit_mode = scfg.get("pit_mode", "laps_since")
        n = scfg["rows"]
        ranked = sorted(
            (idx for idx, pos in enumerate(positions) if pos and pos > 0),
            key=lambda idx: positions[idx],
        )
        total = len(ranked)

        def build(idx):
            return self._build_standings_row(idx, drivers, positions, surface,
                                             car_f2, player, lap_est, cols,
                                             car_lap, sess_time, pit_mode)

        center = scfg.get("center_on_player", True) and player in ranked
        if center:
            # A window of rows_ahead + player + rows_behind, padded with blank
            # rows near the ends so the player stays in the middle slot.
            above = scfg.get("rows_ahead", 4)
            below = scfg.get("rows_behind", 5)
            window = above + 1 + below
            pidx = ranked.index(player)
            start = pidx - above
            rows = []
            for slot in range(start, start + window):
                if 0 <= slot < total:
                    rows.append(build(ranked[slot]))
                else:
                    rows.append(self._empty_row(f"std{slot}"))
        else:
            rows = [build(idx) for idx in ranked[:n]]

        shown = sum(1 for r in rows if not r.get("empty"))
        show_count = "count" in set(scfg.get("header", {}).values())
        self.standings_widget.set_data({
            "title": scfg["title"],
            "header_right": f"{shown}/{total}" if show_count else "",
            "rows": rows,
        })

    @staticmethod
    def _fmt_irating(ir) -> str:
        if not ir:
            return "--"
        return f"{ir / 1000:.1f}k" if ir >= 1000 else str(int(ir))

    @staticmethod
    def _parse_license(lic_str):
        # LicString looks like "A 3.71"; return ("A", "3.7").
        if not lic_str:
            return "", ""
        parts = str(lic_str).split()
        cls = parts[0][:1] if parts else ""
        try:
            sr = f"{float(parts[1]):.1f}"
        except (IndexError, ValueError):
            sr = ""
        return cls, sr

    @staticmethod
    def _class_color(driver, idx) -> str:
        c = driver.get("CarClassColor") if driver else None
        if isinstance(c, int):
            return f"#{c & 0xFFFFFF:06x}"
        if isinstance(c, str) and c:
            return c
        palette = track_map.car_palette()
        return palette[idx % len(palette)]

    @staticmethod
    def _fmt_clock(secs) -> str:
        if secs is None or secs < 0:
            return "--:--"
        secs = int(secs)
        h, rem = divmod(secs, 3600)
        m, s = divmod(rem, 60)
        return f"{h:02d}:{m:02d}:{s:02d}" if h else f"{m:02d}:{s:02d}"

    @staticmethod
    def _fmt_laptime(secs) -> str:
        """A lap time as M:SS.mmm (iRacing uses <=0 for 'no time set')."""
        if not isinstance(secs, (int, float)) or secs <= 0:
            return "\u2014"
        m = int(secs // 60)
        s = secs - m * 60
        return f"{m}:{s:06.3f}"

    def _lap_for(self, idx, arr) -> str:
        """Formatted lap time for a car from a CarIdx lap-time array."""
        if not arr or idx >= len(arr):
            return "\u2014"
        return self._fmt_laptime(arr[idx])

    def _build_relative_row(self, idx, delta, drivers, positions, surface, car_lap,
                            player, is_player, cols, sess_time, pit_mode) -> dict:
        # Only compute fields whose column is actually shown.
        d = drivers.get(idx, {})
        cls = sr = ""
        if cols.get("license"):
            cls, sr = self._parse_license(d.get("LicString"))
        pit = ""
        if cols.get("pit"):
            pit = self._pit_text(idx, car_lap, sess_time, pit_mode)
        in_pit = surface[idx] in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
        lapping = bool(
            car_lap and car_lap[idx] > 0 and car_lap[player] > 0
            and car_lap[idx] != car_lap[player] and not is_player
        )
        return {
            "key": idx,
            "position": (positions[idx] if positions else "") if cols.get("position") else "",
            "car_number": str(d.get("CarNumber", "")) if cols.get("car_number") else "",
            "name": d.get("UserName", f"Car {idx}") if cols.get("name") else "",
            "class_color": self._class_color(d, idx) if cols.get("stripe") else "#888888",
            "sr": sr,
            "lic_class": cls,
            "irating": self._fmt_irating(d.get("IRating")) if cols.get("irating") else "",
            "pit": pit,
            "gap": abs(delta) if cols.get("gap") else None,
            "last_lap": self._lap_for(idx, self._car_last) if cols.get("last_lap") else "",
            "best_lap": self._lap_for(idx, self._car_best) if cols.get("best_lap") else "",
            "is_player": is_player,
            "in_pit": in_pit,
            "lapping": lapping,
        }

    def _update_relative(self, player, est_time, surface, drivers,
                         positions, car_lap, lap_est, sess_time) -> None:
        if player is None or not est_time or not surface or lap_est <= 0:
            return

        rcfg = config.CFG["relative"]
        cols = self._visible_cols("relative")
        pit_mode = rcfg.get("pit_mode", "laps_since")
        me = est_time[player]
        rels = []
        for idx, t in enumerate(est_time):
            if idx == player or t is None:
                continue
            if surface[idx] not in (oc.TRK_ON_TRACK, oc.TRK_APPROACHING_PITS,
                                     oc.TRK_IN_PIT_STALL):
                continue
            delta = t - me
            half = lap_est / 2.0
            if delta > half:
                delta -= lap_est
            elif delta < -half:
                delta += lap_est
            rels.append((delta, idx))

        n_ahead = rcfg.get("rows_ahead", 3)
        n_behind = rcfg.get("rows_behind", 3)
        ahead = sorted((r for r in rels if r[0] > 0), key=lambda r: r[0])[:n_ahead]
        behind = sorted((r for r in rels if r[0] <= 0), key=lambda r: -r[0])[:n_behind]

        rows = []
        # Pad the top so the player stays in the center slot even when there
        # aren't n_ahead cars in front (e.g. when leading).
        if rcfg.get("center_on_player", True):
            for k in range(n_ahead - len(ahead)):
                rows.append(self._empty_row(f"rel_top{k}"))
        for delta, idx in reversed(ahead):
            rows.append(self._build_relative_row(
                idx, delta, drivers, positions, surface, car_lap, player, False,
                cols, sess_time, pit_mode))
        rows.append(self._build_relative_row(
            player, 0.0, drivers, positions, surface, car_lap, player, True,
            cols, sess_time, pit_mode))
        for delta, idx in behind:
            rows.append(self._build_relative_row(
                idx, delta, drivers, positions, surface, car_lap, player, False,
                cols, sess_time, pit_mode))
        if rcfg.get("center_on_player", True):
            for k in range(n_behind - len(behind)):
                rows.append(self._empty_row(f"rel_bot{k}"))

        hdr_items = set(rcfg.get("header", {}).values())
        show_sof = "sof" in hdr_items
        show_pos = "position" in hdr_items
        self.relative_widget.set_data({
            "sof": self._sof(drivers) if show_sof else "--",
            "pos": (positions[player] if positions else None) if show_pos else None,
            "total": (sum(1 for x in positions if x and x > 0)
                      if positions else None) if show_pos else None,
            "rows": rows,
            "footer": (self._relative_footer(player, car_lap, lap_est)
                       if rcfg.get("show_footer", True) else {}),
        })

    def _sof(self, drivers) -> str:
        irs = [d.get("IRating") for d in drivers.values() if d.get("IRating")]
        if not irs:
            return "--"
        return self._fmt_irating(sum(irs) / len(irs))

    def _relative_footer(self, player, car_lap, lap_est) -> dict:
        items = set(config.CFG["relative"].get("footer", {}).values())
        out: dict = {}
        if "race_time" in items:
            out["race_time"] = self._fmt_clock(self.ir["SessionTime"])
            out["race_total"] = self._fmt_clock(self.ir["SessionTimeTotal"])
        if "lap" in items:
            total = self.ir["SessionTimeTotal"]
            lap = self.ir["Lap"] or (car_lap[player] if car_lap else None)
            out["lap"] = lap if lap else "-"
            out["lap_est"] = round(total / lap_est, 1) if (total and lap_est) else "-"
        if "incidents" in items:
            out["incidents"] = self.ir["PlayerCarMyIncidentCount"] or 0
        return out

    def _load_demo_track(self) -> None:
        path = track_map.find_track_file("_demo", self.tracks_dir)
        if path:
            try:
                pts, sf, corners, _ = track_map.load_track(path)
                self.map_widget.set_track(pts, sf, corners)
                return
            except Exception:
                pass
        self.map_widget.set_path(track_map.build_demo_path())

    def _ensure_track(self, player, lap_pct) -> None:
        """Prefer a bundled per-track file (by TrackID); else learn from GPS."""
        if self.demo or self._track_loaded:
            return

        weekend = self.ir["WeekendInfo"]
        track_id = weekend.get("TrackID") if weekend else None
        track_file = track_map.find_track_file(track_id, self.tracks_dir)
        if track_file:
            try:
                pts, sf, corners, _ = track_map.load_track(track_file)
                self.map_widget.set_track(pts, sf, corners)
                self._track_loaded = True
                return
            except Exception:
                pass  # fall back to GPS learning

        # No file for this TrackID: learn the shape from the player's own GPS.
        if not self._path_builder.ready:
            self._path_builder.add(lap_pct[player], self.ir["Lat"], self.ir["Lon"])
            if self._path_builder.ready:
                self.map_widget.set_path(self._path_builder.path)
                self._track_loaded = True

    def _update_map(self, player, lap_pct, surface, drivers) -> None:
        if player is None or not lap_pct or not surface:
            return

        self._ensure_track(player, lap_pct)

        cars = []
        for idx, pct in enumerate(lap_pct):
            if pct is None or pct < 0.0 or pct > 1.0:
                continue
            is_player = idx == player
            if surface[idx] != oc.TRK_ON_TRACK and not is_player:
                continue
            d = drivers.get(idx)
            num = str(d.get("CarNumber", "?")) if d else "?"
            palette = track_map.car_palette()
            color = (
                config.CFG["map"]["colors"]["player"]
                if is_player
                else palette[idx % len(palette)]
            )
            cars.append((pct, num, color, is_player))
        self.map_widget.set_cars(cars)

    def _update_dash(self, player, positions, car_lap) -> None:
        """Feed the dash a full telemetry snapshot."""
        total = self.ir["SessionLapsTotal"]
        if not isinstance(total, (int, float)) or total <= 0 or total > 2000:
            total = None
        # iRacing's Clutch is 1.0 when fully engaged (pedal up); show pedal travel.
        clutch_raw = self.ir["Clutch"]
        clutch = (1.0 - clutch_raw) if isinstance(clutch_raw, (int, float)) else None
        abs_active = self.ir["BrakeABSactive"]
        self.dash_widget.set_data({
            "gear": self.ir["Gear"],
            "rpm": self.ir["RPM"],
            "redline": self._car_info.get("redline"),
            "sl_first": self._car_info.get("sl_first"),
            "sl_last": self._car_info.get("sl_last"),
            "throttle": self.ir["Throttle"],
            "brake": self.ir["Brake"],
            "clutch": clutch,
            "abs_active": bool(abs_active),
            "speed_ms": self.ir["Speed"],
            "position": (positions[player] if positions and player is not None
                         else None),
            "lap": self.ir["Lap"] or (car_lap[player]
                                      if car_lap and player is not None else None),
            "laps_total": total,
            "incidents": self.ir["PlayerCarMyIncidentCount"],
            "tire_l": self.ir["LFwearM"],
            "tire_r": self.ir["RFwearM"],
            "fuel": self.ir["FuelLevel"],
            "fuel_laps": self._fuel_laps(),
            "air_temp": self.ir["AirTemp"],
            "track_temp": self.ir["TrackTemp"] or self.ir["TrackTempCrew"],
            "last_lap": self.ir["LapLastLapTime"],
            "best_lap": self.ir["LapBestLapTime"],
            "cur_lap": self.ir["LapCurrentLapTime"],
            "delta": self.ir["LapDeltaToSessionBest"],
        })

    def _fuel_laps(self):
        """Estimate laps of fuel remaining from level, burn rate and lap time."""
        fuel = self.ir["FuelLevel"]
        per_hr = self.ir["FuelUsePerHour"]
        est_lap = self._car_info.get("est_lap") or 0.0
        if not isinstance(fuel, (int, float)) or not per_hr or est_lap <= 0:
            return None
        per_lap = per_hr * (est_lap / 3600.0)
        return fuel / per_lap if per_lap > 0 else None


def main() -> int:
    if "--dump-config" in sys.argv:
        path = config.write_template()
        print(f"Wrote default config template to {path}")
        print("Edit it to customize every color, font, size, count and toggle.")
        return 0

    click_through = "--no-clickthrough" not in sys.argv
    demo = "--demo" in sys.argv
    settings = "--settings" in sys.argv
    app = QApplication(sys.argv)

    import signal

    signal.signal(signal.SIGINT, lambda *_: app.quit())
    keepalive = QTimer()
    keepalive.start(200)
    keepalive.timeout.connect(lambda: None)

    hud = AdvancedSimHUD(click_through=click_through, demo=demo)
    hud.show()
    if settings:
        hud.open_settings()
    if not click_through:
        print("Edit mode: drag panels to position them; positions are saved.")
        print("Settings UI: relaunch with --settings to customize every widget live.")
    return app.exec()


if __name__ == "__main__":
    sys.exit(main())
