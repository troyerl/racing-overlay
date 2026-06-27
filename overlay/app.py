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

import math
import os
import sys
import time

from PyQt6.QtCore import QTimer
from PyQt6.QtWidgets import QApplication

from . import common as oc
from . import config
from . import layout_store
from . import sysstats
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

        # Repaint + re-apply widget visibility when the config changes (editor UI).
        config.on_change(self._on_config_change)
        # Let the settings UI trigger a fresh track scan on demand.
        config.on_rescan(self._rescan_track)

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
        # Cached, throttled values for the header/footer slots.
        self._track_name = ""
        self._sys_cache: tuple[str, str] | None = None
        self._sys_counter = 0
        self._path_builder = track_map.TrackPathBuilder()
        self._track_loaded = False        # a bundled track FILE is in use
        self._track_file_checked = False  # we've looked for a file for this track
        self._map_version = 0             # last learned-path version pushed
        self._track_id = None             # current track's iRacing TrackID
        self._learn_name = ""             # display name to stamp on a saved scan
        self._track_saved = False         # we've persisted this learned scan
        self._force_learn = False         # rescan: re-learn even if a file exists
        # Pit-lane learning: detect the entry/exit (by lap pct) as the player
        # drives through the pits, and the speed limit from the pit limiter.
        self._pit_was_on = False
        self._pit_enter_pct = None
        self._pit_span = None
        self._pit_speed_ms = 0.0
        self._pit_s0 = None  # speed/time samples for steady-cruise detection
        self._pit_t0 = None
        # Dead-reckoning state, used to learn the map from speed + heading when
        # the sim doesn't expose GPS (Lat/Lon). Re-zeroed each lap.
        self._dr_x = 0.0
        self._dr_y = 0.0
        self._dr_t = None
        self._dr_last_pct = None
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
        self._win_by_key[key] = win
        return win

    def _build_panels(self) -> None:
        self._win_by_key: dict[str, PanelWindow] = {}
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

    @staticmethod
    def _is_shown(key: str) -> bool:
        return bool(config.CFG.get(key, {}).get("show", True))

    def show(self) -> None:
        self._apply_visibility()

    def _apply_visibility(self) -> None:
        """Show or hide each panel window to match its config 'show' flag."""
        for key, win in self._win_by_key.items():
            if self._is_shown(key):
                if not win.isVisible():
                    win.show()
            elif win.isVisible():
                win.hide()

    def _on_config_change(self, _cfg) -> None:
        self._apply_visibility()
        self._repaint_all()

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
            "sl_blink": info.get("DriverCarSLBlinkRPM"),
            "gears": info.get("DriverCarGearNumForward"),
            "est_lap": float(info.get("DriverCarEstLapTime", 0.0) or 0.0),
        }
        # Track name (for the optional header/footer slot) lives in WeekendInfo;
        # only parse it when a slot actually shows it, and only on this throttled
        # refresh rather than every tick.
        if config.slot_in_use("track_name"):
            wk = (self.ir["WeekendInfo"] if self.ir else None) or {}
            name = wk.get("TrackDisplayName") or wk.get("TrackName") or ""
            layout = wk.get("TrackConfigName") or ""
            if layout and str(layout).strip().lower() not in ("", "n/a", "none"):
                self._track_name = f"{name} - {layout}"
            else:
                self._track_name = name
        return self._driver_cache

    # --- Per-tick update ----------------------------------------------------

    def process_telemetry_tick(self) -> None:
        if self.ir is None or (not self.ir.is_connected and not self.ir.startup()):
            return

        # Which widgets are visible: a hidden widget does no reads and no work.
        en = {k: self._is_shown(k)
              for k in ("standings", "relative", "radar", "map", "dash")}
        if not any(en.values()):
            return

        player = self.ir["PlayerCarIdx"]
        need_order = en["standings"] or en["relative"] or en["dash"]
        need_drivers = en["standings"] or en["relative"] or en["map"] or en["dash"]
        # Each array is only read if some visible widget consumes it.
        positions = self.ir["CarIdxPosition"] if need_order else None
        lap_pct = (self.ir["CarIdxLapDistPct"]
                   if (en["radar"] or en["map"]) else None)
        surface = (self.ir["CarIdxTrackSurface"]
                   if (en["radar"] or en["standings"] or en["relative"]
                       or en["map"]) else None)
        est_time = (self.ir["CarIdxEstTime"]
                    if (en["relative"] or en["standings"]) else None)
        car_left_right = self.ir["CarLeftRight"] if en["radar"] else None
        car_lap = self.ir["CarIdxLap"] if need_order else None
        # CarIdxF2Time only feeds the standings gap column.
        car_f2 = (self.ir["CarIdxF2Time"]
                  if (en["standings"] and config.has_column("standings", "gap"))
                  else None)
        # _drivers() also refreshes the dash's engine/shift-light params.
        drivers = self._drivers() if need_drivers else {}
        lap_est = self._lap_est(est_time) if est_time is not None else 0.0

        # Track pit stops only if a *shown* table shows the pit column.
        sess_time = None
        if ((en["relative"] and config.has_column("relative", "pit"))
                or (en["standings"] and config.has_column("standings", "pit"))):
            sess_time = self.ir["SessionTime"]
            self._update_pit_tracking(surface, car_lap, sess_time)

        # Per-car lap times only if a shown table shows that column.
        self._car_last = self._car_best = None
        if (en["relative"] and config.has_column("relative", "last_lap")) or \
                (en["standings"] and config.has_column("standings", "last_lap")):
            self._car_last = self.ir["CarIdxLastLapTime"]
        if (en["relative"] and config.has_column("relative", "best_lap")) or \
                (en["standings"] and config.has_column("standings", "best_lap")):
            self._car_best = self.ir["CarIdxBestLapTime"]

        if en["radar"]:
            self._update_radar(player, lap_pct, surface, car_left_right)
        if en["standings"]:
            self._update_standings(positions, drivers, surface, car_f2, player,
                                   lap_est, car_lap, sess_time)
        if en["relative"]:
            self._update_relative(player, est_time, surface, drivers, positions,
                                  car_lap, lap_est, sess_time)
        if en["map"]:
            self._update_map(player, lap_pct, surface, drivers)
        if en["dash"]:
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
        self.standings_widget.set_data({
            "title": scfg["title"],
            "rows": rows,
            "slots": self._slot_values("standings", drivers, positions, player,
                                       lap_est=lap_est, car_lap=car_lap,
                                       count=f"{shown}/{total}"),
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

        self.relative_widget.set_data({
            "rows": rows,
            "slots": self._slot_values("relative", drivers, positions, player,
                                       car_lap, lap_est),
        })

    def _sof(self, drivers) -> str:
        irs = [d.get("IRating") for d in drivers.values() if d.get("IRating")]
        if not irs:
            return "--"
        return self._fmt_irating(sum(irs) / len(irs))

    # --- header / footer slot values ---------------------------------------

    def _slot_values(self, section, drivers, positions, player, car_lap,
                     lap_est, count=None) -> dict:
        """Pre-format every header/footer item the section actually uses.

        Only configured slots are computed (lazy), so unused readouts cost
        nothing per tick. Returns key -> display string for the table to draw.
        """
        out: dict = {}
        for k in config.table_slot_items(section):
            val = self._slot_value(k, drivers, positions, player, car_lap,
                                   lap_est, count)
            if val is not None:
                out[k] = val
        return out

    def _slot_value(self, key, drivers, positions, player, car_lap, lap_est,
                    count):
        ir = self.ir
        if key == "sof":
            return self._sof(drivers)
        if key == "class_sof":
            return self._class_sof(drivers, player)
        if key == "position":
            pos = positions[player] if positions and player is not None else None
            total = sum(1 for x in positions if x and x > 0) if positions else None
            return f"{pos}/{total}" if pos and total else "\u2014"
        if key == "class_position":
            return self._class_position(drivers, positions, player)
        if key == "session_time":
            rem = ir["SessionTimeRemain"]
            if rem is None:
                tot, el = ir["SessionTimeTotal"], ir["SessionTime"]
                rem = (tot - el) if (tot is not None and el is not None) else None
            return self._fmt_clock(rem) if rem is not None else "\u2014"
        if key == "race_time":
            return (f"{self._fmt_clock(ir['SessionTime'])}"
                    f" / {self._fmt_clock(ir['SessionTimeTotal'])}")
        if key == "lap":
            lap = ir["Lap"] or (car_lap[player]
                                if car_lap and player is not None else None)
            total = ir["SessionTimeTotal"]
            est = round(total / lap_est, 1) if (total and lap_est) else "-"
            return f"{lap if lap else '-'}/~{est}"
        if key == "incidents":
            return f"{ir['PlayerCarMyIncidentCount'] or 0}x"
        if key == "track_name":
            return self._track_name or "\u2014"
        if key == "track_temp":
            t = config.conv_temp(ir["TrackTemp"] if ir["TrackTemp"] is not None
                                 else ir["TrackTempCrew"])
            return f"{t:.0f}{config.temp_unit()}" if t is not None else "\u2014"
        if key == "air_temp":
            t = config.conv_temp(ir["AirTemp"])
            return f"{t:.0f}{config.temp_unit()}" if t is not None else "\u2014"
        if key == "best_lap":
            return self._fmt_laptime(ir["LapBestLapTime"])
        if key == "session_best":
            best = ir["CarIdxBestLapTime"]
            vals = [t for t in best if t and t > 0] if best else []
            return self._fmt_laptime(min(vals)) if vals else "\u2014"
        if key == "local_time":
            return time.strftime("%I:%M %p").lstrip("0")
        if key == "sim_time":
            tod = ir["SessionTimeOfDay"]
            return self._fmt_tod(tod) if tod is not None else "\u2014"
        if key == "cpu":
            return self._sys_stats()[0]
        if key == "mem":
            return self._sys_stats()[1]
        if key == "count":
            return count
        return None

    def _class_sof(self, drivers, player) -> str:
        cid = self._player_class(drivers, player)
        if cid is None:
            return self._sof(drivers)
        irs = [d.get("IRating") for d in drivers.values()
               if d.get("IRating") and d.get("CarClassID") == cid]
        if not irs:
            return "--"
        return self._fmt_irating(sum(irs) / len(irs))

    def _class_position(self, drivers, positions, player) -> str:
        cp = self.ir["CarIdxClassPosition"]
        if not cp or player is None or not cp[player] or cp[player] <= 0:
            return "\u2014"
        cid = self._player_class(drivers, player)
        if cid is not None and drivers and positions:
            total = sum(1 for idx, d in drivers.items()
                        if d.get("CarClassID") == cid
                        and idx < len(positions)
                        and positions[idx] and positions[idx] > 0)
        else:
            total = sum(1 for x in cp if x and x > 0)
        return f"{cp[player]}/{total}" if total else str(cp[player])

    @staticmethod
    def _player_class(drivers, player):
        d = drivers.get(player) if drivers and player is not None else None
        return d.get("CarClassID") if d else None

    def _sys_stats(self) -> tuple[str, str]:
        """(cpu%, mem%) for the local machine, sampled a couple times a second."""
        self._sys_counter += 1
        if self._sys_cache is None or self._sys_counter >= 30:
            self._sys_counter = 0
            cpu = sysstats.cpu_percent()
            mem = sysstats.mem_percent()
            self._sys_cache = (
                f"{cpu:.0f}%" if cpu is not None else "--",
                f"{mem:.0f}%" if mem is not None else "--",
            )
        return self._sys_cache

    @staticmethod
    def _fmt_tod(secs) -> str:
        secs = int(secs) % 86400
        return f"{secs // 3600:02d}:{(secs % 3600) // 60:02d}"

    def _load_demo_track(self) -> None:
        path = track_map.find_track_file("_demo", self.tracks_dir)
        if path:
            try:
                pts, sf, corners, _, _ = track_map.load_track(path)
                self.map_widget.set_track(pts, sf, corners)
                return
            except Exception:
                pass
        self.map_widget.set_path(track_map.build_demo_path())

    def _ensure_track(self, player, lap_pct) -> None:
        """Prefer a bundled per-track file (by TrackID); else learn from GPS."""
        if self.demo or self._track_loaded or self._path_builder.complete:
            return

        # Look for a bundled file once we actually know the track (WeekendInfo
        # can be missing on the first ticks). If none exists, learn from GPS.
        if not self._track_file_checked:
            weekend = self.ir["WeekendInfo"]
            if weekend:
                self._track_file_checked = True
                self._track_id = weekend.get("TrackID")
                self._learn_name = (weekend.get("TrackDisplayName")
                                    or weekend.get("TrackName") or "")
                # On a rescan we skip the saved/bundled file and re-learn; the
                # new scan then overwrites tracks/<id>.json when complete.
                if not self._force_learn:
                    track_file = track_map.find_track_file(
                        self._track_id, self.tracks_dir)
                    if track_file:
                        try:
                            pts, sf, corners, _, meta = track_map.load_track(
                                track_file)
                            self.map_widget.set_track(pts, sf, corners)
                            if meta.get("pit_span"):
                                self._pit_span = meta["pit_span"]
                                self._pit_speed_ms = meta.get("pit_speed", 0.0)
                                self.map_widget.set_pit(
                                    self._pit_span, self._pit_speed_ms)
                            self._track_loaded = True
                            return
                        except Exception:
                            pass  # fall back to GPS learning

        # Learn the shape from the player's own GPS, showing a rough loop early
        # and refining it as more of the lap is sampled. If the sim doesn't
        # expose GPS (Lat/Lon), fall back to dead reckoning from speed + heading.
        b = self._path_builder
        pct = lap_pct[player] if lap_pct and player is not None else None
        lat, lon = self.ir["Lat"], self.ir["Lon"]
        if lat is not None and lon is not None and (lat != 0.0 or lon != 0.0):
            b.add(pct, lat, lon)
        else:
            xy, wrapped = self._dead_reckon(pct)
            # Dead-reckoned coordinates are only consistent within one lap, so
            # start fresh at each start/finish crossing to avoid a kinked path.
            if wrapped:
                b.reset()
            if xy is not None:
                b.add_xy(pct, xy[0], xy[1])

        if b.version != self._map_version:
            self._map_version = b.version
            self.map_widget.set_path(b.path)
        elif not b.ready:
            self.map_widget.set_progress(b.coverage())

        # Once the full loop is learned, persist it so we skip learning next time.
        if b.complete and not self._track_saved:
            self._track_saved = True
            try:
                track_map.save_learned_track(
                    self.tracks_dir, self._track_id, b.path, self._learn_name,
                    pit_span=self._pit_span, pit_speed=self._pit_speed_ms)
            except Exception:
                pass

    def _rescan_track(self) -> None:
        """Forget the current (saved or loaded) scan and re-learn from scratch.

        The freshly learned loop overwrites tracks/<id>.json on completion.
        """
        if self.demo:
            return
        self._force_learn = True
        self._track_loaded = False
        self._track_file_checked = False  # re-detect the track id next tick
        self._track_saved = False
        self._map_version = 0
        self._path_builder = track_map.TrackPathBuilder()
        # Reset dead-reckoning so the new scan starts from a clean origin.
        self._dr_x = self._dr_y = 0.0
        self._dr_t = None
        self._dr_last_pct = None
        # Forget the learned pit lane too (set_track(None) clears it on the map).
        self._pit_was_on = False
        self._pit_enter_pct = None
        self._pit_span = None
        self._pit_speed_ms = 0.0
        self._pit_s0 = None
        self._pit_t0 = None
        # Clear the drawn map back to the "learning" placeholder.
        self.map_widget.set_track(None)
        self.map_widget.set_progress(0.0)

    def _dead_reckon(self, pct):
        """Integrate speed + heading into an (x, y) when GPS is unavailable.

        Re-zeros at the start/finish line so each lap shares one origin; the
        absolute orientation may be mirrored/rotated vs. reality, but the loop
        shape is correct, which is all the 2D map needs. Returns
        ((x, y) or None, wrapped) where wrapped marks a start/finish crossing.
        """
        speed = self.ir["Speed"]
        yaw = self.ir["YawNorth"]
        if yaw is None:
            yaw = self.ir["Yaw"]
        t = self.ir["SessionTime"]
        if pct is None or speed is None or yaw is None or t is None:
            return None, False
        # A sharp drop in lap pct = crossed the line -> start a fresh lap origin.
        wrapped = (self._dr_last_pct is not None
                   and pct + 0.5 < self._dr_last_pct)
        if wrapped:
            self._dr_x = self._dr_y = 0.0
            self._dr_t = t
        if self._dr_t is not None:
            dt = t - self._dr_t
            if 0.0 < dt < 0.5:  # ignore pauses / jumps (resets, replays)
                self._dr_x += speed * dt * math.cos(yaw)
                self._dr_y += speed * dt * math.sin(yaw)
        self._dr_t = t
        self._dr_last_pct = pct
        return (self._dr_x, self._dr_y), wrapped

    def _update_map(self, player, lap_pct, surface, drivers) -> None:
        if player is None or not lap_pct or not surface:
            return

        self._ensure_track(player, lap_pct)
        if config.CFG["map"].get("show_pit", True):
            self._learn_pit(player, lap_pct)

        on_pit_arr = self.ir["CarIdxOnPitRoad"]
        pit_surf = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
        use_pos = config.CFG["map"].get("car_label", "number") == "position"
        positions = self.ir["CarIdxPosition"] if use_pos else None
        cars = []
        for idx, pct in enumerate(lap_pct):
            if pct is None or pct < 0.0 or pct > 1.0:
                continue
            is_player = idx == player
            on_pit = (bool(on_pit_arr[idx]) if on_pit_arr and idx < len(on_pit_arr)
                      else surface[idx] in pit_surf)
            # Show cars that are on track or on pit road; skip garage/off-world.
            if surface[idx] != oc.TRK_ON_TRACK and not on_pit and not is_player:
                continue
            d = drivers.get(idx)
            if use_pos and positions and idx < len(positions) and positions[idx]:
                num = str(positions[idx])
            else:
                num = str(d.get("CarNumber", "?")) if d else "?"
            palette = track_map.car_palette()
            color = (
                config.CFG["map"]["colors"]["player"]
                if is_player
                else palette[idx % len(palette)]
            )
            cars.append((pct, num, color, is_player, on_pit))
        self.map_widget.set_cars(cars)

    # iRacing EngineWarnings bitfield: pit speed limiter engaged.
    _PIT_LIMITER_BIT = 0x10

    def _learn_pit_speed(self, speed) -> None:
        """Estimate the pit speed limit while on pit road.

        The limit is the speed held during the steady cruise down pit lane, so
        we accept a sample when either the pit limiter is engaged or the speed
        has been steady over a ~0.4s window (which rejects the entry braking and
        exit acceleration). The running max of those is the limit.
        """
        if not isinstance(speed, (int, float)) or speed <= 3.0:
            return
        ew = self.ir["EngineWarnings"]
        limiter = bool(ew and (int(ew) & self._PIT_LIMITER_BIT))
        steady = False
        t = self.ir["SessionTime"]
        if isinstance(t, (int, float)):
            if self._pit_t0 is None:
                self._pit_s0, self._pit_t0 = speed, t
            elif t - self._pit_t0 >= 0.4:
                # < ~1.25 m/s change over the window => essentially constant.
                steady = abs(speed - self._pit_s0) < 0.5
                self._pit_s0, self._pit_t0 = speed, t
        if (limiter or steady) and speed > self._pit_speed_ms:
            self._pit_speed_ms = speed

    def _learn_pit(self, player, lap_pct) -> None:
        """Learn where pit road is (entry/exit lap pct) and its speed limit.

        Entry/exit come from the player's OnPitRoad edges; the limit is the
        top speed seen while the pit limiter is engaged (it holds the car at the
        cap). Also feeds the map the live pit-lane speed for an over-limit warning.
        """
        if player is None or not lap_pct:
            return
        on = self.ir["OnPitRoad"]
        if on is None:  # fall back to the per-car array
            arr = self.ir["CarIdxOnPitRoad"]
            on = bool(arr[player]) if arr and player < len(arr) else False
        on = bool(on)
        pct = lap_pct[player]
        speed = self.ir["Speed"]

        if on:
            self._learn_pit_speed(speed)
            self.map_widget.set_pit_live(speed)
            # Keep the limit badge current while cruising (if the span is known).
            if self._pit_span is not None:
                self.map_widget.set_pit(self._pit_span, self._pit_speed_ms)
        else:
            self._pit_s0 = self._pit_t0 = None
            self.map_widget.set_pit_live(None)

        # Rising edge -> entering pit road; record where on the lap it happened.
        if on and not self._pit_was_on:
            if pct is not None and 0.0 <= pct <= 1.0:
                self._pit_enter_pct = pct
        # Falling edge -> left pit road; finalize the span and persist it.
        elif (not on) and self._pit_was_on and self._pit_enter_pct is not None:
            if pct is not None and 0.0 <= pct <= 1.0:
                self._pit_span = (self._pit_enter_pct, pct)
                self.map_widget.set_pit(self._pit_span, self._pit_speed_ms)
                if self._track_id is not None:
                    try:
                        track_map.update_track_meta(
                            self.tracks_dir, self._track_id,
                            pit_span=[round(self._pit_span[0], 5),
                                      round(self._pit_span[1], 5)],
                            pit_speed=round(self._pit_speed_ms, 3))
                    except Exception:
                        pass
            self._pit_enter_pct = None
        self._pit_was_on = on

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
            "sl_blink": self._car_info.get("sl_blink"),
            "top_gear": self._car_info.get("gears"),
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
