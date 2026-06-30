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

import collections
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
from . import sysstats
from . import track_store
from . import version
from .panel import PanelWindow
from .widgets import track_map
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

# Multi-lap scanning: how many full laps the track map needs, and how many pit
# passes the pit lane needs, before each is finalized (saved + uploaded).
SCAN_LAPS = 3
PIT_PASSES = 3

# Pit route capture. The pit lane sits only a little off the racing line, so
# the entry/exit blend thresholds are derived from the *measured* offset of the
# lane from the line (PIT_OFFSET_* fractions of it): the car has "left the
# track" once it's more than ~half-way out to the lane, and has "rejoined" once
# it drops back near the line. Until that offset is known we fall back to a
# small fraction of the track's bounding-box diagonal (PIT_*_FRAC), with a tiny
# diagonal floor to reject GPS/line noise. A safety cap bounds the exit-blend
# capture in case the car never re-converges (off-track, telemetry gap).
PIT_DIVERGE_FRAC = 0.012      # fallback: fraction of diagonal (offset unknown)
PIT_REJOIN_FRAC = 0.008
PIT_OFFSET_DIVERGE = 0.55     # diverge at >55% of the measured lane offset
PIT_OFFSET_REJOIN = 0.10      # rejoin only once nearly back on the racing line
PIT_REJOIN_HOLD = 30          # ticks (~0.5s @ 60Hz) the car must hold near the
                              # line before the exit is final -- a brief dip
                              # toward it mid-exit no longer ends the lane early
PIT_COMMIT_HOLD = 15          # ticks the player must be continuously on pit road
                              # before the dot is drawn on the pit route -- stops
                              # a brief OnPitRoad blip while staying out past the
                              # pit entry from sticking the dot in the pits
PIT_OFFSET_FLOOR_FRAC = 0.0015  # minimum threshold, as a fraction of diagonal
PIT_BLEND_MAX_PTS = 1200
# How far the drawn pit entry / exit blend lines reach is a per-track scan "dial"
# whose defaults live in constants.py (so they're easy to find and tweak). They
# are seeded onto each overlay instance (self.pit_entry_max_pct /
# self.pit_exit_extend_pct) so the settings "Track Scan" tab can nudge them live
# for the running session without persisting the change.
# Rolling GPS buffer length (ticks) used to back-trace the entry blend from
# where the car peeled off the racing line up to the pit-road edge.
PIT_RECENT_MAX = 900


def _coerce_int(value):
    """Best-effort int from a telemetry field (which may be str/float/None)."""
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


