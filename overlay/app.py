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

import json
import logging
import math
import os
import sys
import time

from PyQt6.QtCore import QTimer
from PyQt6.QtWidgets import QApplication

from . import common as oc
from . import config
from . import constants
from . import demo_data
from . import paths
from . import irating_calc
from . import sysstats
from . import track_store
from . import version
from .panel import PanelWindow
from .widgets import track_map
from .widgets.track_map import TrackMapWidget, is_schematic_pit_source
from .widgets.dash import DashWidget
from .widgets.delta_bar import DeltaBarWidget
from .widgets.flags import FlagsWidget
from .widgets.fuel_calc import FuelCalcWidget
from .widgets.inputs import InputTraceWidget
from .widgets.lap_compare import LapCompareEngine, LapCompareWidget
from .widgets.laptime_log import LaptimeLogWidget
from .widgets.radar import RadarWidget
from .widgets.relative import RelativeWidget
from .widgets.sector_timing import SectorTimer, SectorTimingWidget
from .widgets.standings import StandingsWidget

log = logging.getLogger("gridglance.app")

# Default window geometry per panel: (x, y, w, h). Overridden by saved layout.
DEFAULT_GEOMS = {
    "standings": (40, 120, 560, 360),
    "relative": (620, 120, 600, 360),
    "radar": (40, 500, 200, 260),
    "map": (620, 500, 480, 320),
    "dash": (260, 800, 660, 190),
    "laptime_log": (40, 500, 380, 320),
    "fuel_calc": (440, 500, 460, 380),
    "inputs": (260, 600, 660, 150),
    "delta_bar": (380, 60, 420, 120),
    "flags": (820, 60, 320, 150),
    "sector_timing": (380, 200, 360, 170),
    "lap_compare": (40, 60, 380, 320),
}

# Pit route latch: consecutive ticks on pit road before drawing player on route.
PIT_COMMIT_HOLD = 15


def _coerce_int(value):
    """Best-effort int from a telemetry field (which may be str/float/None)."""
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


class AdvancedSimHUD:
    """Owns the telemetry connection and drives the independent panel windows."""

    def __init__(self, click_through: bool = True, demo: bool = False,
                 demo_track: str | None = None, tracks_dir: str | None = None):
        self.click_through = click_through
        self.demo = demo
        self._demo_track_id = demo_track
        # The real telemetry source. self.ir points at whatever feeds the widgets
        # this tick -- usually _sdk, but a synthetic source while arranging the
        # layout offline (see process_telemetry_tick).
        self._sdk = oc.make_irsdk(demo=demo)
        self.ir = self._sdk
        self._demo_ir = None       # lazily created synthetic source for edit mode
        self._demo_active = False
        self._settings_window = None

        # Repaint + re-apply widget visibility when the config changes (editor UI).
        config.on_change(self._on_config_change)
        # Reapply the saved window layout when the active preset changes.
        config.on_preset_change(self._on_preset_change)
        # Last-seen car path + LeagueID (drives auto-switch of presets); the
        # league id is cached because reading WeekendInfo is an expensive parse.
        self._last_car_path: str | None = None
        self._last_league_id: int | None = None
        self._league_id_cache: int | None = None

        self._driver_cache: dict[int, dict] = {}
        # CarIdx values of pace/safety cars, so they're never shown as competitors.
        self._pace_idxs: set[int] = set()
        # Engine/shift-light params from the session YAML (cached with drivers).
        self._car_info: dict = {}
        self._driver_refresh_counter = 0
        # Per-car pit history: idx -> {"on": bool, "lap": int, "time": float}.
        # We track this ourselves because iRacing exposes no per-car "last pit".
        self._pit: dict[int, dict] = {}
        # Per-car lap-time arrays, refreshed per tick only if a column needs them.
        self._car_last = None
        self._car_best = None
        # CarIdxLapDistPct snapshot for the current tick (lapped-traffic check).
        self._lap_pct = None
        # Cached, throttled values for the header/footer slots.
        self._track_name = ""
        self._sys_cache: tuple[str, str] | None = None
        self._sys_counter = 0
        self._track_loaded = False        # a track file is in use
        self._track_file_checked = False  # we've looked for a file for this track
        self._track_id = None             # current track's iRacing TrackID
        self._track_turns = None          # WeekendInfo TrackNumTurns (corner count)
        self._track_is_oval = False       # oval vs road (metadata only)
        self._learn_name = ""             # display name for saved tracks
        self._no_track_hint = False       # throttle "import HTML" flash
        self._pit_speed_ms = 0.0
        self._pit_lane_speed_pct = 1.0
        self._pit_latch_seed_pending = False
        self._pit_s0 = None
        self._pit_t0 = None
        self._pit_span = None
        self._pit_path = None
        self._pit_in = None
        self._pit_out = None
        self._pit_in_pct = None
        self._pit_out_pct = None
        self._player_on_route = False
        self._player_route_ticks = 0
        self._pit_route_latch: dict[int, bool] = {}
        self._pit_prev_on: dict[int, bool] = {}
        self._pit_exit_latch: dict[int, float] = {}
        self._pit_source = ""
        self._v2_loop_doc: dict | None = None
        self._v2_authoring_track_id = None
        self._v2_authoring_name = ""
        # Sector timing: derives sector splits from lap-distance crossings.
        self._sector_timer = SectorTimer()
        # Lap compare: records per-lap input traces and analyses corners.
        self._lap_engine = LapCompareEngine()
        self._track_len_m = 0.0  # parsed once from the session info
        # Cached session-info reads (refreshed on a throttle, not every tick).
        self._lc_key = None
        self._sector_starts_cache = None
        self._session_info_counter = 0
        # Flag state: remember whether we were under yellow so we can flash a
        # brief green when racing resumes, and when that green window ends.
        self._flag_was_yellow = False
        self._green_until = 0.0
        # White flag (approaching the final lap): remember the lap it first
        # waved on so we can hide it once you cross the line onto the last lap.
        self._flag_was_white = False
        self._white_start_lap = None
        # Laptime log: the player's completed laps (newest first), each
        # {"lap": int, "secs": float, "temp_c": float|None}, plus the last seen
        # lap number so we can detect a lap rollover.
        self._ll_prev_lap = None
        self._ll_laps: list[dict] = []
        # Fuel calculator: per-lap fuel burned (litres, newest first), the lap
        # we last saw and the fuel level at the start of the current lap.
        self._fc_prev_lap = None
        self._fc_lap_start_fuel = None
        self._fc_use: list[float] = []
        self._irating_deltas: dict[int, int] = {}
        self._grid_session_uid: int | None = None
        self._grid_positions_cache: list[int] | None = None
        self._grid_class_positions_cache: list[int] | None = None
        self._class_positions: list[int] | None = None
        # Dead-reckoning state, used to learn the map from speed + heading when
        # the sim doesn't expose GPS (Lat/Lon). Re-zeroed each lap.
        self._dr_x = 0.0
        self._dr_y = 0.0
        self._dr_t = None
        self._dr_last_pct = None
        # The player's position in the map's model frame for the current tick,
        # computed once (GPS if available, else dead reckoning) and shared by the
        # track learner and the pit-route capture so their frames always match.
        self._player_pos: tuple[float, float] | None = None
        self._player_pos_wrapped = False  # crossed start/finish this tick (DR)
        self._player_pos_gps = False      # came from real GPS (not dead reckoning)
        self.tracks_dir = tracks_dir or paths.tracks_dir()
        # Shared (cloud) track maps: download missing tracks on demand, and
        # (author only) upload ones learned locally. All DB I/O is off-thread.
        self._track_sync = track_store.TrackSync()
        self._track_sync.fetched.connect(self._on_remote_track)
        self._track_sync.synced.connect(self._on_tracks_synced)
        self._remote_tried: set = set()  # track ids we've already asked for
        # MongoDB is the source of truth: on launch, refresh the local cache so
        # any maps the author changed are pulled in (runs off the GUI thread).
        if not self.demo and config.cloud_tracks():
            self._track_sync.sync_down_async(self.tracks_dir)
        # The overlay widgets start hidden; the settings window (or --start)
        # turns them on, and they keep running after settings is closed.
        self._overlay_running = False
        # Whether iRacing is currently connected. The overlay only paints over
        # the screen while connected (so nothing floats on your desktop when
        # you're not in the sim); updated each tick.
        self._connected = False

        # Layout is per-preset; the active preset's saved geometries live here and
        # changes are persisted back through config (which writes the active preset).
        self._layout_state = config.active_layout()
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
            on_save=config.save_active_layout,
        )
        self.panels.append(win)
        self._win_by_key[key] = win
        return win

    def _build_panels(self) -> None:
        self._win_by_key: dict[str, PanelWindow] = {}
        self.standings_widget = StandingsWidget()
        self.relative_widget = RelativeWidget()
        self.radar_widget = RadarWidget()
        self.map_widget = TrackMapWidget()
        self.dash_widget = DashWidget()
        self.laptime_widget = LaptimeLogWidget()
        self.fuel_widget = FuelCalcWidget()
        self.inputs_widget = InputTraceWidget()
        self.delta_bar_widget = DeltaBarWidget()
        self.flags_widget = FlagsWidget()
        self.sector_widget = SectorTimingWidget()
        self.lap_compare_widget = LapCompareWidget()
        if self.demo:
            self._load_demo_track()
            self._seed_demo_laptimes()
            self._lap_engine.seed_demo()

        self._wrap("standings", self.standings_widget)
        self._wrap("relative", self.relative_widget)
        self._wrap("radar", self.radar_widget)
        self._wrap("map", self.map_widget)
        self._wrap("dash", self.dash_widget)
        self._wrap("laptime_log", self.laptime_widget)
        self._wrap("fuel_calc", self.fuel_widget)
        self._wrap("inputs", self.inputs_widget)
        self._wrap("delta_bar", self.delta_bar_widget)
        self._wrap("flags", self.flags_widget)
        self._wrap("sector_timing", self.sector_widget)
        self._wrap("lap_compare", self.lap_compare_widget)

    @staticmethod
    def _is_shown(key: str) -> bool:
        return bool(config.CFG.get(key, {}).get("show", True))

    def show(self) -> None:
        self._apply_visibility()

    def start_overlay(self) -> None:
        """Show the overlay widgets (per their individual 'show' flags)."""
        self._overlay_running = True
        self._apply_visibility()

    def stop_overlay(self) -> None:
        """Hide every overlay widget without quitting the app."""
        self._overlay_running = False
        for win in self._win_by_key.values():
            if win.isVisible():
                win.hide()

    def toggle_overlay(self) -> bool:
        if self._overlay_running:
            self.stop_overlay()
        else:
            self.start_overlay()
        return self._overlay_running

    def overlay_running(self) -> bool:
        return self._overlay_running

    def edit_mode_enabled(self) -> bool:
        """True when panels are draggable (i.e. not click-through)."""
        return not self.click_through

    def set_edit_mode(self, enabled: bool) -> None:
        """Make every panel draggable (edit) or click-through (locked) live."""
        self.click_through = not bool(enabled)
        for win in self.panels:
            win.set_click_through(self.click_through)
        # Edit mode reveals panels even when the sim isn't connected so the
        # layout can be arranged offline (see _apply_visibility).
        self._apply_visibility()

    def toggle_edit_mode(self) -> bool:
        self.set_edit_mode(not self.edit_mode_enabled())
        return self.edit_mode_enabled()

    def _apply_visibility(self) -> None:
        """Show or hide each panel window to match config.

        A panel is only shown while the overlay is running, its widget is enabled,
        and iRacing is connected -- so nothing floats over the desktop when you're
        not in the sim. Edit-layout mode overrides the connection check so the
        layout can still be arranged while iRacing is closed.
        """
        live = self._connected or self.edit_mode_enabled()
        for key, win in self._win_by_key.items():
            want = self._overlay_running and live and self._is_shown(key)
            if want and not win.isVisible():
                win.show()
            elif not want and win.isVisible():
                win.hide()

    def _on_config_change(self, _cfg) -> None:
        self._apply_visibility()
        self._repaint_all()

    def _on_preset_change(self, _name) -> None:
        """Reapply the newly active preset's saved window layout to every panel."""
        self._layout_state.clear()
        self._layout_state.update(config.active_layout())
        for key, win in self._win_by_key.items():
            geom = self._layout_state.get(key) or DEFAULT_GEOMS.get(key)
            if geom:
                win.setGeometry(int(geom[0]), int(geom[1]),
                                int(geom[2]), int(geom[3]))
        self._apply_visibility()
        self._repaint_all()

    def current_car(self) -> tuple[str, str]:
        """The player's current (car_path, display_name), or ("", "") if unknown.

        Used by the settings UI to bind "the car I'm driving now" to a preset.
        """
        try:
            idx = self.ir["PlayerCarIdx"]
            drv = self._driver_cache.get(idx) if self._driver_cache else None
            if isinstance(drv, dict):
                path = str(drv.get("CarPath") or "")
                name = str(drv.get("CarScreenName") or path)
                return path, name
        except Exception:
            pass
        return "", ""

    def current_league(self) -> tuple[int, str]:
        """The current session's (LeagueID, label), or (0, "") if not a league.

        Reads WeekendInfo directly (only called on demand from the settings UI),
        so it reflects the live session rather than the throttled cache.
        """
        wk = self.ir["WeekendInfo"] if self.ir else None
        lid = 0
        if isinstance(wk, dict):
            try:
                lid = int(wk.get("LeagueID") or 0)
            except (TypeError, ValueError):
                lid = 0
        return (lid, f"League {lid}") if lid > 0 else (0, "")

    def track_authoring_state(self) -> dict:
        """Snapshot for the Track Scan authoring tab (pit speed, corners)."""
        mw = self.map_widget
        tid = self._authoring_track_id()
        n = self._track_turns if self._track_turns else mw.num_turns
        has_geom = mw.path is not None and len(mw.path) >= 2
        in_sim = not self.demo and self._track_id is not None
        can_author = bool(tid is not None and has_geom)
        return {
            "has_track": can_author and not self.demo,
            "can_author_map": can_author,
            "in_sim": in_sim,
            "demo": self.demo,
            "pit_speed_ms": self._pit_speed_ms,
            "pit_lane_speed_pct": self._pit_lane_speed_pct,
            "num_turns": n,
            "corner_count": len(mw.display_corners()),
            "has_pit_geometry": bool(
                self._pit_path and len(self._pit_path) >= 2),
            "authoring_track_id": tid,
        }

    def _refresh_settings_authoring(self) -> None:
        """Re-sync Track Scan controls after a track loads in the background."""
        w = self._settings_window
        if w is None:
            return
        if hasattr(w, "_refresh_track_authoring"):
            w._refresh_track_authoring()
        panel = getattr(w, "_v2_import_panel", None)
        if panel is not None and hasattr(panel, "refresh"):
            panel.refresh()

    def _ensure_local_track_file(self) -> bool:
        """Make sure tracks/<id>.json exists so metadata edits can be saved."""
        tid = self._authoring_track_id()
        if tid is None or not self.map_widget.path:
            return False
        corners = track_map.corners_to_json(self.map_widget.display_corners())
        pit_span = self._pit_span
        return track_map.ensure_track_file(
            self.tracks_dir, tid, self.map_widget.path,
            name=self._learn_name or self._v2_authoring_name,
            start_finish=self.map_widget.start_finish,
            corners=corners,
            pit_span=pit_span,
            pit_speed=self._pit_speed_ms,
            num_turns=self._track_turns or self.map_widget.num_turns,
            pit_path=self._pit_path,
            pit_in=self._pit_in,
            pit_out=self._pit_out,
            pit_in_pct=self._pit_in_pct,
            pit_out_pct=self._pit_out_pct,
            pit_lane_speed_pct=self._pit_lane_speed_pct,
            learned=False)

    def effective_track_id(self):
        """TrackID for file naming (live session, or ``_demo`` in demo mode)."""
        if self._track_id is not None:
            return self._track_id
        if self.demo:
            return self._demo_track_id or "_demo"
        return None

    def _authoring_track_id(self):
        """TrackID for v2 HTML import / manual pit save (HTML or live session)."""
        if self._v2_authoring_track_id is not None:
            return self._v2_authoring_track_id
        return self.effective_track_id()

    def reload_current_track_file(self) -> bool:
        """Re-read tracks/<TrackID>.json into the map."""
        tid = self.effective_track_id()
        if tid is None:
            return False
        path = track_map.find_track_file(tid, self.tracks_dir)
        if not path:
            return False
        try:
            pts, sf, corners, _, meta = track_map.load_track(path)
            self.map_widget.set_track(pts, sf, corners)
            saved_turns = _coerce_int(meta.get("num_turns"))
            self.map_widget.set_num_turns(self._track_turns or saved_turns)
            if saved_turns and not self._track_turns:
                self._track_turns = saved_turns
                self._apply_pit_meta(meta)
            self._track_loaded = True
            self._refresh_settings_authoring()
            return True
        except Exception:
            log.exception("reload track file failed")
            return False

    def _persist_track_meta(self, **fields) -> bool:
        """Write track metadata locally and push to the cloud when allowed."""
        tid = self._authoring_track_id()
        if tid is None or not fields:
            return False
        if not self._ensure_local_track_file():
            return False
        try:
            ok = track_map.update_track_meta(
                self.tracks_dir, tid, **fields)
        except Exception:
            return False
        if not ok:
            return False
        if config.cloud_tracks():
            self._track_sync.upload_local_async(self.tracks_dir, tid)
        return True

    def set_pit_speed_authoring(self, speed_ms: float) -> bool:
        """Override the learned pit speed limit and save to the track record."""
        speed_ms = max(0.0, float(speed_ms))
        self._pit_speed_ms = speed_ms
        if self._pit_span is not None:
            self.map_widget.set_pit(self._pit_span, self._pit_speed_ms)
        elif speed_ms > 0:
            self.map_widget.pit_speed_ms = speed_ms
            self.map_widget.update()
        return self._persist_track_meta(
            pit_speed=round(speed_ms, 3) if speed_ms else None)

    def set_pit_lane_speed_authoring(self, pct: float) -> bool:
        """Override per-track pit lane dot speed (1.0 = 100%) and save."""
        pct = max(0.25, min(3.0, float(pct)))
        self._pit_lane_speed_pct = pct
        self.map_widget.pit_lane_speed_pct = pct
        self.map_widget.update()
        return self._persist_track_meta(
            pit_lane_speed_pct=round(pct, 4))

    def set_num_turns_authoring(self, n: int) -> bool:
        """Set the official corner count, re-detect labels, and save."""
        try:
            val = int(n)
        except (TypeError, ValueError):
            val = 0
        self._track_turns = val if val > 0 else None
        self.map_widget.set_num_turns(self._track_turns)
        self.map_widget.regenerate_corners()
        fields = {"corners": track_map.corners_to_json(self.map_widget.corners)}
        if self._track_turns:
            fields["num_turns"] = self._track_turns
        else:
            fields["num_turns"] = None
        return self._persist_track_meta(**fields)

    def set_corner_edit_mode(self, enabled: bool) -> None:
        """Toggle drag-to-move corner labels on the map widget."""
        if enabled:
            self.set_pit_edit_mode(False)
            self.set_sf_edit_mode(False)
        self.map_widget.set_corner_edit(
            enabled, self._save_corners_authoring if enabled else None)

    def _uncheck_sf_edit_toggle(self) -> None:
        w = self._settings_window
        if w is not None and hasattr(w, "_sf_edit_sw"):
            w._sf_edit_sw.blockSignals(True)
            w._sf_edit_sw.setChecked(False)
            w._sf_edit_sw.blockSignals(False)

    def set_sf_edit_mode(self, enabled: bool) -> None:
        """Toggle drag-to-move start/finish along the racing loop."""
        if enabled:
            self.set_pit_edit_mode(False)
            self.set_corner_edit_mode(False)
            self.map_widget.flash_hint(
                "Drag the white start/finish line along the track")
        self.map_widget.set_sf_edit(
            enabled, self._save_sf_authoring if enabled else None)

    def _save_sf_authoring(self) -> bool:
        """Persist manually placed start/finish lap fraction."""
        sf = round(float(self.map_widget.start_finish), 5)
        ok = self._persist_track_meta(start_finish=sf)
        if not ok:
            self.map_widget.flash_hint("Could not save start/finish position")
        return ok

    def _save_corners_authoring(self) -> bool:
        """Persist manually placed corner labels."""
        corners = track_map.corners_to_json(self.map_widget.corners)
        fields: dict = {"corners": corners}
        if self._track_turns:
            fields["num_turns"] = self._track_turns
        ok = self._persist_track_meta(**fields)
        if not ok:
            self.map_widget.flash_hint("Could not save corner labels")
        return ok

    def import_loop_v2(self, html_path: str) -> tuple[bool, str]:
        """Import racing loop from members HTML (v2); pit drawn manually on map."""
        try:
            from tools.svg_layers_to_track_v2 import (
                import_track_source_v2,
                parse_track_id_from_html,
            )
        except ImportError as exc:
            name = (getattr(exc, "name", None) or str(exc)).lower()
            if "tools" in name:
                msg = ("Track import module not found — run GridGlance from the "
                       "repo root (python run.py).")
            elif "bs4" in name or "beautifulsoup" in name:
                msg = ("Missing beautifulsoup4 — run: pip install -r requirements.txt")
            elif "svgpathtools" in name or "svgpath" in name:
                msg = ("Missing svgpathtools — run: pip install -r requirements.txt")
            else:
                msg = ("Missing HTML import deps — run: pip install -r requirements.txt")
            log.warning("v2 loop import failed: %s (%s)", msg, exc)
            return False, msg

        tid = parse_track_id_from_html(html_path=html_path)
        if tid is None:
            tid = self.effective_track_id()
        if tid is None:
            msg = ("No TrackID — save members HTML with id=\"track-map-123\" "
                   "(outer track-map div), or join a track in iRacing.")
            log.warning("v2 loop import skipped: %s", msg)
            return False, msg

        try:
            doc = import_track_source_v2(
                html_path,
                num_corners=int(self._track_turns or 4) or 4,
            )
        except Exception as exc:
            log.exception("v2 loop import failed")
            msg = str(exc)
            log.warning("v2 loop import failed: %s", msg)
            return False, msg

        stem = os.path.splitext(os.path.basename(html_path))[0]
        self._v2_authoring_track_id = tid
        self._v2_authoring_name = stem or str(tid)
        self._apply_loop_v2_doc(doc)
        self._v2_loop_doc = doc
        n = len(doc.get("points") or [])
        msg = (f"Loop imported for TrackID {tid} — {n} pts. "
               f"Draw pit road on the map, then merge, then Save track.")
        log.info("v2 loop import OK for TrackID %s (%d pts)", tid, n)
        return True, msg

    def _apply_loop_v2_doc(self, doc: dict) -> None:
        """Load loop-only v2 import onto the map; clear pit for manual authoring."""
        pts = [(float(p[0]), float(p[1]))
               for p in doc.get("points", []) if len(p) >= 2]
        if len(pts) < 3:
            raise ValueError("Import produced too few track points.")
        sf = float(doc.get("start_finish", 0.0))
        corners = [c for c in doc.get("corners", []) if isinstance(c, dict)]
        self.map_widget.set_track(pts, sf, corners)
        turns = _coerce_int(doc.get("num_turns"))
        if turns:
            self.map_widget.set_num_turns(self._track_turns or turns)
            if not self._track_turns:
                self._track_turns = turns
        self._pit_source = "manual"
        self.map_widget.set_pit_source("manual")
        self._pit_span = None
        self._pit_path = self._pit_in = self._pit_out = None
        self._pit_in_pct = self._pit_out_pct = None
        self.map_widget.clear_pit()
        self.map_widget.clear_pit_edit()
        self._track_loaded = True
        self._refresh_settings_authoring()
        self.map_widget.flash_hint(
            "Loop imported — draw pit road, then merge (Track Scan)")
        self.map_widget.update()

    def load_pit_into_editor(self, *, force: bool = False) -> bool:
        """Copy saved pit road / exit merge into the manual edit buffers."""
        road, merge = self.map_widget.pit_edit_snapshot()
        if not force and (len(road) >= 2 or len(merge) >= 2):
            return True
        road_pts: list[tuple[float, float]] = []
        for pt in self._pit_path or []:
            if isinstance(pt, (list, tuple)) and len(pt) >= 2:
                road_pts.append((float(pt[0]), float(pt[1])))
        merge_pts: list[tuple[float, float]] = []
        for pt in self._pit_out or []:
            if isinstance(pt, (list, tuple)) and len(pt) >= 2:
                merge_pts.append((float(pt[0]), float(pt[1])))
        if len(road_pts) < 2 and len(merge_pts) < 2:
            return False
        self.map_widget.load_pit_edit(
            road_pts if len(road_pts) >= 2 else [],
            merge_pts if len(merge_pts) >= 2 else [],
        )
        return True

    def set_pit_edit_mode(self, enabled: bool, phase: str = "road") -> None:
        """Toggle manual pit authoring clicks on the live map."""
        if enabled:
            self.set_corner_edit_mode(False)
            self.set_sf_edit_mode(False)
            self.load_pit_into_editor()
            w = self._settings_window
            if w is not None and hasattr(w, "_corner_edit_sw"):
                w._corner_edit_sw.blockSignals(True)
                w._corner_edit_sw.setChecked(False)
                w._corner_edit_sw.blockSignals(False)
            self._uncheck_sf_edit_toggle()
        self.map_widget.set_pit_edit(
            enabled, self._save_pit_authoring if enabled else None)
        if enabled:
            self.map_widget.set_pit_edit_phase(phase)

    def _save_pit_authoring(self) -> None:
        """Refresh in-progress pit preview after a handle drag (no file write)."""
        self.map_widget.update()

    def save_manual_track_v2(self) -> tuple[bool, str]:
        """Finalize manual pit geometry and write tracks/<TrackID>.json."""
        tid = self._authoring_track_id()
        if tid is None:
            return False, ("No TrackID — join a session on track, or import "
                           "members HTML with id=\"track-map-123\".")
        if not self.map_widget.path or len(self.map_widget.path) < 3:
            return False, "No track loop loaded."
        road, merge = self.map_widget.pit_edit_snapshot()
        if len(road) < 2:
            return False, "Need at least 2 pit road points."
        if len(merge) < 2:
            return False, "Need at least 2 merge points."

        from tools.schematic_to_track import (
            _connect_blend_to_loop,
            _pct_on_loop,
            _pit_span_on_loop,
            _resample_open,
        )

        loop = [(p[0], p[1]) for p in self.map_widget.path]
        pit_path = _resample_open(road, 140)
        pit_out_raw = _resample_open(merge, 41)
        pit_in_seed = list(road[: min(8, len(road))])
        pit_in = _connect_blend_to_loop(
            pit_in_seed, loop, attach_end=False, n_loop=12, max_pts=24)
        pit_out = _connect_blend_to_loop(
            pit_out_raw, loop, attach_end=True, n_loop=20, pit_path=pit_path)
        pit_in = _resample_open(pit_in, 24)
        pit_out = _resample_open(pit_out, 41)

        pit_in_pct = round(_pct_on_loop(loop, pit_in[0]), 5)
        pit_out_pct = round(_pct_on_loop(loop, pit_out[-1]), 5)
        lane_lo, lane_hi = _pit_span_on_loop(loop, pit_path)

        doc: dict = {
            "schema": 2,
            "import_version": 2,
            "pit_source": "manual",
            "track_id": tid,
            "name": (self._learn_name or self._v2_authoring_name or str(tid)),
            "start_finish": float(self.map_widget.start_finish),
            "points": [[round(p[0], 7), round(p[1], 7)] for p in loop],
            "corners": track_map.corners_to_json(
                self.map_widget.display_corners()),
            "pit_in": [[round(x, 7), round(y, 7)] for x, y in pit_in],
            "pit_path": [[round(x, 7), round(y, 7)] for x, y in pit_path],
            "pit_out": [[round(x, 7), round(y, 7)] for x, y in pit_out],
            "pit_in_pct": pit_in_pct,
            "pit_span": [round(lane_lo, 5), round(lane_hi, 5)],
            "pit_out_pct": pit_out_pct,
        }
        if self._track_turns:
            doc["num_turns"] = int(self._track_turns)
        elif self.map_widget.num_turns:
            doc["num_turns"] = int(self.map_widget.num_turns)
        if self._pit_speed_ms > 0:
            doc["pit_speed"] = round(self._pit_speed_ms, 3)
        if self._pit_lane_speed_pct != 1.0:
            doc["pit_lane_speed_pct"] = round(self._pit_lane_speed_pct, 4)
        if self._v2_loop_doc:
            for key in ("import_version",):
                if key in self._v2_loop_doc:
                    doc[key] = self._v2_loop_doc[key]

        path = os.path.join(self.tracks_dir, f"{tid}.json")
        os.makedirs(self.tracks_dir, exist_ok=True)
        tmp = f"{path}.tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(doc, fh, indent=2)
            fh.write("\n")
        os.replace(tmp, path)

        meta = {k: doc[k] for k in (
            "pit_span", "pit_path", "pit_in", "pit_out", "pit_in_pct",
            "pit_out_pct", "pit_speed", "pit_source", "pit_lane_speed_pct",
        ) if k in doc}
        self._apply_pit_meta(meta)
        if track_store.can_write():
            self._track_sync.upload_local_async(self.tracks_dir, tid)
        n_in = len(pit_in)
        n_path = len(pit_path)
        n_out = len(pit_out)
        msg = (f"Saved {path} — entry {n_in}, road {n_path}, "
               f"merge {n_out} pts")
        if self.demo:
            msg += f". Restart with: python3 run.py --demo --demo-track {tid}"
        return True, msg

    def pit_edit_state(self) -> dict:
        """Snapshot for Track Scan v2 import panel."""
        road, merge = self.map_widget.pit_edit_snapshot()
        tid = self._authoring_track_id()
        return {
            "road_count": len(road),
            "merge_count": len(merge),
            "pit_edit_mode": self.map_widget.pit_edit_mode,
            "pit_edit_phase": self.map_widget.pit_edit_phase,
            "has_loop": bool(self.map_widget.path and len(self.map_widget.path) >= 3),
            "authoring_track_id": tid,
            "in_sim": not self.demo and self._track_id is not None,
            "has_saved_pit": bool(
                self._pit_path and len(self._pit_path) >= 2),
        }

    def _update_scan_status(self) -> None:
        """Clear legacy scan badges (track learning removed)."""
        self.map_widget.set_scan_status("")

    def _session_league_id(self) -> int:
        """Current LeagueID (0 if none), re-read from WeekendInfo at most ~1/sec."""
        if self._league_id_cache is not None and self._session_info_counter % 60:
            return self._league_id_cache
        wk = self.ir["WeekendInfo"] if self.ir else None
        lid = 0
        if isinstance(wk, dict):
            try:
                lid = int(wk.get("LeagueID") or 0)
            except (TypeError, ValueError):
                lid = 0
        self._league_id_cache = lid
        return lid

    def _repaint_all(self) -> None:
        for w in (self.standings_widget, self.relative_widget,
                  self.radar_widget, self.map_widget, self.dash_widget,
                  self.laptime_widget, self.fuel_widget, self.inputs_widget,
                  self.delta_bar_widget, self.flags_widget,
                  self.sector_widget, self.lap_compare_widget):
            w.update()

    def open_settings(self) -> None:
        from .config_editor import ConfigEditor

        if self._settings_window is None:
            self._settings_window = ConfigEditor(overlay=self)
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
        # Exclude the pace/safety car: it isn't a competitor and shouldn't show
        # in the tables. Remember its CarIdx so the tick can skip it too.
        cache: dict[int, dict] = {}
        pace: set[int] = set()
        for d in info.get("Drivers", []):
            idx = d.get("CarIdx")
            if idx is None:
                continue
            if d.get("CarIsPaceCar"):
                pace.add(idx)
                continue
            cache[idx] = d
        self._driver_cache = cache
        self._pace_idxs = pace
        self._car_info = {
            "redline": info.get("DriverCarRedLine"),
            "sl_first": info.get("DriverCarSLFirstRPM"),
            "sl_last": info.get("DriverCarSLLastRPM"),
            "sl_shift": info.get("DriverCarSLShiftRPM"),
            "sl_blink": info.get("DriverCarSLBlinkRPM"),
            "gears": info.get("DriverCarGearNumForward"),
            "est_lap": float(info.get("DriverCarEstLapTime", 0.0) or 0.0),
            # Usable tank capacity = max litres * max fill fraction.
            "fuel_max": (float(info.get("DriverCarFuelMaxLtr", 0.0) or 0.0)
                         * float(info.get("DriverCarMaxFuelPct", 1.0) or 1.0)),
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

    def _demo_feed(self):
        """A synthetic telemetry source used to populate the overlay while you
        arrange the layout offline. Seeds a demo track + lap log the first time."""
        if self._demo_ir is None:
            self._demo_ir = oc.make_irsdk(demo=True)
        if not self._demo_active:
            self._demo_active = True
            self._load_demo_track()
            self._seed_demo_laptimes()
        return self._demo_ir

    def process_telemetry_tick(self) -> None:
        connected = bool(self._sdk is not None
                         and (self._sdk.is_connected or self._sdk.startup()))
        # Reveal/hide the overlay as iRacing connects or disconnects.
        if connected != self._connected:
            self._connected = connected
            self._apply_visibility()
        # When iRacing isn't connected but you're arranging the layout (overlay
        # on + edit mode), feed the widgets synthetic data instead of leaving
        # them blank. Otherwise, when disconnected, there's nothing to do.
        use_demo = (not connected and self._overlay_running
                    and self.edit_mode_enabled())
        if not connected and not use_demo:
            self.ir = self._sdk
            return
        if connected:
            self._demo_active = False
        self.ir = self._demo_feed() if use_demo else self._sdk

        # Switch the garage vs on-track profile before anything else, so widget
        # visibility + content reflect the right context this tick.
        self._update_context()
        # Drives the ~1 s throttle on expensive session-info (YAML) reads.
        self._session_info_counter += 1

        # Which widgets are visible: a hidden widget does no reads and no work.
        en = {k: self._is_shown(k)
              for k in ("standings", "relative", "radar", "map", "dash",
                        "laptime_log", "fuel_calc", "inputs", "delta_bar",
                        "flags", "sector_timing", "lap_compare")}
        if not any(en.values()):
            return

        player = self.ir["PlayerCarIdx"]
        need_order = en["standings"] or en["relative"] or en["dash"]
        need_drivers = en["standings"] or en["relative"] or en["map"] or en["dash"]
        map_use_pos = (en["map"]
                       and config.CFG["map"].get("car_label", "number") == "position")
        need_pos = need_order or map_use_pos
        # Each array is only read if some visible widget consumes it.
        positions = None
        self._class_positions = None
        if need_pos:
            live_pos = self.ir["CarIdxPosition"]
            positions, self._class_positions = self._resolve_positions(
                live_pos, player)
        lap_pct = (self.ir["CarIdxLapDistPct"]
                   if (en["radar"] or en["map"] or en["standings"]
                       or en["relative"] or en["sector_timing"]
                       or en["lap_compare"]) else None)
        # Used by the tables to tell genuine lapped traffic from same-lap cars.
        self._lap_pct = lap_pct
        surface = (self.ir["CarIdxTrackSurface"]
                   if (en["radar"] or en["standings"] or en["relative"]
                       or en["map"]) else None)
        est_time = (self.ir["CarIdxEstTime"]
                    if (en["relative"] or en["standings"]) else None)
        car_left_right = self.ir["CarLeftRight"] if en["radar"] else None
        car_lap = self.ir["CarIdxLap"] if (need_order or en["map"]) else None
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

        radio_speaker = None
        if ((en["relative"] and config.has_column("relative", "badge"))
                or (en["standings"] and config.has_column("standings", "badge"))
                or en["map"]):
            try:
                idx = int(self.ir["RadioTransmitCarIdx"])
                if idx >= 0:
                    radio_speaker = idx
            except (TypeError, ValueError, KeyError):
                radio_speaker = None

        need_irating_proj = (
            (en["standings"] and config.has_column("standings", "irating")
             and config.CFG.get("standings", {}).get("show_irating_projection"))
            or (en["relative"] and config.has_column("relative", "irating")
                and config.CFG.get("relative", {}).get("show_irating_projection"))
            or (en["dash"] and self._dash_needs_irating_projection())
        )
        self._irating_deltas = {}
        if need_irating_proj and drivers and (
                self._demo_active or self._session_allows_irating_projection()):
            class_pos = self._class_positions
            if class_pos is None:
                try:
                    class_pos = self.ir["CarIdxClassPosition"]
                except (TypeError, ValueError, KeyError):
                    class_pos = None
            self._irating_deltas = irating_calc.project_deltas_by_class(
                drivers, class_pos, positions, self._pace_idxs)

        if en["radar"]:
            self._update_radar(player, lap_pct, surface, car_left_right)
        if en["standings"]:
            self._update_standings(positions, drivers, surface, car_f2, player,
                                   lap_est, car_lap, sess_time, radio_speaker)
        if en["relative"]:
            self._update_relative(player, est_time, surface, drivers, positions,
                                  car_lap, lap_est, sess_time, radio_speaker)
        if en["map"]:
            self._update_map(player, lap_pct, surface, drivers, positions,
                             car_lap, radio_speaker)
        if en["dash"]:
            self._update_dash(player, positions, car_lap)
        if en["laptime_log"]:
            self._update_laptime_log()
        if en["fuel_calc"]:
            self._update_fuel_calc()
        if en["inputs"]:
            self._update_inputs()
        if en["delta_bar"]:
            self._update_delta_bar()
        if en["flags"]:
            self._update_flags()
        if en["sector_timing"]:
            self._update_sectors(player, lap_pct)
        if en["lap_compare"]:
            self._update_lap_compare(player, lap_pct)

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
        # which is shown by the side markers instead). Also track the nearest car
        # of all so the side marker can be positioned fore/aft.
        rc = config.CFG["radar"]
        nearest_ahead = nearest_behind = None
        nearest_abs = None          # signed delta of the overall closest car
        alongside_zone = 0.004
        radar_range = rc["range_pct"]
        want_front = rc.get("show_front", True)
        want_rear = rc.get("show_rear", True)
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
                if nearest_abs is None or abs(delta) < abs(nearest_abs):
                    nearest_abs = delta
                if want_front and alongside_zone < delta <= radar_range:
                    nearest_ahead = delta if nearest_ahead is None else min(nearest_ahead, delta)
                elif want_rear and -radar_range <= delta < -alongside_zone:
                    nearest_behind = delta if nearest_behind is None else max(nearest_behind, delta)

        def closeness(delta):
            if delta is None:
                return None
            return max(0.0, min(1.0, 1.0 - abs(delta) / radar_range))

        # Where the alongside car sits fore/aft: +1 = your front bumper (top of
        # the radar), -1 = your rear bumper (bottom). Derived from the closest
        # car's lap-distance delta since iRacing's CarLeftRight is side-only.
        span = rc.get("side_span_pct", 0.0045) or 0.0045
        if nearest_abs is None:
            side_pos = 0.0
        else:
            side_pos = max(-1.0, min(1.0, nearest_abs / span))

        self.radar_widget.set_data({
            "left": car_left_right in oc.CAR_ON_LEFT,
            "right": car_left_right in oc.CAR_ON_RIGHT,
            "left2": car_left_right == oc.LR_2_CARS_LEFT,
            "right2": car_left_right == oc.LR_2_CARS_RIGHT,
            "left_pos": side_pos,
            "right_pos": side_pos,
            "ahead": closeness(nearest_ahead) if want_front else None,
            "behind": closeness(nearest_behind) if want_rear else None,
        })

    def _build_standings_row(self, idx, drivers, positions, surface, car_f2,
                             player, lap_est, cols, car_lap, sess_time,
                             pit_mode, radio_speaker=None) -> dict:
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

        is_player = idx == player
        lapping, lap_ahead = self._lap_tint(idx, player, car_lap, is_player)

        return {
            "key": idx,
            "position": (positions[idx] if positions else "") if cols.get("position") else "",
            "car_number": str(d.get("CarNumber", "")) if cols.get("car_number") else "",
            "name": d.get("UserName", f"Car {idx}") if cols.get("name") else "",
            "class_color": self._class_color(d, idx) if cols.get("stripe") else "#888888",
            "sr": sr,
            "lic_class": cls,
            "irating": (self._fmt_irating(d.get("IRating"), "standings")
                        if cols.get("irating") or cols.get("license") else ""),
            "irating_delta": self._row_irating_delta(idx, cols, "standings"),
            "pit": pit,
            "gap_text": gap_text,
            "last_lap": self._lap_for(idx, self._car_last) if cols.get("last_lap") else "",
            "best_lap": self._lap_for(idx, self._car_best) if cols.get("best_lap") else "",
            "is_player": is_player,
            "in_pit": surface[idx] in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS),
            "lapping": lapping,
            "lap_ahead": lap_ahead,
            "speaking": radio_speaker is not None and idx == radio_speaker,
        }

    @staticmethod
    def _relative_include(idx, surface, positions, player) -> bool:
        """Relative rows: on-track cars, plus anyone ahead in race position."""
        if not surface or idx >= len(surface):
            return False
        on_track = surface[idx] in (oc.TRK_ON_TRACK, oc.TRK_APPROACHING_PITS,
                                    oc.TRK_IN_PIT_STALL)
        if on_track:
            return True
        if not positions or player is None or idx >= len(positions):
            return False
        pos = positions[idx]
        player_pos = positions[player] if player < len(positions) else 0
        return bool(pos and player_pos and 0 < pos < player_pos)

    def _laps_done(self, idx, car_lap):
        """Total laps completed by a car (lap count + fraction into the lap).

        Combining CarIdxLap with CarIdxLapDistPct gives a continuous distance so
        two cars on the *same* racing lap aren't mistaken for lapped traffic just
        because one has crossed the start/finish line and the other hasn't.
        """
        if not car_lap or idx >= len(car_lap):
            return None
        lap = car_lap[idx]
        if not isinstance(lap, (int, float)) or lap < 0:
            return None
        pct = 0.0
        lp = self._lap_pct
        if lp and idx < len(lp) and isinstance(lp[idx], (int, float)) and lp[idx] >= 0:
            pct = lp[idx]
        return lap + pct

    def _lap_tint(self, idx, player, car_lap, is_player):
        """(lapping, lap_ahead) for the row's lapped-traffic tint.

        A car counts as lapped traffic only when at least ~half a lap of distance
        separates it from the player (so adjacent same-lap cars aren't tinted).
        """
        if is_player:
            return False, False
        me = self._laps_done(player, car_lap)
        them = self._laps_done(idx, car_lap)
        if me is None or them is None:
            return False, False
        diff = them - me
        if abs(diff) < 0.5:
            return False, False
        return True, diff > 0

    def _update_standings(self, positions, drivers, surface, car_f2,
                          player, lap_est, car_lap, sess_time,
                          radio_speaker=None) -> None:
        if not positions:
            return
        scfg = config.CFG["standings"]
        cols = self._visible_cols("standings")
        pit_mode = scfg.get("pit_mode", "laps_since")
        n = scfg["rows"]
        ranked = sorted(
            (idx for idx, pos in enumerate(positions)
             if pos and pos > 0 and idx not in self._pace_idxs),
            key=lambda idx: positions[idx],
        )
        total = len(ranked)

        def build(idx):
            return self._build_standings_row(idx, drivers, positions, surface,
                                             car_f2, player, lap_est, cols,
                                             car_lap, sess_time, pit_mode,
                                             radio_speaker)

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
    def _fmt_irating(ir, section: str | None = None) -> str:
        if not ir:
            return "--"
        ir = int(round(ir))
        sec = config.CFG.get(section or "", {})
        abbreviate = sec.get("irating_abbreviate", True) if isinstance(sec, dict) else True
        if abbreviate and ir >= 1000:
            return f"{ir / 1000:.1f}k"
        return str(ir)

    def _session_allows_irating_projection(self) -> bool:
        """iRating moves in race sessions only (not practice/qual)."""
        try:
            state = int(self.ir["SessionState"])
        except (TypeError, ValueError, KeyError):
            return False
        return state in (4, 5)  # racing, checkered

    @staticmethod
    def _dash_needs_irating_projection() -> bool:
        dc = config.CFG.get("dash", {})
        if not dc.get("show_irating_projection"):
            return False
        slots = {dc.get(k) for k in (
            "top_right", "primary_left", "primary_right",
            "stat_left", "stat_right",
            "strip_left", "strip_center", "strip_right")}
        return "irating" in slots

    @staticmethod
    def _dash_uses_irating() -> bool:
        dc = config.CFG.get("dash", {})
        slots = {dc.get(k) for k in (
            "top_right", "primary_left", "primary_right",
            "stat_left", "stat_right",
            "strip_left", "strip_center", "strip_right")}
        return "irating" in slots

    def _row_irating_delta(self, idx, cols, section: str):
        if not cols.get("irating"):
            return None
        sec = config.CFG.get(section, {})
        if not isinstance(sec, dict) or not sec.get("show_irating_projection"):
            return None
        return self._irating_deltas.get(idx)

    @staticmethod
    def _parse_license(lic_str):
        # LicString looks like "A 3.71"; return ("A", "3.71").
        if not lic_str:
            return "", ""
        parts = str(lic_str).split()
        cls = parts[0][:1] if parts else ""
        try:
            sr = f"{float(parts[1]):.2f}"
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

    # iRacing sends -1 for SessionTimeRemain pre-green; SessionTimeTotal can be
    # 604800 (168 h) as an "unlimited" placeholder while gridding.
    _MAX_SESSION_SEC = 48 * 3600

    @classmethod
    def _sane_session_seconds(cls, secs) -> float | None:
        if not isinstance(secs, (int, float)):
            return None
        if secs < 0:
            return None
        if secs > cls._MAX_SESSION_SEC:
            return None
        return float(secs)

    def _session_time_remain(self) -> float | None:
        """Seconds left in session, or None when not meaningful (e.g. on grid)."""
        try:
            rem = self.ir["SessionTimeRemain"]
        except (TypeError, ValueError, KeyError):
            rem = None
        sane = self._sane_session_seconds(rem)
        if sane is not None:
            return sane
        try:
            el = self.ir["SessionTime"]
            tot = self.ir["SessionTimeTotal"]
        except (TypeError, ValueError, KeyError):
            return None
        if not isinstance(el, (int, float)) or el <= 0:
            return None
        if not isinstance(tot, (int, float)):
            return None
        return self._sane_session_seconds(tot - el)

    def _race_time_display(self) -> str:
        try:
            el = self.ir["SessionTime"]
            tot = self.ir["SessionTimeTotal"]
        except (TypeError, ValueError, KeyError):
            return "\u2014"
        el_s = self._sane_session_seconds(el) if isinstance(el, (int, float)) and el > 0 else None
        tot_s = self._sane_session_seconds(tot)
        if el_s is None and tot_s is None:
            return "\u2014"
        if el_s is not None and tot_s is not None:
            return f"{self._fmt_clock(el_s)} / {self._fmt_clock(tot_s)}"
        if el_s is not None:
            return self._fmt_clock(el_s)
        return f"/ {self._fmt_clock(tot_s)}"

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

    # --- laptime log --------------------------------------------------------

    @staticmethod
    def _fmt_laplog_time(secs) -> str:
        """A lap time as MM:SS.mmm (zero-padded minutes) for the log."""
        if not isinstance(secs, (int, float)) or secs <= 0:
            return "\u2014"
        m = int(secs // 60)
        s = secs - m * 60
        return f"{m:02d}:{s:06.3f}"

    @staticmethod
    def _fmt_temp(c) -> str:
        t = config.conv_temp(c)
        return f"{t:.1f}{config.temp_unit()}" if t is not None else "\u2014"

    def _seed_demo_laptimes(self) -> None:
        """Prefill the log with a few plausible laps so demo mode looks alive."""
        seed = [(17, 136.949), (16, 137.151), (15, 136.704), (14, 136.853),
                (13, 180.165), (11, 142.291), (10, 137.020), (9, 136.840)]
        self._ll_laps = [{"lap": lp, "secs": t, "temp_c": 23.4} for lp, t in seed]
        self._ll_prev_lap = 18

    def _update_laptime_log(self) -> None:
        """Record each completed player lap (time + track temp) and push rows."""
        try:
            lap = int(self.ir["Lap"])
        except (TypeError, ValueError):
            lap = None
        if lap is not None:
            if self._ll_prev_lap is None:
                self._ll_prev_lap = lap
            elif lap > self._ll_prev_lap:
                last = self.ir["LapLastLapTime"]
                completed = lap - 1
                if (isinstance(last, (int, float)) and last > 0
                        and (not self._ll_laps
                             or self._ll_laps[0]["lap"] != completed)):
                    temp = self.ir["TrackTemp"]
                    if temp is None:
                        temp = self.ir["TrackTempCrew"]
                    self._ll_laps.insert(0, {"lap": completed,
                                             "secs": float(last),
                                             "temp_c": temp})
                    del self._ll_laps[60:]  # keep memory bounded
                self._ll_prev_lap = lap
        self.laptime_widget.set_data(self._build_laptime_rows())

    def _build_laptime_rows(self) -> dict:
        cfg = config.CFG["laptime_log"]
        n = max(1, int(cfg.get("rows", 8)))
        mode = cfg.get("delta_mode", "previous")
        laps = self._ll_laps
        best = min((l["secs"] for l in laps if l["secs"] > 0), default=None)
        rows = []
        for i, l in enumerate(laps[:n]):
            secs = l["secs"]
            if mode == "best":
                delta = (secs - best) if (best and secs > 0) else None
                if delta is not None and abs(delta) < 1e-4:
                    delta = None  # the baseline (best) lap itself
            else:  # vs the previous (chronologically older) lap
                prev = laps[i + 1]["secs"] if i + 1 < len(laps) else None
                delta = (secs - prev) if (prev and secs > 0) else None
            rows.append({
                "lap": l["lap"],
                "time": self._fmt_laplog_time(secs),
                "delta": delta,
                "temp": self._fmt_temp(l["temp_c"]),
            })
        return {"rows": rows}

    def _build_relative_row(self, idx, delta, drivers, positions, surface, car_lap,
                            player, is_player, cols, sess_time, pit_mode,
                            radio_speaker=None) -> dict:
        # Only compute fields whose column is actually shown.
        d = drivers.get(idx, {})
        cls = sr = ""
        if cols.get("license"):
            cls, sr = self._parse_license(d.get("LicString"))
        pit = ""
        if cols.get("pit"):
            pit = self._pit_text(idx, car_lap, sess_time, pit_mode)
        in_pit = surface[idx] in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
        # A car ~a lap ahead will lap you (-> red); ~a lap behind you'll lap
        # it (-> blue). Distance-based so same-lap cars near you aren't tinted.
        lapping, lap_ahead = self._lap_tint(idx, player, car_lap, is_player)
        return {
            "key": idx,
            "position": (positions[idx] if positions else "") if cols.get("position") else "",
            "car_number": str(d.get("CarNumber", "")) if cols.get("car_number") else "",
            "name": d.get("UserName", f"Car {idx}") if cols.get("name") else "",
            "class_color": self._class_color(d, idx) if cols.get("stripe") else "#888888",
            "sr": sr,
            "lic_class": cls,
            "irating": (self._fmt_irating(d.get("IRating"), "relative")
                        if cols.get("irating") or cols.get("license") else ""),
            "irating_delta": self._row_irating_delta(idx, cols, "relative"),
            "pit": pit,
            "gap": delta if cols.get("gap") else None,
            "last_lap": self._lap_for(idx, self._car_last) if cols.get("last_lap") else "",
            "best_lap": self._lap_for(idx, self._car_best) if cols.get("best_lap") else "",
            "is_player": is_player,
            "in_pit": in_pit,
            "lapping": lapping,
            "lap_ahead": lap_ahead,
            "speaking": radio_speaker is not None and idx == radio_speaker,
        }

    def _update_relative(self, player, est_time, surface, drivers,
                         positions, car_lap, lap_est, sess_time,
                         radio_speaker=None) -> None:
        if player is None or not est_time or not surface or lap_est <= 0:
            return

        rcfg = config.CFG["relative"]
        cols = self._visible_cols("relative")
        pit_mode = rcfg.get("pit_mode", "laps_since")
        me = est_time[player]
        rels = []
        for idx, t in enumerate(est_time):
            if idx == player or t is None or idx in self._pace_idxs:
                continue
            if not self._relative_include(idx, surface, positions, player):
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
                cols, sess_time, pit_mode, radio_speaker))
        rows.append(self._build_relative_row(
            player, 0.0, drivers, positions, surface, car_lap, player, True,
            cols, sess_time, pit_mode, radio_speaker))
        for delta, idx in behind:
            rows.append(self._build_relative_row(
                idx, delta, drivers, positions, surface, car_lap, player, False,
                cols, sess_time, pit_mode, radio_speaker))
        if rcfg.get("center_on_player", True):
            for k in range(n_behind - len(behind)):
                rows.append(self._empty_row(f"rel_bot{k}"))

        self.relative_widget.set_data({
            "rows": rows,
            "slots": self._slot_values("relative", drivers, positions, player,
                                       car_lap, lap_est),
        })

    def _sof(self, drivers, section=None) -> str:
        irs = [d.get("IRating") for d in drivers.values() if d.get("IRating")]
        if not irs:
            return "--"
        return self._fmt_irating(sum(irs) / len(irs), section)

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
                                   lap_est, count, section)
            if val is not None:
                out[k] = val
        return out

    def _slot_value(self, key, drivers, positions, player, car_lap, lap_est,
                    count, section=None):
        ir = self.ir
        if key == "sof":
            return self._sof(drivers, section)
        if key == "class_sof":
            return self._class_sof(drivers, player, section)
        if key == "position":
            pos = positions[player] if positions and player is not None else None
            total = sum(1 for x in positions if x and x > 0) if positions else None
            return f"{pos}/{total}" if pos and total else "\u2014"
        if key == "class_position":
            return self._class_position(drivers, positions, player)
        if key == "session_time":
            rem = self._session_time_remain()
            return self._fmt_clock(rem) if rem is not None else "\u2014"
        if key == "race_time":
            return self._race_time_display()
        if key == "lap":
            lap = ir["Lap"] or (car_lap[player]
                                if car_lap and player is not None else None)
            total = self._sane_session_seconds(ir["SessionTimeTotal"])
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

    def _class_sof(self, drivers, player, section=None) -> str:
        cid = self._player_class(drivers, player)
        if cid is None:
            return self._sof(drivers, section)
        irs = [d.get("IRating") for d in drivers.values()
               if d.get("IRating") and d.get("CarClassID") == cid]
        if not irs:
            return "--"
        return self._fmt_irating(sum(irs) / len(irs), section)

    def _qualify_grid_positions(self) -> tuple[list[int] | None, list[int] | None]:
        """Starting grid from QualifyResultsInfo (CarIdxPosition is 0 pre-green)."""
        uid = 0
        try:
            uid = int(self.ir["SessionUniqueID"])
        except (TypeError, ValueError, KeyError):
            pass
        if self._grid_session_uid == uid and self._grid_positions_cache is not None:
            return self._grid_positions_cache, self._grid_class_positions_cache
        pos: list[int] | None = None
        cls: list[int] | None = None
        try:
            q = self.ir["QualifyResultsInfo"]
            results = q.get("Results") if isinstance(q, dict) else None
            if results:
                max_idx = 0
                entries: list[tuple[int, int | None, int | None]] = []
                for r in results:
                    if not isinstance(r, dict):
                        continue
                    idx = r.get("CarIdx")
                    if idx is None:
                        continue
                    idx = int(idx)
                    max_idx = max(max_idx, idx)
                    entries.append((idx, r.get("Position"), r.get("ClassPosition")))
                n = max(max_idx + 1, 64)
                pos = [0] * n
                cls = [0] * n
                for idx, p, cp in entries:
                    if p is not None and int(p) > 0:
                        pos[idx] = int(p)
                    if cp is not None and int(cp) > 0:
                        cls[idx] = int(cp)
                    elif p is not None and int(p) > 0:
                        cls[idx] = int(p)
                if not any(pos):
                    pos = None
                    cls = None
        except (TypeError, ValueError, KeyError):
            pass
        self._grid_session_uid = uid
        self._grid_positions_cache = pos
        self._grid_class_positions_cache = cls
        return pos, cls

    def _prefer_grid_positions(
        self, live: list | None, player: int | None, grid: list[int] | None,
    ) -> bool:
        if not grid or not any(grid):
            return False
        try:
            state = int(self.ir["SessionState"])
        except (TypeError, ValueError, KeyError):
            state = 4
        if state < 4:
            return True
        try:
            lc = self.ir["LapCompleted"]
            if isinstance(lc, int) and lc < 0:
                return True
        except (TypeError, ValueError, KeyError):
            pass
        if live and player is not None and 0 <= player < len(live):
            if not live[player] or int(live[player]) <= 0:
                return True
        valid_live = sum(
            1 for p in (live or []) if isinstance(p, int) and int(p) > 0
        )
        grid_count = sum(1 for p in grid if p > 0)
        if grid_count >= 2 and valid_live < max(2, grid_count // 2):
            return True
        return False

    def _resolve_positions(
        self, live: list | None, player: int | None,
    ) -> tuple[list | None, list[int] | None]:
        """Live race positions, falling back to qualify grid before green."""
        if self._demo_active:
            return live, None
        grid_pos, grid_class = self._qualify_grid_positions()
        if not self._prefer_grid_positions(live, player, grid_pos):
            return live, None
        n = max(len(live or []), len(grid_pos or []), 64)
        pos = list(grid_pos or [])
        if len(pos) < n:
            pos.extend([0] * (n - len(pos)))
        cls = list(grid_class) if grid_class else None
        if cls is not None and len(cls) < n:
            cls.extend([0] * (n - len(cls)))
        return pos, cls

    def _class_position(self, drivers, positions, player) -> str:
        cp = self._class_positions
        if cp is None:
            try:
                cp = self.ir["CarIdxClassPosition"]
            except (TypeError, ValueError, KeyError):
                cp = None
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

    def _resolve_demo_track_id(self) -> str:
        """Pick which track file to load in demo mode."""
        if self._demo_track_id:
            return self._demo_track_id
        if self._v2_authoring_track_id is not None:
            return str(self._v2_authoring_track_id)
        tracks_dir = self.tracks_dir or ""
        try:
            best_mtime = 0.0
            best_tid: str | None = None
            for name in os.listdir(tracks_dir):
                if not name.endswith(".json") or name.startswith("_"):
                    continue
                tid = name[:-5]
                path = os.path.join(tracks_dir, name)
                mtime = os.path.getmtime(path)
                if mtime > best_mtime:
                    best_mtime = mtime
                    best_tid = tid
            if best_tid is not None:
                return best_tid
        except OSError:
            pass
        return "_demo"

    def _load_demo_track(self) -> None:
        pts = None
        file_meta = None
        tid = self._resolve_demo_track_id()
        path = track_map.find_track_file(tid, self.tracks_dir)
        if path:
            try:
                pts, sf, corners, name, file_meta = track_map.load_track(path)
                self.map_widget.set_track(pts, sf, corners)
                if tid not in ("_demo", "demo"):
                    try:
                        self._v2_authoring_track_id = int(tid)
                    except (TypeError, ValueError):
                        pass
                    if name:
                        self._v2_authoring_name = name
            except Exception:
                pts = None
                file_meta = None
        if pts is None:
            pts = track_map.build_demo_path()
            self.map_widget.set_path(pts)
        # Prefer pit geometry from the track file (e.g. schematic import); only
        # synthesize a demo lane when the file has no pit polylines.
        if file_meta and file_meta.get("pit_path"):
            if not file_meta.get("pit_source"):
                file_meta = dict(file_meta, pit_source="schematic")
            self._apply_pit_meta(file_meta)
            n_in = len(file_meta.get("pit_in") or [])
            n_path = len(file_meta.get("pit_path") or [])
            n_out = len(file_meta.get("pit_out") or [])
            self.map_widget.flash_hint(
                f"Schematic — entry {n_in}, road {n_path}, merge {n_out}")
        else:
            meta = self._demo_pit_geometry(pts)
            if meta:
                self._apply_pit_meta(meta)

    def _demo_pit_geometry(self, pts):
        """Build a believable pit lane (lane + entry/exit blends + lap-% extents
        + speed) offset inward from the demo track, aligned with the lap stretch
        where the demo cars duck onto pit road (just after start/finish)."""
        n = len(pts) if pts else 0
        if n < 24:
            return None
        cx = sum(p[0] for p in pts) / n
        cy = sum(p[1] for p in pts) / n
        xs = [p[0] for p in pts]
        ys = [p[1] for p in pts]
        diag = math.hypot(max(xs) - min(xs), max(ys) - min(ys)) or 1.0
        off = 0.045 * diag

        def at(pct):
            return pts[int((pct % 1.0) * n) % n]

        def inward(p, frac):
            dx, dy = cx - p[0], cy - p[1]
            ln = math.hypot(dx, dy) or 1.0
            return (p[0] + dx / ln * off * frac, p[1] + dy / ln * off * frac)

        def span_pts(a, b, steps, f0, f1):
            s = (b - a) % 1.0
            out = []
            for k in range(steps + 1):
                t = k / steps
                out.append(inward(at(a + s * t), f0 + (f1 - f0) * t))
            return out

        # Entry blend (on track -> lane), the lane itself (wraps start/finish),
        # then the exit blend (lane -> back on track). The entry/exit extents are
        # shared with demo_data so the demo's "always pit" cars ride this exact
        # span; the lane sub-span (lane_lo..lane_hi) sits inside it.
        in_pct = demo_data.DEMO_PIT_IN_PCT
        out_pct = demo_data.DEMO_PIT_OUT_PCT
        lane_lo = demo_data.DEMO_PIT_LANE_LO
        lane_hi = demo_data.DEMO_PIT_LANE_HI
        return {
            "pit_source": "schematic",
            "pit_span": (lane_lo, lane_hi),
            "pit_speed": 22.0,  # ~50 mph / 80 km/h, shown as a static badge
            "pit_in": span_pts(in_pct, lane_lo, 14, 0.0, 1.0),
            "pit_path": span_pts(lane_lo, lane_hi, 44, 1.0, 1.0),
            "pit_out": span_pts(lane_hi, out_pct, 14, 1.0, 0.0),
            "pit_in_pct": in_pct,
            "pit_out_pct": out_pct,
            "pit_lane_speed_pct": 0.38,
        }

    def _ensure_track(self, player, lap_pct) -> None:
        """Load track from local file or cloud; prompt HTML import if missing."""
        if self.demo or self._track_loaded:
            return

        if not self._track_file_checked:
            weekend = self.ir["WeekendInfo"]
            if not weekend:
                return
            self._track_file_checked = True
            self._track_id = weekend.get("TrackID")
            self._learn_name = (weekend.get("TrackDisplayName")
                                or weekend.get("TrackName") or "")
            self._track_turns = _coerce_int(weekend.get("TrackNumTurns"))
            self.map_widget.set_num_turns(self._track_turns)
            self._track_is_oval = constants.is_oval_track(weekend)
            track_file = track_map.find_track_file(
                self._track_id, self.tracks_dir)
            if not track_file and config.cloud_tracks() \
                    and self._track_id is not None \
                    and self._track_id not in self._remote_tried:
                self._remote_tried.add(self._track_id)
                self._track_sync.fetch_async(self._track_id)
            if track_file:
                try:
                    pts, sf, corners, _, meta = track_map.load_track(
                        track_file)
                    self.map_widget.set_track(pts, sf, corners)
                    saved_turns = _coerce_int(meta.get("num_turns"))
                    self.map_widget.set_num_turns(
                        self._track_turns or saved_turns)
                    if saved_turns and not self._track_turns:
                        self._track_turns = saved_turns
                    self._apply_pit_meta(meta)
                    self._track_loaded = True
                    track_store.touch(self.tracks_dir, self._track_id)
                    self._refresh_settings_authoring()
                    return
                except Exception:
                    pass

        if not self._no_track_hint:
            self._no_track_hint = True
            self.map_widget.flash_hint(
                "No track file — import HTML in Settings \u2192 Track Scan")

    def _on_tracks_synced(self, n) -> None:
        """Startup cache refresh finished; if the map we're showing was one of
        the tracks that changed, reload it so the live view is up to date."""
        if self.demo or not n or self._track_id is None:
            return
        path = track_map.find_track_file(self._track_id, self.tracks_dir)
        if not path:
            return
        try:
            pts, sf, corners, _, meta = track_map.load_track(path)
        except Exception:
            return
        self.map_widget.set_track(pts, sf, corners)
        self.map_widget.set_num_turns(self._track_turns or meta.get("num_turns"))
        self._apply_pit_meta(meta)
        self._track_loaded = True
        self._refresh_settings_authoring()

    def _on_remote_track(self, track_id, doc) -> None:
        """A shared track map arrived from the cloud (off the GUI thread)."""
        if not doc or not track_store.same_track_id(track_id, self._track_id):
            return
        if self._track_loaded:
            return
        try:
            path = track_store.write_local(self.tracks_dir, doc)
            if not path:
                return
            pts, sf, corners, _, meta = track_map.load_track(path)
            self.map_widget.set_track(pts, sf, corners)
            saved_turns = _coerce_int(meta.get("num_turns"))
            self.map_widget.set_num_turns(self._track_turns or saved_turns)
            if saved_turns and not self._track_turns:
                self._track_turns = saved_turns
            self._apply_pit_meta(meta)
            self._track_loaded = True
            track_store.enforce_cache_limit(
                self.tracks_dir, protect=[self._track_id])
            self._refresh_settings_authoring()
        except Exception:
            pass

    def _reset_pit_state(self) -> None:
        """Clear pit-route runtime state (latches, not saved geometry)."""
        self._pit_s0 = self._pit_t0 = None
        self._player_on_route = False
        self._player_route_ticks = 0
        self._pit_route_latch.clear()
        self._pit_prev_on.clear()
        self._pit_exit_latch.clear()

    def _seed_pit_latches(self, lap_pct, on_pit_arr, player) -> None:
        """Latch cars already on the pit route when a track loads mid-session."""
        if not is_schematic_pit_source(self._pit_source):
            return
        route = self._route_interval()
        if not route:
            return
        lo, hi = route
        for idx, pct in enumerate(lap_pct or []):
            if pct is None or pct < 0:
                continue
            on_pit = (bool(on_pit_arr[idx])
                      if on_pit_arr and idx < len(on_pit_arr) else False)
            if on_pit or self._pct_in_interval(pct, lo, hi):
                self._pit_route_latch[idx] = True
        if (player is not None and on_pit_arr
                and player < len(on_pit_arr) and on_pit_arr[player]):
            self._player_on_route = True
            self._player_route_ticks = PIT_COMMIT_HOLD

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

    def _apply_pit_meta(self, meta) -> None:
        """Push loaded pit geometry (span / speed / lane / blend lines / route
        extent) from a track file into both our state and the map widget. A meta
        with no pit data fully clears any previous pit lane (e.g. when leaving
        the demo or switching to a track that hasn't been scanned)."""
        self._reset_pit_state()
        self._pit_latch_seed_pending = True
        self._pit_source = (meta.get("pit_source") or "").strip().lower()
        self.map_widget.set_pit_source(self._pit_source or None)
        pct = meta.get("pit_lane_speed_pct")
        self._pit_lane_speed_pct = float(pct) if pct is not None else 1.0
        self.map_widget.pit_lane_speed_pct = self._pit_lane_speed_pct
        if meta.get("pit_speed"):
            self._pit_speed_ms = float(meta["pit_speed"])

        pit_path = meta.get("pit_path")
        pit_span = meta.get("pit_span")
        if not pit_span and pit_path and len(pit_path) >= 2:
            if is_schematic_pit_source(self._pit_source) and self.map_widget.path:
                from tools.schematic_to_track import _pit_span_on_loop

                loop = [(p[0], p[1]) for p in self.map_widget.path]
                lo, hi = _pit_span_on_loop(loop, pit_path)
                pit_span = (round(lo, 5), round(hi, 5))
                meta = dict(meta, pit_span=pit_span)

        if not meta.get("pit_span"):
            if pit_path and len(pit_path) >= 2 and is_schematic_pit_source(self._pit_source):
                self._pit_span = None
                self._pit_speed_ms = meta.get("pit_speed", 0.0)
                self._pit_path = pit_path
                self._pit_in = meta.get("pit_in")
                self._pit_out = meta.get("pit_out")
                self._pit_in_pct = meta.get("pit_in_pct")
                self._pit_out_pct = meta.get("pit_out_pct")
                self.map_widget.set_pit_path(pit_path)
                self.map_widget.set_pit_blends(self._pit_in, self._pit_out)
                self.map_widget.set_pit_route_pct(
                    self._pit_in_pct, self._pit_out_pct)
                if self._pit_speed_ms > 0:
                    self.map_widget.pit_speed_ms = self._pit_speed_ms
                self.map_widget.update()
                return
            self._pit_span = None
            self._pit_path = self._pit_in = self._pit_out = None
            self._pit_in_pct = self._pit_out_pct = None
            self.map_widget.clear_pit()
            if self._pit_speed_ms > 0:
                self.map_widget.pit_speed_ms = self._pit_speed_ms
                self.map_widget.update()
            return
        self._pit_span = meta["pit_span"]
        self._pit_speed_ms = meta.get("pit_speed", 0.0)
        self.map_widget.set_pit(self._pit_span, self._pit_speed_ms)
        self._pit_path = meta.get("pit_path")
        self.map_widget.set_pit_path(self._pit_path or [])
        self._pit_in = meta.get("pit_in")
        self._pit_out = meta.get("pit_out")
        self.map_widget.set_pit_blends(self._pit_in, self._pit_out)
        self._pit_in_pct = meta.get("pit_in_pct")
        self._pit_out_pct = meta.get("pit_out_pct")
        self.map_widget.set_pit_route_pct(self._pit_in_pct, self._pit_out_pct)
        self.map_widget.update()

    def _update_map(self, player, lap_pct, surface, drivers,
                    positions=None, car_lap=None,
                    radio_speaker=None) -> None:
        if player is None or not lap_pct or not surface:
            return

        # Resolve the player's model-space position once (GPS or dead reckoning)
        # so the learner and the pit capture share a single, consistent frame.
        self._update_player_pos(lap_pct[player])
        self._ensure_track(player, lap_pct)
        # In demo mode the pit lane is synthesized once (see _load_demo_track);
        # skip live learning so it isn't overwritten by the demo's fake pit dips.
        if config.CFG["map"].get("show_pit", True) and not self.demo:
            self._update_pit_route(player, lap_pct)
        if config.CFG["map"].get("show_wind", True):
            self.map_widget.set_wind(self.ir["WindDir"], self.ir["WindVel"])
        else:
            self.map_widget.set_wind(None, 0.0)

        on_pit_arr = self.ir["CarIdxOnPitRoad"]
        if self._pit_latch_seed_pending:
            self._seed_pit_latches(lap_pct, on_pit_arr, player)
            self._pit_latch_seed_pending = False
        pit_surf = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
        use_pos = config.CFG["map"].get("car_label", "number") == "position"
        if use_pos and positions is None:
            positions, _ = self._resolve_positions(
                self.ir["CarIdxPosition"], player)
        route = self._route_interval()  # (lo, hi) lap-% extent, or None
        blends_on = config.CFG["map"].get("show_pit_blends", True)
        if self.demo and is_schematic_pit_source(self._pit_source):
            self._update_schematic_pit_latches(lap_pct, on_pit_arr)
        player_color = config.CFG["map"]["colors"]["player"]
        cars = []
        for idx, pct in enumerate(lap_pct):
            if pct is None or pct < 0.0 or pct > 1.0:
                continue
            is_player = idx == player
            on_pit = (bool(on_pit_arr[idx]) if on_pit_arr and idx < len(on_pit_arr)
                      else surface[idx] in pit_surf)
            approaching = (surface is not None and idx < len(surface)
                           and surface[idx] == oc.TRK_APPROACHING_PITS)
            # Show cars that are on track or on pit road; skip garage/off-world.
            if surface[idx] != oc.TRK_ON_TRACK and not on_pit and not is_player:
                continue
            on_route = self._car_on_route(idx, pct, on_pit, is_player, route,
                                          blends_on, approaching=approaching)
            d = drivers.get(idx)
            if use_pos and positions and idx < len(positions) and positions[idx]:
                num = str(positions[idx])
            else:
                num = str(d.get("CarNumber", "?")) if d else "?"
            if is_player:
                color = player_color
            else:
                color = self._map_car_color(
                    idx, player, car_lap, lap_pct)
            speaking = radio_speaker is not None and idx == radio_speaker
            cars.append((idx, pct, num, color, is_player, on_route, on_pit,
                           speaking))
        self.map_widget.set_schematic_exit_pcts(self._pit_exit_latch)
        self.map_widget.set_cars(cars)

    def _route_interval(self):
        """The pit route's lap-% extent (divergence -> rejoin), preferring the
        learned blend extents and falling back to the lane span. None if unknown.
        """
        lo, hi = self._pit_in_pct, self._pit_out_pct
        if lo is None or hi is None:
            if self._pit_span is None:
                return None
            lo, hi = self._pit_span
        return (lo, hi)

    @staticmethod
    def _pct_in_interval(pct, lo, hi) -> bool:
        """Whether pct lies within the circular interval [lo, hi] (driving dir)."""
        span = (hi - lo) % 1.0
        if span <= 1e-6:
            return False
        return ((pct - lo) % 1.0) <= span

    def _is_demo_pit_car(self, idx: int) -> bool:
        if not self.demo:
            return False
        pit_cars = getattr(self.ir, "_pit_cars", None)
        return pit_cars is not None and idx in pit_cars

    def _pit_route_phases(self, pct: float):
        """(entry, lane_span, exit) lap-% intervals for schematic pit phases."""
        pit_in = self._pit_in_pct
        pit_out = self._pit_out_pct
        lane = self._pit_span
        lane_lo = lane[0] if lane else None
        lane_hi = lane[1] if lane else None
        in_entry = (pit_in is not None and lane_lo is not None
                    and self._pct_in_interval(pct, pit_in, lane_lo))
        in_lane_span = (lane_lo is not None and lane_hi is not None
                        and self._pct_in_interval(pct, lane_lo, lane_hi))
        in_exit = (lane_hi is not None and pit_out is not None
                   and self._pct_in_interval(pct, lane_hi, pit_out))
        return in_entry, in_lane_span, in_exit

    def _close_on_track(self, player, idx, lap_pct) -> bool:
        """Whether a car is within the map's lap-distance proximity window."""
        if (player is None or lap_pct is None or idx >= len(lap_pct)
                or player >= len(lap_pct)):
            return False
        me = lap_pct[player]
        them = lap_pct[idx]
        if me is None or them is None or me < 0 or them < 0:
            return False
        delta = them - me
        if delta > 0.5:
            delta -= 1.0
        elif delta < -0.5:
            delta += 1.0
        prox = config.CFG["map"].get("lap_proximity_pct", 0.04)
        return abs(delta) <= prox

    def _map_car_color(self, idx, player, car_lap, lap_pct) -> str:
        """Map dot color: default competitor, blue if lapped, red if lapping."""
        colors = config.CFG["map"]["colors"]
        base = colors.get("competitor", colors.get("player", "#3aa0ff"))
        lapping, lap_ahead = self._lap_tint(idx, player, car_lap, False)
        if not lapping:
            return base
        if not lap_ahead:
            return colors.get("lapped", "#4a8cff")
        me = self._laps_done(player, car_lap)
        them = self._laps_done(idx, car_lap)
        if me is None or them is None:
            return base
        diff = them - me
        if diff >= 1.5:
            return colors.get("lapping", "#ff5050")
        if self._close_on_track(player, idx, lap_pct):
            return colors.get("lapping", "#ff5050")
        return base

    def _car_on_route(self, idx, pct, on_pit, is_player, route,
                      blends_on=True, approaching=False) -> bool:
        """Decide whether a car should be drawn on the pit route this tick.

        On schematic tracks, route membership is phased (entry / lane / exit)
        and gated by pit telemetry so cars on the racing line are not drawn in
        the pits just because lap % falls inside a wrapping pit_in..pit_out arc.
        """
        if not blends_on:
            return on_pit
        schematic = is_schematic_pit_source(self._pit_source)
        if not schematic:
            if is_player:
                return on_pit or self._player_on_route
            latched = self._pit_route_latch.get(idx, False)
            if on_pit:
                latched = True
            elif latched and route is not None:
                if not self._pct_in_interval(pct, route[0], route[1]):
                    latched = False
            elif latched and route is None:
                latched = False
            self._pit_route_latch[idx] = latched
            return on_pit or latched

        in_entry, _, in_exit = self._pit_route_phases(pct)

        if is_player:
            if on_pit or self._player_on_route:
                return True
            return in_entry and approaching

        latched = self._pit_route_latch.get(idx, False)
        if on_pit:
            self._pit_route_latch[idx] = True
            return True
        if self.demo and self._is_demo_pit_car(idx) and in_entry:
            self._pit_route_latch[idx] = True
            return True
        if latched and in_exit:
            return True
        if latched:
            self._pit_route_latch[idx] = False
        return False

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

    def _update_player_pos(self, pct) -> None:
        """Compute the player's model-space position for this tick.

        Prefers real GPS (Lat/Lon, equirectangular-projected); when the sim
        doesn't expose it, falls back to dead reckoning from speed + heading.
        """
        lat, lon = self.ir["Lat"], self.ir["Lon"]
        if lat is not None and lon is not None and (lat != 0.0 or lon != 0.0):
            self._player_pos = (math.radians(lon) * math.cos(math.radians(lat)),
                                -math.radians(lat))
            self._player_pos_wrapped = False
            self._player_pos_gps = True
        else:
            xy, wrapped = self._dead_reckon(pct)
            self._player_pos = xy
            self._player_pos_wrapped = wrapped
            self._player_pos_gps = False

    def _update_schematic_pit_latches(self, lap_pct, on_pit_arr) -> None:
        """Record per-car exit lap % when leaving pit road (schematic tracks)."""
        for idx, pct in enumerate(lap_pct):
            if pct is None or pct < 0.0 or pct > 1.0:
                continue
            on = (bool(on_pit_arr[idx]) if on_pit_arr and idx < len(on_pit_arr)
                  else False)
            if self._pit_prev_on.get(idx) and not on:
                self._pit_exit_latch[idx] = float(pct)
            self._pit_prev_on[idx] = on

    def _update_pit_route(self, player, lap_pct) -> None:
        """Update schematic pit route latches and learn pit speed from telemetry."""
        if player is None or not lap_pct:
            return
        on = self.ir["OnPitRoad"]
        if on is None:
            arr = self.ir["CarIdxOnPitRoad"]
            on = bool(arr[player]) if arr and player < len(arr) else False
        on = bool(on)
        pct = lap_pct[player]
        speed = self.ir["Speed"]
        on_pit_arr = self.ir["CarIdxOnPitRoad"]
        self._update_schematic_pit_latches(lap_pct, on_pit_arr)
        route = self._route_interval()
        valid_pct = pct if (pct is not None and 0.0 <= pct <= 1.0) else None
        self._player_route_ticks = self._player_route_ticks + 1 if on else 0
        if self._player_route_ticks >= PIT_COMMIT_HOLD:
            self._player_on_route = True
        elif self._player_on_route and valid_pct is not None:
            end = self._pit_out_pct if self._pit_out_pct is not None else (
                route[1] if route else None)
            if end is None or not self._pct_in_interval(
                    valid_pct, route[0] if route else 0.0, end):
                self._player_on_route = False
        if on:
            self._learn_pit_speed(speed)
        else:
            self._pit_s0 = self._pit_t0 = None
        self.map_widget.set_player_xy(None)

    # iRacing SessionFlags bitfield bits (irsdk_Flags).
    _FLAG_CHECKERED = 0x00000001           # session finished
    _FLAG_WHITE = 0x00000002               # white flag (final lap)
    _FLAG_GREEN = 0x00000004 | 0x00000400   # green + green-held
    _FLAG_YELLOW = 0x00000008 | 0x00000100 | 0x00004000 | 0x00008000
    _FLAG_YELLOW_BASE = 0x00000008
    _FLAG_YELLOW_WAVING = 0x00000100
    _FLAG_ONE_LAP_GREEN = 0x00000200
    _FLAG_GREEN_HELD = 0x00000400
    _FLAG_TEN_TO_GO = 0x00000800
    _FLAG_FIVE_TO_GO = 0x00001000
    _FLAG_CAUTION = 0x00004000
    _FLAG_CAUTION_WAVING = 0x00008000
    _FLAG_RED = 0x00000010                  # session stopped
    _FLAG_BLUE = 0x00000020                 # faster car behind, let it by
    _FLAG_DEBRIS = 0x00000040               # debris on track
    _FLAG_CROSSED = 0x00000080              # crossed flag = race halfway
    _FLAG_BLACK = 0x00010000                # black flag (penalty)
    _FLAG_DQ = 0x00020000                   # disqualified
    _FLAG_FURLED = 0x00080000               # furled/rolled black = warning
    _FLAG_REPAIR = 0x00100000               # meatball (must pit to repair)
    _FLAG_START_READY = 0x20000000
    _FLAG_START_SET = 0x40000000
    _FLAG_START_GO = 0x80000000

    def _session_flag_bundle(self) -> tuple[str | None, str | None]:
        """Resolved flag name plus a short contextual hint for the UI."""
        flag = self._session_flag()
        ctx = self._flag_context(flag, getattr(self, "_last_session_flags", 0))
        return flag, ctx

    def _flag_context(self, flag: str | None, sf: int) -> str | None:
        """Human-readable detail for the current flag state."""
        if flag is None:
            if sf & self._FLAG_START_GO:
                return "Green light — go"
            if sf & self._FLAG_START_SET:
                return "Start lights set"
            if sf & self._FLAG_START_READY:
                return "Get ready — start imminent"
            return None

        if flag == "yellow":
            if sf & self._FLAG_ONE_LAP_GREEN:
                return "1 lap to green"
            if sf & self._FLAG_TEN_TO_GO:
                return "10 to go"
            if sf & self._FLAG_FIVE_TO_GO:
                return "5 to go"
            if sf & self._FLAG_CAUTION_WAVING:
                return "Caution waving — pits closed"
            if sf & self._FLAG_CAUTION:
                return "Full course caution — hold position"
            if sf & self._FLAG_YELLOW_WAVING:
                return "Local yellow — slow in sector"
            if sf & self._FLAG_YELLOW_BASE:
                return "Local yellow — slow down"
            return "Slow down — no passing"

        if flag == "green":
            if sf & self._FLAG_GREEN_HELD:
                return "Green held — stay in formation"
            return "Track clear — racing resumes"

        if flag == "red":
            remain = self._session_time_remain()
            if remain is not None and remain > 0:
                return f"Session stopped — {self._fmt_clock(remain)} left"
            return "Session stopped — stand by"

        if flag == "white":
            try:
                laps_rem = self.ir["SessionLapsRemainEx"]
            except Exception:
                laps_rem = None
            if isinstance(laps_rem, int) and laps_rem == 1:
                return "1 lap remaining"
            try:
                total = self.ir["SessionLapsTotal"]
                lap = self.ir["Lap"]
            except Exception:
                total = lap = None
            if isinstance(total, (int, float)) and isinstance(lap, int) and total > 0:
                return f"Lap {lap} of {int(total)} — finish this lap"
            return "Final lap — finish the race"

        if flag == "blue":
            return "Faster car approaching — let them pass"

        if flag == "black":
            return "Report to the pits — penalty"

        if flag == "meatball":
            return "Mandatory pit — repairs required"

        if flag == "furled":
            return "Warning — next infraction is a penalty"

        if flag == "dq":
            return "Disqualified — exit the track"

        if flag == "debris":
            return "Debris on track — reduce speed"

        if flag == "crossed":
            try:
                total = self.ir["SessionLapsTotal"]
                lap = self.ir["Lap"]
            except Exception:
                total = lap = None
            if isinstance(total, (int, float)) and isinstance(lap, int) and total > 0:
                rem = max(0, int(total) - lap)
                return f"Halfway — {rem} laps to go"
            return "Halfway point"

        if flag == "checkered":
            return "Session complete"

        return None

    def _update_context(self) -> None:
        """Pick the 'garage' or 'race' profile from telemetry (iRacing's
        IsInGarage). Switching only happens on a change, which recomputes the
        live config and re-applies widget visibility."""
        try:
            in_garage = bool(self.ir["IsInGarage"])
        except Exception:
            in_garage = False
        ctx = "garage" if in_garage else "race"
        if ctx != config.active_context():
            config.set_context(ctx)
        self._maybe_auto_switch_preset()

    def _maybe_auto_switch_preset(self) -> None:
        """Switch presets to match the session: league, then car, then default.

        Re-evaluated whenever the car or league changes, so leaving a bound car
        or league falls back through the chain (and to the default if enabled).
        """
        if self.demo or not config.auto_switch_enabled():
            return
        # Make sure we know the player's car even when no driver-list widget is
        # enabled this tick (the call is internally throttled, so it's cheap).
        if not self._driver_cache:
            self._drivers()
        car, _name = self.current_car()
        league_id = self._session_league_id()
        if car == self._last_car_path and league_id == self._last_league_id:
            return
        self._last_car_path = car
        self._last_league_id = league_id
        target = config.preset_for_session(league_id, car)
        if target and target != config.active_preset():
            config.set_active_preset(target)

    def _session_flag(self):
        """Current flag to show: 'checkered', 'red', 'dq', 'black', 'meatball',
        'furled', 'blue', 'white', 'yellow', 'debris', 'crossed',
        'green' (briefly) or None. Personal/penalty flags take priority over the
        caution; green (resume) only flashes briefly, and white shows while you
        approach the final lap and clears as you cross the line onto it."""
        raw = self.ir["SessionFlags"]
        try:
            sf = int(raw) & 0xFFFFFFFF
        except (TypeError, ValueError):
            sf = 0
        self._last_session_flags = sf
        now = self.ir["SessionTime"]
        if not isinstance(now, (int, float)):
            now = time.time()

        yellow = bool(sf & self._FLAG_YELLOW)
        # Leaving a yellow (or an explicit green wave) opens the green window.
        if self._flag_was_yellow and not yellow:
            secs = float(config.CFG["dash"].get("flag_green_seconds", 3.0) or 3.0)
            self._green_until = now + secs
        self._flag_was_yellow = yellow

        # White flag waves as you approach the line to start the final lap. Note
        # the lap it first appears on, then hide it once you cross that line
        # (your lap count ticks up onto the final lap).
        white = bool(sf & self._FLAG_WHITE)
        lap = self.ir["Lap"]
        if white and not self._flag_was_white:
            self._white_start_lap = lap
        if not white:
            self._white_start_lap = None
        self._flag_was_white = white
        white_show = white and (
            self._white_start_lap is None or not isinstance(lap, int)
            or lap <= self._white_start_lap)

        # Checkered (session over) trumps everything else, then a red (session
        # stopped), then personal penalty/black flags directed at you.
        if sf & self._FLAG_CHECKERED:
            return "checkered"
        if sf & self._FLAG_RED:
            return "red"
        if sf & self._FLAG_DQ:
            return "dq"
        if sf & self._FLAG_BLACK:
            return "black"
        if sf & self._FLAG_REPAIR:
            return "meatball"
        if sf & self._FLAG_FURLED:
            return "furled"
        # Blue (let a faster car by) is directed at you, so above the caution.
        if sf & self._FLAG_BLUE:
            return "blue"
        # White flag only while approaching the final lap (hidden once crossed).
        if white_show:
            return "white"
        if yellow:
            return "yellow"
        if sf & self._FLAG_DEBRIS:
            return "debris"
        if sf & self._FLAG_CROSSED:
            return "crossed"
        if now < self._green_until:
            return "green"
        return None

    def _update_dash(self, player, positions, car_lap) -> None:
        """Feed the dash a full telemetry snapshot."""
        total = self.ir["SessionLapsTotal"]
        if not isinstance(total, (int, float)) or total <= 0 or total > 2000:
            total = None
        # iRacing's Clutch is 1.0 when fully engaged (pedal up); show pedal travel.
        clutch_raw = self.ir["Clutch"]
        clutch = (1.0 - clutch_raw) if isinstance(clutch_raw, (int, float)) else None
        abs_active = self.ir["BrakeABSactive"]
        show_flags = config.CFG["dash"].get("show_flags", True)
        flag = ctx = None
        if show_flags:
            flag, ctx = self._session_flag_bundle()
        irating = irating_delta = None
        car_number = ""
        drv = self._drivers().get(player) if player is not None else None
        if drv:
            if self._dash_uses_irating() or self._dash_needs_irating_projection():
                ir = drv.get("IRating")
                if isinstance(ir, (int, float)) and ir > 0:
                    irating = int(round(ir))
            car_number = str(drv.get("CarNumber", "")).strip()
        if (self._dash_needs_irating_projection()
                and (self._demo_active or self._session_allows_irating_projection())
                and player is not None):
            irating_delta = self._irating_deltas.get(player)
        self.dash_widget.set_data({
            "gear": self.ir["Gear"],
            "rpm": self.ir["RPM"],
            "redline": self._car_info.get("redline"),
            "sl_first": self._car_info.get("sl_first"),
            "sl_last": self._car_info.get("sl_last"),
            "sl_shift": self._car_info.get("sl_shift"),
            "sl_blink": self._car_info.get("sl_blink"),
            "top_gear": self._car_info.get("gears"),
            "throttle": self.ir["Throttle"],
            "brake": self.ir["Brake"],
            "clutch": clutch,
            "abs_active": bool(abs_active),
            "speed_ms": self.ir["Speed"],
            "position": (positions[player] if positions and player is not None
                         else None),
            "car_number": car_number,
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
            "irating": irating,
            "irating_delta": irating_delta,
            "flag": flag,
            "flag_context": ctx,
        })

    def _update_inputs(self) -> None:
        """Feed the input-telemetry trace (throttle/brake/clutch/steer + gear)."""
        # iRacing's Clutch is 1.0 when fully engaged (pedal up); show pedal travel.
        clutch_raw = self.ir["Clutch"]
        clutch = (1.0 - clutch_raw) if isinstance(clutch_raw, (int, float)) else None
        self.inputs_widget.set_data({
            "throttle": self.ir["Throttle"],
            "brake": self.ir["Brake"],
            "clutch": clutch,
            "steer": self._steer_norm(),
            "abs_active": bool(self.ir["BrakeABSactive"]),
            "gear": self.ir["Gear"],
            "speed_ms": self.ir["Speed"],
        })

    def _steer_norm(self):
        """Steering angle normalized to 0..1 (0.5 centered), using the car's
        max lock when reported, else a sane fallback so the trace stays on-scale."""
        angle = self.ir["SteeringWheelAngle"]
        if not isinstance(angle, (int, float)):
            return None
        amax = self.ir["SteeringWheelAngleMax"]
        if not isinstance(amax, (int, float)) or amax <= 0.1:
            amax = 5.0  # radians (~286 deg of lock) -- typical road-car fallback
        return 0.5 + 0.5 * max(-1.0, min(1.0, angle / amax))

    # --- delta bar / flags / pit service / sector timing -------------------

    def _update_delta_bar(self) -> None:
        """Feed the live delta against the configured reference lap."""
        mode = config.CFG["delta_bar"].get("mode", "session_best")
        key = {"session_best": "LapDeltaToSessionBest",
               "best_lap": "LapDeltaToBestLap",
               "optimal": "LapDeltaToOptimalLap"}.get(mode, "LapDeltaToSessionBest")
        delta = self.ir[key]
        self.delta_bar_widget.set_data(
            {"delta": delta if isinstance(delta, (int, float)) else None})

    def _update_flags(self) -> None:
        """Feed the standalone flag banner (hidden entirely when no flag flies,
        except in layout-edit mode where a placeholder stays visible)."""
        flag, ctx = self._session_flag_bundle()
        self.flags_widget.set_data({"flag": flag, "flag_context": ctx,
                                    "edit": self.edit_mode_enabled()})

    def _update_sectors(self, player, lap_pct) -> None:
        """Feed live sector splits derived from the player's lap distance."""
        self._sector_timer.set_boundaries(self._sector_starts())
        pct = None
        if (isinstance(lap_pct, (list, tuple)) and isinstance(player, int)
                and 0 <= player < len(lap_pct)):
            pct = lap_pct[player]
        self._sector_timer.update(pct, self.ir["LapCurrentLapTime"],
                                  self.ir["LapLastLapTime"])
        self.sector_widget.set_data(self._sector_timer.snapshot(
            self.ir["LapCurrentLapTime"], self.ir["LapLastLapTime"],
            self.ir["LapBestLapTime"]))

    def _sector_starts(self):
        """Sector start percentages from the session, else N equal divisions.

        The session layout is static, so this expensive YAML read is cached and
        only refreshed on the ~1 s throttle counter.
        """
        if (self._sector_starts_cache is not None
                and self._session_info_counter % 60 != 0):
            return self._sector_starts_cache
        info = self.ir["SplitTimeInfo"]
        starts = []
        secs = info.get("Sectors") if isinstance(info, dict) else None
        if isinstance(secs, list):
            for s in secs:
                pct = s.get("SectorStartPct") if isinstance(s, dict) else None
                if isinstance(pct, (int, float)):
                    starts.append(float(pct))
        if not starts:
            n = max(1, int(config.CFG["sector_timing"].get("sectors", 3) or 3))
            starts = [i / n for i in range(n)]
        self._sector_starts_cache = starts
        return starts

    def _update_lap_compare(self, player, lap_pct) -> None:
        """Record the player's lap and feed the corner-by-corner comparison."""
        if not self.demo:  # demo keeps its seeded benchmark; no persistence
            self._lap_engine.set_identity(self._lap_compare_key(),
                                          self._car_info.get("redline"))
        pct = None
        if (isinstance(lap_pct, (list, tuple)) and isinstance(player, int)
                and 0 <= player < len(lap_pct)):
            pct = lap_pct[player]
        surf = self.ir["PlayerTrackSurface"]
        off_track = surf == oc.TRK_OFF_TRACK
        self._lap_engine.update(
            pct,
            on_pit=bool(self.ir["OnPitRoad"]),
            throttle=self.ir["Throttle"],
            brake=self.ir["Brake"],
            steer=self._steer_norm(),
            speed=self.ir["Speed"],
            laptime=self.ir["LapCurrentLapTime"],
            last_lap_time=self.ir["LapLastLapTime"],
            lat=self.ir["LatAccel"],
            lon=self.ir["LongAccel"],
            gear=self.ir["Gear"],
            rpm=self.ir["RPM"],
            off_track=off_track,
            incidents=self.ir["PlayerCarMyIncidentCount"],
            corner_pcts=self._corner_pcts(),
            track_len=self._track_length_m(),
        )
        self.lap_compare_widget.set_data(self._lap_engine.snapshot())

    def _lap_compare_key(self):
        """A stable "<track>::<car>" key so the benchmark persists per combo.

        Track + car don't change mid-stint, so the result is cached and only
        recomputed on the ~1 s throttle counter (the WeekendInfo read is costly).
        """
        if self._lc_key is not None and self._session_info_counter % 60 != 0:
            return self._lc_key
        wk = self.ir["WeekendInfo"]
        track = ""
        if isinstance(wk, dict):
            track = str(wk.get("TrackID") or wk.get("TrackName") or "")
        car = ""
        drv = self._driver_cache.get(self.ir["PlayerCarIdx"]) if self._driver_cache else None
        if isinstance(drv, dict):
            car = str(drv.get("CarPath") or drv.get("CarID") or "")
        self._lc_key = f"{track}::{car}" if (track or car) else None
        return self._lc_key

    def _corner_pcts(self):
        """Corner (pct, label) pairs from the map, when a layout is known."""
        w = getattr(self, "map_widget", None)
        if w is None:
            return []
        return getattr(w, "corners", None) or getattr(w, "_auto_corners", None) or []

    def _track_length_m(self) -> float:
        """Track length in meters, parsed once from the session info."""
        if self._track_len_m:
            return self._track_len_m
        wk = self.ir["WeekendInfo"]
        s = wk.get("TrackLength") if isinstance(wk, dict) else None
        if isinstance(s, str):
            parts = s.split()
            try:
                val = float(parts[0])
            except (ValueError, IndexError):
                return 0.0
            unit = parts[1].lower() if len(parts) > 1 else "km"
            self._track_len_m = val * (1609.344 if unit.startswith("mi") else 1000.0)
        return self._track_len_m

    def _fuel_laps(self):
        """Estimate laps of fuel remaining from level, burn rate and lap time."""
        fuel = self.ir["FuelLevel"]
        per_hr = self.ir["FuelUsePerHour"]
        est_lap = self._car_info.get("est_lap") or 0.0
        if not isinstance(fuel, (int, float)) or not per_hr or est_lap <= 0:
            return None
        per_lap = per_hr * (est_lap / 3600.0)
        return fuel / per_lap if per_lap > 0 else None

    # --- fuel calculator ----------------------------------------------------

    def _fuel_capacity(self, level):
        """Usable tank capacity (L), from the session YAML or level/level-pct."""
        cap = self._car_info.get("fuel_max") or 0.0
        if cap > 0:
            return cap
        pct = self.ir["FuelLevelPct"]
        if isinstance(level, (int, float)) and isinstance(pct, (int, float)) \
                and pct > 0.01:
            return level / pct
        return None

    def _avg_lap_secs(self):
        """Average recent lap time (s): logged laps if any, else the est lap."""
        good = [l["secs"] for l in self._ll_laps if l.get("secs", 0) > 0]
        if good:
            return sum(good[:10]) / len(good[:10])
        est = self._car_info.get("est_lap") or 0.0
        return est if est > 0 else None

    def _race_remaining(self, lap_avg):
        """(laps_remaining, time_remaining) for the current session, or Nones."""
        laps = self.ir["SessionLapsRemainEx"]
        if not isinstance(laps, (int, float)) or laps < 0 or laps > 32000:
            laps = self.ir["SessionLapsRemain"]
        if not isinstance(laps, (int, float)) or laps < 0 or laps > 32000:
            laps = None
        t = self.ir["SessionTimeRemain"]
        t = self._sane_session_seconds(t)
        if laps is None and t is not None and lap_avg:
            laps = t / lap_avg
        if t is None and laps is not None and lap_avg:
            t = laps * lap_avg
        return laps, t

    def _update_fuel_calc(self) -> None:
        fuel = self.ir["FuelLevel"]
        cap = self._fuel_capacity(fuel)
        try:
            lap = int(self.ir["Lap"])
        except (TypeError, ValueError):
            lap = None

        # Record fuel burned on each completed lap.
        if lap is not None and isinstance(fuel, (int, float)):
            if self._fc_prev_lap is None:
                self._fc_prev_lap = lap
                self._fc_lap_start_fuel = fuel
            elif lap > self._fc_prev_lap:
                used = (self._fc_lap_start_fuel or 0.0) - fuel
                if 0.0 < used < (cap or 1e9):
                    self._fc_use.insert(0, float(used))
                    n = int(config.CFG["fuel_calc"].get("history_laps", 10) or 10)
                    del self._fc_use[max(1, n):]
                self._fc_prev_lap = lap
                self._fc_lap_start_fuel = fuel

        lap_avg = self._avg_lap_secs()
        laps_rem, time_rem = self._race_remaining(lap_avg)

        # Usage scenarios. Fall back to the live burn estimate before we have
        # any per-lap samples.
        if self._fc_use:
            u_avg = sum(self._fc_use) / len(self._fc_use)
            u_max = max(self._fc_use)
            u_min = min(self._fc_use)
        else:
            est = None
            per_hr = self.ir["FuelUsePerHour"]
            if per_hr and lap_avg:
                est = per_hr * (lap_avg / 3600.0)
            u_avg = u_max = u_min = est

        def scenario(u):
            if not u or u <= 0 or not isinstance(fuel, (int, float)):
                return {"usage": u, "laps": None, "pits": None, "refuel": None}
            laps_on_fuel = fuel / u
            refuel = None
            pits = None
            if laps_rem is not None:
                refuel = max(0.0, laps_rem * u - fuel)
                if cap and cap > 0:
                    pits = refuel / cap
            return {"usage": u, "laps": laps_on_fuel, "pits": pits,
                    "refuel": refuel}

        rows = {"avg": scenario(u_avg), "max": scenario(u_max),
                "min": scenario(u_min)}

        laps_empty = rows["avg"]["laps"]
        time_empty = (laps_empty * lap_avg) if (laps_empty and lap_avg) else None
        laps_margin = (laps_empty - laps_rem) if (laps_empty is not None
                                                  and laps_rem is not None) else None
        time_margin = (time_empty - time_rem) if (time_empty is not None
                                                  and time_rem is not None) else None
        add = rows["avg"]["refuel"]

        # Pit window: with the heaviest usage you must pit soonest, with the
        # lightest, latest -- that range is the window (in absolute lap numbers).
        window = None
        win_open = False
        strip = {"total": 0, "window": None, "now": None}
        if lap is not None and rows["max"]["laps"] and rows["min"]["laps"] \
                and add and add > 0:
            a = lap + int(rows["max"]["laps"])
            b = lap + int(rows["min"]["laps"])
            window = (a, b)
            win_open = lap >= a - 1
            if laps_rem is not None and laps_rem > 0:
                total = max(1, min(40, int(round(laps_rem))))
                wa = max(0, min(total - 1, int(rows["max"]["laps"])))
                wb = max(0, min(total - 1, int(rows["min"]["laps"])))
                strip = {"total": total, "window": (wa, wb), "now": 0}

        self.fuel_widget.set_data({
            "level": fuel, "cap": cap, "add": add,
            "window": window, "window_open": win_open,
            "rows": rows,
            "time_empty": time_empty, "time_margin": time_margin,
            "laps_empty": laps_empty, "laps_margin": laps_margin,
            "strip": strip,
        })


def main() -> int:
    if "--dump-config" in sys.argv:
        path = config.write_template()
        print(f"Wrote default config template to {path}")
        print("Edit it to customize every color, font, size, count and toggle.")
        return 0

    click_through = "--no-clickthrough" not in sys.argv
    demo = "--demo" in sys.argv
    demo_track = None
    if "--demo-track" in sys.argv:
        i = sys.argv.index("--demo-track")
        if i + 1 < len(sys.argv):
            demo_track = sys.argv[i + 1]
    tracks_dir = None
    if "--tracks-dir" in sys.argv:
        i = sys.argv.index("--tracks-dir")
        if i + 1 < len(sys.argv):
            tracks_dir = os.path.abspath(sys.argv[i + 1])
    # Default launch (e.g. double-clicking the desktop icon) opens settings and
    # waits for you to press "Start Overlay". --start shows the widgets right
    # away; --no-settings skips opening the settings window.
    start_now = "--start" in sys.argv or demo
    open_settings = "--no-settings" not in sys.argv
    # Give Windows an explicit app identity so the taskbar uses our icon (not the
    # generic python/pythonw one) and groups/pins the app under it. Must be set
    # before any window is created.
    if sys.platform == "win32":
        try:
            import ctypes
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(
                "GridGlance.App")
        except Exception:  # noqa: BLE001
            pass
    app = QApplication(sys.argv)
    app.setApplicationName("GridGlance")
    app.setDesktopFileName("GridGlance")
    icon_path = paths.app_icon()
    if icon_path:
        from PyQt6.QtGui import QIcon
        app.setWindowIcon(QIcon(icon_path))

    import signal

    signal.signal(signal.SIGINT, lambda *_: app.quit())
    keepalive = QTimer()
    keepalive.start(200)
    keepalive.timeout.connect(lambda: None)

    hud = AdvancedSimHUD(click_through=click_through, demo=demo,
                         demo_track=demo_track, tracks_dir=tracks_dir)

    # In-app auto-update: check GitHub for a newer release and offer to install.
    from .updater import UpdateChecker
    hud._updater = UpdateChecker()
    hud._updater.found.connect(lambda info: _prompt_update(app, hud, info))
    hud._updater.downloaded.connect(lambda path: _run_installer(app, path))
    if config.CFG.get("check_updates_on_launch", True):
        hud._updater.start()

    # A tray icon keeps the app reachable (reopen settings / quit) after the
    # settings window is closed while the overlay keeps running. If there's no
    # system tray, fall back to quitting when the last window closes.
    tray = _install_tray(app, hud, icon_path)
    app.setQuitOnLastWindowClosed(tray is None)

    if start_now:
        hud.start_overlay()
    if open_settings or not start_now:
        hud.open_settings()
    if not click_through:
        print("Edit mode: drag panels to position them; positions are saved.")
    return app.exec()


def _prompt_update(app, hud, info) -> None:
    from PyQt6.QtWidgets import QMessageBox
    box = QMessageBox()
    box.setWindowTitle("Update available")
    box.setIcon(QMessageBox.Icon.Information)
    box.setText(f"GridGlance {info['version']} is available. Update and "
                f"restart now?\n\nThe app will close, update itself and reopen "
                f"automatically -- no setup steps.")
    notes = (info.get("notes") or "").strip()
    if notes:
        box.setDetailedText(notes)
    box.setStandardButtons(QMessageBox.StandardButton.Yes
                           | QMessageBox.StandardButton.No)
    if box.exec() == QMessageBox.StandardButton.Yes and info.get("url"):
        hud._updater.download_async(info["url"])


def _run_installer(app, path) -> None:
    """Silently apply the update and quit so it can replace files + relaunch.

    On Windows the installer runs with /VERYSILENT so there's no setup wizard:
    it closes the running app, swaps in the new files and the installer's [Run]
    entry relaunches GridGlance automatically -- the user just sees the app
    reopen on the new version.
    """
    try:
        import subprocess
        if os.name == "nt":
            subprocess.Popen([path, "/VERYSILENT", "/SUPPRESSMSGBOXES",
                              "/NORESTART"])
        else:
            subprocess.Popen([path])
    except Exception:
        return
    app.quit()


def _install_tray(app, hud, icon_path):
    """Add a system-tray icon with Settings / Start-Stop / Quit. Returns it or None."""
    from PyQt6.QtWidgets import QSystemTrayIcon, QMenu
    if not QSystemTrayIcon.isSystemTrayAvailable():
        return None
    from PyQt6.QtGui import QIcon
    icon = QIcon(icon_path) if icon_path else app.windowIcon()
    tray = QSystemTrayIcon(icon, app)
    tray.setToolTip("GridGlance")
    menu = QMenu()
    act_settings = menu.addAction("Open Settings")
    act_toggle = menu.addAction("Start Overlay")
    act_edit = menu.addAction("Edit Layout")
    act_edit.setCheckable(True)
    menu.addSeparator()
    act_update = menu.addAction("Check for Updates")
    act_quit = menu.addAction("Quit")

    def refresh():
        act_toggle.setText("Stop Overlay" if hud.overlay_running()
                           else "Start Overlay")
        act_edit.setChecked(hud.edit_mode_enabled())

    def toggle():
        hud.toggle_overlay()
        refresh()

    act_settings.triggered.connect(hud.open_settings)
    act_toggle.triggered.connect(toggle)
    act_edit.triggered.connect(lambda checked: hud.set_edit_mode(checked))
    act_update.triggered.connect(lambda: getattr(hud, "_updater", None)
                                 and hud._updater.start())
    act_update.setVisible(bool(version.GITHUB_REPO))
    act_quit.triggered.connect(app.quit)
    menu.aboutToShow.connect(refresh)
    tray.setContextMenu(menu)
    tray.activated.connect(
        lambda reason: hud.open_settings()
        if reason in (QSystemTrayIcon.ActivationReason.Trigger,
                      QSystemTrayIcon.ActivationReason.DoubleClick) else None)
    tray.show()
    return tray


if __name__ == "__main__":
    sys.exit(main())