class AdvancedSimHUD:
    """Owns the telemetry connection and drives the independent panel windows."""

    def __init__(self, click_through: bool = True, demo: bool = False):
        self.click_through = click_through
        self.demo = demo
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
        # Let the settings UI trigger a fresh track scan on demand.
        config.on_rescan(self._rescan_track)
        config.on_rescan_pits(self._rescan_pits)
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
        self._path_builder = track_map.TrackPathBuilder()
        self._track_loaded = False        # a bundled track FILE is in use
        self._track_file_checked = False  # we've looked for a file for this track
        self._map_version = 0             # last learned-path version pushed
        self._track_id = None             # current track's iRacing TrackID
        self._track_turns = None          # WeekendInfo TrackNumTurns (corner count)
        self._learn_name = ""             # display name to stamp on a saved scan
        self._track_saved = False         # we've persisted this learned scan
        self._force_learn = False         # rescan: re-learn even if a file exists
        # Pit-lane learning: detect the entry/exit (by lap pct) as the player
        # drives through the pits, and the speed limit from the pit limiter.
        self._pit_was_on = False
        self._pit_enter_pct = None
        self._pit_span = None
        self._pit_speed_ms = 0.0
        # Session-only overrides of the pit blend-length dials (constants.py).
        # The settings "Track Scan" tab tunes these live for the running session;
        # they're never persisted -- on relaunch they reset when WeekendInfo arrives.
        self.pit_entry_max_pct, self.pit_exit_extend_pct = (
            constants.pit_blend_defaults(None))
        self._pit_blend_defaults = (self.pit_entry_max_pct,
                                    self.pit_exit_extend_pct)
        self._pit_s0 = None  # speed/time samples for steady-cruise detection
        self._pit_t0 = None
        # Real pit-lane geometry: the player's GPS trace from leaving the track
        # to rejoining it, split into three parts -- the entry blend (track ->
        # pit road), the lane itself (on pit road), and the exit blend (pit road
        # -> track). The *_cur lists accumulate the current pass; the finalized
        # (averaged-over-passes) versions are stored without the suffix.
        self._pit_geo_cur: list[tuple[float, float]] = []  # current lane
        self._pit_in_cur: list[tuple[float, float]] = []   # current entry blend
        self._pit_out_cur: list[tuple[float, float]] = []  # current exit blend
        self._pit_path = None
        self._pit_in = None
        self._pit_out = None
        self._pit_in_pct = None   # lap pct where the car left the racing line
        self._pit_out_pct = None  # lap pct where it rejoined the racing line
        # Pit-route capture state machine: None (on track), "lane" (on pit road),
        # or "exit" (off pit road but still tracing the exit blend until rejoin).
        self._pit_phase = None
        self._pit_in_pct_cur = None
        self._pit_exit_pct = None    # lap pct at the OnPitRoad falling edge
        self._pit_exit_ticks = 0     # ticks spent tracing the current exit blend
        self._pit_rejoin_ticks = 0   # consecutive ticks held near the racing line
        self._pit_merge_pct = None   # lap % where the exit merged (None until then)
        self._pit_exit_dbg: list = []  # (pct, dist, surf) over the exit, for logs
        self._pit_surf_prev = None     # player's last CarIdxTrackSurface, for logs
        # Drift-free blend-line lap %s straight from iRacing's own track-surface
        # zones: where it flips OnTrack -> ApproachingPits (the entry blend line)
        # and back ApproachingPits -> OnTrack (the exit blend line). These are
        # exact and rock-steady, so they drive the blend extents when available.
        self._pit_surf_entry_pct = None
        self._pit_surf_exit_pct = None
        # On-track (dead-reckon xy, drift-free lap %) correspondences gathered
        # over the exit while the car is back on the racing line. Together with
        # the approach buffer they bracket the pit on both sides and drive the
        # least-squares similarity that un-drifts the captured pit geometry.
        self._pit_exit_corr: list = []
        self._pit_align_n = 0       # anchors used by the last alignment fit
        self._pit_align_ls = False  # whether that fit was the least-squares one
        # Max distance of the current pass's lane from the racing line, used to
        # scale the divergence thresholds, and a snapshot of the rolling buffer
        # at pit entry from which the entry blend is back-traced at finalize.
        self._pit_lane_offset = 0.0
        self._pit_entry_buf: list[tuple] = []
        # Whether the player is currently on the pit route (pit road or a blend),
        # computed in _learn_pit and read when placing the player dot.
        self._player_on_route = False
        # Consecutive ticks the player has been committed to the pit route (on
        # pit road or clearly off the racing line); must exceed PIT_COMMIT_HOLD
        # before the dot is moved onto the route, to ignore brief blips.
        self._player_route_ticks = 0
        # Rolling (pct, x, y, dist_to_track) buffer for back-tracing the entry
        # blend, and a cache of the racing-line geometry to measure divergence.
        self._pit_recent: collections.deque = collections.deque(
            maxlen=PIT_RECENT_MAX)
        self._track_pts: list[tuple[float, float]] | None = None
        self._track_diag = 0.0
        # Per-opponent latch: once a car is seen on pit road we keep treating it
        # as "on the pit route" (so it follows the exit blend) until its lap pct
        # passes the rejoin point. idx -> True while latched.
        self._pit_route_latch: dict[int, bool] = {}
        # Multi-lap scan progress: the track map finalizes only after SCAN_LAPS
        # *complete* laps; the pit lane only after PIT_PASSES passes (and only
        # once the track scan is done). _scan_seen_lap is the lap we were on when
        # scanning began (its remainder is a partial lap that must not count);
        # _scan_anchor_lap is the lap number at the first start/finish crossing,
        # from which whole laps are counted. Pit passes accumulate
        # (entry_pct, exit_pct, speed).
        self._scan_seen_lap = None
        self._scan_anchor_lap = None
        self._scan_laps = 0
        self._scan_done = False
        # Each completed pit pass: (entry_pct, exit_pct, speed, geo_points).
        self._pit_passes: list[tuple] = []
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
        self.tracks_dir = paths.tracks_dir()
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
        self.map_widget = track_map.TrackMapWidget()
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

    def pit_tuning(self) -> tuple[float, float]:
        """The (entry_max_pct, exit_extend_pct) blend dials in effect this run.

        Read by the settings "Track Scan" tab to position its sliders.
        """
        return (self.pit_entry_max_pct, self.pit_exit_extend_pct)

    def pit_tuning_defaults(self) -> tuple[float, float]:
        """Track-type defaults for the pit blend dials (entry, exit)."""
        return self._pit_blend_defaults

    def set_pit_tuning(self, entry_max: float | None = None,
                       exit_extend: float | None = None) -> None:
        """Override the pit blend-length dials for the running session only.

        Not persisted: the next launch reverts to the track-type defaults.
        """
        if entry_max is not None:
            self.pit_entry_max_pct = float(entry_max)
        if exit_extend is not None:
            self.pit_exit_extend_pct = float(exit_extend)

    def track_authoring_state(self) -> dict:
        """Snapshot for the Track Scan authoring tab (pit speed, corners)."""
        mw = self.map_widget
        n = self._track_turns if self._track_turns else mw.num_turns
        return {
            "has_track": mw.path is not None and self._track_id is not None,
            "pit_speed_ms": self._pit_speed_ms,
            "num_turns": n,
            "corner_count": len(mw.display_corners()),
        }

    def _persist_track_meta(self, **fields) -> bool:
        """Write track metadata locally and push to the cloud when allowed."""
        if self._track_id is None or not fields:
            return False
        try:
            track_map.update_track_meta(self.tracks_dir, self._track_id, **fields)
        except Exception:
            return False
        if config.cloud_tracks():
            self._track_sync.upload_local_async(self.tracks_dir, self._track_id)
        return True

    def set_pit_speed_authoring(self, speed_ms: float) -> None:
        """Override the learned pit speed limit and save to the track record."""
        speed_ms = max(0.0, float(speed_ms))
        self._pit_speed_ms = speed_ms
        if self._pit_span is not None:
            self.map_widget.set_pit(self._pit_span, self._pit_speed_ms)
        elif speed_ms > 0:
            self.map_widget.pit_speed_ms = speed_ms
            self.map_widget.update()
        self._persist_track_meta(pit_speed=round(speed_ms, 3) if speed_ms else None)

    def set_num_turns_authoring(self, n: int) -> None:
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
        self._persist_track_meta(**fields)

    def set_corner_edit_mode(self, enabled: bool) -> None:
        """Toggle drag-to-move corner labels on the map widget."""
        self.map_widget.set_corner_edit(
            enabled, self._save_corners_authoring if enabled else None)

    def _save_corners_authoring(self) -> None:
        """Persist manually placed corner labels."""
        corners = track_map.corners_to_json(self.map_widget.corners)
        fields: dict = {"corners": corners}
        if self._track_turns:
            fields["num_turns"] = self._track_turns
        self._persist_track_meta(**fields)

    def _pit_scan_active(self) -> bool:
        """True while the track is scanned but pit data still needs gathering."""
        return (self._scan_done and not self.demo
                and len(self._pit_passes) < PIT_PASSES
                and self._pit_span is None)

    def _update_scan_status(self) -> None:
        """Refresh the persistent map badge: 'LAP n/3' or 'PIT n/3'."""
        if self.demo:
            return
        if not self._scan_done:
            self.map_widget.set_scan_status(
                f"LAP {min(self._scan_laps + 1, SCAN_LAPS)}/{SCAN_LAPS}")
        elif self._pit_scan_active():
            self.map_widget.set_scan_status(
                f"PIT {len(self._pit_passes) + 1}/{PIT_PASSES}")
        else:
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
        # Each array is only read if some visible widget consumes it.
        positions = self.ir["CarIdxPosition"] if need_order else None
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
            "irating": (self._fmt_irating(d.get("IRating"), "standings")
                        if cols.get("irating") or cols.get("license") else ""),
            "pit": pit,
            "gap_text": gap_text,
            "last_lap": self._lap_for(idx, self._car_last) if cols.get("last_lap") else "",
            "best_lap": self._lap_for(idx, self._car_best) if cols.get("best_lap") else "",
            "is_player": idx == player,
            "in_pit": surface[idx] in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS),
            "lapping": False,
        }

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
                          player, lap_est, car_lap, sess_time) -> None:
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
    def _fmt_irating(ir, section: str | None = None) -> str:
        if not ir:
            return "--"
        ir = int(round(ir))
        sec = config.CFG.get(section or "", {})
        abbreviate = sec.get("irating_abbreviate", True) if isinstance(sec, dict) else True
        if abbreviate and ir >= 1000:
            return f"{ir / 1000:.1f}k"
        return str(ir)

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
            "pit": pit,
            "gap": abs(delta) if cols.get("gap") else None,
            "last_lap": self._lap_for(idx, self._car_last) if cols.get("last_lap") else "",
            "best_lap": self._lap_for(idx, self._car_best) if cols.get("best_lap") else "",
            "is_player": is_player,
            "in_pit": in_pit,
            "lapping": lapping,
            "lap_ahead": lap_ahead,
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
            if idx == player or t is None or idx in self._pace_idxs:
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

    def _class_sof(self, drivers, player, section=None) -> str:
        cid = self._player_class(drivers, player)
        if cid is None:
            return self._sof(drivers, section)
        irs = [d.get("IRating") for d in drivers.values()
               if d.get("IRating") and d.get("CarClassID") == cid]
        if not irs:
            return "--"
        return self._fmt_irating(sum(irs) / len(irs), section)

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
        pts = None
        path = track_map.find_track_file("_demo", self.tracks_dir)
        if path:
            try:
                pts, sf, corners, _, _ = track_map.load_track(path)
                self.map_widget.set_track(pts, sf, corners)
            except Exception:
                pts = None
        if pts is None:
            pts = track_map.build_demo_path()
            self.map_widget.set_path(pts)
        # Synthesize a pit lane so all the map's pit features -- the lane, the
        # yellow entry / blue exit blends, the static speed badge, and on-pit
        # car placement -- are visible in demo mode without a live scan.
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
        lane_lo, lane_hi = 0.95, 0.06
        return {
            "pit_span": (lane_lo, lane_hi),
            "pit_speed": 22.0,  # ~50 mph / 80 km/h, shown as a static badge
            "pit_in": span_pts(in_pct, lane_lo, 14, 0.0, 1.0),
            "pit_path": span_pts(lane_lo, lane_hi, 44, 1.0, 1.0),
            "pit_out": span_pts(lane_hi, out_pct, 14, 1.0, 0.0),
            "pit_in_pct": in_pct,
            "pit_out_pct": out_pct,
        }

    def _ensure_track(self, player, lap_pct) -> None:
        """Prefer a bundled per-track file (by TrackID); else learn from GPS."""
        if self.demo or self._track_loaded or self._scan_done:
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
                # iRacing's official corner count -- used to number auto-detected
                # corners exactly the way the sim does (ovals and road courses).
                self._track_turns = _coerce_int(weekend.get("TrackNumTurns"))
                self.map_widget.set_num_turns(self._track_turns)
                self.pit_entry_max_pct, self.pit_exit_extend_pct = (
                    constants.pit_blend_defaults(weekend))
                self._pit_blend_defaults = (self.pit_entry_max_pct,
                                            self.pit_exit_extend_pct)
                # On a rescan we skip the saved/bundled file and re-learn; the
                # new scan then overwrites tracks/<id>.json when complete.
                if not self._force_learn:
                    track_file = track_map.find_track_file(
                        self._track_id, self.tracks_dir)
                    if not track_file and config.cloud_tracks() \
                            and self._track_id is not None \
                            and self._track_id not in self._remote_tried:
                        # No local file -- ask the shared library for it while we
                        # start learning from GPS as a fallback. The download
                        # lands in _on_remote_track if it beats the learner.
                        self._remote_tried.add(self._track_id)
                        self._track_sync.fetch_async(self._track_id)
                    if track_file:
                        try:
                            pts, sf, corners, _, meta = track_map.load_track(
                                track_file)
                            self.map_widget.set_track(pts, sf, corners)
                            self.map_widget.set_num_turns(
                                self._track_turns or meta.get("num_turns"))
                            self._set_track_geom(pts)
                            self._apply_pit_meta(meta)
                            self._track_loaded = True
                            # A complete track is present, so pit scanning is
                            # immediately allowed (no 3-lap learn needed).
                            self._scan_done = True
                            # Mark as just-used so LRU eviction keeps it around.
                            track_store.touch(self.tracks_dir, self._track_id)
                            return
                        except Exception:
                            pass  # fall back to GPS learning

        # Learn the shape from the player's own GPS, showing a rough loop early
        # and refining it as more of the lap is sampled. If the sim doesn't
        # expose GPS (Lat/Lon), fall back to dead reckoning from speed + heading.
        b = self._path_builder
        pct = lap_pct[player] if lap_pct and player is not None else None
        # The position was resolved once this tick (GPS or dead reckoning) in
        # _update_map; dead-reckoned coordinates are only consistent within one
        # lap, so start fresh at each start/finish crossing to avoid a kink.
        if self._player_pos_wrapped:
            b.reset()
        if self._player_pos is not None and pct is not None:
            b.add_xy(pct, self._player_pos[0], self._player_pos[1])

        # Count only *complete* laps: ignore the partial lap in progress when the
        # scan began by anchoring at the first start/finish crossing, then count
        # whole laps from there. Rebuild the averaged path each new lap so it
        # visibly refines, and show a "LAP n/3" badge.
        lap = self.ir["Lap"]
        if isinstance(lap, int) and lap >= 0:
            if self._scan_seen_lap is None:
                self._scan_seen_lap = lap
            if self._scan_anchor_lap is None:
                if lap > self._scan_seen_lap:  # first crossing -> start counting
                    self._scan_anchor_lap = lap
                    # Drop the partial lap's samples so only the 3 complete laps
                    # feed the averaged map (the last preview stays on screen).
                    b.reset()
            else:
                laps_done = max(0, lap - self._scan_anchor_lap)
                if laps_done != self._scan_laps:
                    self._scan_laps = laps_done
                    b.rebuild()
        self._update_scan_status()

        if b.version != self._map_version:
            self._map_version = b.version
            self.map_widget.set_path(b.path)
        elif not b.ready:
            self.map_widget.set_progress(b.coverage())

        # Finalize only after SCAN_LAPS full laps AND a fully-covered loop, then
        # persist + share it (so we skip learning next time / others can use it).
        if (not self._scan_done and self._scan_laps >= SCAN_LAPS
                and b.coverage() >= 0.96):
            self._scan_done = True
            self._track_saved = True
            # Cache the finished loop so pit-route divergence can be measured.
            self._set_track_geom(b.path)
            self._update_scan_status()
            self.map_widget.flash_hint("Track saved \u00b7 scan the pits")
            try:
                track_map.save_learned_track(
                    self.tracks_dir, self._track_id, b.path, self._learn_name,
                    pit_span=self._pit_span, pit_speed=self._pit_speed_ms,
                    num_turns=self._track_turns)
            except Exception:
                pass
            # Share the freshly learned map (no-op unless we have write access).
            if config.cloud_tracks():
                self._track_sync.upload_local_async(
                    self.tracks_dir, self._track_id)

    def _on_tracks_synced(self, n) -> None:
        """Startup cache refresh finished; if the map we're showing was one of
        the tracks that changed, reload it so the live view is up to date."""
        if self.demo or not n or self._force_learn or self._track_id is None:
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
        self._set_track_geom(pts)
        self._apply_pit_meta(meta)
        self._track_loaded = True
        self._track_saved = True
        self._scan_done = True

    def _on_remote_track(self, track_id, doc) -> None:
        """A shared track map arrived from the cloud (off the GUI thread).

        Only adopt it if we haven't already loaded/learned this track locally,
        so a download can't clobber a scan in progress or a bundled file.
        """
        if not doc or track_id != self._track_id:
            return
        if self._track_loaded or self._path_builder.complete:
            return
        try:
            path = track_store.write_local(self.tracks_dir, doc)
            if not path:
                return
            pts, sf, corners, _, meta = track_map.load_track(path)
            self.map_widget.set_track(pts, sf, corners)
            self.map_widget.set_num_turns(
                self._track_turns or meta.get("num_turns"))
            self._set_track_geom(pts)
            self._apply_pit_meta(meta)
            self._track_loaded = True
            self._track_saved = True
            self._scan_done = True
            # A new track just landed in the cache -- trim old ones (keeping the
            # one we're using) so the folder stays bounded.
            track_store.enforce_cache_limit(
                self.tracks_dir, protect=[self._track_id])
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
        # Restart the multi-lap scan from scratch (pit depends on the track, so
        # its passes reset too).
        self._scan_seen_lap = None
        self._scan_anchor_lap = None
        self._scan_laps = 0
        self._scan_done = False
        self._pit_passes = []
        self._update_scan_status()
        # Reset dead-reckoning so the new scan starts from a clean origin.
        self._dr_x = self._dr_y = 0.0
        self._dr_t = None
        self._dr_last_pct = None
        # Forget the learned pit lane too (set_track(None) clears it on the map).
        self._reset_pit_state()
        # The track geometry is being relearned, so drop the divergence cache.
        self._track_pts = None
        self._track_diag = 0.0
        # Clear the drawn map back to the "learning" placeholder.
        self.map_widget.set_track(None)
        self.map_widget.set_progress(0.0)

    def _reset_pit_state(self) -> None:
        """Clear all in-progress and learned pit-lane / pit-route state."""
        self._pit_was_on = False
        self._pit_enter_pct = None
        self._pit_exit_pct = None
        self._pit_span = None
        self._pit_speed_ms = 0.0
        self._pit_s0 = self._pit_t0 = None
        self._pit_geo_cur = []
        self._pit_in_cur = []
        self._pit_out_cur = []
        self._pit_path = None
        self._pit_in = None
        self._pit_out = None
        self._pit_in_pct = None
        self._pit_out_pct = None
        self._pit_in_pct_cur = None
        self._pit_phase = None
        self._pit_exit_ticks = 0
        self._pit_rejoin_ticks = 0
        self._pit_merge_pct = None
        self._pit_lane_offset = 0.0
        self._pit_entry_buf = []
        self._pit_exit_corr = []
        self._pit_surf_prev = None
        self._pit_surf_entry_pct = None
        self._pit_surf_exit_pct = None
        self._player_on_route = False
        self._player_route_ticks = 0
        self._pit_recent.clear()
        self._pit_route_latch.clear()
        self._pit_passes = []

    def _rescan_pits(self) -> None:
        """Forget just the learned pit lane and re-learn it on the next pit pass.

        Leaves the track geometry intact; clears the pit data from the map and
        strips it from the saved track file.
        """
        if self.demo:
            return
        self._reset_pit_state()  # re-learn the pit over PIT_PASSES fresh passes
        self._update_scan_status()
        self.map_widget.clear_pit()
        if self._track_id is not None:
            try:
                track_map.update_track_meta(
                    self.tracks_dir, self._track_id, pit_span=None,
                    pit_speed=None, pit_path=None, pit_in=None, pit_out=None,
                    pit_in_pct=None, pit_out_pct=None)
            except Exception:
                pass

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

    def _set_track_geom(self, pts) -> None:
        """Cache the racing-line geometry (and its bounding-box diagonal) so we
        can measure how far the car has strayed from it when learning the pit
        route. Accepts any iterable of (x, y) points."""
        pts = [(float(x), float(y)) for x, y in pts] if pts else None
        self._track_pts = pts
        if pts:
            xs = [p[0] for p in pts]
            ys = [p[1] for p in pts]
            self._track_diag = math.hypot(max(xs) - min(xs), max(ys) - min(ys))
        else:
            self._track_diag = 0.0

    def _dist_to_track(self, x: float, y: float, near_pct=None,
                       window: float = 0.2) -> float | None:
        """Min distance from (x, y) to the racing line, measured to the nearest
        *segment* of the (closed) polyline -- not just the nearest vertex, which
        would add discretization noise on the order of the small pit offset we're
        trying to detect. None if there's no geometry.

        When ``near_pct`` is given the search is limited to the stretch of line
        within ``window`` lap-fraction of that position. This is essential on a
        narrow oval: otherwise the globally nearest point can sit on the
        *opposite* straight (a couple of metres across the infield), faking a
        merge while the car is still out on the pit-exit apron.
        """
        pts = self._track_pts
        if not pts or len(pts) < 2:
            return None
        n = len(pts)
        if near_pct is None:
            idxs = range(n)
        else:
            c = int((near_pct % 1.0) * n)
            w = max(1, int(window * n))
            idxs = ((c + k) % n for k in range(-w, w + 1))
        best = float("inf")
        for i in idxs:
            ax, ay = pts[i]
            bx, by = pts[(i + 1) % n]  # wrap: the track is a closed loop
            dx, dy = bx - ax, by - ay
            seg2 = dx * dx + dy * dy
            if seg2 <= 0.0:
                t = 0.0
            else:
                t = ((x - ax) * dx + (y - ay) * dy) / seg2
                t = 0.0 if t < 0.0 else (1.0 if t > 1.0 else t)
            px, py = ax + dx * t, ay + dy * t
            d = (x - px) * (x - px) + (y - py) * (y - py)
            if d < best:
                best = d
        return math.sqrt(best) if best < float("inf") else None

    def _track_point_at_pct(self, pct):
        """Interpolate the racing-line point at a lap fraction. The cached track
        polyline is ordered by lap %, so index = pct * n. None without geometry."""
        pts = self._track_pts
        if not pts:
            return None
        n = len(pts)
        if n == 1 or pct is None:
            return pts[0]
        f = (pct % 1.0) * n
        i = int(f) % n
        frac = f - math.floor(f)
        x0, y0 = pts[i]
        x1, y1 = pts[(i + 1) % n]
        return (x0 + (x1 - x0) * frac, y0 + (y1 - y0) * frac)

    def _track_normal_at_pct(self, pct):
        """Unit normal (left of tangent) on the racing line at a lap fraction."""
        pts = self._track_pts
        if not pts or len(pts) < 2 or pct is None:
            return 0.0, 0.0
        n = len(pts)
        f = (pct % 1.0) * n
        i = int(f) % n
        x0, y0 = pts[i]
        x1, y1 = pts[(i + 1) % n]
        dx, dy = x1 - x0, y1 - y0
        ln = math.hypot(dx, dy) or 1.0
        return -dy / ln, dx / ln

    @staticmethod
    def _seg_has_pct(seg) -> bool:
        return bool(seg and len(seg[0]) == 3)

    @staticmethod
    def _seg_to_xy(seg):
        """Drop lap-% metadata, returning plain (x, y) points."""
        if not seg:
            return seg
        if len(seg[0]) == 3:
            return [(x, y) for _, x, y in seg]
        return list(seg)

    @staticmethod
    def _pct_past(from_pct, to_pct) -> float:
        """Lap-fraction travelled forward from ``from_pct`` to ``to_pct``."""
        if from_pct is None or to_pct is None:
            return 0.0
        d = (to_pct - from_pct) % 1.0
        return 0.0 if d > 0.5 else d

    def _append_exit_extension(self, pit_out, surf_pct, limit):
        """Extend the exit blend forward along the racing line past the surface
        merge, tapering lateral offset to zero.

        Avoids following the car through corners after rejoin -- on a road course
        the driver often hits esses immediately while the painted commitment
        line runs straight beside the track.
        """
        if limit <= 0.0 or not self._track_pts or surf_pct is None:
            return pit_out
        merge_tp = self._track_point_at_pct(surf_pct)
        if merge_tp is None:
            return pit_out
        lane_w = 0.0
        sgn = 1.0
        if pit_out:
            last = pit_out[-1]
            lx, ly = (last[1], last[2]) if len(last) == 3 else last
            near, nx, ny = self._nearest_on_track_at_pct(lx, ly, surf_pct)
            if near is not None:
                signed = (lx - near[0]) * nx + (ly - near[1]) * ny
                lane_w = abs(signed)
                sgn = 1.0 if signed >= 0 else -1.0
        if lane_w <= 0.0:
            lane_w = max(0.15 * self._pit_lane_offset,
                         0.015 * self._track_diag)
        if lane_w <= 0.0:
            return pit_out
        n = max(6, int(limit * 120))
        ext = []
        for k in range(1, n + 1):
            t = k / n
            pct = (surf_pct + limit * t) % 1.0
            tp = self._track_point_at_pct(pct)
            if tp is None:
                continue
            nx, ny = self._track_normal_at_pct(pct)
            nx, ny = sgn * nx, sgn * ny
            off = lane_w * (1.0 - t)
            ext.append((pct, tp[0] + nx * off, tp[1] + ny * off))
        return list(pit_out) + ext

    def _nearest_on_track_at_pct(self, x, y, pct, window: float = 0.08):
        """Closest point on the racing line near ``pct`` (lap-fraction window)."""
        pts = self._track_pts
        if not pts or len(pts) < 2 or pct is None:
            return None, 0.0, 0.0
        n = len(pts)
        c = int((pct % 1.0) * n)
        w = max(1, int(window * n))
        idxs = ((c + k) % n for k in range(-w, w + 1))
        best = None
        bd = float("inf")
        bnx = bny = 0.0
        for i in idxs:
            ax, ay = pts[i]
            bx, by = pts[(i + 1) % n]
            dx, dy = bx - ax, by - ay
            seg2 = dx * dx + dy * dy
            t = 0.0 if seg2 <= 0.0 else ((x - ax) * dx + (y - ay) * dy) / seg2
            t = 0.0 if t < 0.0 else (1.0 if t > 1.0 else t)
            px, py = ax + dx * t, ay + dy * t
            d = (x - px) * (x - px) + (y - py) * (y - py)
            if d < bd:
                bd = d
                best = (px, py)
                ln = math.hypot(dx, dy) or 1.0
                bnx, bny = -dy / ln, dx / ln
        return best, bnx, bny

    def _reanchor_by_lap_pct(self, seg):
        """Rebuild a captured segment using drift-free lap % for its position
        around the circuit and the aligned frame for its lateral offset.

        Dead reckoning drifts in both dimensions, but lap % is exact. After the
        similarity alignment puts the segment in the track's frame, each point's
        signed offset from the racing line at its lap % is preserved while its
        longitudinal position is snapped to that % -- so the lane sits beside
        the track instead of drifting on top of it.
        """
        if not seg or not self._track_pts:
            return self._seg_to_xy(seg)
        if not self._seg_has_pct(seg):
            return seg
        out = []
        for pct, x, y in seg:
            if pct is None:
                out.append((x, y))
                continue
            tp = self._track_point_at_pct(pct)
            nx, ny = self._track_normal_at_pct(pct)
            near, _, _ = self._nearest_on_track_at_pct(x, y, pct)
            if tp is None or near is None:
                out.append((x, y))
                continue
            off = (x - near[0]) * nx + (y - near[1]) * ny
            out.append((tp[0] + nx * off, tp[1] + ny * off))
        return out

    def _nearest_on_track(self, x, y):
        """Closest point on the racing-line polyline to (x, y) plus the unit
        normal there, taken as the left of the driving tangent -- a *consistent*
        side independent of the loop centroid. Measured to the nearest *segment*
        so the normal is stable. (None, 0, 0) if no geometry.
        """
        pts = self._track_pts
        if not pts or len(pts) < 2:
            return None, 0.0, 0.0
        n = len(pts)
        best = None
        bd = float("inf")
        bnx = bny = 0.0
        for i in range(n):
            ax, ay = pts[i]
            bx, by = pts[(i + 1) % n]
            dx, dy = bx - ax, by - ay
            seg2 = dx * dx + dy * dy
            t = 0.0 if seg2 <= 0.0 else ((x - ax) * dx + (y - ay) * dy) / seg2
            t = 0.0 if t < 0.0 else (1.0 if t > 1.0 else t)
            px, py = ax + dx * t, ay + dy * t
            d = (x - px) * (x - px) + (y - py) * (y - py)
            if d < bd:
                bd = d
                best = (px, py)
                ln = math.hypot(dx, dy) or 1.0
                bnx, bny = -dy / ln, dx / ln  # left of the tangent (consistent)
        return best, bnx, bny

    def _offset_blend_parallel(self, seg):
        """Nudge the stretch of a blend that hugs the racing line sideways so it
        reads as a pit lane running *parallel* to the track rather than drawn on
        top of it -- while still letting it touch the track where it joins it.

        The side to offset toward is taken from the *recorded* geometry (the side
        the captured blend actually sits on relative to the track), not the loop
        centroid. On an oval the pit is on the infield so the two agree, but on a
        road course the pit is often on the outside of the loop -- using the
        centroid there would flip the lane across the track. The end of the blend
        nearer the racing line is its junction (the merge for the exit, the
        departure for the entry); there the offset tapers to zero so the lane
        visibly meets the track, then ramps up to a full lane-width away from it.
        For each point the perpendicular distance is clamped up to that tapered
        target while the along-track position is preserved, so the result stays
        continuous and points already out toward the pit are left untouched.
        No-op without track geometry.
        """
        pts = self._track_pts
        if not seg or len(seg) < 2 or not pts or len(pts) < 2:
            return seg
        lane_w = max(0.15 * self._pit_lane_offset, 0.015 * self._track_diag)
        if lane_w <= 0.0:
            return seg
        # Nearest track point + consistent (left-of-tangent) normal + signed
        # offset for each blend point, then the dominant side the lane sits on.
        near_list = []
        signed = []
        for x, y in seg:
            near, nx, ny = self._nearest_on_track(x, y)
            near_list.append((near, nx, ny))
            signed.append(0.0 if near is None
                          else (x - near[0]) * nx + (y - near[1]) * ny)
        side_sum = sum(signed)
        if abs(side_sum) < 1e-9:  # degenerate: use the most-offset point's side
            k = max(range(len(signed)), key=lambda i: abs(signed[i]))
            side_sum = signed[k]
        sgn = 1.0 if side_sum >= 0 else -1.0
        # Whichever end sits nearer the racing line is the track junction; taper
        # the offset to zero there over ~2.5 lane-widths of arc length.
        d0 = self._dist_to_track(seg[0][0], seg[0][1]) or 0.0
        d1 = self._dist_to_track(seg[-1][0], seg[-1][1]) or 0.0
        junction_start = d0 <= d1
        taper_len = lane_w * 2.5
        cum = [0.0]
        for i in range(1, len(seg)):
            cum.append(cum[-1] + math.hypot(seg[i][0] - seg[i - 1][0],
                                            seg[i][1] - seg[i - 1][1]))
        total = cum[-1]
        out = []
        for i, (x, y) in enumerate(seg):
            s = cum[i] if junction_start else (total - cum[i])  # arc from junction
            target = lane_w * min(1.0, s / taper_len) if taper_len > 0 else lane_w
            near, nx, ny = near_list[i]
            if near is None:
                out.append((x, y))
                continue
            nx, ny = sgn * nx, sgn * ny          # toward the pit's actual side
            vx, vy = x - near[0], y - near[1]
            along = vx * nx + vy * ny            # perpendicular distance to track
            tx, ty = vx - nx * along, vy - ny * along   # tangential remainder
            if along < target:
                out.append((near[0] + nx * target + tx,
                            near[1] + ny * target + ty))
            else:
                out.append((x, y))
        return out

    @staticmethod
    def _fit_similarity(src, dst):
        """Least-squares 2D similarity (rotate + uniform scale + translate) that
        best maps the ``src`` points onto ``dst`` (Umeyama / Procrustes, solved
        in closed form via complex numbers). Returns (ax, ay, bx, by, ca, sa)
        such that a point (x, y) maps to
            (bx + ca*(x-ax) - sa*(y-ay),  by + sa*(x-ax) + ca*(y-ay))
        where (ca, sa) already fold in the scale. None if the fit is degenerate
        (fewer than two pairs, or all source points coincide)."""
        n = len(src)
        if n < 2 or n != len(dst):
            return None
        ax = sum(p[0] for p in src) / n
        ay = sum(p[1] for p in src) / n
        bx = sum(p[0] for p in dst) / n
        by = sum(p[1] for p in dst) / n
        sxx = sxy = denom = 0.0
        for (sx, sy), (tx_, ty_) in zip(src, dst):
            dax, day = sx - ax, sy - ay
            dbx, dby = tx_ - bx, ty_ - by
            sxx += dax * dbx + day * dby
            sxy += dax * dby - day * dbx
            denom += dax * dax + day * day
        if denom < 1e-12:
            return None
        ca, sa = sxx / denom, sxy / denom
        if abs(ca) < 1e-12 and abs(sa) < 1e-12:
            return None
        return (ax, ay, bx, by, ca, sa)

    def _alignment_anchors(self):
        """Build the on-track (dead-reckon xy -> true racing-line xy) anchor
        pairs that bracket the pit on both sides: the approach buffer up to the
        point the car left the line, and the exit samples once it was back on
        it. Lap % is drift-free, so each pairs a captured position with where it
        truly was. Returns (src_pts, dst_pts)."""
        diverge, _ = self._pit_thresholds()
        src, dst = [], []
        for pct_i, x, y, d in self._pit_entry_buf:
            if pct_i is None or d is None:
                continue
            if diverge is None or d <= diverge:
                tp = self._track_point_at_pct(pct_i)
                if tp is not None:
                    src.append((x, y))
                    dst.append(tp)
        for x, y, pct_i in self._pit_exit_corr:
            tp = self._track_point_at_pct(pct_i)
            if tp is not None:
                src.append((x, y))
                dst.append(tp)
        return src, dst

    def _align_pit_to_track(self, pit_in, lane, pit_out,
                            in_pct, out_pct, entry_pct, exit_pct):
        """Snap a captured pit route onto the track to undo dead-reckoning drift.

        Dead reckoning (the only positioning iRacing exposes -- it broadcasts no
        live GPS) drifts over a lap, so by the time the pit is reached the
        captured geometry has rotated/scaled away from the multi-lap-averaged
        racing line. Lap % is *not* subject to drift, so every tick the car is
        demonstrably on the racing line gives a true correspondence between a
        dead-reckoned position and where it actually was. We gather all of those
        on both sides of the pit (the approach and the exit) and fit a single
        similarity transform through them by least squares -- far steadier in
        rotation and scale than pinning just the entry/exit endpoints, which is
        what used to leave the middle of the lane warped onto the wrong part of
        the track. Falls back to the two-endpoint fit when too few anchors were
        captured, and is a no-op without track geometry.
        """
        if not self._track_pts:
            return pit_in, lane, pit_out

        def apply(ax, ay, bx, by, ca, sa):
            def xf_xy(x, y):
                dx, dy = x - ax, y - ay
                return (bx + ca * dx - sa * dy, by + sa * dx + ca * dy)

            def xf(seg):
                if not seg:
                    return seg
                if self._seg_has_pct(seg):
                    return [(p, *xf_xy(x, y)) for p, x, y in seg]
                return [xf_xy(x, y) for x, y in seg]

            return (xf(pit_in) if pit_in else pit_in,
                    xf(lane) if lane else lane,
                    xf(pit_out) if pit_out else pit_out)

        src, dst = self._alignment_anchors()
        self._pit_align_n = len(src)
        fit = self._fit_similarity(src, dst) if len(src) >= 3 else None
        self._pit_align_ls = fit is not None
        if fit is not None:
            return apply(*fit)

        # Fallback: pin the two endpoints (entry start / exit end) we know by
        # lap %, the original behaviour when too few line anchors were seen.
        head = pit_in if pit_in else lane
        tail = pit_out if pit_out else lane
        if not head or not tail:
            return pit_in, lane, pit_out
        s0, s1 = head[0], tail[-1]
        t0 = self._track_point_at_pct(in_pct if pit_in else entry_pct)
        t1 = self._track_point_at_pct(out_pct if pit_out else exit_pct)
        if t0 is None or t1 is None:
            return pit_in, lane, pit_out
        sx, sy = s1[0] - s0[0], s1[1] - s0[1]
        tx_, ty_ = t1[0] - t0[0], t1[1] - t0[1]
        src_len = math.hypot(sx, sy)
        tgt_len = math.hypot(tx_, ty_)
        if src_len < 1e-9 or tgt_len < 1e-9:
            return pit_in, lane, pit_out
        scale = tgt_len / src_len
        ang = math.atan2(ty_, tx_) - math.atan2(sy, sx)
        return apply(s0[0], s0[1], t0[0], t0[1],
                     math.cos(ang) * scale, math.sin(ang) * scale)

    def _apply_pit_meta(self, meta) -> None:
        """Push loaded pit geometry (span / speed / lane / blend lines / route
        extent) from a track file into both our state and the map widget. A meta
        with no pit data fully clears any previous pit lane (e.g. when leaving
        the demo or switching to a track that hasn't been scanned)."""
        if not meta.get("pit_span"):
            self._pit_span = None
            self._pit_path = self._pit_in = self._pit_out = None
            self._pit_in_pct = self._pit_out_pct = None
            self.map_widget.clear_pit()
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

    def _update_map(self, player, lap_pct, surface, drivers) -> None:
        if player is None or not lap_pct or not surface:
            return

        # Resolve the player's model-space position once (GPS or dead reckoning)
        # so the learner and the pit capture share a single, consistent frame.
        self._update_player_pos(lap_pct[player])
        self._ensure_track(player, lap_pct)
        # In demo mode the pit lane is synthesized once (see _load_demo_track);
        # skip live learning so it isn't overwritten by the demo's fake pit dips.
        if config.CFG["map"].get("show_pit", True) and not self.demo:
            self._learn_pit(player, lap_pct)
        if config.CFG["map"].get("show_wind", True):
            self.map_widget.set_wind(self.ir["WindDir"], self.ir["WindVel"])
        else:
            self.map_widget.set_wind(None, 0.0)

        on_pit_arr = self.ir["CarIdxOnPitRoad"]
        pit_surf = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
        use_pos = config.CFG["map"].get("car_label", "number") == "position"
        positions = self.ir["CarIdxPosition"] if use_pos else None
        route = self._route_interval()  # (lo, hi) lap-% extent, or None
        blends_on = config.CFG["map"].get("show_pit_blends", True)
        palette = track_map.car_palette()
        player_color = config.CFG["map"]["colors"]["player"]
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
            on_route = self._car_on_route(idx, pct, on_pit, is_player, route,
                                          blends_on)
            d = drivers.get(idx)
            if use_pos and positions and idx < len(positions) and positions[idx]:
                num = str(positions[idx])
            else:
                num = str(d.get("CarNumber", "?")) if d else "?"
            color = player_color if is_player else palette[idx % len(palette)]
            cars.append((pct, num, color, is_player, on_route))
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

    def _car_on_route(self, idx, pct, on_pit, is_player, route,
                      blends_on=True) -> bool:
        """Decide whether a car should be drawn on the pit route this tick.

        The player uses real GPS (set in _learn_pit), so we trust on-pit-road or
        being off the racing line. Opponents have no GPS, so we latch them onto
        the route once seen on pit road and hold them (through the exit blend)
        until their lap pct leaves the route extent -- they rejoin the track.

        When the pit blends are hidden (``blends_on`` False) there's no entry/exit
        lane to ride, so a car simply shows in the pits while it's actually on
        pit road and snaps back to the track the moment it leaves.
        """
        if not blends_on:
            return on_pit
        if is_player:
            return on_pit or self._player_on_route
        latched = self._pit_route_latch.get(idx, False)
        if on_pit:
            latched = True
        elif latched and route is not None:
            if not self._pct_in_interval(pct, route[0], route[1]):
                latched = False  # past the rejoin point -> back on track
        elif latched and route is None:
            latched = False
        self._pit_route_latch[idx] = latched
        return on_pit or latched

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
        doesn't expose it, falls back to dead reckoning -- exactly the same
        source the track learner uses, so the learned map and the pit-route
        geometry always live in the same coordinate frame. Called once per tick
        (before the learner) so dead reckoning integrates continuously even
        after the track scan is finished and pit scanning begins.
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

    def _pit_thresholds(self):
        """(diverge, rejoin) distance thresholds for the pit-route blends.

        Scaled to the measured lane offset from the racing line once known, else
        a small fraction of the track diagonal, with a tiny diagonal floor so
        racing-line/GPS noise doesn't read as leaving the track. (None, None)
        when there's no track geometry to measure against.
        """
        diag = self._track_diag
        if not diag:
            return None, None
        off = self._pit_lane_offset
        if off and off > 0.0:
            floor = diag * PIT_OFFSET_FLOOR_FRAC
            return (max(off * PIT_OFFSET_DIVERGE, floor),
                    max(off * PIT_OFFSET_REJOIN, floor))
        return diag * PIT_DIVERGE_FRAC, diag * PIT_REJOIN_FRAC

    @staticmethod
    def _trace_entry_blend(buf, diverge, max_pct=None, stop_on_diverge=True):
        """Back-trace a GPS buffer to reconstruct the pit-entry blend: the
        stretch from where the car peeled off the racing line up to the pit-road
        edge. Returns (points, divergence_lap_pct).

        Points are ``(lap_pct, x, y)`` so the entry can be re-anchored by lap %.
        ``max_pct`` caps how far back (in lap fraction from the pit-road edge)
        the blend is allowed to reach. When ``stop_on_diverge`` is False the
        trace runs purely to ``max_pct`` and ignores the distance test."""
        if (diverge is None and stop_on_diverge) or not buf:
            return [], None
        edge_pct = buf[-1][0]
        blend = []
        in_pct = None
        for pct_i, x, y, d in reversed(buf):
            if (max_pct is not None and pct_i is not None
                    and edge_pct is not None
                    and ((edge_pct - pct_i) % 1.0) > max_pct):
                in_pct = pct_i
                break
            blend.append((pct_i, x, y))
            if stop_on_diverge and d is not None and d <= diverge:
                in_pct = pct_i
                break
        blend.reverse()
        return (blend if len(blend) >= 2 else []), in_pct

    def _learn_pit(self, player, lap_pct) -> None:
        """Learn pit road from the player's pass: the lane's entry/exit lap pct,
        the speed limit, and the real geometry of the whole route -- the yellow
        entry blend (track -> pit road), the lane, and the exit blend (pit road
        -> track) -- captured from GPS using divergence from the racing line.
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
        xy = self._player_pos  # GPS or dead reckoning (resolved this tick)
        # Player's track-surface zone, logged on every change through the pit
        # sequence so we can compare iRacing's own zone boundaries (OnTrack <->
        # ApproachingPits <-> InPitStall) against the OnPitRoad edge and our
        # distance-based merge -- to see if the zone gives a cleaner blend line.
        surf_arr = self.ir["CarIdxTrackSurface"]
        surf = (surf_arr[player] if surf_arr and player < len(surf_arr)
                else None)
        pit_zone = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
        prev_surf = self._pit_surf_prev
        if surf != prev_surf and (
                on or self._pit_phase is not None
                or surf in pit_zone or prev_surf in pit_zone):
            log.warning("pit surface: %s -> %s @ pct=%.4f on_pit=%s phase=%s",
                        self._surf_name(prev_surf), self._surf_name(surf),
                        pct if pct is not None else -1.0, on, self._pit_phase)
            vp = pct if (pct is not None and 0.0 <= pct <= 1.0) else None
            if vp is not None:
                # Leaving the track for the pit approach: the entry blend line.
                if prev_surf == oc.TRK_ON_TRACK and surf in pit_zone \
                        and self._pit_phase != "exit":
                    self._pit_surf_entry_pct = vp
                # Rejoining the track off the pit exit: the exit blend line.
                elif surf == oc.TRK_ON_TRACK and prev_surf in pit_zone \
                        and self._pit_phase == "exit":
                    self._pit_surf_exit_pct = vp
        self._pit_surf_prev = surf
        # Measure distance only against the racing line near the car's own lap
        # position so the opposite straight can't fake a merge on a narrow oval.
        dist = (self._dist_to_track(xy[0], xy[1], near_pct=pct)
                if xy is not None else None)
        diverge, rejoin = self._pit_thresholds()

        # Live player position on the map. Only feed the widget a true (x, y)
        # when it comes from real GPS -- that shares the racing line's frame, so
        # the dot lands exactly on it. Dead-reckoned coordinates drift relative
        # to the drift-corrected pit geometry, so we pass None and let the widget
        # place the player onto the route by lap % instead (like other cars).
        self.map_widget.set_player_xy(xy if self._player_pos_gps else None)
        # Decide whether to draw the player on the pit route. We ride it only
        # after a *sustained* stint on pit road -- not from merely being off the
        # racing line, because near the pit entry a slightly wide line (or a
        # rough patch in the learned line) reads as "off line" and used to drag
        # the dot into the pits on a normal lap. Once on, hold through the exit
        # blend until lap pct leaves the route extent; lap position only moves
        # forward, so this can't flicker mid-corner.
        route = self._route_interval()
        in_route = (route is not None and pct is not None and 0.0 <= pct <= 1.0
                    and self._pct_in_interval(pct, route[0], route[1]))
        self._player_route_ticks = self._player_route_ticks + 1 if on else 0
        if self._player_route_ticks >= PIT_COMMIT_HOLD:
            self._player_on_route = True          # firmly on pit road
        elif self._player_on_route and not in_route:
            self._player_on_route = False         # passed the route's end

        # Rolling buffer for back-tracing the entry blend.
        if xy is not None and pct is not None and 0.0 <= pct <= 1.0:
            self._pit_recent.append((pct, xy[0], xy[1], dist))

        if on:
            self._learn_pit_speed(speed)
            if self._pit_span is not None:
                self.map_widget.set_pit(self._pit_span, self._pit_speed_ms)
        else:
            self._pit_s0 = self._pit_t0 = None

        valid_pct = pct if (pct is not None and 0.0 <= pct <= 1.0) else None

        # --- Pit-route capture state machine (builds the averaged geometry) ---
        if self._pit_phase is None:
            if on and not self._pit_was_on:  # entering pit road
                self._pit_phase = "lane"
                self._update_scan_status()
                self._pit_enter_pct = valid_pct
                self._pit_exit_pct = None
                # Snapshot the approach so the entry blend can be back-traced at
                # finalize once the lane offset (and thus the threshold) is known.
                self._pit_entry_buf = list(self._pit_recent)
                self._pit_exit_corr = []
                self._pit_surf_exit_pct = None
                self._pit_lane_offset = 0.0
                self._pit_in_cur = []
                self._pit_in_pct_cur = None
                self._pit_geo_cur = []
                self._pit_out_cur = []
                if xy is not None and valid_pct is not None:
                    self._pit_geo_cur.append((valid_pct, xy[0], xy[1]))
                if dist is not None:
                    self._pit_lane_offset = max(self._pit_lane_offset, dist)
        elif self._pit_phase == "lane":
            if on:
                if xy is not None and valid_pct is not None:
                    self._pit_geo_cur.append((valid_pct, xy[0], xy[1]))
                if dist is not None:
                    self._pit_lane_offset = max(self._pit_lane_offset, dist)
            else:  # left pit road
                self._pit_exit_pct = valid_pct
                if rejoin is None:
                    # No racing-line reference: finalize at the edge (no exit
                    # blend), matching the simpler legacy behaviour.
                    self._finalize_pit_pass(valid_pct)
                else:
                    self._pit_phase = "exit"
                    self._pit_exit_ticks = 0
                    self._pit_rejoin_ticks = 0
                    self._pit_merge_pct = None
                    self._pit_exit_dbg = [(valid_pct, dist, surf)]
                    self._pit_out_cur = []
                    if self._pit_geo_cur:
                        self._pit_out_cur.append(self._pit_geo_cur[-1])
                    if xy is not None and valid_pct is not None:
                        self._pit_out_cur.append((valid_pct, xy[0], xy[1]))
        elif self._pit_phase == "exit":
            if on:  # dipped back onto pit road -> resume the lane
                self._pit_phase = "lane"
                if xy is not None and valid_pct is not None:
                    self._pit_geo_cur.append((valid_pct, xy[0], xy[1]))
            else:
                self._pit_exit_ticks += 1
                if xy is not None and valid_pct is not None:
                    self._pit_out_cur.append((valid_pct, xy[0], xy[1]))
                self._pit_exit_dbg.append((valid_pct, dist, surf))
                # Once the car is back on the racing line, record it as a
                # drift-free anchor (true lap % <-> dead-reckon position) for the
                # exit side of the alignment fit.
                if (xy is not None and valid_pct is not None
                        and rejoin is not None and dist is not None
                        and dist <= rejoin):
                    self._pit_exit_corr.append((xy[0], xy[1], valid_pct))
                # Require the car to *hold* near the racing line for a moment, so
                # a brief dip toward it (noise, or the apron passing close) won't
                # finalize the exit before the real merge point.
                if rejoin is not None and dist is not None and dist <= rejoin:
                    self._pit_rejoin_ticks += 1
                else:
                    self._pit_rejoin_ticks = 0
                rejoined = self._pit_rejoin_ticks >= PIT_REJOIN_HOLD
                capped = self._pit_exit_ticks > PIT_BLEND_MAX_PTS
                # iRacing's surface flip (ApproachingPits -> OnTrack) is the
                # exact, drift-free exit blend line. Finalize as soon as it is
                # seen -- the post-merge commitment length is synthesized along
                # the racing line in finalize, not traced from the car (which
                # on a road course often dives through the next corner).
                if self._pit_surf_exit_pct is not None:
                    self._log_exit_profile("surface", rejoin)
                    self._finalize_pit_pass(valid_pct, capped=False)
                elif rejoined:
                    if self._pit_merge_pct is None:
                        self._pit_merge_pct = valid_pct
                    past = (valid_pct - self._pit_merge_pct) % 1.0
                    if past > 0.5:
                        past = 0.0
                    if past >= self.pit_exit_extend_pct or capped:
                        self._log_exit_profile("rejoined", rejoin)
                        self._finalize_pit_pass(valid_pct, capped=False)
                elif capped:
                    # A capped pass never actually merged (the driver kept going,
                    # e.g. looping back to re-pit), so its exit blend is garbage;
                    # flag it so finalize ignores it when building the exit lane.
                    self._log_exit_profile("capped", rejoin)
                    self._finalize_pit_pass(valid_pct, capped=True)
        self._pit_was_on = on

    @staticmethod
    def _surf_name(surf):
        """Short name for a CarIdxTrackSurface enum value, for readable logs."""
        return {oc.TRK_NOT_IN_WORLD: "void", oc.TRK_OFF_TRACK: "off",
                oc.TRK_IN_PIT_STALL: "stall", oc.TRK_APPROACHING_PITS: "appr",
                oc.TRK_ON_TRACK: "track"}.get(surf, str(surf))

    def _log_exit_profile(self, reason, rejoin) -> None:
        """Log how the car's distance-to-racing-line evolved over the pit exit,
        so we can see where (lap %) and why it decided the car had merged.

        Each entry is `pct@dist:surf`; the threshold the car must drop under to
        count as rejoined is printed too. If dist drops under the threshold well
        before the real merge, the racing line is too close to the apron there.
        ``surf_track_pct`` is the lap % where iRacing's own zone first flips back
        to OnTrack -- a candidate blend-line boundary to compare our merge to.
        """
        dbg = self._pit_exit_dbg
        if not dbg:
            return
        step = max(1, len(dbg) // 24)  # ~24 evenly spaced samples
        prof = []
        for i in range(0, len(dbg), step):
            pct, dist, surf = dbg[i]
            prof.append(f"{(pct if pct is not None else -1):.3f}@"
                        f"{(dist if dist is not None else -1):.0f}:"
                        f"{self._surf_name(surf)}")
        dists = [d for _, d, _ in dbg if d is not None]
        surf_track_pct = next(
            (p for p, _, s in dbg if s == oc.TRK_ON_TRACK and p is not None),
            None)
        log.warning(
            "pit exit: reason=%s rejoin_thr=%.1f ticks=%d min_dist=%.1f "
            "end_pct=%.3f surf_track_pct=%.3f profile=[%s]",
            reason, rejoin if rejoin is not None else -1.0, len(dbg),
            min(dists) if dists else -1.0,
            (dbg[-1][0] if dbg[-1][0] is not None else -1.0),
            surf_track_pct if surf_track_pct is not None else -1.0,
            " ".join(prof))

    def _finalize_pit_pass(self, out_pct, capped=False) -> None:
        """End the in-progress pit pass and hand its three segments to the
        accumulator, then reset the capture state machine.

        The entry blend is back-traced here (not at pit entry) so it can use the
        threshold scaled to the lane offset measured over this whole pass.
        ``capped`` marks a pass whose exit ran to the tick cap without merging.
        """
        if self._pit_enter_pct is not None:
            diverge, _ = self._pit_thresholds()
            # Prefer iRacing's exact entry blend line: back-trace to it directly.
            # Otherwise fall back to the distance/lap-% heuristic.
            entry_line = self._pit_surf_entry_pct
            if entry_line is not None:
                span = (self._pit_enter_pct - entry_line) % 1.0
                if span > 0.5:        # nonsense span (stale boundary) -> fallback
                    entry_line = None
                else:
                    # Slider adds extra length past the surface entry line.
                    span = min(0.5, span + self.pit_entry_max_pct)
            if entry_line is not None:
                in_blend, in_pct = self._trace_entry_blend(
                    self._pit_entry_buf, diverge, max_pct=span,
                    stop_on_diverge=False)
            else:
                in_blend, in_pct = self._trace_entry_blend(
                    self._pit_entry_buf, diverge, max_pct=self.pit_entry_max_pct)
            # Exit blend: captured path up to the surface merge, then a synthetic
            # extension along the racing line (not the car's line through esses).
            pit_out = list(self._pit_out_cur)
            if self._pit_surf_exit_pct is not None:
                limit = self.pit_exit_extend_pct
                tol = 0.003
                if self._seg_has_pct(pit_out):
                    pit_out = [(p, x, y) for p, x, y in pit_out
                               if p is None or self._pct_past(
                                   self._pit_surf_exit_pct, p) <= tol]
                pit_out = self._append_exit_extension(
                    pit_out, self._pit_surf_exit_pct, limit)
                out_pct = (self._pit_surf_exit_pct + limit) % 1.0
            self._record_pit_pass(
                self._pit_enter_pct,
                self._pit_exit_pct if self._pit_exit_pct is not None
                else out_pct,
                list(self._pit_geo_cur),
                in_blend,
                pit_out,
                in_pct if in_pct is not None else self._pit_surf_entry_pct,
                out_pct,
                capped,
            )
        self._pit_phase = None
        self._pit_enter_pct = None
        self._pit_exit_pct = None
        self._pit_in_pct_cur = None
        self._pit_exit_ticks = 0
        self._pit_merge_pct = None
        self._pit_entry_buf = []
        self._pit_surf_entry_pct = None
        self._pit_surf_exit_pct = None

    def _record_pit_pass(self, entry_pct, exit_pct, lane, pit_in, pit_out,
                         in_pct, out_pct, capped=False) -> None:
        """Accumulate a completed pit pass; finalize after PIT_PASSES of them.

        Locked until the track scan (SCAN_LAPS laps) is done -- a pass driven
        before then just flashes a hint. Each pass carries the lane geometry,
        the entry/exit blend lines, and the lap-% extent of the whole route.
        Once enough passes are gathered everything is averaged (percentages by
        circular mean, polylines by arc-length resampling) and saved + uploaded.
        """
        if not self._scan_done:
            self.map_widget.flash_hint("Finish track scan first")
            return
        if len(self._pit_passes) >= PIT_PASSES:
            return  # already finalized; use "Rescan pits" to redo
        # Correct dead-reckoning drift. Entry/exit blends are short and hug the
        # track, so re-anchor them by lap % (exact). The main pit lane is left
        # in the LSQ-aligned frame only -- on a road course the lane can span
        # most of a lap in CarIdxLapDistPct while physically running just the
        # pit straight; re-anchoring that by % would wrap the lane around the
        # whole circuit.
        pit_in, lane, pit_out = self._align_pit_to_track(
            pit_in, lane, pit_out, in_pct, out_pct, entry_pct, exit_pct)
        pit_in = self._seg_to_xy(self._reanchor_by_lap_pct(pit_in))
        lane = self._seg_to_xy(lane)
        pit_out = self._seg_to_xy(self._reanchor_by_lap_pct(pit_out))
        self._pit_passes.append((entry_pct, exit_pct, self._pit_speed_ms,
                                 lane, pit_in, pit_out, in_pct, out_pct,
                                 self._pit_lane_offset, capped))
        n = len(self._pit_passes)
        n_in = len(pit_in) if pit_in else 0
        n_out = len(pit_out) if pit_out else 0
        # Diagnostics: surfaces whether the entry/exit blends were captured (they
        # need track geometry to measure divergence from the racing line).
        log.warning("pit pass %d/%d: entry_blend=%d lane=%d exit_blend=%d "
                    "lane_offset=%.3e track_geom=%s pos=%s align=%s(%d)",
                    n, PIT_PASSES, n_in, len(lane) if lane else 0, n_out,
                    self._pit_lane_offset, bool(self._track_pts),
                    "gps" if self._player_pos_gps else "dead-reckon",
                    "lsq" if getattr(self, "_pit_align_ls", False) else "2pt",
                    getattr(self, "_pit_align_n", 0))
        # Preview the latest pass (span + geometry) while gathering more.
        self.map_widget.set_pit((entry_pct, exit_pct), self._pit_speed_ms)
        if lane and len(lane) >= 2:
            self.map_widget.set_pit_path(lane)
        self.map_widget.set_pit_blends(pit_in, pit_out)
        self.map_widget.set_pit_route_pct(in_pct, out_pct)
        self._update_scan_status()
        if n < PIT_PASSES:
            if not self._track_pts:
                self.map_widget.flash_hint(
                    f"Pit pass {n}/{PIT_PASSES} \u00b7 no track geometry")
            elif n_in < 2 or n_out < 2:
                self.map_widget.flash_hint(
                    f"Pit pass {n}/{PIT_PASSES} \u00b7 blend not captured")
            return

        # Discard outlier passes (e.g. the first lap after the track scan, before
        # dead reckoning re-zeros at the line) so they don't corrupt the mean.
        passes = self._select_pit_passes(self._pit_passes)
        m = len(passes)
        log.warning("pit finalize: kept %d/%d passes, offsets=%s",
                    m, len(self._pit_passes),
                    [round(p[8], 1) for p in passes])
        # Use the median lane offset from the kept passes for blend parallel width.
        offs = sorted(p[8] for p in passes if p[8] and p[8] > 0)
        if offs:
            self._pit_lane_offset = offs[len(offs) // 2]
        # Keep the full entry/exit blends -- the pit approach/exit genuinely hug
        # the track (through the turn and along the straight) -- but nudge the
        # parts that hug the racing line into the infield so they read as a lane
        # running parallel to the track rather than drawn on top of it.
        span = self._avg_pit_span(passes)
        speed = sum(p[2] for p in passes) / m
        # Smooth each finished polyline. Blends get the parallel-lane nudge;
        # the main lane keeps the LSQ-aligned capture shape (re-anchoring it by
        # lap % would wrap a road-course pit around the whole circuit).
        self._pit_path = track_map._smooth_open(
            self._avg_pit_path([p[3] for p in passes]), passes=2)
        self._pit_in = track_map._smooth_open(self._offset_blend_parallel(
            self._avg_pit_path([p[4] for p in passes])), passes=2)
        # The exit blend must come only from passes that genuinely merged back
        # onto the racing line (p[9] is the "capped" flag). A capped pass kept
        # driving past the merge -- often a whole extra lap to re-pit -- so its
        # exit blend runs far too long and would drag the lane onto the track.
        merged = [p for p in passes if not p[9]] or passes
        self._pit_out = track_map._smooth_open(self._offset_blend_parallel(
            self._avg_pit_path([p[5] for p in merged])), passes=2)
        self._pit_in_pct = self._circ_mean([p[6] for p in passes])
        self._pit_out_pct = self._circ_mean([p[7] for p in merged])
        self._pit_span = span
        self._pit_speed_ms = speed
        self.map_widget.set_pit(self._pit_span, self._pit_speed_ms)
        self.map_widget.set_pit_path(self._pit_path)
        self.map_widget.set_pit_blends(self._pit_in, self._pit_out)
        self.map_widget.set_pit_route_pct(self._pit_in_pct, self._pit_out_pct)
        self._update_scan_status()
        self.map_widget.flash_hint("Pit lane saved")
        if self._track_id is not None:
            fields = dict(pit_span=[round(span[0], 5), round(span[1], 5)],
                          pit_speed=round(speed, 3))
            for key, seg in (("pit_path", self._pit_path),
                             ("pit_in", self._pit_in),
                             ("pit_out", self._pit_out)):
                if seg:
                    fields[key] = [[round(x, 7), round(y, 7)] for x, y in seg]
            if self._pit_in_pct is not None:
                fields["pit_in_pct"] = round(self._pit_in_pct, 5)
            if self._pit_out_pct is not None:
                fields["pit_out_pct"] = round(self._pit_out_pct, 5)
            try:
                track_map.update_track_meta(
                    self.tracks_dir, self._track_id, **fields)
            except Exception:
                pass
            if config.cloud_tracks():
                self._track_sync.upload_local_async(
                    self.tracks_dir, self._track_id)

    @staticmethod
    def _circ_mean(vals):
        """Circular mean of lap percentages (handles start/finish wrap), or None
        if no usable values are given."""
        vals = [v for v in vals if v is not None]
        if not vals:
            return None
        sx = sum(math.cos(2 * math.pi * v) for v in vals)
        sy = sum(math.sin(2 * math.pi * v) for v in vals)
        if sx == 0.0 and sy == 0.0:
            return vals[0]
        return (math.atan2(sy, sx) / (2 * math.pi)) % 1.0

    @staticmethod
    def _select_pit_passes(passes):
        """Pick the pit passes worth averaging together.

        Keeps passes that captured both blend lines and a genuine exit merge.
        Also drops capture-time outliers (usually pass 1 right after the track
        scan): dead reckoning can still be ~100 m off in the lane even after
        LSQ alignment, which warps the averaged pit path while the blends (re-
        anchored by lap % and surface zones) still look fine.
        """
        if len(passes) <= 1:
            return list(passes)

        def has_both(p):
            return p[4] and len(p[4]) >= 2 and p[5] and len(p[5]) >= 2

        merged = [p for p in passes if not p[9]]
        kept = [p for p in merged if has_both(p)] or [p for p in passes
                                                       if has_both(p)]
        if not kept:
            return merged or list(passes)
        if len(kept) <= 1:
            return kept

        offs = sorted(p[8] for p in kept if p[8] and p[8] > 0)
        if len(offs) >= 2:
            lo = offs[0]
            # Reject passes whose lane offset blew up vs the best capture (e.g.
            # 35 m vs 12 m on a road course after dead-reckoning drift).
            cap = max(lo * 2.0, lo + 8.0)
            near = [p for p in kept if p[8] and p[8] <= cap]
            if near:
                kept = near
        return kept

    @classmethod
    def _avg_pit_span(cls, passes) -> tuple:
        """Circular mean of entry/exit lap percentages across passes, so a pit
        lane that straddles the start/finish line (e.g. 0.98 & 0.04) averages
        correctly instead of collapsing to mid-lap."""
        return (cls._circ_mean([p[0] for p in passes]),
                cls._circ_mean([p[1] for p in passes]))

    @staticmethod
    def _avg_pit_path(geos, n: int = 120):
        """Average the recorded pit-lane traces into one smooth path.

        Each pass is resampled to n arc-length-even points (entry -> exit) and
        the passes are averaged point-by-point. Returns None if no usable GPS
        geometry was captured (e.g. dead-reckoning fallback)."""
        usable = [g for g in geos if g and len(g) >= 2]
        if not usable:
            return None
        res = [track_map._resample_open(g, n) for g in usable]
        m = len(res)
        return [(sum(r[k][0] for r in res) / m, sum(r[k][1] for r in res) / m)
                for k in range(n)]

    # iRacing SessionFlags bitfield bits (irsdk_Flags).
    _FLAG_CHECKERED = 0x00000001           # session finished
    _FLAG_WHITE = 0x00000002               # white flag (final lap)
    _FLAG_GREEN = 0x00000004 | 0x00000400   # green + green-held
    _FLAG_YELLOW = 0x00000008 | 0x00000100 | 0x00004000 | 0x00008000
    _FLAG_RED = 0x00000010                  # session stopped
    _FLAG_BLUE = 0x00000020                 # faster car behind, let it by
    _FLAG_DEBRIS = 0x00000040               # debris on track
    _FLAG_CROSSED = 0x00000080              # crossed flag = race halfway
    _FLAG_BLACK = 0x00010000                # black flag (penalty)
    _FLAG_DQ = 0x00020000                   # disqualified
    _FLAG_FURLED = 0x00080000               # furled/rolled black = warning
    _FLAG_REPAIR = 0x00100000               # meatball (must pit to repair)

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
            "flag": (self._session_flag()
                     if config.CFG["dash"].get("show_flags", True) else None),
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
        self.flags_widget.set_data({"flag": self._session_flag(),
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
        if not isinstance(t, (int, float)) or t < 0 or t > 604000:
            t = None
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

    hud = AdvancedSimHUD(click_through=click_through, demo=demo)

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
