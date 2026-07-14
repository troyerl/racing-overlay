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
import threading
import time
from collections import deque
from datetime import datetime, timezone

from PyQt6.QtCore import QObject, QTimer, Qt, pyqtSlot
from PyQt6.QtWidgets import QApplication, QMessageBox, QProgressDialog

from . import common as oc
from . import config
from . import constants
from . import demo_data
from . import paths
from . import irating_calc
from . import layout_store
from . import pit_strategy as pstrat
from . import sysstats
from . import track_store
from . import traffic as tr
from . import telemetry as tele
from . import version
from . import driver_groups as dgroups
from .busy_dialog import BusySpinnerDialog
from .map_markers import (
    fresh_hold_states, resolve_traffic_markers, select_marker_candidates,
    wrap_lap_delta,
)
from .standings_rows import standings_row_list
from .panel import PanelWindow
from .widgets import track_map
from .widgets.track_map import TrackMapWidget, is_schematic_pit_source
from .widgets.dash import DashWidget
from .widgets.delta_bar import DeltaBarWidget
from .widgets.flags import FlagsWidget
from .widgets.ers_hybrid import ErsHybridWidget
from .widgets.fuel_calc import FuelCalcWidget
from .widgets.inputs import InputTraceWidget
from .widgets.lap_compare import LapCompareEngine, LapCompareWidget
from .widgets.laptime_log import LaptimeLogWidget
from .widgets.leaderboard_strip import LeaderboardStripWidget
from .widgets.pit_advisor import PitAdvisorWidget
from .widgets.pit_board import PitBoardWidget
from .widgets.system_panel import SystemPanelWidget
from .widgets.radio_tower import RadioTowerWidget
from .widgets.radar import RadarWidget
from .widgets.relative import RelativeWidget
from .widgets.sector_timing import SectorTimer, SectorTimingWidget
from .widgets.standings import StandingsWidget
from .widgets.tire_panel import TirePanelWidget
from .widgets.weather_panel import WeatherPanelWidget
from . import hybrid as hy
from . import pit_service as ps

log = logging.getLogger("gridglance.app")

# Default window geometry per panel: (x, y, w, h). Overridden by saved layout.
DEFAULT_GEOMS = {
    "standings": (40, 120, 560, 360),
    "relative": (620, 120, 600, 360),
    "radar": (40, 500, 200, 260),
    "map": (620, 500, 480, 320),
    "dash": (260, 680, 660, 190),
    "laptime_log": (40, 500, 380, 320),
    "fuel_calc": (440, 500, 460, 380),
    "inputs": (260, 600, 660, 150),
    "delta_bar": (380, 60, 420, 120),
    "flags": (820, 60, 320, 150),
    "sector_timing": (380, 200, 360, 170),
    "lap_compare": (40, 60, 380, 320),
    "tire_panel": (920, 500, 220, 180),
    "pit_board": (920, 700, 240, 200),
    "weather_panel": (40, 720, 260, 150),
    "leaderboard_strip": (320, 720, 96, 300),
    "radio_tower": (200, 720, 220, 56),
    "ers_hybrid": (620, 720, 220, 110),
    "system_panel": (1140, 720, 180, 150),
    "pit_advisor": (420, 720, 220, 100),
}

_WIDGET_KEYS = (
    "standings", "relative", "radar", "map", "dash",
    "laptime_log", "fuel_calc", "inputs", "delta_bar",
    "flags", "sector_timing", "lap_compare",
    "tire_panel", "pit_board", "weather_panel",
    "leaderboard_strip", "radio_tower", "ers_hybrid", "system_panel", "pit_advisor",
)

_TRACK_FETCH_RETRY_SEC = 10.0

# Pit route latch: consecutive ticks on pit road before drawing player on route.
PIT_COMMIT_HOLD = 15


def _coerce_int(value):
    """Best-effort int from a telemetry field (which may be str/float/None)."""
    try:
        return int(value)
    except (TypeError, ValueError):
        return None


def _weekend_split_field(wk: dict) -> int | None:
    """Pull an undocumented numeric split field from WeekendInfo if present."""
    if not isinstance(wk, dict):
        return None
    candidates = (
        "RaceSplit", "SplitNum", "SplitNumber", "SessionSplit",
        "SessionSplitNum", "EventSplit",
    )
    for key in candidates:
        if key in wk:
            n = _coerce_int(wk.get(key))
            if n is not None and n > 0:
                return n
    for key, val in wk.items():
        if "split" not in str(key).casefold():
            continue
        n = _coerce_int(val)
        if n is not None and n > 0:
            return n
    opts = wk.get("WeekendOptions")
    if isinstance(opts, dict):
        for key, val in opts.items():
            if "split" not in str(key).casefold():
                continue
            n = _coerce_int(val)
            if n is not None and n > 0:
                return n
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
        self._map_authoring_depth = 0
        self._map_click_through_saved: bool | None = None
        self._profile_loading_depth = 0
        self._profile_loading_dialog: BusySpinnerDialog | None = None
        self._profile_loading_shown_at = 0.0

        # Repaint + re-apply widget visibility when the config changes (editor UI).
        config.on_change(self._on_config_change)
        # Reapply the saved window layout when the active preset changes.
        config.on_preset_change(self._on_preset_change)
        # Swap race/garage widget positions when the context changes.
        config.on_context_change(self._on_context_change)
        # Last-seen car path + LeagueID (drives auto-switch of presets); the
        # league id is cached because reading WeekendInfo is an expensive parse.
        self._last_car_path: str | None = None
        self._last_league_id: int | None = None
        self._league_id_cache: int | None = None

        self._driver_cache: dict[int, dict] = {}
        # CarIdx values of pace/safety cars, so they're never shown as competitors.
        self._pace_idxs: set[int] = set()
        # Local driver identity from DriverInfo YAML (not live PlayerCarIdx).
        self._driver_car_idx: int | None = None
        self._driver_user_id: int | None = None
        # Bypass the driver-cache throttle once (session change / identity mismatch).
        self._force_driver_refresh = False
        # Map ahead/behind/leader icon hold state (debounce side-by-side flicker).
        self._marker_hold = fresh_hold_states()
        # Engine/shift-light params from the session YAML (cached with drivers).
        self._car_info: dict = {}
        self._driver_refresh_counter = 0
        self._session_num: int | None = None
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
        self._sys_cache: tuple[str, str, str, float | None, float | None, float | None] | None = None
        self._sys_counter = 0
        self._track_loaded = False        # a track file is in use
        self._loaded_track_updated_at: str | None = None
        self._track_file_checked = False  # we've looked for a file for this track
        self._track_id = None             # current track's iRacing TrackID
        self._track_turns = None          # WeekendInfo TrackNumTurns (corner count)
        self._track_is_oval = False       # oval vs road (metadata only)
        self._learn_name = ""             # display name for saved tracks
        self._no_track_hint = False       # throttle "import HTML" flash
        self._track_fetch_last = 0.0      # monotonic time of last cloud fetch attempt
        self._pit_speed_ms = 0.0
        self._pit_lane_speed_pct = 1.0
        self._pit_latch_seed_pending = False
        self._pit_span = None
        self._pit_path = None
        self._pit_in = None
        self._pit_out = None
        self._pit_in_pct = None
        self._pit_out_pct = None
        self._pit_span_2 = None
        self._pit_path_2 = None
        self._pit_in_2 = None
        self._pit_out_2 = None
        self._pit_in_pct_2 = None
        self._pit_out_pct_2 = None
        self._pit_lane_speed_pct_2 = 1.0
        self._player_on_route = False
        self._player_route_ticks = 0
        self._pit_route_latch: dict[int, bool] = {}
        self._pit_prev_on: dict[int, bool] = {}
        self._pit_exit_latch: dict[int, float] = {}
        self._pit_source = ""
        self._v2_loop_doc: dict | None = None
        self._v2_authoring_track_id = None
        self._v2_authoring_name = ""
        self._alias_track_ids: list[int] = []
        # Sector timing: derives sector splits from lap-distance crossings.
        self._sector_timer = SectorTimer()
        # Lap compare: records per-lap input traces and analyses corners.
        self._lap_engine = LapCompareEngine()
        self._track_len_m = 0.0  # parsed once from the session info
        # Cached session-info reads (refreshed on a throttle, not every tick).
        self._lc_key = None
        self._sector_starts_cache = None
        self._session_info_counter = 0
        self._practice_cache: bool | None = None
        self._qualifying_cache: bool | None = None
        self._session_type_cache = ""
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
        self._ll_lap_start_incidents = None
        self._ll_lap_on_pit = False
        self._ll_lap_tag: str | None = None
        self._ll_personal_best: float | None = None
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
        # EstTime delta history for the closing-rate column.
        self._closing_state: dict = {}
        self._closing_session_uid: int | None = None
        self._pit_advisor_closing_state: dict = {}
        self._pit_advisor_has_content = False
        self._radar_closing_state: dict = {}
        self._radar_clear_since: float | None = None
        self._need_weekend_info = False
        self._track_zones: dict = {}
        self._delta_last_lap_time: float | None = None
        self._delta_prev_lap: int | None = None
        self._delta_pit_hold: bool = False
        self._delta_was_on_pit: bool = False
        self._delta_pit_hold_since: float | None = None
        self._lap_wetness_start: float | None = None
        self._weather_track_hist: deque | None = None
        self._weather_sample_counter = 0
        self._ll_laps_version = 0
        self._ll_render_version = -1
        self._fuel_payload_key = None
        self._lap_compare_snap_key = None
        self._caution_tracker = pstrat.CautionTracker()
        self._visible_widgets: dict[str, bool] = {}
        self._cfg_section_snap: dict = {}
        # Throttled WeekendInfo cache for header/footer slots.
        self._weekend_cache: dict = {}
        self._weekend_counter = 0
        # Registration split (throttled; optional iRacing results API).
        self._race_split_cache: int | None = None
        self._race_split_subsession: int | None = None
        self._race_split_fetch_at: float = 0.0
        self._race_split_fetching = False
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
        self._track_sync.app_settingsFetched.connect(self._on_app_settings_fetched)
        self._remote_tried: set = set()  # track ids we've already asked for
        self._demo_track_pending_id: str | None = None
        self._shared_demo_track_id: str | None = None
        self._session_demo_track_id: str | None = None
        self._pro_drivers: list[dict] = []
        self._driver_groups: list[dict] = []
        self._reload_driver_groups()
        self._apply_app_settings_cache()
        # MongoDB is the source of truth: on launch, refresh the local cache so
        # any maps the author changed are pulled in (runs off the GUI thread).
        if not self.demo:
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
        self.tire_panel_widget = TirePanelWidget()
        self.pit_board_widget = PitBoardWidget()
        self.weather_panel_widget = WeatherPanelWidget()
        self.leaderboard_strip_widget = LeaderboardStripWidget()
        self.radio_tower_widget = RadioTowerWidget()
        self.ers_hybrid_widget = ErsHybridWidget()
        self.system_panel_widget = SystemPanelWidget()
        self.pit_advisor_widget = PitAdvisorWidget()
        if self.demo:
            self._load_demo_track()
            self._fetch_shared_app_settings()
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
        self._wrap("tire_panel", self.tire_panel_widget)
        self._wrap("pit_board", self.pit_board_widget)
        self._wrap("weather_panel", self.weather_panel_widget)
        self._wrap("leaderboard_strip", self.leaderboard_strip_widget)
        self._wrap("radio_tower", self.radio_tower_widget)
        self._wrap("ers_hybrid", self.ers_hybrid_widget)
        self._wrap("system_panel", self.system_panel_widget)
        self._wrap("pit_advisor", self.pit_advisor_widget)
        self._refresh_visible_widgets()

    @staticmethod
    def _is_shown(key: str) -> bool:
        return bool(config.CFG.get(key, {}).get("show", True))

    def _refresh_visible_widgets(self) -> None:
        self._visible_widgets = {k: self._is_shown(k) for k in _WIDGET_KEYS}
        if not self._visible_widgets.get("weather_panel"):
            self._clear_weather_hist()

    @staticmethod
    def _needs_sector_timer(en: dict) -> bool:
        scfg = config.CFG.get("sector_timing", {})
        dash = config.CFG.get("dash", {})
        return (
            en.get("sector_timing")
            or en.get("delta_bar")
            or dash.get("show_delta_bar")
            or (en.get("dash") and config.dash_metric_in_use("delta"))
            or (en.get("laptime_log") and config.laptime_log_has_column("sectors"))
            or (en.get("map") and scfg.get("highlight_active_sector_on_map"))
        )

    @staticmethod
    def _needs_delta_last_lap_ref(en: dict) -> bool:
        dash = config.CFG.get("dash", {})
        dash_wants = (
            (dash.get("show_delta_bar")
             or config.dash_metric_in_use("delta"))
            and dash.get("delta_bar_mode", "session_best") == "last_lap"
        )
        bar_wants = (
            en.get("delta_bar")
            and config.CFG.get("delta_bar", {}).get("mode") == "last_lap"
        )
        return bool(dash_wants or bar_wants)

    @staticmethod
    def _needs_lap_pct_for_delta(en: dict) -> bool:
        dash = config.CFG.get("dash", {})
        return bool(
            en.get("delta_bar")
            or dash.get("show_delta_bar")
            or (en.get("dash") and config.dash_metric_in_use("delta"))
        )

    @staticmethod
    def _needs_lap_engine(en: dict) -> bool:
        return en.get("lap_compare") or (
            en.get("dash") and config.dash_metric_in_use("lap_corners"))

    @staticmethod
    def _needs_fuel_lap_tracking(en: dict) -> bool:
        return en.get("fuel_calc") or en.get("pit_advisor") or (
            en.get("laptime_log") and config.laptime_log_has_column("fuel"))

    def _clear_weather_hist(self) -> None:
        if self._weather_track_hist is not None:
            self._weather_track_hist.clear()
        self._weather_sample_counter = 0

    def _ensure_weather_hist(self) -> deque:
        wcfg = config.CFG.get("weather_panel", {})
        window = int(float(wcfg.get("trend_window_seconds", 300.0) or 300.0))
        maxlen = max(60, window + 10)
        if self._weather_track_hist is None:
            self._weather_track_hist = deque(maxlen=maxlen)
        elif self._weather_track_hist.maxlen != maxlen:
            self._weather_track_hist = deque(
                self._weather_track_hist, maxlen=maxlen)
        return self._weather_track_hist

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
        # Map authoring (pit/corner/SF) needs the map panel interactive even
        # when the rest of the overlay stays click-through.
        if self._map_authoring_depth > 0:
            map_win = self._win_by_key.get("map")
            if map_win is not None:
                map_win.set_click_through(False)
        # Edit mode reveals panels even when the sim isn't connected so the
        # layout can be arranged offline (see _apply_visibility).
        self._apply_visibility()

    def _set_map_authoring_interactive(self, on: bool) -> None:
        """Ref-count map panel mouse capture for pit/corner/SF authoring."""
        map_win = self._win_by_key.get("map")
        if map_win is None:
            return
        if on:
            if self._map_authoring_depth == 0:
                self._map_click_through_saved = map_win.click_through
                map_win.set_click_through(False)
            self._map_authoring_depth += 1
            return
        if self._map_authoring_depth <= 0:
            return
        self._map_authoring_depth -= 1
        if self._map_authoring_depth == 0:
            if self._map_click_through_saved is not None:
                map_win.set_click_through(self._map_click_through_saved)
            self._map_click_through_saved = None

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
            if key == "pit_advisor" and want and not self.edit_mode_enabled():
                want = bool(self._pit_advisor_has_content)
            if want and not win.isVisible():
                win.ensure_on_screen()
                win.show()
            elif not want and win.isVisible():
                win.hide()

    def _on_config_change(self, cfg) -> None:
        self._reload_driver_groups(cfg)
        self._refresh_visible_widgets()
        self._apply_visibility()
        self._repaint_config_sections(cfg)

    def _apply_layout(self, persist_clamps: bool = True) -> None:
        """Reload the active context's layout onto every panel window."""
        self._layout_state.clear()
        self._layout_state.update(config.active_layout())
        layout_changed = False
        for key, win in self._win_by_key.items():
            geom = self._layout_state.get(key) or DEFAULT_GEOMS.get(key)
            if geom:
                x, y, w, h = layout_store.clamp_panel_geometry(*geom)
                win.setGeometry(x, y, w, h)
                if list(geom) != [x, y, w, h]:
                    self._layout_state[key] = [x, y, w, h]
                    layout_changed = True
        if persist_clamps and layout_changed:
            config.save_active_layout(self._layout_state)

    def _show_profile_loading(self, message: str = "Loading preset\u2026") -> None:
        """Show a blocking busy spinner while a preset switch applies."""
        self._profile_loading_depth += 1
        if self._profile_loading_dialog is not None:
            self._profile_loading_dialog.set_message(message)
            return
        parent = self._settings_window
        if parent is None or not parent.isVisible():
            parent = None
            for win in self._win_by_key.values():
                if win.isVisible():
                    parent = win
                    break
        dlg = BusySpinnerDialog(message, parent)
        self._profile_loading_dialog = dlg
        self._profile_loading_shown_at = time.monotonic()
        dlg.show()
        dlg.raise_()
        QApplication.processEvents()

    def _hide_profile_loading(self) -> None:
        self._profile_loading_depth = 0
        dlg = self._profile_loading_dialog
        self._profile_loading_dialog = None
        self._profile_loading_shown_at = 0.0
        if dlg is not None:
            dlg.stop()
            dlg.close()
            dlg.deleteLater()

    def _finish_profile_loading(self) -> None:
        """Repaint, keep the spinner visible briefly so it can animate, then dismiss."""
        if self._profile_loading_dialog is None and self._profile_loading_depth <= 0:
            return
        self._repaint_all()
        QApplication.processEvents()
        # Minimum show so the spinner ticks after the switch starts (bar used to freeze).
        shown_at = self._profile_loading_shown_at
        if shown_at > 0:
            deadline = shown_at + 0.25
            while time.monotonic() < deadline:
                QApplication.processEvents()
                time.sleep(0.016)
        self._hide_profile_loading()

    def _on_preset_change(self, _name) -> None:
        """Reapply the newly active preset's saved window layout to every panel."""
        try:
            if self._profile_loading_depth <= 0:
                label = f"Loading preset\u2026 {config.active_preset()}"
                self._show_profile_loading(label)
            self._apply_layout()
            QApplication.processEvents()
            self._refresh_visible_widgets()
            QApplication.processEvents()
            self._apply_visibility()
            QApplication.processEvents()
            self.map_widget._invalidate_static_cache()
        finally:
            self._finish_profile_loading()

    def _on_context_change(self, _ctx) -> None:
        """Swap panels to the race or garage layout for the active preset."""
        label = config.CONTEXT_LABELS.get(
            config.effective_context(), config.effective_context())
        try:
            if self._profile_loading_depth <= 0:
                self._show_profile_loading(f"Loading profile\u2026 {label}")
            # Don't persist clamps here — that would bake race fallbacks into
            # layout_garage just because a widget needed an on-screen nudge.
            self._apply_layout(persist_clamps=False)
            QApplication.processEvents()
            self._refresh_visible_widgets()
            QApplication.processEvents()
            self._apply_visibility()
            QApplication.processEvents()
            self.map_widget._invalidate_static_cache()
        finally:
            self._finish_profile_loading()

    def _update_context(self) -> None:
        """Pick the 'garage' or 'race' profile from telemetry.

        ``IsInGarage`` is true when the player's car has garage physics running.
        Spectators / out-of-car clients often have that false while the garage
        UI is open — ``IsGarageVisible`` covers that case.
        """
        def _flag(key: str) -> bool:
            try:
                return bool(self.ir[key])
            except Exception:
                return False

        in_garage = _flag("IsInGarage") or _flag("IsGarageVisible")
        ctx = "garage" if in_garage else "race"
        if ctx != config.active_context():
            config.set_context(ctx)
        # Settings pins a preview context for editing; that must not block
        # telemetry-driven garage/race switching on the live overlay.
        pin = config.preview_context()
        if pin is not None and pin != ctx:
            config.set_preview_context(None)
        self._maybe_auto_switch_preset()

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
            "canonical_track_id": self._canonical_track_id(tid),
            "alias_track_ids": list(self._alias_track_ids),
        }

    def _canonical_track_id(self, tid):
        """On-disk / cloud id for a session or authoring TrackID."""
        if tid is None:
            return None
        return track_store.resolve_track_id(self.tracks_dir, tid) or tid

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
        canonical = self._canonical_track_id(tid)
        corners = track_map.corners_to_json(self.map_widget.display_corners())
        pit_span = self._pit_span
        return track_map.ensure_track_file(
            self.tracks_dir, canonical, self.map_widget.path,
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
            pit_span_2=self._pit_span_2,
            pit_path_2=self._pit_path_2,
            pit_in_2=self._pit_in_2,
            pit_out_2=self._pit_out_2,
            pit_in_pct_2=self._pit_in_pct_2,
            pit_out_pct_2=self._pit_out_pct_2,
            pit_lane_speed_pct_2=self._pit_lane_speed_pct_2,
            learned=False)

    def effective_track_id(self):
        """TrackID for file naming (live session, or demo track id in demo mode)."""
        if self._track_id is not None:
            return self._track_id
        if self.demo:
            return self._resolve_demo_track_id()
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
        canonical = self._canonical_track_id(tid)
        if not self._ensure_local_track_file():
            return False
        try:
            ok = track_map.update_track_meta(
                self.tracks_dir, canonical, **fields)
        except Exception:
            return False
        if not ok:
            return False
        if track_store.can_write():
            self._track_sync.upload_local_async(self.tracks_dir, canonical)
        return True

    def set_pit_speed_authoring(self, speed_ms: float) -> bool:
        """Set the pit speed limit manually and save to the track record."""
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

    def set_alias_track_ids_authoring(self, ids: list[int]) -> bool:
        """Save alternate iRacing TrackIDs that share this map layout."""
        canonical = self._canonical_track_id(self._authoring_track_id())
        if canonical is None:
            return False
        self._alias_track_ids = track_store._normalize_alias_ids(ids, canonical)
        return self._persist_track_meta(
            alias_track_ids=(self._alias_track_ids or None))

    def set_corner_edit_mode(self, enabled: bool) -> None:
        """Toggle drag-to-move corner labels on the map widget."""
        was = self.map_widget.corner_edit_mode
        if enabled:
            self.set_pit_edit_mode(False)
            self.set_sf_edit_mode(False)
        self.map_widget.set_corner_edit(
            enabled, self._save_corners_authoring if enabled else None)
        if bool(enabled) != was:
            self._set_map_authoring_interactive(enabled)

    def _uncheck_sf_edit_toggle(self) -> None:
        w = self._settings_window
        if w is not None and hasattr(w, "_sf_edit_sw"):
            w._sf_edit_sw.blockSignals(True)
            w._sf_edit_sw.setChecked(False)
            w._sf_edit_sw.blockSignals(False)

    def set_sf_edit_mode(self, enabled: bool) -> None:
        """Toggle drag-to-move start/finish along the racing loop."""
        was = self.map_widget.sf_edit_mode
        if enabled:
            self.set_pit_edit_mode(False)
            self.set_corner_edit_mode(False)
            self.map_widget.flash_hint(
                "Drag the white start/finish line along the track")
        self.map_widget.set_sf_edit(
            enabled, self._save_sf_authoring if enabled else None)
        if bool(enabled) != was:
            self._set_map_authoring_interactive(enabled)

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

    def parse_loop_v2(
        self, html_path: str,
    ) -> tuple[bool, str, dict | None, int | None]:
        """Thread-safe: parse members HTML into a loop doc (no Qt widgets)."""
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
            return False, msg, None, None

        tid = parse_track_id_from_html(html_path=html_path)
        if tid is None:
            tid = self.effective_track_id()
        if tid is None:
            msg = ("No TrackID — save members HTML with id=\"track-map-123\" "
                   "(outer track-map div), or join a track in iRacing.")
            log.warning("v2 loop import skipped: %s", msg)
            return False, msg, None, None

        try:
            doc = import_track_source_v2(
                html_path,
                num_corners=int(self._track_turns or 4) or 4,
            )
        except Exception as exc:
            log.exception("v2 loop import failed")
            msg = str(exc)
            log.warning("v2 loop import failed: %s", msg)
            return False, msg, None, None

        return True, "", doc, tid

    def apply_loop_v2_import(
        self, doc: dict, tid: int, html_path: str,
    ) -> tuple[bool, str]:
        """GUI thread: load a parsed loop doc onto the map."""
        stem = os.path.splitext(os.path.basename(html_path))[0]
        self._v2_authoring_track_id = tid
        self._v2_authoring_name = stem or str(tid)
        self._apply_loop_v2_doc(doc)
        self._v2_loop_doc = doc
        n = len(doc.get("points") or [])
        msg = (f"Loop imported for TrackID {tid} — {n} pts. "
               f"Save loop to upload now, or draw pit road + merge and Save track.")
        log.info("v2 loop import OK for TrackID %s (%d pts)", tid, n)
        return True, msg

    def import_loop_v2(self, html_path: str) -> tuple[bool, str]:
        """Import racing loop from members HTML (v2); pit drawn manually on map."""
        ok, msg, doc, tid = self.parse_loop_v2(html_path)
        if not ok:
            return False, msg
        return self.apply_loop_v2_import(doc, tid, html_path)

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
        self.map_widget.set_track_is_oval(self._track_is_oval)
        self._pit_source = "manual"
        self.map_widget.set_pit_source("manual")
        self._pit_span = None
        self._pit_path = self._pit_in = self._pit_out = None
        self._pit_in_pct = self._pit_out_pct = None
        self._pit_span_2 = None
        self._pit_path_2 = self._pit_in_2 = self._pit_out_2 = None
        self._pit_in_pct_2 = self._pit_out_pct_2 = None
        self.map_widget.clear_pit()
        self.map_widget.clear_pit_edit()
        self._apply_track_orientation({
            "schema": 2,
            "import_version": 2,
            "map_rotation": 0,
            "map_mirror": False,
        })
        self._track_loaded = True
        self._refresh_settings_authoring()
        self.map_widget.flash_hint(
            "Loop imported — map orientation reset to match HTML")
        self.map_widget.update()

    def _pit_pts_from_saved(self, seg) -> list[tuple[float, float]]:
        pts: list[tuple[float, float]] = []
        for pt in seg or []:
            if isinstance(pt, (list, tuple)) and len(pt) >= 2:
                pts.append((float(pt[0]), float(pt[1])))
        return pts

    def _load_saved_pit_lane_into_editor(self, *, lane: int) -> bool:
        if lane == 2:
            entry_src, road_src, merge_src = (
                self._pit_in_2, self._pit_path_2, self._pit_out_2)
        else:
            entry_src, road_src, merge_src = (
                self._pit_in, self._pit_path, self._pit_out)
        entry_pts = self._pit_pts_from_saved(entry_src)
        if (len(entry_pts) >= 2
                and abs(entry_pts[0][0] - entry_pts[-1][0]) < 1e-6
                and abs(entry_pts[0][1] - entry_pts[-1][1]) < 1e-6):
            entry_pts = []
        road_pts = self._pit_pts_from_saved(road_src)
        merge_pts = self._pit_pts_from_saved(merge_src)
        if len(road_pts) < 2 and len(merge_pts) < 2 and len(entry_pts) < 2:
            return False
        self.map_widget.load_pit_edit(
            road_pts if len(road_pts) >= 2 else [],
            merge_pts if len(merge_pts) >= 2 else [],
            entry_pts if len(entry_pts) >= 2 else [],
            lane=lane,
        )
        return True

    def load_pit_into_editor(self, *, force: bool = False) -> bool:
        """Copy saved pit entry / road / exit merge into the manual edit buffers."""
        e1, r1, m1 = self.map_widget.pit_edit_snapshot(lane=1)
        e2, r2, m2 = self.map_widget.pit_edit_snapshot(lane=2)
        if not force and (
                len(e1) >= 2 or len(r1) >= 2 or len(m1) >= 2
                or len(e2) >= 2 or len(r2) >= 2 or len(m2) >= 2):
            return True
        loaded1 = self._load_saved_pit_lane_into_editor(lane=1)
        loaded2 = self._load_saved_pit_lane_into_editor(lane=2)
        return loaded1 or loaded2

    def set_pit_edit_mode(self, enabled: bool, phase: str = "road",
                          lane: int | None = None) -> None:
        """Toggle manual pit authoring clicks on the live map."""
        was = self.map_widget.pit_edit_mode
        if lane is not None:
            self.map_widget.set_pit_edit_lane(lane)
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
        self.map_widget.set_pit_edit_phase(phase)
        if bool(enabled) != was:
            self._set_map_authoring_interactive(enabled)

    def clear_pit_edit_phase(self, phase: str, lane: int | None = None) -> None:
        """Clear one pit-edit phase on the map (entry, road, or merge)."""
        phase = (phase or "road").strip().lower()
        lane = self.map_widget.pit_edit_lane if lane is None else (
            2 if lane == 2 else 1)
        self.map_widget.clear_pit_edit_phase(phase, lane=lane)
        mw = self.map_widget
        if lane == 2:
            if phase == "entry":
                self._pit_in_2 = []
                mw.set_pit_blends_2(None, mw.pit_out_2)
            elif phase == "road":
                self._pit_path_2 = []
                self._pit_out_2 = []
                mw.set_pit_path_2(None)
                mw.set_pit_blends_2(mw.pit_in_2, None)
            elif phase == "merge":
                self._pit_out_2 = []
                mw.set_pit_blends_2(mw.pit_in_2, None)
        elif phase == "entry":
            self._pit_in = []
            mw.set_pit_blends(None, mw.pit_out)
        elif phase == "road":
            self._pit_path = []
            self._pit_out = []
            mw.set_pit_path(None)
            mw.set_pit_blends(mw.pit_in, None)
        elif phase == "merge":
            self._pit_out = []
            mw.set_pit_blends(mw.pit_in, None)
        lane_lbl = f"lane {lane} " if lane == 2 else ""
        labels = {"entry": "entry", "road": "pit road", "merge": "merge"}
        mw.flash_hint(f"Cleared {lane_lbl}{labels.get(phase, phase)} points")

    def _save_pit_authoring(self) -> None:
        """Refresh in-progress pit preview after a handle drag (no file write)."""
        self.map_widget.update()

    def _orientation_from_cfg(self) -> tuple[int, bool]:
        """Map rotation/mirror stamped into saved track JSON."""
        mcfg = config.CFG.get("map", {})
        rot = int(round((mcfg.get("rotation", 0) or 0) / 90.0)) * 90 % 360
        return rot, bool(mcfg.get("mirror", False))

    def _build_loop_doc(self, tid) -> dict:
        """Racing loop + corners for v2 track save (no pit geometry)."""
        canonical = self._canonical_track_id(tid)
        try:
            doc_tid = int(canonical)
        except (TypeError, ValueError):
            doc_tid = canonical
        loop = [(p[0], p[1]) for p in self.map_widget.path]
        rot, mirror = self._orientation_from_cfg()
        doc: dict = {
            "schema": 2,
            "import_version": 2,
            "pit_source": "manual",
            "track_id": doc_tid,
            "name": (self._learn_name or self._v2_authoring_name or str(tid)),
            "start_finish": float(self.map_widget.start_finish),
            "points": [[round(p[0], 7), round(p[1], 7)] for p in loop],
            "corners": track_map.corners_to_json(
                self.map_widget.display_corners()),
            "map_rotation": rot,
            "map_mirror": mirror,
        }
        if self._track_turns:
            doc["num_turns"] = int(self._track_turns)
        elif self.map_widget.num_turns:
            doc["num_turns"] = int(self.map_widget.num_turns)
        if self._v2_loop_doc:
            for key in ("import_version",):
                if key in self._v2_loop_doc:
                    doc[key] = self._v2_loop_doc[key]
        aliases = getattr(self, "_alias_track_ids", None)
        if aliases:
            doc["alias_track_ids"] = list(aliases)
        return doc

    def _write_track_json(self, tid, doc: dict) -> str:
        path = os.path.join(self.tracks_dir, f"{tid}.json")
        os.makedirs(self.tracks_dir, exist_ok=True)
        stamped = dict(doc)
        stamped["updated_at"] = datetime.now(timezone.utc).isoformat()
        tmp = f"{path}.tmp"
        with open(tmp, "w", encoding="utf-8") as fh:
            json.dump(stamped, fh, indent=2)
            fh.write("\n")
        os.replace(tmp, path)
        track_store.invalidate_alias_cache()
        return path

    def _apply_track_orientation(self, meta: dict) -> None:
        """Apply per-track map rotation/mirror from saved JSON into base config."""
        if not meta:
            return
        schema = meta.get("schema")
        imp = meta.get("import_version")
        has_orient = ("map_rotation" in meta or "map_mirror" in meta)
        if not has_orient:
            return
        if schema != 2 and imp != 2:
            return
        full = config.base_cfg()
        mcfg = full.setdefault("map", {})
        if "map_rotation" in meta:
            mcfg["rotation"] = int(meta["map_rotation"]) % 360
        if "map_mirror" in meta:
            mcfg["mirror"] = bool(meta["map_mirror"])
        config.apply_base(full, notify=True)

    def _preview_uploaded_track_in_demo(self, tid) -> None:
        """Session-only demo map preview after an author upload (not persisted)."""
        self._session_demo_track_id = str(tid)
        if self.demo:
            self._load_demo_track()
            self.map_widget.flash_hint(
                f"Demo map previewing TrackID {tid} this session")

    def _cloud_blocks_track_save(self, canonical) -> str | None:
        """Error message when cloud already has this track; None if save may proceed."""
        if not track_store.can_write():
            return None
        exists = track_store.cloud_track_exists(canonical)
        if exists is True:
            return (f"TrackID {canonical} is already in the shared library "
                    "— save skipped.")
        return None

    def save_loop_v2(self) -> tuple[bool, str]:
        """Write loop + corners only; upload without pit lane."""
        tid = self._authoring_track_id()
        if tid is None:
            return False, ("No TrackID — join a session on track, or import "
                           "members HTML with id=\"track-map-123\".")
        if not self.map_widget.path or len(self.map_widget.path) < 3:
            return False, "No track loop loaded."
        canonical = self._canonical_track_id(tid)
        block = self._cloud_blocks_track_save(canonical)
        if block:
            self.map_widget.flash_hint(block)
            return False, block
        doc = self._build_loop_doc(tid)
        path = self._write_track_json(canonical, doc)
        if track_store.can_write():
            self._track_sync.upload_local_async(self.tracks_dir, canonical)
        self._preview_uploaded_track_in_demo(tid)
        msg = f"Saved loop to {path} (no pit lane)."
        if track_store.can_write():
            msg += " Uploaded to cloud."
        if self.demo:
            msg += " Demo map updated for this session."
        return True, msg

    def _build_manual_pit_lane_fields(
        self, loop, entry, road, merge,
    ) -> dict:
        """Schematic pit pipeline for one lane -> pit_path / blends / pcts."""
        if len(road) < 2 or len(merge) < 2:
            return {}
        from tools.schematic_to_track import (
            _connect_blend_to_loop,
            _pct_on_loop,
            _pit_span_on_loop,
            _resample_open,
        )

        pit_path = _resample_open(road, 140)
        pit_out_raw = _resample_open(merge, 41)
        pit_out = _connect_blend_to_loop(
            pit_out_raw, loop, attach_end=True, n_loop=20, pit_path=pit_path)
        pit_out = _resample_open(pit_out, 41)

        lane_lo, lane_hi = _pit_span_on_loop(loop, pit_path)
        pit_out_pct = round(_pct_on_loop(loop, pit_out[-1]), 5)
        fields: dict = {
            "pit_path": [[round(x, 7), round(y, 7)] for x, y in pit_path],
            "pit_out": [[round(x, 7), round(y, 7)] for x, y in pit_out],
            "pit_in_pct": None,
            "pit_span": [round(lane_lo, 5), round(lane_hi, 5)],
            "pit_out_pct": pit_out_pct,
        }
        if len(entry) >= 2:
            pit_in_seed = _resample_open(entry, 24)
            pit_in = _connect_blend_to_loop(
                pit_in_seed, loop, attach_end=False, n_loop=12, max_pts=24)
            if pit_path:
                pit_in = list(pit_in)
                pit_in[-1] = pit_path[0]
            pit_in = _resample_open(pit_in, 24)
            fields["pit_in"] = [[round(x, 7), round(y, 7)] for x, y in pit_in]
            fields["pit_in_pct"] = round(_pct_on_loop(loop, pit_in[0]), 5)
        else:
            fields["pit_in_pct"] = round(lane_lo, 5)
        return fields

    @staticmethod
    def _suffix_pit_lane_keys(fields: dict, suffix: str) -> dict:
        if not suffix:
            return dict(fields)
        out: dict = {}
        for key, val in fields.items():
            out[f"{key}{suffix}"] = val
        return out

    def save_manual_track_v2(self) -> tuple[bool, str]:
        """Finalize manual pit geometry and write tracks/<TrackID>.json."""
        tid = self._authoring_track_id()
        if tid is None:
            return False, ("No TrackID — join a session on track, or import "
                           "members HTML with id=\"track-map-123\".")
        if not self.map_widget.path or len(self.map_widget.path) < 3:
            return False, "No track loop loaded."
        road_snap = self.map_widget.pit_edit_snapshot(lane=1)
        entry, road, merge = road_snap
        if len(road) < 2:
            return False, "Need at least 2 pit road points."
        if len(merge) < 2:
            return False, "Need at least 2 merge points."

        canonical = self._canonical_track_id(tid)
        block = self._cloud_blocks_track_save(canonical)
        if block:
            self.map_widget.flash_hint(block)
            return False, block

        loop = [(p[0], p[1]) for p in self.map_widget.path]
        lane1 = self._build_manual_pit_lane_fields(loop, entry, road, merge)
        doc = self._build_loop_doc(tid)
        doc.update(lane1)
        if self._pit_speed_ms > 0:
            doc["pit_speed"] = round(self._pit_speed_ms, 3)
        if self._pit_lane_speed_pct != 1.0:
            doc["pit_lane_speed_pct"] = round(self._pit_lane_speed_pct, 4)

        entry2, road2, merge2 = self.map_widget.pit_edit_snapshot(lane=2)
        _PIT2_KEYS = (
            "pit_path_2", "pit_in_2", "pit_out_2", "pit_span_2",
            "pit_in_pct_2", "pit_out_pct_2", "pit_lane_speed_pct_2",
        )
        for key in _PIT2_KEYS:
            doc.pop(key, None)
        lane2 = self._build_manual_pit_lane_fields(loop, entry2, road2, merge2)
        if lane2:
            doc.update(self._suffix_pit_lane_keys(lane2, "_2"))
            if self._pit_lane_speed_pct_2 != 1.0:
                doc["pit_lane_speed_pct_2"] = round(
                    self._pit_lane_speed_pct_2, 4)

        path = self._write_track_json(canonical, doc)

        meta = {k: doc[k] for k in (
            "pit_span", "pit_path", "pit_in", "pit_out", "pit_in_pct",
            "pit_out_pct", "pit_speed", "pit_source", "pit_lane_speed_pct",
            "pit_span_2", "pit_path_2", "pit_in_2", "pit_out_2",
            "pit_in_pct_2", "pit_out_pct_2", "pit_lane_speed_pct_2",
        ) if k in doc}
        # Clearing a previously saved entry on re-save without drawing one.
        if "pit_in" not in meta:
            meta["pit_in"] = None
        if lane2 and "pit_in_2" not in meta:
            meta["pit_in_2"] = None
        if not lane2:
            for key in _PIT2_KEYS:
                meta[key] = None
        self._apply_pit_meta(meta)
        if track_store.can_write():
            self._track_sync.upload_local_async(
                self.tracks_dir, canonical)
        self._preview_uploaded_track_in_demo(tid)
        pit_path = lane1.get("pit_path") or []
        pit_out = lane1.get("pit_out") or []
        pit_in = lane1.get("pit_in") or []
        n_in = len(pit_in)
        n_path = len(pit_path)
        n_out = len(pit_out)
        entry_note = f"entry {n_in}" if n_in else "no entry"
        msg = (f"Saved {path} — {entry_note}, road {n_path}, "
               f"merge {n_out} pts")
        if lane2:
            msg += f"; lane 2 road {len(lane2.get('pit_path') or [])} pts"
        if track_store.can_write():
            msg += " Uploaded to cloud."
        if self.demo:
            msg += " Demo map updated for this session."
        return True, msg

    def save_pit_v2(self) -> tuple[bool, str]:
        """Patch pit geometry into the local track file and upload (even if already in cloud).

        Unlike Save track, this does not rewrite the racing line and is not blocked
        when the TrackID already exists in the shared library.
        """
        tid = self._authoring_track_id()
        if tid is None:
            return False, ("No TrackID — join a session on track, or import "
                           "members HTML with id=\"track-map-123\".")
        if not self.map_widget.path or len(self.map_widget.path) < 3:
            return False, "No track loop loaded."
        entry, road, merge = self.map_widget.pit_edit_snapshot(lane=1)
        if len(road) < 2:
            return False, "Need at least 2 pit road points."
        if len(merge) < 2:
            return False, "Need at least 2 merge points."

        canonical = self._canonical_track_id(tid)
        if not self._ensure_local_track_file():
            return False, "Could not create local track file."

        loop = [(p[0], p[1]) for p in self.map_widget.path]
        lane1 = self._build_manual_pit_lane_fields(loop, entry, road, merge)
        if not lane1:
            return False, "Could not build pit geometry."

        _PIT2_KEYS = (
            "pit_path_2", "pit_in_2", "pit_out_2", "pit_span_2",
            "pit_in_pct_2", "pit_out_pct_2", "pit_lane_speed_pct_2",
        )
        meta: dict = dict(lane1)
        meta["pit_source"] = "manual"
        if self._pit_speed_ms > 0:
            meta["pit_speed"] = round(self._pit_speed_ms, 3)
        if self._pit_lane_speed_pct != 1.0:
            meta["pit_lane_speed_pct"] = round(self._pit_lane_speed_pct, 4)
        if "pit_in" not in meta:
            meta["pit_in"] = None

        entry2, road2, merge2 = self.map_widget.pit_edit_snapshot(lane=2)
        lane2 = self._build_manual_pit_lane_fields(loop, entry2, road2, merge2)
        if lane2:
            meta.update(self._suffix_pit_lane_keys(lane2, "_2"))
            if self._pit_lane_speed_pct_2 != 1.0:
                meta["pit_lane_speed_pct_2"] = round(
                    self._pit_lane_speed_pct_2, 4)
            if "pit_in_2" not in meta:
                meta["pit_in_2"] = None
        else:
            for key in _PIT2_KEYS:
                meta[key] = None

        try:
            ok = track_map.update_track_meta(
                self.tracks_dir, canonical, **meta)
        except Exception as exc:
            return False, f"Could not save pit: {exc}"
        if not ok:
            return False, "Could not update local track file."

        self._apply_pit_meta({k: v for k, v in meta.items() if v is not None})
        if track_store.can_write():
            self._track_sync.upload_local_async(self.tracks_dir, canonical)
        self._preview_uploaded_track_in_demo(tid)

        pit_path = lane1.get("pit_path") or []
        pit_out = lane1.get("pit_out") or []
        pit_in = lane1.get("pit_in") or []
        entry_note = f"entry {len(pit_in)}" if pit_in else "no entry"
        msg = (f"Saved pit — {entry_note}, road {len(pit_path)}, "
               f"merge {len(pit_out)} pts")
        if lane2:
            msg += f"; lane 2 road {len(lane2.get('pit_path') or [])} pts"
        if track_store.can_write():
            msg += " Uploaded to cloud."
        if self.demo:
            msg += " Demo map updated for this session."
        return True, msg

    def pit_edit_state(self) -> dict:
        """Snapshot for Track Scan v2 import panel."""
        entry, road, merge = self.map_widget.pit_edit_snapshot(lane=1)
        entry2, road2, merge2 = self.map_widget.pit_edit_snapshot(lane=2)
        tid = self._authoring_track_id()
        return {
            "entry_count": len(entry),
            "road_count": len(road),
            "merge_count": len(merge),
            "entry_count_2": len(entry2),
            "road_count_2": len(road2),
            "merge_count_2": len(merge2),
            "pit_edit_lane": self.map_widget.pit_edit_lane,
            "pit_edit_mode": self.map_widget.pit_edit_mode,
            "pit_edit_phase": self.map_widget.pit_edit_phase,
            "has_loop": bool(self.map_widget.path and len(self.map_widget.path) >= 3),
            "authoring_track_id": tid,
            "in_sim": not self.demo and self._track_id is not None,
            "has_saved_pit": bool(
                self._pit_path and len(self._pit_path) >= 2),
            "has_saved_pit_2": bool(
                self._pit_path_2 and len(self._pit_path_2) >= 2),
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

    def _widget_by_key(self) -> dict:
        return {
            "standings": self.standings_widget,
            "relative": self.relative_widget,
            "radar": self.radar_widget,
            "map": self.map_widget,
            "dash": self.dash_widget,
            "laptime_log": self.laptime_widget,
            "fuel_calc": self.fuel_widget,
            "inputs": self.inputs_widget,
            "delta_bar": self.delta_bar_widget,
            "flags": self.flags_widget,
            "sector_timing": self.sector_widget,
            "lap_compare": self.lap_compare_widget,
            "tire_panel": self.tire_panel_widget,
            "pit_board": self.pit_board_widget,
            "weather_panel": self.weather_panel_widget,
            "leaderboard_strip": self.leaderboard_strip_widget,
            "radio_tower": self.radio_tower_widget,
            "ers_hybrid": self.ers_hybrid_widget,
            "system_panel": self.system_panel_widget,
            "pit_advisor": self.pit_advisor_widget,
        }

    @staticmethod
    def _cfg_section_snapshot(cfg: dict) -> dict:
        snap: dict = {}
        for k, v in cfg.items():
            if isinstance(v, dict):
                snap[k] = repr(v)
            else:
                snap[k] = v
        return snap

    _CFG_GLOBAL_KEYS = frozenset({
        "font_family", "tabular_font_family", "fonts",
    })

    def _repaint_config_sections(self, cfg: dict) -> None:
        """Repaint only widgets whose config section changed."""
        prev = self._cfg_section_snap
        snap = self._cfg_section_snapshot(cfg)
        self._cfg_section_snap = snap
        if not prev:
            self._repaint_all()
            return
        changed = {k for k in set(prev) | set(snap) if prev.get(k) != snap.get(k)}
        if not changed:
            return
        if changed & self._CFG_GLOBAL_KEYS:
            self._repaint_all()
            self.map_widget._invalidate_static_cache()
            return
        widgets = self._widget_by_key()
        painted = False
        for key in _WIDGET_KEYS:
            if key in changed and key in widgets:
                widgets[key].update()
                painted = True
        if "map" in changed:
            self.map_widget._invalidate_static_cache()
        if not painted:
            self._repaint_all()

    def _repaint_all(self) -> None:
        for w in self._widget_by_key().values():
            w.update()

    def open_settings(self) -> None:
        from .config_editor import ConfigEditor

        if self._settings_window is None:
            self._settings_window = ConfigEditor(overlay=self)
        self._settings_window.show()
        self._settings_window.raise_()
        self._settings_window.activateWindow()

    # --- Telemetry helpers --------------------------------------------------

    def _invalidate_driver_cache(self) -> None:
        """Drop the DriverInfo map so the next tick rebuilds from YAML."""
        self._driver_cache = {}
        self._driver_refresh_counter = 0
        self._pace_idxs = set()
        self._driver_car_idx = None
        self._driver_user_id = None
        self._force_driver_refresh = True

    def _int_or_none(self, value) -> int | None:
        if value is None:
            return None
        try:
            return int(value)
        except (TypeError, ValueError):
            return None

    def _car_idx_or_none(self, value) -> int | None:
        """Car index usable as an array subscript (rejects missing / negative)."""
        idx = self._int_or_none(value)
        if idx is None or idx < 0:
            return None
        return idx

    def _focus_car_idx(self) -> int | None:
        """Car to center Relative/Map on: ego car, else camera focus, else YAML.

        Spectators often have ``PlayerCarIdx == -1`` while ``CamCarIdx`` tracks
        the car they are watching.
        """
        try:
            p = self._car_idx_or_none(self.ir["PlayerCarIdx"])
        except (TypeError, ValueError, KeyError):
            p = None
        if p is not None:
            return p
        try:
            c = self._car_idx_or_none(self.ir["CamCarIdx"])
        except (TypeError, ValueError, KeyError):
            c = None
        if c is not None:
            return c
        if self._driver_car_idx is not None and self._driver_car_idx >= 0:
            return self._driver_car_idx
        return None

    def _leader_car_idx(self, positions) -> int | None:
        """CarIdx currently in P1, or None."""
        if not positions:
            return None
        for idx, pos in enumerate(positions):
            if idx in self._pace_idxs:
                continue
            try:
                if pos and int(pos) == 1:
                    return idx
            except (TypeError, ValueError):
                continue
        return None

    def _drivers(self, player=None) -> dict[int, dict]:
        """CarIdx -> driver dict, cached and refreshed about twice a second."""
        self._driver_refresh_counter += 1
        force = self._force_driver_refresh
        if (self._driver_cache and not force
                and self._driver_refresh_counter < 30
                and not self._driver_identity_mismatch(player)):
            return self._driver_cache
        self._driver_refresh_counter = 0
        self._force_driver_refresh = False

        info = self.ir["DriverInfo"] if self.ir else None
        if not info:
            return self._driver_cache
        # Exclude the pace/safety car: it isn't a competitor and shouldn't show
        # in the tables. Remember its CarIdx so the tick can skip it too.
        cache: dict[int, dict] = {}
        pace: set[int] = set()
        for d in info.get("Drivers", []):
            idx = self._int_or_none(d.get("CarIdx"))
            if idx is None:
                continue
            if d.get("CarIsPaceCar"):
                pace.add(idx)
                continue
            cache[idx] = d
        self._driver_cache = cache
        self._pace_idxs = pace
        self._driver_car_idx = self._int_or_none(info.get("DriverCarIdx"))
        self._driver_user_id = self._int_or_none(info.get("DriverUserID"))
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

    def _driver_identity_mismatch(self, player) -> bool:
        """True when the cached PlayerCarIdx slot is not the local DriverInfo driver.

        Index-only PlayerCarIdx vs DriverCarIdx differences are handled by
        ``_player_driver`` without a full refresh; we only force a rebuild when
        the cached *identity* at the live slot disagrees with DriverUserID /
        DriverCarIdx's UserName.
        """
        if not self._driver_cache:
            return False
        p = self._int_or_none(player)
        if p is None:
            return False
        slot = self._driver_cache.get(p)
        if slot is None:
            return False
        yaml_uid = self._driver_user_id
        if yaml_uid is not None:
            slot_uid = self._int_or_none(slot.get("UserID"))
            if slot_uid is not None and slot_uid != yaml_uid:
                return True
        yaml_idx = self._driver_car_idx
        if yaml_idx is not None and yaml_idx in self._driver_cache:
            yaml_drv = self._driver_cache[yaml_idx]
            su = self._int_or_none(slot.get("UserID"))
            yu = self._int_or_none(yaml_drv.get("UserID"))
            if su is not None and yu is not None and su != yu:
                return True
            sn = slot.get("UserName")
            yn = yaml_drv.get("UserName")
            if sn and yn and sn != yn:
                return True
        return False

    def _player_driver(self, player, drivers: dict | None = None) -> dict:
        """Local driver dict, preferring DriverCarIdx / DriverUserID over PlayerCarIdx."""
        drivers = self._driver_cache if drivers is None else drivers
        if not drivers:
            return {}
        if self._driver_car_idx is not None and self._driver_car_idx in drivers:
            return drivers[self._driver_car_idx]
        if self._driver_user_id is not None:
            for d in drivers.values():
                if self._int_or_none(d.get("UserID")) == self._driver_user_id:
                    return d
        p = self._int_or_none(player)
        if p is not None:
            return drivers.get(p) or {}
        return {}

    def _driver_for_row(self, idx, player, drivers: dict) -> dict:
        """Driver dict for a table row; reconcile identity on the player row."""
        if player is not None and idx == player:
            return self._player_driver(player, drivers) or drivers.get(idx, {})
        return drivers.get(idx, {})

    # --- Per-tick update ----------------------------------------------------

    def _demo_feed(self):
        """A synthetic telemetry source used to populate the overlay while you
        arrange the layout offline. Seeds a demo track + lap log the first time."""
        if self._demo_ir is None:
            self._demo_ir = oc.make_irsdk(demo=True)
        if not self._demo_active:
            self._demo_active = True
            self._load_demo_track()
            # Seed from the synthetic SDK — self.ir may still be a disconnected
            # real IRSDK until process_telemetry_tick assigns the return value.
            self._seed_demo_laptimes(self._demo_ir)
        return self._demo_ir

    def process_telemetry_tick(self) -> None:
        self._tick_stage = "init"
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
        if connected and not self._overlay_running and not self.edit_mode_enabled():
            return
        if connected:
            self._demo_active = False
        self.ir = self._demo_feed() if use_demo else self._sdk
        if hasattr(self.ir, "begin_tick"):
            self.ir.begin_tick()

        # Switch the garage vs on-track profile before anything else, so widget
        # visibility + content reflect the right context this tick.
        self._update_context()
        # Drives the ~1 s throttle on expensive session-info (YAML) reads.
        self._session_info_counter += 1

        # Which widgets are visible: a hidden widget does no reads and no work.
        en = self._visible_widgets
        if not any(en.values()):
            return

        player = self._focus_car_idx()
        need_order = (en["standings"] or en["relative"] or en["dash"]
                      or en["leaderboard_strip"] or en["radio_tower"])
        need_drivers = (en["standings"] or en["relative"] or en["map"] or en["dash"]
                        or en["leaderboard_strip"] or en["radio_tower"])
        map_use_pos = (en["map"]
                       and config.CFG["map"].get("car_label", "number") == "position")
        delta_mode = config.CFG.get("delta_bar", {}).get("mode")
        need_leader_last = (delta_mode == "leader_last"
                            and (en.get("delta_bar")
                                 or config.CFG.get("dash", {}).get("show_delta_bar")))
        need_pos = (need_order or map_use_pos or en["leaderboard_strip"]
                    or en["radio_tower"] or need_leader_last)
        # Each array is only read if some visible widget consumes it.
        positions = None
        self._class_positions = None
        if need_pos:
            live_pos = self.ir["CarIdxPosition"]
            positions, self._class_positions = self._resolve_positions(
                live_pos, player)
        need_lap_pct = (
            en["radar"] or en["map"] or en["standings"]
            or en["relative"] or self._needs_lap_engine(en)
            or self._needs_sector_timer(en)
            or self._needs_lap_pct_for_delta(en))
        lap_pct = self.ir["CarIdxLapDistPct"] if need_lap_pct else None
        # Used by the tables to tell genuine lapped traffic from same-lap cars.
        self._lap_pct = lap_pct
        surface = (self.ir["CarIdxTrackSurface"]
                   if (en["radar"] or en["standings"] or en["relative"]
                       or en["map"]) else None)
        need_est = (
            en["relative"]
            or (en["standings"] and config.any_table_column(
                "gap_ahead", "closing", sections=("standings",)))
            or (en["dash"] and config.dash_uses_any("gap_ahead", "gap_behind"))
            or (en["radar"] and (
                config.CFG["radar"].get("show_side_labels")
                or config.CFG["radar"].get("closing_rate_color")))
            or (en["flags"] and config.CFG["flags"].get("show_blue_detail"))
        )
        est_time = self.ir["CarIdxEstTime"] if need_est else None
        car_left_right = self.ir["CarLeftRight"] if en["radar"] else None
        mcfg = config.CFG.get("map", {})
        need_map_status = en["map"] and mcfg.get("show_car_status", True)
        need_on_pit = config.any_table_column("status") or need_map_status
        need_car_flags = config.any_table_column("car_flag") or need_map_status
        car_lap = (self.ir["CarIdxLap"]
                   if (need_order or en["map"]
                       or config.any_table_column("laps")) else None)
        need_f2 = (
            (en["standings"] and config.any_table_column(
                "gap", "gap_leader", "gap_ahead", sections=("standings",)))
            or (en["relative"] and config.any_table_column(
                "gap_leader", "gap_ahead", sections=("relative",)))
            or en["leaderboard_strip"]
        )
        car_f2 = self.ir["CarIdxF2Time"] if need_f2 else None
        on_pit_arr = (self.ir["CarIdxOnPitRoad"] if need_on_pit else None)
        car_flags = (self.ir["CarIdxSessionFlags"] if need_car_flags else None)
        self._need_weekend_info = (
            config.slot_in_use("weather", "incident_limit", "race_split")
            or config.slot_in_use("track_wetness")
            or (en["map"] and mcfg.get("show_expanded_weather"))
            or (en["dash"] and config.dash_uses_any("incidents_limit"))
            or en["weather_panel"]
        )
        try:
            sess_uid = int(self.ir["SessionUniqueID"])
        except (TypeError, ValueError, KeyError):
            sess_uid = None
        try:
            sess_num = int(self.ir["SessionNum"])
        except (TypeError, ValueError, KeyError):
            sess_num = None
        session_changed = False
        if sess_uid != self._closing_session_uid:
            session_changed = True
            self._closing_state = {}
            self._radar_closing_state = {}
            self._radar_clear_since = None
            self._closing_session_uid = sess_uid
            self._delta_last_lap_time = None
            self._delta_prev_lap = None
            self._delta_pit_hold = False
            self._delta_was_on_pit = False
            self._delta_pit_hold_since = None
            self._sector_timer.reset_session()
            self._ll_personal_best = None
            self._clear_weather_hist()
            self._ll_laps_version += 1
            self._caution_tracker = pstrat.CautionTracker()
        if sess_num != self._session_num:
            session_changed = True
            self._session_num = sess_num
        if session_changed:
            # DriverInfo CarIdx↔name map is session-scoped; drop it so a stale
            # qual map cannot label the player as someone else into the race.
            self._invalidate_driver_cache()
            self._session_info_counter = 0
            self._practice_cache = None
            self._qualifying_cache = None
            self._session_type_cache = ""
        # _drivers() also refreshes the dash's engine/shift-light params.
        drivers = self._drivers(player) if need_drivers else {}
        lap_est = self._lap_est(est_time) if est_time is not None else 0.0

        # Track pit stops only if a *shown* table shows the pit column.
        sess_time = None
        if ((en["relative"] and config.has_column("relative", "pit"))
                or (en["standings"] and config.has_column("standings", "pit"))
                or en["pit_advisor"]):
            sess_time = self.ir["SessionTime"]
            self._update_pit_tracking(surface, car_lap, sess_time)

        # Per-car lap times only if a shown table shows that column.
        self._car_last = self._car_best = None
        if (en["relative"] and config.any_table_column("last_lap", "qual_best",
                                                        sections=("relative",))) or \
                (en["standings"] and config.any_table_column(
                    "last_lap", "qual_best", sections=("standings",))) or \
                (need_leader_last) or \
                en["pit_advisor"]:
            self._car_last = self.ir["CarIdxLastLapTime"]
        if (en["relative"] and config.any_table_column("best_lap", "qual_best",
                                                       "gap_pole",
                                                       sections=("relative",))) or \
                (en["standings"] and config.any_table_column(
                    "best_lap", "qual_best", "gap_pole",
                    sections=("standings",))):
            self._car_best = self.ir["CarIdxBestLapTime"]

        radio_speaker = None
        if ((en["relative"] and config.has_column("relative", "badge"))
                or (en["standings"] and config.has_column("standings", "badge"))
                or en["map"] or en["radio_tower"]):
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
            results_pos = results_cls = None
            laps_complete: dict[int, int] = {}
            try:
                state = int(self.ir["SessionState"])
            except (TypeError, ValueError, KeyError):
                state = 4
            if state >= 5:
                results_pos, results_cls, laps_complete = self._race_results_positions()
            proj_class_pos = (results_cls if results_cls and any(results_cls)
                              else class_pos)
            proj_positions = (results_pos if results_pos and any(results_pos)
                              else positions)
            started = self._irating_started_flags(
                drivers, proj_class_pos, proj_positions, laps_complete)
            self._irating_deltas = irating_calc.project_deltas_by_class(
                drivers, class_pos, positions, self._pace_idxs,
                started_by_idx=started,
                results_class_positions=results_cls,
                results_positions=results_pos,
            )

        try:
            if en["radar"]:
                self._tick_stage = "radar"
                self._update_radar(player, lap_pct, surface, car_left_right,
                                   est_time, lap_est, drivers)
            if en["standings"]:
                self._tick_stage = "standings"
                self._update_standings(positions, drivers, surface, car_f2, player,
                                       lap_est, car_lap, sess_time, radio_speaker,
                                       est_time, on_pit_arr, car_flags)
            if en["relative"]:
                self._tick_stage = "relative"
                self._update_relative(player, est_time, surface, drivers, positions,
                                      car_lap, lap_est, sess_time, radio_speaker,
                                      car_f2, on_pit_arr, car_flags)
            if en["map"]:
                self._tick_stage = "map"
                self._update_map(player, lap_pct, surface, drivers, positions,
                                 car_lap, radio_speaker, on_pit_arr, car_flags)
            if self._needs_sector_timer(en):
                self._tick_stage = "sector_timer"
                self._advance_sector_timer(player, lap_pct)
            if self._needs_delta_last_lap_ref(en):
                self._update_delta_last_lap_ref()
            if en["dash"]:
                self._tick_stage = "dash"
                self._update_dash(player, positions, car_lap)
            if en["sector_timing"]:
                self._tick_stage = "sector_timing"
                self._update_sector_widget()
            if self._needs_fuel_lap_tracking(en):
                self._tick_stage = "fuel_lap"
                self._track_fuel_per_lap()
            if en["laptime_log"]:
                self._tick_stage = "laptime_log"
                self._update_laptime_log(player)
            if en["fuel_calc"]:
                self._tick_stage = "fuel_calc"
                self._update_fuel_calc()
            if en["inputs"]:
                self._tick_stage = "inputs"
                self._update_inputs()
            if en["delta_bar"]:
                self._tick_stage = "delta_bar"
                self._update_delta_bar(player, positions)
            if en["flags"]:
                self._tick_stage = "flags"
                self._update_flags(player, positions, est_time, lap_est, drivers)
            if self._needs_lap_engine(en):
                self._tick_stage = "lap_engine"
                self._update_lap_engine(player, lap_pct)
            if en["lap_compare"]:
                self._tick_stage = "lap_compare"
                self._update_lap_compare_widget()
            if en["tire_panel"]:
                self._tick_stage = "tire_panel"
                self._update_tire_panel()
            if en["pit_board"]:
                self._tick_stage = "pit_board"
                self._update_pit_board()
            if en["weather_panel"]:
                self._tick_stage = "weather_panel"
                self._update_weather_panel()
            if en["leaderboard_strip"]:
                self._tick_stage = "leaderboard_strip"
                self._update_leaderboard_strip(positions, drivers, car_f2,
                                               lap_est, player, car_lap)
            if en["radio_tower"]:
                self._tick_stage = "radio_tower"
                self._update_radio_tower(positions, drivers, player, radio_speaker)
            if en["ers_hybrid"]:
                self._tick_stage = "ers_hybrid"
                self._update_ers_hybrid()
            if en["system_panel"]:
                self._tick_stage = "system_panel"
                self._update_system_panel()
            if en["pit_advisor"]:
                self._tick_stage = "pit_advisor"
                self._update_pit_advisor()
        except Exception:
            stage = getattr(self, "_tick_stage", "?")
            log.exception("telemetry tick failed during widget update (%s)", stage)
            raise
        finally:
            self._tick_stage = None

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
        try:
            player = int(self.ir["PlayerCarIdx"])
        except (TypeError, ValueError, KeyError):
            player = None
        for idx in range(n):
            if on_pit is not None:
                now_on = bool(on_pit[idx])
            else:  # fall back to the track-surface enum
                now_on = surface[idx] in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
            st = self._pit.setdefault(idx, {"on": False, "lap": None, "time": None})
            if now_on and not st["on"]:  # rising edge = just entered the pits
                st["lap"] = car_lap[idx] if car_lap else None
                st["time"] = sess_time
                st["pit_count"] = int(st.get("pit_count") or 0) + 1
                if idx == player:
                    wear_corners = tele.read_tire_corners(
                        self.ir, wear=True, temp=False)
                    wears = []
                    for entry in wear_corners.values():
                        w = entry.get("wear")
                        if isinstance(w, (int, float)):
                            fv = float(w)
                            wears.append(fv * 100.0 if fv <= 1.0 else fv)
                    if wears:
                        st["wear_at_pit"] = min(wears)
            elif st["on"] and not now_on:
                entry = st.get("time")
                if entry is not None and sess_time is not None:
                    duration = float(sess_time) - float(entry)
                    st["last_pit_duration"] = duration
                    pacfg = config.CFG.get("pit_advisor", {})
                    splash_max = float(
                        pacfg.get("opponent_splash_pit_max_s", 0) or 0)
                    is_full_stop = splash_max <= 0 or duration >= splash_max
                    if is_full_stop:
                        st["tire_stop_count"] = (
                            int(st.get("tire_stop_count") or 0) + 1)
                    if idx == player:
                        pstrat.record_pit_loss_sample(
                            self._pit,
                            player,
                            duration,
                            cfg=pacfg,
                        )
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
        if lap_est <= 0 and est_time:
            lap_est = max((t for t in est_time if t and t > 0), default=0.0)
        # Cold mid-race connect can briefly lack DriverCarEstLapTime / EstTime;
        # keep relative wrap math alive so the table still paints.
        if lap_est <= 0:
            lap_est = 90.0
        return lap_est

    def _update_radar(self, player, lap_pct, surface, car_left_right,
                      est_time, lap_est, drivers) -> None:
        rc = config.CFG["radar"]
        nearest_ahead = nearest_behind = None
        alongside_zone = float(rc.get("alongside_zone_pct", 0.004) or 0.004)
        radar_range = rc["range_pct"]
        want_front = rc.get("show_front", True)
        want_rear = rc.get("show_rear", True)
        want_labels = rc.get("show_side_labels", False)
        want_closing = rc.get("closing_rate_color", False)
        closing_full = float(rc.get("closing_rate_full", 1.5) or 1.5)
        want_clear = rc.get("show_clear_timer", False)

        def on_track(idx: int) -> bool:
            return not surface or (idx < len(surface)
                                   and surface[idx] == oc.TRK_ON_TRACK)

        if player is not None and lap_pct:
            me = lap_pct[player]
            for idx, pct in enumerate(lap_pct):
                if idx == player or pct is None or pct < 0:
                    continue
                if not on_track(idx):
                    continue
                delta = wrap_lap_delta(pct, me)
                if want_front and alongside_zone < delta <= radar_range:
                    nearest_ahead = (delta if nearest_ahead is None
                                       else min(nearest_ahead, delta))
                elif want_rear and -radar_range <= delta < -alongside_zone:
                    nearest_behind = (delta if nearest_behind is None
                                      else max(nearest_behind, delta))

        def closeness(delta):
            if delta is None:
                return None
            return max(0.0, min(1.0, 1.0 - abs(delta) / radar_range))

        span = rc.get("side_span_pct", 0.0045) or 0.0045
        left_active = car_left_right in oc.CAR_ON_LEFT
        right_active = car_left_right in oc.CAR_ON_RIGHT

        def side_pos(delta):
            if delta is None:
                return 0.0
            return max(-1.0, min(1.0, delta / span))

        def car_label(idx: int | None) -> str:
            if idx is None:
                return ""
            d = drivers.get(idx, {}) if drivers else {}
            return str(d.get("CarNumber") or "")

        def side_closing(idx: int | None, est_delta: float | None):
            if not want_closing or idx is None or est_delta is None:
                return None
            try:
                now = float(self.ir["SessionTime"])
            except (TypeError, ValueError, KeyError):
                now = 0.0
            rate = tr.closing_rate(
                self._radar_closing_state, idx, est_delta, now)
            return tr.closing_rate_tint(rate, closing_full)

        left_idx = left_delta = left_est = None
        right_idx = right_delta = right_est = None
        if player is not None and lap_pct and (left_active or right_active):
            include = on_track if surface else None
            if left_active:
                left_idx, left_delta, left_est = tr.nearest_alongside(
                    lap_pct, player, est_time, lap_est,
                    alongside_zone=alongside_zone,
                    include_fn=include,
                    pace_idxs=self._pace_idxs,
                )
            if right_active:
                right_idx, right_delta, right_est = tr.nearest_alongside(
                    lap_pct, player, est_time, lap_est,
                    alongside_zone=alongside_zone,
                    include_fn=include,
                    pace_idxs=self._pace_idxs,
                    exclude={left_idx} if left_idx is not None else None,
                )

        try:
            now = float(self.ir["SessionTime"])
        except (TypeError, ValueError, KeyError):
            now = 0.0
        if left_active or right_active:
            self._radar_clear_since = None
        elif self._radar_clear_since is None:
            self._radar_clear_since = now
        clear_secs = (tr.radar_clear_seconds(self._radar_clear_since, now)
                      if want_clear else None)

        payload = {
            "left": left_active,
            "right": right_active,
            "left2": car_left_right == oc.LR_2_CARS_LEFT,
            "right2": car_left_right == oc.LR_2_CARS_RIGHT,
            "left_pos": side_pos(left_delta),
            "right_pos": side_pos(right_delta),
            "ahead": closeness(nearest_ahead) if want_front else None,
            "behind": closeness(nearest_behind) if want_rear else None,
            "left_label": car_label(left_idx) if want_labels else "",
            "right_label": car_label(right_idx) if want_labels else "",
            "left_closing": side_closing(left_idx, left_est),
            "right_closing": side_closing(right_idx, right_est),
            "clear_secs": clear_secs,
        }
        if payload == self.radar_widget.data and not self.radar_widget._animating:
            return
        self.radar_widget.set_data(payload)

    def _table_extra_fields(self, idx, cols, *, drivers, positions, surface,
                            on_pit, est_time, car_f2, car_lap, car_flags,
                            player, lap_est, is_qual) -> dict:
        """Optional table columns beyond the original set."""
        out: dict = {}
        d = drivers.get(idx, {})
        if cols.get("class_pos"):
            cp = self._class_positions
            if (cp and idx < len(cp) and cp[idx] and cp[idx] > 0
                    and tr.is_multiclass(cp, positions)):
                out["class_pos"] = str(cp[idx])
            else:
                out["class_pos"] = "\u2014"
        if cols.get("status"):
            on = bool(on_pit[idx]) if on_pit and idx < len(on_pit) else False
            surf = surface[idx] if surface and idx < len(surface) else None
            out["status"] = tr.car_status_text(surf, on_pit=on)
        if cols.get("car_flag"):
            sf = car_flags[idx] if car_flags and idx < len(car_flags) else None
            out["car_flag"] = tr.car_flag_text(sf)
            out["car_flag_kind"] = tr.car_flag_kind(sf)
        if cols.get("laps"):
            lap = car_lap[idx] if car_lap and idx < len(car_lap) else None
            out["laps"] = (str(int(lap))
                           if isinstance(lap, (int, float)) and lap >= 0
                           else "\u2014")
        if cols.get("gap_leader"):
            pos = positions[idx] if positions else None
            f2 = car_f2[idx] if car_f2 else None
            out["gap_leader_text"] = tr.fmt_leader_gap(f2, pos, lap_est)
        if cols.get("gap_ahead"):
            ahead_idx = tr.position_ahead_idx(positions, idx)
            secs = None
            if ahead_idx is not None and est_time:
                secs = tr.est_interval(est_time, idx, ahead_idx, lap_est)
            elif ahead_idx is not None and car_f2:
                secs = tr.f2_interval(car_f2, idx, ahead_idx)
            out["gap_ahead_text"] = tr.fmt_interval_gap(secs, lap_est)
        if cols.get("closing") and est_time and player is not None:
            me = est_time[player] if player < len(est_time) else None
            t = est_time[idx] if idx < len(est_time) else None
            if (me is not None and t is not None and idx != player
                    and lap_est > 0):
                delta = tr.wrap_est_delta(t, me, lap_est)
                rate = tr.closing_rate(self._closing_state, idx, delta,
                                       time.time())
                out["closing"] = tr.fmt_closing_rate(rate)
            else:
                out["closing"] = "\u2014"
        if cols.get("team"):
            team = str(d.get("TeamName", "")).strip()
            out["team"] = team or "\u2014"
        if cols.get("nickname"):
            nick = str(d.get("UserNickName", "")).strip()
            out["nickname"] = nick or "\u2014"
        if cols.get("qual_pos") or cols.get("qual_best") or cols.get("gap_pole"):
            dash = "\u2014"
            if is_qual:
                grid_pos, _ = self._qualify_grid_positions(live=True)
                if cols.get("qual_pos"):
                    qp = grid_pos[idx] if grid_pos and idx < len(grid_pos) else None
                    out["qual_pos"] = str(qp) if qp and qp > 0 else dash
                best = (self._car_best[idx]
                        if self._car_best and idx < len(self._car_best) else None)
                if cols.get("qual_best"):
                    out["qual_best"] = (self._fmt_laptime(best)
                                        if best and best > 0 else dash)
                if cols.get("gap_pole"):
                    pole_best = None
                    if self._car_best:
                        vals = [t for t in self._car_best if t and t > 0]
                        pole_best = min(vals) if vals else None
                    if pole_best and best and best > 0:
                        out["gap_pole"] = tr.fmt_interval_gap(best - pole_best,
                                                              lap_est)
                    else:
                        out["gap_pole"] = dash
            else:
                if cols.get("qual_pos"):
                    out["qual_pos"] = dash
                if cols.get("qual_best"):
                    out["qual_best"] = dash
                if cols.get("gap_pole"):
                    out["gap_pole"] = dash
        return out

    def _build_standings_row(self, idx, drivers, positions, surface, car_f2,
                             player, lap_est, cols, car_lap, sess_time,
                             pit_mode, radio_speaker=None, est_time=None,
                             on_pit=None, car_flags=None) -> dict:
        # Only compute fields whose column is actually shown.
        d = self._driver_for_row(idx, player, drivers)
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
        is_qual = self._is_qualifying_session()
        user_name = d.get("UserName", f"Car {idx}")
        g_icon, g_color = self._group_badge_fields(user_name)
        row = {
            "key": idx,
            "position": (positions[idx] if positions else "") if cols.get("position") else "",
            "car_number": str(d.get("CarNumber", "")) if cols.get("car_number") else "",
            "name": user_name if cols.get("name") else "",
            "is_pro": self._is_pro_driver_name(user_name),
            "group_icon": g_icon,
            "group_color": g_color,
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
            "inactive": tr.is_standings_inactive(
                surface[idx] if surface and idx < len(surface) else None,
                self._lap_pct[idx] if self._lap_pct and idx < len(self._lap_pct)
                else None),
            "lapping": lapping,
            "lap_ahead": lap_ahead,
            "speaking": radio_speaker is not None and idx == radio_speaker,
        }
        row.update(self._table_extra_fields(
            idx, cols, drivers=drivers, positions=positions, surface=surface,
            on_pit=on_pit, est_time=est_time, car_f2=car_f2, car_lap=car_lap,
            car_flags=car_flags, player=player, lap_est=lap_est,
            is_qual=is_qual))
        return row

    def _current_session_type(self) -> str:
        """Lower-case SessionType for the active session (cached ~1 s)."""
        if self._session_info_counter % 60 == 0 or self._practice_cache is None:
            st = ""
            try:
                sn = int(self.ir["SessionNum"])
                info = self.ir["SessionInfo"]
                if isinstance(info, dict):
                    sessions = info.get("Sessions") or []
                    if 0 <= sn < len(sessions):
                        st = str(sessions[sn].get("SessionType") or "").lower()
            except (TypeError, ValueError, KeyError):
                st = ""
            self._practice_cache = ("practice" in st or st == "open")
            self._qualifying_cache = ("qualif" in st or st == "qual")
            self._session_type_cache = st
        return getattr(self, "_session_type_cache", "")

    def _is_practice_session(self) -> bool:
        """True during a practice session (open or scheduled)."""
        if self.demo or self._demo_active:
            return False
        self._current_session_type()
        return bool(self._practice_cache)

    def _is_qualifying_session(self) -> bool:
        """True during a qualifying session."""
        if self.demo or self._demo_active:
            return False
        self._current_session_type()
        return bool(self._qualifying_cache)

    def _relative_include(self, idx, surface, positions, player) -> bool:
        """Relative rows: on-track cars; in practice, strictly on-track only."""
        if not surface or idx >= len(surface):
            return False
        if self._is_practice_session():
            return surface[idx] == oc.TRK_ON_TRACK
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
                          radio_speaker=None, est_time=None, on_pit=None,
                          car_flags=None) -> None:
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
                                             radio_speaker, est_time, on_pit,
                                             car_flags)

        center = scfg.get("center_on_player", True) and player in ranked
        rows = standings_row_list(
            ranked,
            player=player,
            center_on_player=center,
            pin_podium=bool(scfg.get("pin_podium", False)),
            rows=n,
            rows_ahead=scfg.get("rows_ahead", 4),
            rows_behind=scfg.get("rows_behind", 5),
            build=build,
            empty=self._empty_row,
        )

        shown = sum(1 for r in rows if not r.get("empty"))
        payload = {
            "title": scfg["title"],
            "rows": rows,
            "slots": self._slot_values("standings", drivers, positions, player,
                                       lap_est=lap_est, car_lap=car_lap,
                                       count=f"{shown}/{total}"),
        }
        if payload == self.standings_widget.data:
            return
        self.standings_widget.set_data(payload)

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
        return "irating" in config.dash_active_slots()

    @staticmethod
    def _dash_uses_irating() -> bool:
        return "irating" in config.dash_active_slots()

    @staticmethod
    def _dash_uses_metric(key: str) -> bool:
        return config.dash_metric_in_use(key)

    @staticmethod
    def _dash_uses_any(*keys: str) -> bool:
        return config.dash_uses_any(*keys)

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
        """Seconds left in session.

        iRacing sends SessionTimeRemain = -1 before the green flag; in that case
        derive remaining from SessionTimeTotal − SessionTime when the total is
        a sane race length (not the 168 h unlimited placeholder).
        """
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
        if not isinstance(tot, (int, float)):
            return None
        tot_s = self._sane_session_seconds(tot)
        if tot_s is None:
            return None
        el_s = 0.0
        if isinstance(el, (int, float)) and el >= 0:
            el_s = self._sane_session_seconds(el) or 0.0
        left = tot_s - el_s
        return left if left > 0 else None

    def _race_time_display(self) -> str:
        try:
            el = self.ir["SessionTime"]
            tot = self.ir["SessionTimeTotal"]
        except (TypeError, ValueError, KeyError):
            return "\u2014"
        el_s = self._sane_session_seconds(el) if isinstance(el, (int, float)) and el >= 0 else None
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

    def _seed_demo_laptimes(self, ir=None) -> None:
        """Prefill the log with recent oval laps and sync the lap counter."""
        src = ir if ir is not None else self.ir
        try:
            cur = int(src["Lap"])
        except (TypeError, ValueError, KeyError, AttributeError):
            cur = 1
        est = float(self._car_info.get("est_lap") or 0.0)
        if est <= 0:
            try:
                est = float(src["DriverInfo"]["DriverCarEstLapTime"])
            except (TypeError, ValueError, KeyError, AttributeError):
                est = 32.0
        seed = []
        for lp in range(max(1, cur - 8), cur):
            jit = (((lp * 37) % 7) - 3) * 0.04
            seed.append((lp, est + jit))
        try:
            temp_c = src["TrackTemp"] or 27.0
        except (TypeError, ValueError, KeyError, AttributeError):
            temp_c = 27.0
        self._ll_laps = [{"lap": lp, "secs": t, "temp_c": temp_c}
                         for lp, t in seed]
        self._ll_prev_lap = cur
        self._fc_prev_lap = None
        self._fc_lap_start_fuel = None
        self._fc_use = []

    def _player_last_lap_time(self) -> float | None:
        """Last completed lap for the player (LapLastLapTime, then CarIdx)."""
        last = self.ir["LapLastLapTime"]
        if isinstance(last, (int, float)) and last > 0:
            return float(last)
        player = self.ir["PlayerCarIdx"]
        try:
            arr = self.ir["CarIdxLastLapTime"]
            if arr is not None and player is not None:
                idx = int(player)
                if 0 <= idx < len(arr):
                    v = arr[idx]
                    if isinstance(v, (int, float)) and v > 0:
                        return float(v)
        except (TypeError, ValueError, KeyError):
            pass
        return None

    def _update_laptime_log(self, player=None) -> None:
        """Record each completed player lap (time + track temp) and push rows."""
        cols = set(config.laptime_log_column_order())
        try:
            lap = int(self.ir["Lap"])
        except (TypeError, ValueError):
            lap = None
        on_pit = bool(self.ir["OnPitRoad"]) if "tag" in cols else None
        surf = None
        if "tag" in cols:
            try:
                surf = self.ir["PlayerTrackSurface"]
            except (TypeError, ValueError, KeyError):
                pass
        if on_pit is not None or "tag" in cols:
            if on_pit or surf in (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS):
                self._ll_lap_tag = "PIT"
            elif on_pit is False and self._ll_lap_on_pit:
                self._ll_lap_tag = "OUT"
            self._ll_lap_on_pit = bool(on_pit)
        if lap is not None:
            if self._ll_prev_lap is None:
                self._ll_prev_lap = lap
                if "incidents" in cols:
                    try:
                        self._ll_lap_start_incidents = int(
                            self.ir["PlayerCarMyIncidentCount"] or 0)
                    except (TypeError, ValueError):
                        self._ll_lap_start_incidents = 0
            elif lap > self._ll_prev_lap:
                last = self._player_last_lap_time()
                completed = lap - 1
                if (last is not None
                        and (not self._ll_laps
                             or self._ll_laps[0]["lap"] != completed)):
                    temp = tele.read_track_temp(self.ir)
                    entry: dict = {"lap": completed, "secs": last,
                                   "temp_c": temp}
                    if "sectors" in cols and self._sector_timer.last:
                        entry["sectors"] = self._sector_timer.last[:]
                    if "fuel" in cols and self._fc_use:
                        entry["fuel_l"] = self._fc_use[0]
                    if "tires" in cols:
                        ts = self.ir["dcTireSet"]
                        if isinstance(ts, (int, float)) and ts > 0:
                            entry["tires"] = int(ts)
                    if "incidents" in cols:
                        try:
                            inc = int(self.ir["PlayerCarMyIncidentCount"] or 0)
                        except (TypeError, ValueError):
                            inc = 0
                        start = self._ll_lap_start_incidents or 0
                        delta = inc - start
                        entry["incidents"] = delta if delta > 0 else 0
                    if "tag" in cols and self._ll_lap_tag:
                        entry["tag"] = self._ll_lap_tag
                    if config.CFG["laptime_log"].get("delta_mode") == "personal_best":
                        try:
                            pb = self.ir["LapBestLapTime"]
                            if isinstance(pb, (int, float)) and pb > 0:
                                entry["personal_best"] = float(pb)
                        except (TypeError, ValueError, KeyError):
                            pass
                    self._ll_laps.insert(0, entry)
                    del self._ll_laps[60:]
                    self._ll_laps_version += 1
                    self._delta_last_lap_time = last
                self._ll_prev_lap = lap
                self._ll_lap_tag = None
                if "incidents" in cols:
                    try:
                        self._ll_lap_start_incidents = int(
                            self.ir["PlayerCarMyIncidentCount"] or 0)
                    except (TypeError, ValueError):
                        self._ll_lap_start_incidents = 0
            elif lap < self._ll_prev_lap:
                self._ll_laps = []
                self._ll_laps_version += 1
                self._ll_prev_lap = lap
                self._fc_use = []
                self._fc_prev_lap = lap
                self._fc_lap_start_fuel = None
        if self._ll_laps_version == getattr(self, "_ll_render_version", -1):
            return
        self._ll_render_version = self._ll_laps_version
        self.laptime_widget.set_data(self._build_laptime_rows())

    def _build_laptime_rows(self) -> dict:
        cfg = config.CFG["laptime_log"]
        n = max(1, int(cfg.get("rows", 8)))
        mode = cfg.get("delta_mode", "previous")
        laps = self._ll_laps
        best = min((l["secs"] for l in laps if l["secs"] > 0), default=None)
        order = config.laptime_log_column_order()
        rows = []
        for i, l in enumerate(laps[:n]):
            secs = l["secs"]
            if mode == "best":
                delta = (secs - best) if (best and secs > 0) else None
                if delta is not None and abs(delta) < 1e-4:
                    delta = None
            elif mode == "personal_best":
                pb = l.get("personal_best")
                delta = (secs - pb) if (pb and secs > 0) else None
                if delta is not None and abs(delta) < 1e-4:
                    delta = None
            else:
                prev = laps[i + 1]["secs"] if i + 1 < len(laps) else None
                delta = (secs - prev) if (prev and secs > 0) else None
            row: dict = {
                "lap": str(l["lap"]),
                "time": self._fmt_laplog_time(secs),
                "delta": delta,
                "temp": self._fmt_temp(l["temp_c"]),
            }
            if "sectors" in order:
                splits = l.get("sectors") or []
                parts = []
                for j, t in enumerate(splits):
                    if isinstance(t, (int, float)) and t > 0:
                        parts.append(f"{t:.1f}")
                    else:
                        parts.append("\u2014")
                row["sectors"] = " ".join(parts) if parts else "\u2014"
            if "fuel" in order:
                fl = l.get("fuel_l")
                row["fuel"] = (f"{config.conv_fuel(fl):.1f}"
                               if isinstance(fl, (int, float)) else "\u2014")
            if "tires" in order:
                row["tires"] = (str(l["tires"]) if l.get("tires") else "\u2014")
            if "incidents" in order:
                inc = l.get("incidents")
                row["incidents"] = (f"+{inc}x" if inc else "\u2014")
            if "tag" in order:
                row["tag"] = l.get("tag") or "\u2014"
            rows.append(row)
        return {"rows": rows, "columns": order}

    def _build_relative_row(self, idx, delta, drivers, positions, surface, car_lap,
                            player, is_player, cols, sess_time, pit_mode,
                            radio_speaker=None, est_time=None, car_f2=None,
                            on_pit=None, car_flags=None, lap_est=0.0) -> dict:
        # Only compute fields whose column is actually shown.
        d = self._driver_for_row(idx, player, drivers)
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
        is_qual = self._is_qualifying_session()
        user_name = d.get("UserName", f"Car {idx}")
        g_icon, g_color = self._group_badge_fields(user_name)
        row = {
            "key": idx,
            "position": (positions[idx] if positions else "") if cols.get("position") else "",
            "car_number": str(d.get("CarNumber", "")) if cols.get("car_number") else "",
            "name": user_name if cols.get("name") else "",
            "is_pro": self._is_pro_driver_name(user_name),
            "group_icon": g_icon,
            "group_color": g_color,
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
        row.update(self._table_extra_fields(
            idx, cols, drivers=drivers, positions=positions, surface=surface,
            on_pit=on_pit, est_time=est_time, car_f2=car_f2, car_lap=car_lap,
            car_flags=car_flags, player=player, lap_est=lap_est,
            is_qual=is_qual))
        return row

    def _update_relative(self, player, est_time, surface, drivers,
                         positions, car_lap, lap_est, sess_time,
                         radio_speaker=None, car_f2=None, on_pit=None,
                         car_flags=None) -> None:
        if not est_time or not surface:
            return
        # Spectators may have no ego car; center on camera focus (caller) or P1.
        center = player if player is not None else self._leader_car_idx(positions)
        if center is None or center >= len(est_time) or est_time[center] is None:
            return
        if lap_est <= 0:
            lap_est = self._lap_est(est_time)

        rcfg = config.CFG["relative"]
        cols = self._visible_cols("relative")
        pit_mode = rcfg.get("pit_mode", "laps_since")
        me = est_time[center]
        rels = []
        for idx, t in enumerate(est_time):
            if idx == center or t is None or idx in self._pace_idxs:
                continue
            if not self._relative_include(idx, surface, positions, center):
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
        # Pad the top so the focus car stays in the center slot even when there
        # aren't n_ahead cars in front (e.g. when leading).
        if rcfg.get("center_on_player", True):
            for k in range(n_ahead - len(ahead)):
                rows.append(self._empty_row(f"rel_top{k}"))
        for delta, idx in reversed(ahead):
            rows.append(self._build_relative_row(
                idx, delta, drivers, positions, surface, car_lap, center, False,
                cols, sess_time, pit_mode, radio_speaker, est_time, car_f2,
                on_pit, car_flags, lap_est))
        rows.append(self._build_relative_row(
            center, 0.0, drivers, positions, surface, car_lap, center, True,
            cols, sess_time, pit_mode, radio_speaker, est_time, car_f2,
            on_pit, car_flags, lap_est))
        for delta, idx in behind:
            rows.append(self._build_relative_row(
                idx, delta, drivers, positions, surface, car_lap, center, False,
                cols, sess_time, pit_mode, radio_speaker, est_time, car_f2,
                on_pit, car_flags, lap_est))
        if rcfg.get("center_on_player", True):
            for k in range(n_behind - len(behind)):
                rows.append(self._empty_row(f"rel_bot{k}"))

        payload = {
            "rows": rows,
            "slots": self._slot_values("relative", drivers, positions, center,
                                       car_lap, lap_est),
        }
        if payload == self.relative_widget.data:
            return
        self.relative_widget.set_data(payload)

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
        if key == "my_session_best":
            try:
                player = int(ir["PlayerCarIdx"])
                best = ir["CarIdxBestLapTime"]
                if best and 0 <= player < len(best):
                    t = best[player]
                    if t and t > 0:
                        return self._fmt_laptime(t)
            except (TypeError, ValueError, KeyError):
                pass
            return "\u2014"
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
        if key == "gpu":
            return self._sys_stats()[2]
        if key == "laps_remain":
            try:
                rem = self.ir["SessionLapsRemainEx"]
            except (TypeError, ValueError, KeyError):
                rem = None
            return str(rem) if isinstance(rem, int) and rem >= 0 else "\u2014"
        if key == "incident_limit":
            wk = self._weekend_info()
            limit = wk.get("IncidentLimit")
            try:
                count = int(self.ir["PlayerCarMyIncidentCount"] or 0)
            except (TypeError, ValueError, KeyError):
                count = 0
            if limit is not None and int(limit) > 0:
                return f"{count}/{int(limit)}x"
            return f"{count}x"
        if key == "fast_repairs":
            try:
                used = int(self.ir["FastRepairUsed"] or 0)
                avail = int(self.ir["FastRepairAvailable"] or 0)
            except (TypeError, ValueError, KeyError):
                return "\u2014"
            total = used + avail
            return f"{used}/{total}" if total > 0 else "\u2014"
        if key == "weather":
            wk = self._weekend_info()
            skies = wk.get("Skies")
            hum = wk.get("RelativeHumidity")
            parts = []
            if skies is not None:
                parts.append(str(skies))
            if hum is not None:
                try:
                    parts.append(f"{int(float(hum))}%")
                except (TypeError, ValueError):
                    pass
            return " ".join(parts) if parts else "\u2014"
        if key == "track_wetness":
            try:
                wet = self.ir["TrackWetness"]
            except (TypeError, ValueError, KeyError):
                wet = None
            if wet is None:
                return "\u2014"
            try:
                return f"{float(wet):.0f}%"
            except (TypeError, ValueError):
                return "\u2014"
        if key == "session_type":
            st = self._current_session_type()
            if not st:
                return "\u2014"
            if "qualif" in st or st == "qual":
                return "Qualifying"
            if "race" in st:
                return "Race"
            if "practice" in st or st == "open":
                return "Practice"
            return st.replace("_", " ").title()
        if key == "race_split":
            n = self._race_split_number()
            return f"Split {n}" if n else "\u2014"
        if key == "count":
            return count
        return None

    def _race_split_number(self) -> int | None:
        """1-based registration split, or None when unknown."""
        if self.demo or self._demo_active:
            return 2
        if not config.slot_in_use("race_split"):
            return None
        wk = self._weekend_info()
        raw = wk.get("RaceSplit")
        if raw is not None:
            try:
                n = int(raw)
                if n > 0:
                    return n
            except (TypeError, ValueError):
                pass
        try:
            sid = int(wk.get("SubSessionID") or 0)
        except (TypeError, ValueError):
            sid = 0
        if sid <= 0:
            return self._race_split_cache
        if (self._race_split_subsession == sid
                and self._race_split_cache is not None):
            return self._race_split_cache
        now = time.monotonic()
        if (self._race_split_subsession == sid
                and now - self._race_split_fetch_at < 120.0):
            return self._race_split_cache
        if self._race_split_fetching:
            return self._race_split_cache
        self._race_split_subsession = sid
        self._race_split_fetch_at = now
        self._race_split_fetching = True

        def _worker(sub_id: int) -> None:
            n = None
            try:
                from . import iracing_results
                n = iracing_results.split_number_for_subsession(sub_id)
            except Exception:  # noqa: BLE001
                n = None
            self._on_race_split_fetched(sub_id, n)

        threading.Thread(target=_worker, args=(sid,), daemon=True).start()
        return self._race_split_cache

    def _on_race_split_fetched(self, subsession_id: int, n) -> None:
        self._race_split_fetching = False
        if self._race_split_subsession != subsession_id:
            return
        try:
            if n is not None and int(n) > 0:
                self._race_split_cache = int(n)
        except (TypeError, ValueError):
            pass

    def _weekend_info(self) -> dict:
        """Throttled WeekendInfo + WeekendOptions for header/footer slots."""
        if not self._need_weekend_info:
            return self._weekend_cache or {}
        if self._weekend_cache and self._weekend_counter < 60:
            self._weekend_counter += 1
            return self._weekend_cache
        self._weekend_counter = 0
        out: dict = {}
        try:
            wk = self.ir["WeekendInfo"]
            if isinstance(wk, dict):
                opts = wk.get("WeekendOptions") or {}
                if isinstance(opts, dict):
                    out["IncidentLimit"] = opts.get("IncidentLimit")
                out["Skies"] = wk.get("Skies")
                out["RelativeHumidity"] = wk.get("RelativeHumidity")
                for key in ("SubSessionID", "SessionID", "Official"):
                    if key in wk:
                        out[key] = wk[key]
                split = _weekend_split_field(wk)
                if split is not None:
                    out["RaceSplit"] = split
        except (TypeError, ValueError, KeyError):
            pass
        self._weekend_cache = out
        return out

    def _class_sof(self, drivers, player, section=None) -> str:
        cid = self._player_class(drivers, player)
        if cid is None:
            return self._sof(drivers, section)
        irs = [d.get("IRating") for d in drivers.values()
               if d.get("IRating") and d.get("CarClassID") == cid]
        if not irs:
            return "--"
        return self._fmt_irating(sum(irs) / len(irs), section)

    def _qualify_grid_positions(self, *, live: bool = False) -> tuple[list[int] | None, list[int] | None]:
        """Qualifying / starting-grid positions from QualifyResultsInfo.

        When ``live`` is true the YAML is re-read every call so provisional qual
        standings stay current; otherwise the result is cached for the session.
        """
        uid = 0
        try:
            uid = int(self.ir["SessionUniqueID"])
        except (TypeError, ValueError, KeyError):
            pass
        if (not live
                and self._grid_session_uid == uid
                and self._grid_positions_cache is not None):
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
        if not live:
            self._grid_session_uid = uid
            self._grid_positions_cache = pos
            self._grid_class_positions_cache = cls
        return pos, cls

    def _race_results_positions(
        self,
    ) -> tuple[list[int] | None, list[int] | None, dict[int, int]]:
        """Race results from SessionInfo ResultsPositions (checkered / official)."""
        pos: list[int] | None = None
        cls: list[int] | None = None
        laps_complete: dict[int, int] = {}
        try:
            sn = int(self.ir["SessionNum"])
            info = self.ir["SessionInfo"]
            sessions = info.get("Sessions") if isinstance(info, dict) else None
            if not sessions or sn < 0 or sn >= len(sessions):
                return None, None, laps_complete
            session = sessions[sn]
            if not isinstance(session, dict):
                return None, None, laps_complete
            results = session.get("ResultsPositions")
            if not results:
                return None, None, laps_complete
            max_idx = 0
            entries: list[tuple[int, int | None, int | None, int]] = []
            for r in results:
                if not isinstance(r, dict):
                    continue
                idx = r.get("CarIdx")
                if idx is None:
                    continue
                idx = int(idx)
                max_idx = max(max_idx, idx)
                lc = r.get("LapsComplete", r.get("LapsDriven", 0))
                try:
                    lc_val = int(lc or 0)
                except (TypeError, ValueError):
                    lc_val = 0
                entries.append((idx, r.get("Position"), r.get("ClassPosition"), lc_val))
            if not entries:
                return None, None, laps_complete
            n = max(max_idx + 1, 64)
            pos = [0] * n
            cls = [0] * n
            for idx, p, cp, lc_val in entries:
                laps_complete[idx] = max(laps_complete.get(idx, 0), lc_val)
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
        return pos, cls, laps_complete

    def _irating_started_flags(
        self,
        drivers: dict[int, dict],
        class_positions,
        positions,
        laps_complete: dict[int, int] | None,
    ) -> dict[int, bool]:
        """Per CarIdx whether the driver started (vs registered DNS)."""
        lap_completed_arr = None
        try:
            lap_completed_arr = self.ir["CarIdxLapCompleted"]
        except (TypeError, ValueError, KeyError):
            pass
        laps_complete = laps_complete or {}
        started: dict[int, bool] = {}
        for idx, d in drivers.items():
            if idx in self._pace_idxs or d.get("IsSpectator"):
                continue
            if laps_complete.get(idx, 0) > 0:
                started[idx] = True
                continue
            if (lap_completed_arr and idx < len(lap_completed_arr)
                    and isinstance(lap_completed_arr[idx], int)
                    and lap_completed_arr[idx] > 0):
                started[idx] = True
                continue
            started[idx] = _coerce_int(
                (class_positions[idx] if class_positions and idx < len(class_positions)
                 else None)
                or (positions[idx] if positions and idx < len(positions) else None)
            ) > 0
        return started

    def _positions_from_best_lap(self, live: list | None) -> list[int] | None:
        """Provisional qual order from best lap times when results aren't ready."""
        try:
            best = self.ir["CarIdxBestLapTime"]
        except (TypeError, ValueError, KeyError):
            return None
        if not best:
            return None
        n = max(len(live or []), len(best), 64)
        entries: list[tuple[float, int]] = []
        for idx in range(n):
            if idx in self._pace_idxs:
                continue
            t = best[idx] if idx < len(best) else None
            if isinstance(t, (int, float)) and t > 0:
                entries.append((float(t), idx))
        if not entries:
            return None
        entries.sort(key=lambda e: e[0])
        pos = [0] * n
        for rank, (_, idx) in enumerate(entries, start=1):
            pos[idx] = rank
        return pos

    @staticmethod
    def _pad_position_arrays(
        pos: list[int], cls: list[int] | None, live: list | None,
    ) -> tuple[list[int], list[int] | None]:
        n = max(len(live or []), len(pos), 64)
        out = list(pos)
        if len(out) < n:
            out.extend([0] * (n - len(out)))
        out_cls = list(cls) if cls else None
        if out_cls is not None and len(out_cls) < n:
            out_cls.extend([0] * (n - len(out_cls)))
        return out, out_cls

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
            1 for p in (live or []) if isinstance(p, (int, float)) and int(p) > 0
        )
        # Cold mid-race connect: live positions often all zeros until SDK catches up.
        if valid_live == 0:
            return True
        grid_count = sum(1 for p in grid if p > 0)
        if grid_count >= 2 and valid_live < max(2, grid_count // 2):
            return True
        return False

    def _resolve_positions(
        self, live: list | None, player: int | None,
    ) -> tuple[list | None, list[int] | None]:
        """Live race positions; qual standings or starting grid when appropriate."""
        if self._demo_active:
            return live, None
        if self._is_qualifying_session():
            qpos, qcls = self._qualify_grid_positions(live=True)
            if qpos and any(qpos):
                pos, cls = self._pad_position_arrays(qpos, qcls, live)
                if live:
                    for idx, p in enumerate(live):
                        if idx >= len(pos):
                            break
                        lp = int(p or 0)
                        if lp > 0 and (not pos[idx] or pos[idx] <= 0):
                            pos[idx] = lp
                return pos, cls
            blap = self._positions_from_best_lap(live)
            if blap and any(blap):
                return blap, None
            return live, None
        grid_pos, grid_class = self._qualify_grid_positions()
        if not self._prefer_grid_positions(live, player, grid_pos):
            return live, None
        pos, cls = self._pad_position_arrays(grid_pos or [], grid_class, live)
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

    def _player_class(self, drivers, player):
        d = self._player_driver(player, drivers) if drivers else {}
        return d.get("CarClassID") if d else None

    def _sys_stats(self) -> tuple[str, str, str, float | None, float | None, float | None]:
        """(cpu%, mem%, gpu% strings + raw floats) for the local machine."""
        self._sys_counter += 1
        if self._sys_cache is None or self._sys_counter >= 30:
            self._sys_counter = 0
            cpu = sysstats.cpu_percent()
            mem = sysstats.mem_percent()
            gpu = sysstats.gpu_percent()
            self._sys_cache = (
                f"{cpu:.0f}%" if cpu is not None else "--",
                f"{mem:.0f}%" if mem is not None else "--",
                f"{gpu:.0f}%" if gpu is not None else "--",
                cpu,
                mem,
                gpu,
            )
        return self._sys_cache

    @staticmethod
    def _fmt_tod(secs) -> str:
        secs = int(secs) % 86400
        return f"{secs // 3600:02d}:{(secs % 3600) // 60:02d}"

    def _resolve_demo_track_id(self) -> str:
        """Pick which track file to load in demo mode."""
        if self._session_demo_track_id is not None:
            return str(self._session_demo_track_id)
        if self._shared_demo_track_id is not None:
            return str(self._shared_demo_track_id)
        return str(demo_data.DEMO_TRACK_ID)

    def _apply_app_settings_cache(self) -> None:
        cached = track_store.load_app_settings_cache(self.tracks_dir)
        if not cached:
            return
        if cached.get("demo_track_id") is not None:
            self._shared_demo_track_id = str(cached["demo_track_id"])
        if "pro_drivers" in cached:
            self._pro_drivers = track_store.normalize_pro_drivers(
                cached.get("pro_drivers"))

    def _fetch_shared_app_settings(self) -> None:
        if track_store.read_available():
            self._track_sync.fetch_app_settings_async()

    def _on_app_settings_fetched(self, settings) -> None:
        if not settings:
            return
        track_store.merge_app_settings_cache(self.tracks_dir, settings)
        if settings.get("demo_track_id") is not None:
            new_id = str(settings["demo_track_id"])
            old_id = self._shared_demo_track_id
            self._shared_demo_track_id = new_id
            if self.demo and old_id != new_id and self._session_demo_track_id is None:
                self._load_demo_track()
        if "pro_drivers" in settings:
            self._pro_drivers = track_store.normalize_pro_drivers(
                settings.get("pro_drivers"))
        if self._settings_window is not None:
            self._settings_window.refresh_demo_track_admin(settings)
            self._settings_window.refresh_pro_drivers_admin(settings)

    def _is_pro_driver_name(self, user_name: str | None) -> bool:
        return track_store.is_pro_driver(user_name, self._pro_drivers)

    def _reload_driver_groups(self, cfg=None) -> None:
        src = cfg if isinstance(cfg, dict) else config.CFG
        self._driver_groups = dgroups.normalize_driver_groups(
            (src or {}).get("driver_groups"))

    def _group_badge_fields(self, user_name: str | None) -> tuple[str, str]:
        """Return ``(group_icon, group_color)`` for a driver name."""
        g = dgroups.driver_group_for_name(user_name, self._driver_groups)
        if not g:
            return "", ""
        return str(g.get("icon") or ""), str(g.get("color") or "")

    def _sync_demo_pit_from_meta(self, meta: dict | None) -> None:
        """Align demo pit-car simulation with loaded track pit lap-% extents."""
        if not self.demo or not meta:
            return
        span = meta.get("pit_span")
        lane_lo = lane_hi = None
        if isinstance(span, (list, tuple)) and len(span) >= 2:
            lane_lo, lane_hi = span[0], span[1]
        demo_data.configure_pit_extents(
            meta.get("pit_in_pct"),
            meta.get("pit_out_pct"),
            lane_lo,
            lane_hi,
        )

    def _apply_track_from_path(self, path: str, tid: str) -> bool:
        """Load a track JSON/SVG into the map widget; return False on failure."""
        try:
            pts, sf, corners, name, file_meta = track_map.load_track(path)
        except Exception:
            return False
        self.map_widget.set_track(pts, sf, corners)
        if tid not in ("_demo", "demo"):
            try:
                self._v2_authoring_track_id = int(tid)
            except (TypeError, ValueError):
                pass
            if name:
                self._v2_authoring_name = name
        saved_turns = _coerce_int((file_meta or {}).get("num_turns"))
        if saved_turns:
            self.map_widget.set_num_turns(saved_turns)
            if not self._track_turns:
                self._track_turns = saved_turns
        meta = file_meta or {}
        aliases = meta.get("alias_track_ids")
        if isinstance(aliases, list):
            self._alias_track_ids = track_store._normalize_alias_ids(
                aliases, meta.get("track_id"))
        else:
            self._alias_track_ids = []
        self._apply_track_orientation(meta)
        self._loaded_track_updated_at = meta.get("updated_at")
        if self.demo and tid not in ("_demo", "demo"):
            nt = saved_turns or _coerce_int(meta.get("num_turns"))
            demo_data.configure_weekend_info(
                tid,
                name=name or meta.get("name", ""),
                num_turns=nt,
            )
            if nt:
                self.map_widget.set_num_turns(nt)
                self.map_widget.set_track_is_oval(nt == 4)
            elif str(tid) == str(demo_data.DEMO_TRACK_ID):
                self.map_widget.set_track_is_oval(True)
        self._track_zones = {
            "drs_zones": meta.get("drs_zones") or [],
            "p2p_zones": meta.get("p2p_zones") or [],
        }
        mcfg = config.CFG.get("map", {})
        self.map_widget.set_track_zones(
            drs_zones=(self._track_zones["drs_zones"]
                       if mcfg.get("show_drs_zones") else []),
            p2p_zones=(self._track_zones["p2p_zones"]
                       if mcfg.get("show_p2p_zones") else []),
        )
        if meta.get("pit_path"):
            if not meta.get("pit_source"):
                meta = dict(meta, pit_source="schematic")
            self._apply_pit_meta(meta)
            self._sync_demo_pit_from_meta(meta)
        return True

    def _try_apply_track_file(self, tid) -> bool:
        path = track_map.find_track_file(tid, self.tracks_dir)
        return bool(path and self._apply_track_from_path(path, str(tid)))

    def _load_demo_track(self) -> None:
        tid = self._resolve_demo_track_id()
        self._demo_track_pending_id = tid
        legacy_demo = tid in ("_demo", "demo")
        default_demo = (
            str(tid) == str(demo_data.DEMO_TRACK_ID)
            or tid in ("1", 1)
        )
        if default_demo and self._shared_demo_track_id is None:
            self.map_widget.set_track_is_oval(False)
            self.map_widget.set_num_turns(8)

        loaded = self._try_apply_track_file(tid)
        if not loaded:
            loaded = self._try_apply_track_file("_demo")
        if not loaded and (legacy_demo or default_demo):
            pts = track_map.build_demo_path()
            self.map_widget.set_path(pts)
            meta = self._demo_pit_geometry(pts)
            if meta:
                self._apply_pit_meta(meta)
                self._sync_demo_pit_from_meta(meta)
            loaded = True
        elif not loaded:
            label = "Demo Speedpark" if default_demo else tid
            self.map_widget.flash_hint(f"Loading {label} from cloud…")
        elif not self._pit_path:
            meta = self._demo_pit_geometry(self.map_widget.path)
            if meta:
                self._apply_pit_meta(meta)
                self._sync_demo_pit_from_meta(meta)

        if not legacy_demo and not default_demo:
            self._track_sync.fetch_async(tid)

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

    def _reset_track_load_state(self) -> None:
        """Clear track-load flags when the session TrackID changes."""
        self._track_loaded = False
        self._loaded_track_updated_at = None
        self._track_file_checked = False
        self._no_track_hint = False
        self._remote_tried.clear()
        self._track_fetch_last = 0.0
        self._reset_pit_state()
        self._pit_latch_seed_pending = True

    def _sync_track_session(self) -> bool:
        """Read WeekendInfo track metadata. False when unavailable."""
        weekend = self.ir["WeekendInfo"]
        if not weekend:
            return False
        new_tid = weekend.get("TrackID")
        if (self._track_id is not None and new_tid is not None
                and str(self._track_id) != str(new_tid)):
            self._reset_track_load_state()
        self._track_id = new_tid
        self._learn_name = (weekend.get("TrackDisplayName")
                            or weekend.get("TrackName") or "")
        self._track_turns = _coerce_int(weekend.get("TrackNumTurns"))
        self.map_widget.set_num_turns(self._track_turns)
        self._track_is_oval = constants.is_oval_track(weekend)
        self.map_widget.set_track_is_oval(self._track_is_oval)
        self._track_file_checked = True
        return True

    def _try_load_local_track(self, tid) -> bool:
        path = track_map.find_track_file(tid, self.tracks_dir)
        if not path:
            return False
        if not self._apply_track_from_path(path, str(tid)):
            return False
        self._track_loaded = True
        self._remote_tried.add(str(tid))
        track_store.touch(self.tracks_dir, tid)
        self._refresh_settings_authoring()
        return True

    def _maybe_fetch_remote_track(self, tid, *, stale: bool = False) -> None:
        now = time.monotonic()
        if (self._track_fetch_last > 0
                and now - self._track_fetch_last < _TRACK_FETCH_RETRY_SEC):
            return
        self._track_fetch_last = now
        self._track_sync.fetch_async(tid)
        if stale:
            self.map_widget.flash_hint(
                "Refreshing track map from cloud\u2026")

    def _ensure_track(self, player, lap_pct) -> None:
        """Load track from local file or cloud; prompt HTML import if missing."""
        if self.demo:
            return
        if not self._sync_track_session():
            return
        if self._track_loaded:
            return
        tid = self._track_id
        if tid is None:
            return

        track_file = track_map.find_track_file(tid, self.tracks_dir)
        local_doc = (track_store.load_local(self.tracks_dir, tid)
                     if track_file else None)
        manifest = track_store.cached_manifest()
        stale = track_store.needs_cloud_refresh(
            tid, local_doc, manifest, self.tracks_dir)
        need_fetch = not track_file or stale

        if track_file and self._try_load_local_track(tid):
            if stale:
                self._maybe_fetch_remote_track(tid, stale=True)
            return

        if need_fetch:
            self._maybe_fetch_remote_track(tid, stale=stale)

        if not self._no_track_hint:
            self._no_track_hint = True
            self.map_widget.flash_hint(
                "No track map — loading from cloud library\u2026")

    def _on_tracks_synced(self, n) -> None:
        """Startup cache refresh finished; if the map we're showing was one of
        the tracks that changed, reload it so the live view is up to date."""
        if not n:
            return
        if self.demo:
            tid = self._demo_track_pending_id or self._resolve_demo_track_id()
            path = track_map.find_track_file(tid, self.tracks_dir)
            if not path:
                return
            if not self._apply_track_from_path(path, str(tid)):
                return
            self._refresh_settings_authoring()
            return
        if self._track_id is None:
            return
        path = track_map.find_track_file(self._track_id, self.tracks_dir)
        if not path:
            return
        if not self._apply_track_from_path(path, str(self._track_id)):
            return
        self._track_loaded = True
        self._refresh_settings_authoring()

    def _on_remote_track(self, track_id, doc) -> None:
        """A shared track map arrived from the cloud (off the GUI thread)."""
        target = (self._demo_track_pending_id if self.demo else self._track_id)
        if not doc:
            log.warning("cloud track fetch returned no document for TrackID %s",
                        track_id)
            return
        if not track_store.track_doc_matches_session(
                self.tracks_dir, target, doc):
            log.warning("cloud track %s does not match session TrackID %s",
                        doc.get("track_id"), target)
            return
        remote_ts = doc.get("updated_at") or ""
        if self._track_loaded and not self.demo:
            if self._loaded_track_updated_at and \
                    self._loaded_track_updated_at == remote_ts:
                return
        try:
            path = track_store.write_local(self.tracks_dir, doc)
            if not path:
                log.warning("could not cache cloud track %s locally",
                            doc.get("track_id"))
                return
            if not self._apply_track_from_path(path, str(target)):
                log.warning("could not apply cloud track %s from %s",
                            doc.get("track_id"), path)
                return
            if not self.demo:
                saved_turns = _coerce_int(doc.get("num_turns"))
                self.map_widget.set_num_turns(self._track_turns or saved_turns)
                if saved_turns and not self._track_turns:
                    self._track_turns = saved_turns
            self._track_loaded = True
            if target is not None:
                self._remote_tried.add(str(target))
            track_store.enforce_cache_limit(
                self.tracks_dir, protect=[target, doc.get("track_id")])
            self._refresh_settings_authoring()
        except Exception as exc:
            log.warning("remote track apply failed: %s: %s",
                        type(exc).__name__, exc)

    def _reset_pit_state(self) -> None:
        """Clear pit-route runtime state (latches, not saved geometry)."""
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
                self._apply_pit_lane2_meta(meta)
                self.map_widget.update()
                return
            self._pit_span = None
            self._pit_path = self._pit_in = self._pit_out = None
            self._pit_in_pct = self._pit_out_pct = None
            self.map_widget.clear_pit()
            if self._pit_speed_ms > 0:
                self.map_widget.pit_speed_ms = self._pit_speed_ms
            self._apply_pit_lane2_meta(meta)
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
        self._apply_pit_lane2_meta(meta)
        self.map_widget.update()

    def _apply_pit_lane2_meta(self, meta: dict) -> None:
        """Push optional second pit lane geometry into state and map widget."""
        pct2 = meta.get("pit_lane_speed_pct_2")
        self._pit_lane_speed_pct_2 = float(pct2) if pct2 is not None else 1.0
        self.map_widget.pit_lane_speed_pct_2 = self._pit_lane_speed_pct_2

        if meta.get("pit_path_2") is None and "pit_path_2" in meta:
            self._pit_span_2 = None
            self._pit_path_2 = self._pit_in_2 = self._pit_out_2 = None
            self._pit_in_pct_2 = self._pit_out_pct_2 = None
            self.map_widget.set_pit_span_2(None)
            self.map_widget.set_pit_path_2(None)
            self.map_widget.set_pit_blends_2(None, None)
            self.map_widget.set_pit_route_pct_2(None, None)
            return

        pit_path_2 = meta.get("pit_path_2")
        if not pit_path_2 or len(pit_path_2) < 2:
            return

        pit_span_2 = meta.get("pit_span_2")
        if (not pit_span_2 and is_schematic_pit_source(self._pit_source)
                and self.map_widget.path):
            from tools.schematic_to_track import _pit_span_on_loop

            loop = [(p[0], p[1]) for p in self.map_widget.path]
            lo, hi = _pit_span_on_loop(loop, pit_path_2)
            pit_span_2 = (round(lo, 5), round(hi, 5))

        self._pit_span_2 = pit_span_2
        self._pit_path_2 = pit_path_2
        self._pit_in_2 = meta.get("pit_in_2")
        self._pit_out_2 = meta.get("pit_out_2")
        self._pit_in_pct_2 = meta.get("pit_in_pct_2")
        self._pit_out_pct_2 = meta.get("pit_out_pct_2")
        if pit_span_2:
            self.map_widget.set_pit_span_2(pit_span_2)
        self.map_widget.set_pit_path_2(pit_path_2)
        self.map_widget.set_pit_blends_2(self._pit_in_2, self._pit_out_2)
        self.map_widget.set_pit_route_pct_2(
            self._pit_in_pct_2, self._pit_out_pct_2)

    def _pit_route_interval_lane(self, lane: int):
        if lane == 2:
            lo, hi = self._pit_in_pct_2, self._pit_out_pct_2
            if lo is None or hi is None:
                if self._pit_span_2 is None:
                    return None
                lo, hi = self._pit_span_2
            return (lo, hi)
        return self._route_interval()

    def _pit_lane_for_car(self, idx: int, pct: float, on_pit: bool) -> int:
        """Pick pit lane 1 or 2 for OnPitRoad car placement."""
        if not on_pit:
            return 1
        if not (self._pit_path_2 and len(self._pit_path_2) >= 2):
            return 1
        matches: list[int] = []
        for lane in (1, 2):
            route = self._pit_route_interval_lane(lane)
            if route and self._pct_in_interval(pct, route[0], route[1]):
                matches.append(lane)
        if len(matches) == 1:
            return matches[0]
        loop_pt = self.map_widget._loop_point_at_pct(pct)
        if loop_pt is None:
            return 1
        best_lane = 1
        best_d = float("inf")
        for lane in (1, 2):
            path = self._pit_path if lane == 1 else self._pit_path_2
            if not path or len(path) < 2:
                continue
            nearest = self.map_widget._closest_point_on_chain([path], loop_pt)
            if nearest is None:
                continue
            d = math.hypot(nearest[0] - loop_pt[0], nearest[1] - loop_pt[1])
            if d < best_d:
                best_d = d
                best_lane = lane
        return best_lane

    def _update_map(self, player, lap_pct, surface, drivers,
                    positions=None, car_lap=None,
                    radio_speaker=None,
                    on_pit_arr=None, car_flags=None) -> None:
        # Load the track outline even when car telemetry isn't ready yet (common
        # on mid-race app restart); car dots need lap%/surface (focus optional).
        self._ensure_track(player, lap_pct)
        if not lap_pct or not surface:
            return

        mcfg = config.CFG["map"]
        # Resolve the focus car's model-space position when we have one (GPS or
        # dead reckoning) so the learner and the pit capture share a frame.
        if player is not None and 0 <= player < len(lap_pct):
            self._update_player_pos(lap_pct[player])
            # In demo mode the pit lane is synthesized once (see _load_demo_track);
            # skip live learning so it isn't overwritten by the demo's fake pit dips.
            if mcfg.get("show_pit", True) and not self.demo:
                self._update_pit_route(player, lap_pct)
        if mcfg.get("show_wind", True):
            self.map_widget.set_wind(self.ir["WindDir"], self.ir["WindVel"])
        else:
            self.map_widget.set_wind(None, 0.0)
        if mcfg.get("show_expanded_weather", False):
            wet = rain = None
            try:
                wet = self.ir["TrackWetness"]
            except (TypeError, ValueError, KeyError):
                pass
            try:
                rain = self.ir["RainIntensity"]
            except (TypeError, ValueError, KeyError):
                pass
            self.map_widget.set_weather(wet, rain)
        else:
            self.map_widget.set_weather(None, None)
        self.map_widget.set_track_zones(
            drs_zones=(self._track_zones.get("drs_zones") or []
                       if mcfg.get("show_drs_zones") else []),
            p2p_zones=(self._track_zones.get("p2p_zones") or []
                       if mcfg.get("show_p2p_zones") else []),
        )

        if on_pit_arr is None:
            on_pit_arr = self.ir["CarIdxOnPitRoad"]
        if self._pit_latch_seed_pending and player is not None:
            self._seed_pit_latches(lap_pct, on_pit_arr, player)
            self._pit_latch_seed_pending = False
        elif self._pit_latch_seed_pending and player is None:
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
            if pct is None or pct < 0.0:
                continue
            # Normalize tiny S/F overshoot instead of dropping the car for a tick.
            if pct > 1.0:
                pct = pct % 1.0
            is_player = idx == player
            on_pit = (bool(on_pit_arr[idx]) if on_pit_arr and idx < len(on_pit_arr)
                      else surface[idx] in pit_surf)
            approaching = (surface is not None and idx < len(surface)
                           and surface[idx] == oc.TRK_APPROACHING_PITS)
            is_pace = idx in self._pace_idxs
            # Show cars on track, pit road, or the brief APPROACHING_PITS exit
            # window after OnPitRoad clears; pace car only on the racing line.
            if not is_pace and not is_player:
                surf_ok = (surface is not None and idx < len(surface)
                           and surface[idx] in (oc.TRK_ON_TRACK,
                                                oc.TRK_APPROACHING_PITS))
                if not surf_ok and not on_pit:
                    continue
            if is_pace:
                if not config.CFG["map"].get("show_pace_car", True):
                    continue
                if surface[idx] != oc.TRK_ON_TRACK:
                    continue
            on_route = self._car_on_route(idx, pct, on_pit, is_player, route,
                                          blends_on, approaching=approaching)
            in_entry, _in_lane, in_exit = self._pit_route_phases(
                pct, on_pit=on_pit)
            d = drivers.get(idx)
            if is_pace:
                num = "PC"
                color = config.CFG["map"]["colors"].get("pace_car", "#0b0e12")
            elif use_pos and positions and idx < len(positions) and positions[idx]:
                num = str(positions[idx])
                color = player_color if is_player else self._map_car_color(
                    idx, player, car_lap, lap_pct)
            else:
                num = str(d.get("CarNumber", "?")) if d else "?"
                if is_player:
                    color = player_color
                else:
                    color = self._map_car_color(
                        idx, player, car_lap, lap_pct)
            speaking = radio_speaker is not None and idx == radio_speaker
            status_kind = None
            if mcfg.get("show_car_status", True):
                on = (bool(on_pit_arr[idx])
                      if on_pit_arr and idx < len(on_pit_arr) else False)
                sf = (car_flags[idx]
                      if car_flags and idx < len(car_flags) else None)
                surf = surface[idx] if idx < len(surface) else None
                status_kind = tr.map_car_status_kind(surf, on_pit=on, car_flag=sf)
            cars.append((idx, pct, num, color, is_player, on_route, on_pit,
                         speaking, is_pace, status_kind, in_entry, in_exit,
                         self._pit_lane_for_car(idx, pct, on_pit)))
        if mcfg.get("show_sector_boundaries", True):
            self.map_widget.set_sector_boundaries(self._sector_starts())
        else:
            self.map_widget.set_sector_boundaries([])
        if mcfg.get("show_traffic_markers", True):
            markers = self._map_traffic_markers(
                player, lap_pct, surface, positions, on_pit_arr, drivers)
            self.map_widget.set_traffic_markers(markers)
        else:
            self.map_widget.set_traffic_markers({})
        self.map_widget.set_schematic_exit_pcts(self._pit_exit_latch)
        self.map_widget.set_cars(cars)

    def _map_traffic_markers(self, player, lap_pct, surface, positions,
                             on_pit_arr, drivers) -> dict[str, dict | None]:
        """Ahead/behind/leader targets with hold-before-switch debouncing."""
        try:
            now = float(self.ir["SessionTime"])
        except (TypeError, ValueError, KeyError):
            now = 0.0
        hold = float(config.CFG["map"].get("marker_hold_seconds", 3.0) or 3.0)
        pit_surf = (oc.TRK_IN_PIT_STALL, oc.TRK_APPROACHING_PITS)
        candidates = select_marker_candidates(
            player, lap_pct, surface, positions,
            pace_idxs=self._pace_idxs,
            on_pit_arr=on_pit_arr,
            pit_surfaces=pit_surf,
        )
        raw = resolve_traffic_markers(
            self._marker_hold, candidates, lap_pct,
            now=now, hold_sec=hold,
            surface=surface, on_pit_arr=on_pit_arr,
            pace_idxs=self._pace_idxs,
            pit_surfaces=pit_surf,
        )
        out: dict[str, dict | None] = {}
        for slot, m in raw.items():
            if not m:
                out[slot] = None
                continue
            idx = m.get("idx")
            d = drivers.get(idx) if idx is not None else None
            label = str(d.get("CarNumber", "")) if d else ""
            out[slot] = {**m, "label": label}
        return out

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

    def _pit_route_phases(self, pct: float, *, on_pit: bool = False):
        """(entry, lane_span, exit) lap-% intervals for schematic pit phases."""
        pit_in = self._pit_in_pct
        pit_out = self._pit_out_pct
        lane = self._pit_span
        lane_lo = lane[0] if lane else None
        lane_hi = lane[1] if lane else None
        in_entry = (not on_pit and pit_in is not None and lane_lo is not None
                    and self._pct_in_interval(pct, pit_in, lane_lo))
        in_lane_span = (lane_lo is not None and lane_hi is not None
                        and self._pct_in_interval(pct, lane_lo, lane_hi))
        in_exit = (not on_pit and lane_hi is not None and pit_out is not None
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
        """Map dot color: one competitor color, blue if lapped, red if lapping."""
        colors = config.CFG["map"]["colors"]
        base = colors.get("competitor", "#b06bff")
        lapping, lap_ahead = self._lap_tint(idx, player, car_lap, False)
        if not lapping:
            return base
        if not lap_ahead:
            return colors.get("lapped", "#2563eb")
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

        in_entry, _, _ = self._pit_route_phases(pct, on_pit=on_pit)

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
        # Hold through pit exit like the player: stay on-route until past
        # pit_out / route end, not only while the exit-blend phase is active.
        if latched:
            end = self._pit_out_pct if self._pit_out_pct is not None else (
                route[1] if route else None)
            start = route[0] if route else 0.0
            if end is not None and self._pct_in_interval(pct, start, end):
                return True
            self._pit_route_latch[idx] = False
        return False

    def _update_pit_route(self, player, lap_pct) -> None:
        """Update schematic pit route latches while on pit road."""
        if player is None or not lap_pct:
            return
        on = self.ir["OnPitRoad"]
        if on is None:
            arr = self.ir["CarIdxOnPitRoad"]
            on = bool(arr[player]) if arr and player < len(arr) else False
        on = bool(on)
        pct = lap_pct[player]
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
        self.map_widget.set_player_xy(None)

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

    def _session_flag_bundle(self, player=None, positions=None, est_time=None,
                             lap_est=0.0, drivers=None) -> tuple[str | None, str | None]:
        """Resolved flag name plus a short contextual hint for the UI."""
        flag = self._session_flag()
        sf = getattr(self, "_last_session_flags", 0)
        ctx = self._flag_context(flag, sf, player, positions, est_time,
                                 lap_est, drivers)
        return flag, ctx

    def _flag_context(self, flag: str | None, sf: int, player=None,
                      positions=None, est_time=None, lap_est=0.0,
                      drivers=None) -> str | None:
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
                base = "1 lap to green"
            elif sf & self._FLAG_TEN_TO_GO:
                base = "10 to go"
            elif sf & self._FLAG_FIVE_TO_GO:
                base = "5 to go"
            elif sf & self._FLAG_CAUTION_WAVING:
                pits_open = None
                try:
                    pits_open = self.ir["PitsOpen"]
                except (TypeError, ValueError, KeyError):
                    pass
                if pits_open is True:
                    base = "Caution waving — pits open"
                elif pits_open is False:
                    base = "Caution waving — pits closed"
                else:
                    base = "Caution waving — pits closed"
            elif sf & self._FLAG_CAUTION:
                base = "Full course caution — hold position"
            elif sf & self._FLAG_YELLOW_WAVING:
                base = "Local yellow — slow in sector"
            elif sf & self._FLAG_YELLOW_BASE:
                base = "Local yellow — slow down"
            else:
                base = "Slow down — no passing"
            sec = self._sector_timer.idx + 1 if self._sector_timer.starts else None
            if sec:
                return f"{base} — Sector S{sec}"
            return base

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
            fcfg = config.CFG.get("flags", {})
            if (fcfg.get("show_blue_detail", True) and player is not None
                    and est_time and lap_est > 0 and drivers):
                ahead, _ = tr.nearest_ahead_behind(
                    est_time, player, lap_est, pace_idxs=self._pace_idxs)
                if ahead is not None and ahead > 0:
                    ahead_idx = None
                    me = est_time[player]
                    for idx, t in enumerate(est_time):
                        if idx == player or t is None:
                            continue
                        if tr.wrap_est_delta(t, me, lap_est) == ahead:
                            ahead_idx = idx
                            break
                    if ahead_idx is not None:
                        num = drivers.get(ahead_idx, {}).get("CarNumber", "?")
                        return f"Car #{num} +{ahead:.1f}s"
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
            fcfg = config.CFG.get("flags", {})
            if (fcfg.get("show_finish_position", True) and player is not None
                    and positions and player < len(positions)
                    and positions[player]):
                return f"Session complete — P{positions[player]}"
            return "Session complete"

        return None

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
        # DriverInfo often isn't ready on the first connected tick; treating an
        # empty car as a session change incorrectly falls through to Default and
        # desyncs the settings preset combo from the live overlay.
        if not car:
            return
        league_id = self._session_league_id()
        if car == self._last_car_path and league_id == self._last_league_id:
            return
        self._last_car_path = car
        self._last_league_id = league_id
        target = config.preset_for_session(league_id, car)
        if target and target != config.active_preset():
            self._show_profile_loading(f"Loading preset\u2026 {target}")
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
        # Demo packs continuous green bits so the flags panel stays busy; live
        # sessions only flash green briefly after a yellow (handled above).
        if (self.demo or self._demo_active) and (sf & self._FLAG_GREEN):
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
            flag, ctx = self._session_flag_bundle(
                player, positions, None, 0.0, self._driver_cache)
        irating = irating_delta = None
        car_number = ""
        drv = self._player_driver(player) if player is not None else None
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

        dash_data = {
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
        }

        if self._dash_uses_metric("incidents"):
            dash_data["incidents"] = self.ir["PlayerCarMyIncidentCount"]
        if self._dash_uses_metric("tires"):
            dash_data["tire_l"] = self.ir["LFwearM"]
            dash_data["tire_r"] = self.ir["RFwearM"]
        if self._dash_uses_any("fuel", "fuel_stack"):
            dash_data["fuel"] = self.ir["FuelLevel"]
        if self._dash_uses_any("fuel_laps", "fuel_stack"):
            dash_data["fuel_laps"] = self._fuel_laps()
        if self._dash_uses_metric("air_temp"):
            dash_data["air_temp"] = self.ir["AirTemp"]
        if self._dash_uses_metric("track_temp"):
            dash_data["track_temp"] = tele.read_track_temp(self.ir)
        if self._dash_uses_metric("last_lap"):
            dash_data["last_lap"] = self.ir["LapLastLapTime"]
        if self._dash_uses_metric("best_lap"):
            dash_data["best_lap"] = self.ir["LapBestLapTime"]
        if self._dash_uses_metric("my_session_best"):
            try:
                player = int(self.ir["PlayerCarIdx"])
                best = self.ir["CarIdxBestLapTime"]
                if best and 0 <= player < len(best):
                    t = best[player]
                    dash_data["my_session_best"] = float(t) if t and t > 0 else None
                else:
                    dash_data["my_session_best"] = None
            except (TypeError, ValueError, KeyError):
                dash_data["my_session_best"] = None
        if self._dash_uses_metric("cur_lap"):
            dash_data["cur_lap"] = self.ir["LapCurrentLapTime"]
        dash_cfg = config.CFG.get("dash", {})
        dash_delta_mode = dash_cfg.get("delta_bar_mode", "session_best")
        if self._dash_uses_metric("delta"):
            if self._delta_pit_hold:
                dash_data["delta"] = None
            else:
                dash_data["delta"] = self._resolve_lap_delta(
                    dash_delta_mode, positions)
        if dash_cfg.get("show_delta_bar"):
            if self._delta_pit_hold:
                dash_data["delta"] = None
            else:
                dash_data["delta"] = self._resolve_lap_delta(
                    dash_delta_mode, positions)
        if irating is not None:
            dash_data["irating"] = irating
        if irating_delta is not None:
            dash_data["irating_delta"] = irating_delta
        if flag is not None:
            dash_data["flag"] = flag
        if ctx is not None:
            dash_data["flag_context"] = ctx

        if self._dash_uses_metric("tires_4"):
            corners = tele.read_tire_corners(self.ir, wear=True, temp=False)
            for key, entry in corners.items():
                dash_data[f"tire_{key}"] = entry.get("wear")
        if self._dash_uses_metric("tire_temp"):
            corners = tele.read_tire_corners(self.ir, wear=False, temp=True)
            for key, entry in corners.items():
                dash_data[f"tire_temp_{key}"] = entry.get("temp")
            temps = [dash_data[k] for k in (
                "tire_temp_lf", "tire_temp_rf", "tire_temp_lr", "tire_temp_rr")
                     if isinstance(dash_data.get(k), (int, float))]
            dash_data["tire_temp_max"] = max(temps) if temps else None
        if self._dash_uses_metric("fuel_pct"):
            dash_data["fuel_pct"] = self.ir["FuelLevelPct"]
        if self._dash_uses_metric("fuel_burn"):
            dash_data["fuel_burn"] = self.ir["FuelUsePerHour"]
        if self._dash_uses_metric("delta_best"):
            dash_data["delta_best"] = tele.read_lap_delta(self.ir, "best_lap")
        if self._dash_uses_metric("delta_optimal"):
            dash_data["delta_optimal"] = tele.read_lap_delta(self.ir, "optimal")
        if self._dash_uses_metric("time_remain"):
            dash_data["time_remain"] = self._session_time_remain()
        if self._dash_uses_metric("class_pos") and player is not None:
            cp = self._class_positions
            if cp is None:
                try:
                    cp = self.ir["CarIdxClassPosition"]
                except (TypeError, ValueError, KeyError):
                    cp = None
            if cp and player < len(cp) and cp[player] and cp[player] > 0:
                dash_data["class_pos"] = cp[player]
                cid = self._player_class(self._driver_cache, player)
                if cid is not None and positions:
                    dash_data["class_total"] = sum(
                        1 for idx, d in self._driver_cache.items()
                        if d.get("CarClassID") == cid
                        and idx < len(positions)
                        and positions[idx] and positions[idx] > 0)
                else:
                    dash_data["class_total"] = sum(1 for x in cp if x and x > 0)
        if self._dash_uses_metric("incidents_team"):
            dash_data["incidents_team"] = self.ir["PlayerCarTeamIncidentCount"]
        if self._dash_uses_metric("incidents_limit"):
            wk = self._weekend_info()
            limit = wk.get("IncidentLimit")
            dash_data["incident_limit"] = (int(limit) if limit is not None
                                           and int(limit) > 0 else None)
        if self._dash_uses_metric("dc_brake_bias"):
            dash_data["dc_brake_bias"] = self.ir["dcBrakeBias"]
        if self._dash_uses_metric("dc_tc"):
            dash_data["dc_traction_control"] = self.ir["dcTractionControl"]
        if self._dash_uses_metric("dc_abs"):
            dash_data["dc_abs"] = self.ir["dcABS"]
        if self._dash_uses_metric("dc_fuel_mix"):
            dash_data["dc_fuel_mixture"] = self.ir["dcFuelMixture"]
        if self._dash_uses_metric("dc_tire_set"):
            dash_data["dc_tire_set"] = self.ir["dcTireSet"]
        if self._dash_uses_metric("engine_warn"):
            dash_data["engine_warnings"] = self.ir["EngineWarnings"]
        if self._dash_uses_metric("oil_temp"):
            dash_data["oil_temp"] = self.ir["OilTemp"]
        if self._dash_uses_metric("water_temp"):
            dash_data["water_temp"] = self.ir["WaterTemp"]
        if self._dash_uses_metric("voltage"):
            dash_data["voltage"] = self.ir["Voltage"]
        if self._dash_uses_any("gap_ahead", "gap_behind") and player is not None:
            est = self.ir["CarIdxEstTime"]
            lap_est = float(self._car_info.get("est_lap", 0.0) or 0.0)
            if lap_est <= 0 and est:
                lap_est = max((t for t in est if t and t > 0), default=0.0)
            if est and lap_est > 0:
                def _include(idx):
                    surf = self.ir["CarIdxTrackSurface"]
                    return self._relative_include(idx, surf, positions, player)
                ahead, behind = tr.nearest_ahead_behind(
                    est, player, lap_est, include_fn=_include,
                    pace_idxs=self._pace_idxs)
                if self._dash_uses_metric("gap_ahead"):
                    dash_data["gap_ahead"] = ahead
                if self._dash_uses_metric("gap_behind"):
                    dash_data["gap_behind"] = behind

        if self._dash_uses_metric("lap_corners"):
            snap = self._lap_engine.snapshot()
            turns = snap.get("turns") or []
            if turns:
                parts = []
                for t in turns[:2]:
                    lbl = t.get("label", "?")
                    lost = t.get("t_lost", 0.0)
                    parts.append(f"{lbl} {lost:+.2f}")
                dash_data["lap_corners"] = " ".join(parts)

        self.dash_widget.set_data(dash_data)

    def _update_inputs(self) -> None:
        """Feed the input-telemetry trace (throttle/brake/clutch/steer + gear)."""
        icfg = config.CFG.get("inputs", {})
        clutch_raw = self.ir["Clutch"]
        clutch = (1.0 - clutch_raw) if isinstance(clutch_raw, (int, float)) else None
        payload = {
            "throttle": self.ir["Throttle"],
            "brake": self.ir["Brake"],
            "clutch": clutch,
            "steer": self._steer_norm(),
            "abs_active": bool(self.ir["BrakeABSactive"]),
            "gear": self.ir["Gear"],
            "speed_ms": self.ir["Speed"],
        }
        if icfg.get("show_handbrake"):
            payload["handbrake"] = self.ir["HandbrakeRaw"]
        if icfg.get("show_steering_torque"):
            payload["steer_torque"] = self.ir["SteeringWheelPctTorque"]
        if icfg.get("show_tc_abs"):
            payload["tc_active"] = self.ir["dcTractionControl"]
            payload["abs_setting"] = self.ir["dcABS"]
        self.inputs_widget.set_data(payload)

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

    _DELTA_PIT_HOLD_TIMEOUT_SEC = 45.0

    def _ir_float(self, key: str) -> float | None:
        try:
            return tele.as_float(self.ir[key])
        except (TypeError, ValueError, KeyError):
            return None

    def _update_delta_last_lap_ref(self) -> None:
        """Track last completed lap time for delta_bar last_lap mode."""
        try:
            lap = int(self.ir["Lap"])
        except (TypeError, ValueError, KeyError):
            return
        if self._delta_prev_lap is None:
            self._delta_prev_lap = lap
            return
        if lap > self._delta_prev_lap:
            last = self._player_last_lap_time()
            if last is not None:
                self._delta_last_lap_time = last
            self._delta_prev_lap = lap
        elif lap < self._delta_prev_lap:
            self._delta_prev_lap = lap

    def _track_delta_pit_hold(self, on_pit: bool, sector_idx_before: int,
                              sector_idx_after: int, *,
                              lap_rolled: bool = False) -> None:
        """Suppress live delta on pit road and until first sector after exit."""
        if on_pit:
            self._delta_pit_hold = True
            self._delta_was_on_pit = True
            self._delta_pit_hold_since = None
        elif self._delta_was_on_pit:
            self._delta_pit_hold = True
            self._delta_was_on_pit = False
            self._delta_pit_hold_since = time.time()
        if self._delta_pit_hold:
            if sector_idx_after > sector_idx_before:
                self._delta_pit_hold = False
                self._delta_pit_hold_since = None
            elif lap_rolled and not on_pit:
                self._delta_pit_hold = False
                self._delta_pit_hold_since = None
            elif (not on_pit and self._delta_pit_hold_since is not None
                  and time.time() - self._delta_pit_hold_since
                  > self._DELTA_PIT_HOLD_TIMEOUT_SEC):
                self._delta_pit_hold = False
                self._delta_pit_hold_since = None

    def _resolve_lap_delta(self, mode: str, positions=None) -> float | None:
        """Whole-lap delta for the configured reference mode."""
        if mode == "last_lap":
            cur = self._ir_float("LapCurrentLapTime")
            ref = tele.as_float(self._delta_last_lap_time)
            if cur and cur > 0 and ref and ref > 0:
                return cur - ref
            return None
        if mode == "leader_last":
            leader_idx = None
            if positions:
                for idx, pos in enumerate(positions):
                    if pos == 1:
                        leader_idx = idx
                        break
            if leader_idx is not None:
                times = self._car_last
                cur = self._ir_float("LapCurrentLapTime")
                leader_last = None
                if times and leader_idx < len(times):
                    leader_last = tele.as_float(times[leader_idx])
                if (leader_last and leader_last > 0 and cur and cur > 0):
                    return cur - leader_last
            return None
        return tele.read_lap_delta(self.ir, mode)

    def _delta_bar_value(self, positions=None) -> float | None:
        if self._delta_pit_hold:
            return None
        mode = config.CFG["delta_bar"].get("mode", "session_best")
        return self._resolve_lap_delta(mode, positions)

    def _update_delta_bar(self, player=None, positions=None) -> None:
        """Feed the live delta against the configured reference lap."""
        delta_val = self._delta_bar_value(positions)
        widget = self.delta_bar_widget
        prev = getattr(widget, "data", None) or {}
        never_fed = not prev
        if (never_fed
                or tele.delta_value_moved(prev.get("delta"), delta_val)
                or getattr(widget, "_animating", False)):
            widget.set_data({"delta": delta_val})

    def _update_flags(self, player=None, positions=None, est_time=None,
                      lap_est=0.0, drivers=None) -> None:
        """Feed the standalone flag banner."""
        fcfg = config.CFG.get("flags", {})
        flag, ctx = self._session_flag_bundle(
            player, positions, est_time, lap_est, drivers)
        secondary = None
        incident_warn = False
        if flag is None and fcfg.get("show_incident_warning", True):
            try:
                count = int(self.ir["PlayerCarMyIncidentCount"] or 0)
            except (TypeError, ValueError):
                count = 0
            wk = self._weekend_info()
            limit = wk.get("IncidentLimit")
            if isinstance(limit, (int, float)) and limit > 0:
                pct = count / float(limit)
                thresh = float(fcfg.get("incident_warn_pct", 0.75) or 0.75)
                if pct >= thresh:
                    incident_warn = True
                    secondary = f"Incidents {count}/{int(limit)}"
        if fcfg.get("show_pit_limiter", True):
            try:
                ew = self.ir["EngineWarnings"]
                warn = tr.engine_warning_text(ew)
                if warn and "LIM" in warn.split():
                    secondary = secondary or "Pit limiter active"
            except (TypeError, ValueError, KeyError):
                pass
        payload = {
            "flag": flag,
            "flag_context": ctx,
            "secondary": secondary,
            "incident_warn": incident_warn,
            "edit": self.edit_mode_enabled(),
        }
        if payload == getattr(self.flags_widget, "data", None):
            return
        self.flags_widget.set_data(payload)

    def _advance_sector_timer(self, player, lap_pct) -> None:
        """Advance sector splits from lap distance (no widget paint)."""
        self._sector_timer.set_boundaries(self._sector_starts())
        prev_idx = self._sector_timer.idx
        prev_pct = self._sector_timer._prev_pct
        pct = None
        if (isinstance(lap_pct, (list, tuple)) and isinstance(player, int)
                and 0 <= player < len(lap_pct)):
            pct = lap_pct[player]
        self._sector_timer.update(pct, self.ir["LapCurrentLapTime"],
                                  self.ir["LapLastLapTime"])
        lap_rolled = (
            isinstance(pct, (int, float)) and pct >= 0
            and prev_pct is not None and pct + 0.5 < prev_pct
        )
        try:
            on_pit = bool(self.ir["OnPitRoad"])
        except (TypeError, ValueError, KeyError):
            on_pit = False
        self._track_delta_pit_hold(
            on_pit, prev_idx, self._sector_timer.idx, lap_rolled=lap_rolled)
        scfg = config.CFG.get("sector_timing", {})
        if (scfg.get("highlight_active_sector_on_map", False)
                and self._visible_widgets.get("map") and self._sector_timer.starts):
            self.map_widget.set_active_sector(
                self._sector_timer.idx, self._sector_timer.starts)
        elif self._visible_widgets.get("map"):
            self.map_widget.set_active_sector(None, None)

    def _update_sector_widget(self) -> None:
        """Push sector snapshot to the sector timing widget."""
        scfg = config.CFG.get("sector_timing", {})
        snap = self._sector_timer.snapshot(
            self.ir["LapCurrentLapTime"], self.ir["LapLastLapTime"],
            self.ir["LapBestLapTime"],
            show_delta=scfg.get("show_sector_delta", False))
        key = tele.sector_timing_snap_key(snap)
        if key == getattr(self, "_sector_snap_key", None):
            return
        self._sector_snap_key = key
        self.sector_widget.set_data(snap)

    def _track_fuel_per_lap(self) -> None:
        """Record fuel burned on each completed lap (shared by fuel calc + lap log)."""
        fuel = self.ir["FuelLevel"]
        cap = self._fuel_capacity(fuel)
        try:
            lap = int(self.ir["Lap"])
        except (TypeError, ValueError):
            lap = None
        if lap is None or not isinstance(fuel, (int, float)):
            return
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
        elif lap < self._fc_prev_lap:
            self._fc_use = []
            self._fc_prev_lap = lap
            self._fc_lap_start_fuel = fuel

    def _update_lap_engine(self, player, lap_pct) -> None:
        """Record the player's lap for corner comparison (engine only)."""
        if not self.demo:
            self._lap_engine.set_identity(self._lap_compare_key(),
                                          self._car_info.get("redline"))
        pct = None
        if (isinstance(lap_pct, (list, tuple)) and isinstance(player, int)
                and 0 <= player < len(lap_pct)):
            pct = lap_pct[player]
        surf = self.ir["PlayerTrackSurface"]
        off_track = surf == oc.TRK_OFF_TRACK
        lcfg = config.CFG.get("lap_compare", {})
        wet = None
        if lcfg.get("exclude_wet_laps", True):
            try:
                wet = self.ir["TrackWetness"]
            except (TypeError, ValueError, KeyError):
                pass
        gear = rpm = None
        if lcfg.get("show_gear_rpm", False):
            gear = self.ir["Gear"]
            rpm = self.ir["RPM"]
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
            gear=gear,
            rpm=rpm,
            off_track=off_track,
            incidents=self.ir["PlayerCarMyIncidentCount"],
            corner_pcts=self._corner_pcts(),
            track_len=self._track_length_m(),
            track_wetness=wet,
            exclude_wet=lcfg.get("exclude_wet_laps", True),
            wet_threshold=float(lcfg.get("wetness_delta_threshold", 5.0) or 5.0),
        )

    @staticmethod
    def _lap_compare_snap_key(snap: dict) -> tuple:
        turns = snap.get("turns") or []
        return (
            snap.get("live_delta"),
            snap.get("last_delta"),
            snap.get("ref_time"),
            snap.get("have_ref"),
            snap.get("is_new_best"),
            tuple((t.get("label"), t.get("t_lost")) for t in turns[:6]),
        )

    def _update_lap_compare_widget(self) -> None:
        """Feed the lap compare widget from the engine snapshot."""
        snap = self._lap_engine.snapshot()
        key = self._lap_compare_snap_key(snap)
        if key == self._lap_compare_snap_key:
            return
        self._lap_compare_snap_key = key
        self.lap_compare_widget.set_data(snap)

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

    def _fuel_lap_secs(self):
        """Lap time used for fuel time projections (prefer car est over mismatched log)."""
        est = self._car_info.get("est_lap") or 0.0
        good = [l["secs"] for l in self._ll_laps if l.get("secs", 0) > 0]
        if good and est > 0:
            avg = sum(good[:10]) / len(good[:10])
            if abs(avg - est) / est <= 0.20:
                return avg
            return est
        if good:
            return sum(good[:10]) / len(good[:10])
        return est if est > 0 else None

    def _avg_lap_secs(self):
        """Average recent lap time (s): logged laps if any, else the est lap."""
        return self._fuel_lap_secs()

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
        fcfg = config.CFG["fuel_calc"]
        payload = pstrat.build_fuel_snapshot(
            self.ir,
            car_info=self._car_info,
            fc_use=self._fc_use,
            ll_laps=self._ll_laps,
            cfg=fcfg,
        )
        key = tele.fuel_payload_key(payload)
        if key == getattr(self, "_fuel_payload_key", None):
            return
        self._fuel_payload_key = key
        self.fuel_widget.set_data(payload)

    def _update_pit_advisor(self) -> None:
        pacfg = config.CFG.get("pit_advisor", {})
        edit = self.edit_mode_enabled()
        telem = tele.read_pit_advisor_telemetry(self.ir, self._car_info)
        player = telem.get("player")
        lap_est = float(telem.get("est_lap") or self._car_info.get("est_lap", 0.0) or 0.0)
        est_time = telem.get("est_time")
        positions = telem.get("positions")
        drivers = self._drivers()
        car_lap = telem.get("car_lap")
        on_pit = telem.get("on_pit_road")
        surface = telem.get("surface")
        lap_pcts = telem.get("lap_pcts")
        f2_time = telem.get("f2_time")
        car_last = telem.get("car_last")
        car_flags = telem.get("car_flags")
        flag, ctx = self._session_flag_bundle(
            player, positions, est_time, lap_est, drivers)
        # Quiet during the short green-resume window after yellow clears —
        # same window the dash flag bar uses; no advice needed mid-restart.
        if flag == "green" and not edit:
            self._set_pit_advisor_payload({
                "rec": None, "label": None, "rationale": None,
                "secondary": None, "actionable": False, "edit": False,
            }, has_content=False)
            return
        pit_ctx = pstrat.build_pit_context(
            player=player,
            positions=positions,
            est_time=est_time,
            lap_est=lap_est,
            drivers=drivers,
            pace_idxs=self._pace_idxs,
            flag=flag,
            flag_context=ctx,
            session_flags=telem.get("session_flags")
            or getattr(self, "_last_session_flags", 0),
            pits_open=telem.get("pits_open"),
        )
        cur_lap = telem.get("lap")
        sess_time = telem.get("session_time")
        pstrat.update_caution_tracker(
            self._caution_tracker,
            yellow=bool(pit_ctx.get("caution")),
            lap=cur_lap,
            session_time=sess_time,
        )
        caution_hist = self._caution_tracker.as_dict()
        pit_loss = pstrat.resolve_pit_loss(pacfg, self._pit, player)
        laps_rem = telem.get("session_laps_remain_ex")
        if laps_rem is None:
            laps_rem = telem.get("session_laps_remain")
        dry_limit = telem.get("dry_tire_set_limit")
        sets_total = (
            int(dry_limit) if isinstance(dry_limit, (int, float))
            and 0 < int(dry_limit) < 255 else None)
        field_intel = pstrat.build_field_intel(
            player=player,
            positions=positions,
            car_lap=car_lap,
            on_pit_road=on_pit,
            surface=surface,
            pit_state=self._pit,
            lap_pcts=lap_pcts,
            f2_time=f2_time,
            est_time=est_time,
            lap_est=lap_est,
            pace_idxs=self._pace_idxs,
            current_lap=cur_lap,
            pit_loss=pit_loss,
            cfg=pacfg,
            ll_laps=self._ll_laps,
            car_last=car_last,
            car_flags=car_flags,
            session_time=sess_time,
            closing_state=self._pit_advisor_closing_state,
            laps_remaining=laps_rem,
            sets_total=sets_total,
        )
        tire_snapshot = pstrat.build_tire_snapshot(
            telem,
            ll_laps=self._ll_laps,
            cfg=pacfg,
            pit_state=self._pit,
            player=player,
            car_lap=car_lap,
            laps_remaining=laps_rem,
            pit_loss=pit_loss,
        )
        caution_outlook = pstrat.build_caution_outlook(
            caution_hist,
            field_intel,
            current_lap=cur_lap,
            cfg=pacfg,
        )
        session_phase = pstrat.build_session_phase(telem, pacfg)
        pit_menu = tele.read_pit_menu(telem)
        snapshot = pstrat.build_fuel_snapshot(
            self.ir,
            car_info=self._car_info,
            fc_use=self._fc_use,
            ll_laps=self._ll_laps,
            cfg=pacfg,
            caution=bool(pit_ctx.get("caution")),
        )
        strategy = pstrat.build_strategy_extras(
            snapshot,
            ll_laps=self._ll_laps,
            cfg=pacfg,
            pit_state=self._pit,
            player=player,
            car_lap=car_lap,
            on_pit_road=on_pit,
            field_intel=field_intel,
            tire_snapshot=tire_snapshot,
            session_phase=session_phase,
            pit_loss=pit_loss,
        )
        advice = pstrat.advise_pit_strategy(
            snapshot,
            pit_ctx,
            pacfg,
            field_intel=field_intel,
            caution_hist=caution_hist,
            strategy=strategy,
            tire_snapshot=tire_snapshot,
            caution_outlook=caution_outlook,
            pit_menu=pit_menu,
            session_phase=session_phase,
        )
        payload = {
            "rec": advice.rec.value,
            "label": advice.label,
            "rationale": advice.rationale,
            "secondary": advice.secondary,
            "actionable": advice.actionable,
            "edit": edit,
        }
        has_content = True
        if not edit:
            has_content = bool(advice.label)
            if has_content and pacfg.get("show_only_when_actionable", True):
                has_content = bool(advice.actionable)
        self._set_pit_advisor_payload(payload, has_content=has_content)

    def _set_pit_advisor_payload(self, payload: dict, *, has_content: bool) -> None:
        """Push pit-advisor data and keep panel visibility in sync."""
        prev = getattr(self, "_pit_advisor_has_content", False)
        self._pit_advisor_has_content = has_content
        if payload != getattr(self.pit_advisor_widget, "data", None):
            self.pit_advisor_widget.set_data(payload)
        if has_content != prev:
            self._apply_visibility()

    def _update_tire_panel(self) -> None:
        cfg = config.CFG.get("tire_panel", {})
        corners = tele.read_tire_corners(
            self.ir,
            wear=cfg.get("show_wear", True),
            temp=cfg.get("show_temp", True),
            pressure=cfg.get("show_pressure", False),
        )
        payload = {
            "corners": corners,
            "edit": self.edit_mode_enabled(),
        }
        if payload == getattr(self.tire_panel_widget, "data", None):
            return
        self.tire_panel_widget.set_data(payload)

    def _update_pit_board(self) -> None:
        cfg = config.CFG.get("pit_board", {})
        flags = self.ir["PitSvFlags"]
        services = ps.decode_flags(flags)
        fuel_l = None
        try:
            fuel_l = float(self.ir["PitSvFuel"])
        except (TypeError, ValueError, KeyError):
            pass
        compound = None
        if cfg.get("show_compound", True):
            try:
                compound = int(self.ir["PitSvTireCompound"])
            except (TypeError, ValueError, KeyError):
                pass
        repairs = None
        if cfg.get("show_fast_repairs", True):
            try:
                used = int(self.ir["FastRepairUsed"] or 0)
                avail = int(self.ir["FastRepairAvailable"] or 0)
                repairs = f"{max(0, avail - used)}/{avail}" if avail > 0 else None
            except (TypeError, ValueError, KeyError):
                pass
        pressures = None
        if cfg.get("show_pressures", False):
            pressures = {}
            for key, sdk_key in zip(
                    ("lf", "rf", "lr", "rr"), tele.PIT_PRESSURE_KEYS):
                try:
                    val = self.ir[sdk_key]
                    if isinstance(val, (int, float)):
                        pressures[key] = float(val)
                except (TypeError, ValueError, KeyError):
                    pass
            if not pressures:
                pressures = None
        pit_active = bool(self.ir["PitstopActive"])
        payload = {
            "services": services,
            "fuel_l": fuel_l if fuel_l and fuel_l > 0 else None,
            "compound": compound if compound and compound > 0 else None,
            "repairs": repairs,
            "pressures": pressures,
            "pit_active": pit_active,
            "edit": self.edit_mode_enabled(),
        }
        if payload == getattr(self.pit_board_widget, "data", None):
            return
        self.pit_board_widget.set_data(payload)

    def _weather_track_trend(self, track_temp, window_s: float) -> float | None:
        if not isinstance(track_temp, (int, float)):
            return None
        hist = self._ensure_weather_hist()
        now = time.time()
        self._weather_sample_counter += 1
        if self._weather_sample_counter >= 60:
            self._weather_sample_counter = 0
            hist.append((now, float(track_temp)))
        if not hist:
            return None
        cutoff = now - max(30.0, window_s)
        old = None
        for ts, val in hist:
            if ts >= cutoff:
                old = val
                break
        if old is None:
            old = hist[0][1]
        t = config.conv_temp(track_temp)
        o = config.conv_temp(old)
        if t is None or o is None:
            return None
        return t - o

    def _update_weather_panel(self) -> None:
        cfg = config.CFG.get("weather_panel", {})
        wk = self._weekend_info()
        snap = tele.weather_snapshot(self.ir, wk, cfg=cfg)
        window = float(cfg.get("trend_window_seconds", 300.0) or 300.0)
        if cfg.get("show_trend", True):
            track_temp = snap.get("track_temp")
            snap["track_trend"] = self._weather_track_trend(track_temp, window)
        snap["edit"] = self.edit_mode_enabled()
        if snap == getattr(self.weather_panel_widget, "data", None):
            return
        self.weather_panel_widget.set_data(snap)

    def _update_system_panel(self) -> None:
        cfg = config.CFG.get("system_panel", {})
        cpu, mem, gpu, cpu_pct, mem_pct, gpu_pct = self._sys_stats()
        snap = tele.perf_snapshot(self.ir, cfg=cfg)
        snap["cpu"] = cpu
        snap["mem"] = mem
        snap["gpu"] = gpu
        snap["cpu_pct"] = cpu_pct
        snap["mem_pct"] = mem_pct
        snap["gpu_pct"] = gpu_pct
        if cfg.get("show_network", True):
            if not tele.channel_usable(
                    snap.get("chan_quality"), snap.get("chan_latency")):
                wifi = sysstats.wifi_signal()
                if wifi:
                    snap["wifi"] = wifi
        snap["edit"] = self.edit_mode_enabled()
        if snap == getattr(self.system_panel_widget, "data", None):
            return
        self.system_panel_widget.set_data(snap)

    @staticmethod
    def _leaderboard_speed_mph(speed_ms) -> int | None:
        if speed_ms is None:
            return None
        try:
            v = float(speed_ms)
        except (TypeError, ValueError):
            return None
        if v <= 0:
            return None
        return int(round(v * 2.2369362921))

    def _update_radio_tower(self, positions, drivers, player,
                            radio_speaker) -> None:
        edit = self.edit_mode_enabled()
        rows = []
        # Show whoever is transmitting — does not require a local ego car
        # (spectators often have PlayerCarIdx == -1).
        if (radio_speaker is not None
                and radio_speaker >= 0
                and radio_speaker not in self._pace_idxs):
            pos = ""
            if positions and 0 <= radio_speaker < len(positions):
                raw = positions[radio_speaker]
                if raw and raw > 0:
                    pos = raw
            d = self._driver_for_row(radio_speaker, player, drivers or {})
            user_name = d.get("UserName", "") or f"Car {radio_speaker}"
            g_icon, g_color = self._group_badge_fields(user_name)
            rows.append({
                "position": pos,
                "car_number": d.get("CarNumber", ""),
                "name": user_name,
                "active": True,
                "is_player": (player is not None and radio_speaker == player),
                "is_pro": self._is_pro_driver_name(user_name),
                "group_icon": g_icon,
                "group_color": g_color,
            })
        payload = {"rows": rows, "edit": edit}
        if payload == getattr(self.radio_tower_widget, "data", None):
            return
        self.radio_tower_widget.set_data(payload)

    def _update_leaderboard_strip(self, positions, drivers, car_f2,
                                  lap_est, player, car_lap=None) -> None:
        cfg = config.CFG.get("leaderboard_strip", {})
        car_speed = None
        try:
            car_speed = self.ir["CarIdxSpeed"]
        except (KeyError, TypeError):
            pass
        if not positions:
            if self.edit_mode_enabled():
                self.leaderboard_strip_widget.set_data({
                    "rows": [],
                    "edit": True,
                })
            return
        cap = int(cfg.get("rows", 0) or 0)
        ranked = sorted(
            (idx for idx, pos in enumerate(positions)
             if pos and pos > 0 and idx not in self._pace_idxs),
            key=lambda idx: positions[idx],
        )
        if cap > 0:
            ranked = ranked[:cap]
        rows = []
        for idx in ranked:
            d = self._driver_for_row(idx, player, drivers)
            pos = positions[idx]
            gap = tr.fmt_leader_gap(
                car_f2[idx] if car_f2 and idx < len(car_f2) else None,
                pos, lap_est)
            lap = None
            if car_lap and idx < len(car_lap):
                try:
                    lap = int(car_lap[idx])
                except (TypeError, ValueError):
                    lap = None
            speed_mph = None
            if car_speed and idx < len(car_speed):
                speed_mph = self._leaderboard_speed_mph(car_speed[idx])
            rows.append({
                "position": pos,
                "car_number": d.get("CarNumber", ""),
                "name": d.get("UserName", f"Car {idx}"),
                "class_color": d.get("CarClassColor", "#888888"),
                "gap": gap,
                "lap": lap,
                "speed_mph": speed_mph,
                "is_player": idx == player,
            })
        payload = {
            "rows": rows,
            "edit": self.edit_mode_enabled(),
        }
        if payload == getattr(self.leaderboard_strip_widget, "data", None):
            return
        self.leaderboard_strip_widget.set_data(payload)

    def _update_ers_hybrid(self) -> None:
        snap = hy.snapshot(self.ir)
        snap["edit"] = self.edit_mode_enabled()
        if snap == getattr(self.ers_hybrid_widget, "data", None):
            return
        self.ers_hybrid_widget.set_data(snap)


def _resolve_backend() -> str:
    """Pick overlay backend: rust (default when binary present) or python."""
    if "--python" in sys.argv:
        return "python"
    if "--rust" in sys.argv:
        return "rust"
    if "--backend" in sys.argv:
        i = sys.argv.index("--backend")
        if i + 1 < len(sys.argv):
            return sys.argv[i + 1].strip().lower()
    env = os.environ.get("GRIDGLANCE_BACKEND", "").strip().lower()
    if env in ("python", "rust"):
        return env
    from .rust_launcher import find_overlay_binary
    if find_overlay_binary() is not None:
        return "rust"
    return "python"


def _main_rust() -> int:
    """Launch Rust overlay process + Python settings (exits with settings)."""
    import atexit
    import signal
    import time
    import traceback

    from .ipc_client import OverlayIpcClient, OverlayIpcError, RemoteOverlay
    from .rust_launcher import start_rust_overlay

    click_through = "--no-clickthrough" not in sys.argv
    demo = "--demo" in sys.argv
    start_now = (
        "--start" in sys.argv
        or demo
        or bool(config.CFG.get("start_overlay_on_launch", False))
    )
    open_settings = "--no-settings" not in sys.argv

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

    from . import single_instance
    instance = single_instance.acquire(app)
    if instance is None:
        return 0

    # Mutable holder so cleanup can reach the child even if spawn partially
    # succeeded or remote/client setup fails mid-way.
    hold: dict = {"proc": None, "remote": None, "done": False}

    def _shutdown_overlay() -> None:
        if hold["done"]:
            return
        hold["done"] = True
        remote = hold.get("remote")
        if remote is not None:
            try:
                remote.stop_overlay()
            except OverlayIpcError:
                pass
        proc = hold.get("proc")
        if proc is not None and proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except Exception:  # noqa: BLE001
                proc.kill()

    def _excepthook(exc_type, exc, tb) -> None:
        _shutdown_overlay()
        traceback.print_exception(exc_type, exc, tb, file=sys.stderr)
        sys.__excepthook__(exc_type, exc, tb)

    sys.excepthook = _excepthook
    atexit.register(_shutdown_overlay)
    signal.signal(signal.SIGINT, lambda *_: app.quit())

    try:
        try:
            proc = start_rust_overlay(
                demo=demo,
                click_through=click_through,
                stopped=not start_now,
            )
        except FileNotFoundError as exc:
            QMessageBox.critical(None, "GridGlance", str(exc))
            return 1
        hold["proc"] = proc

        client = OverlayIpcClient()
        deadline = time.monotonic() + 8.0
        while time.monotonic() < deadline:
            try:
                client.ping()
                break
            except OverlayIpcError:
                if proc.poll() is not None:
                    QMessageBox.critical(
                        None, "GridGlance",
                        "Rust overlay exited before IPC became ready.")
                    return 1
                time.sleep(0.1)
                QApplication.processEvents()
        else:
            QMessageBox.critical(
                None, "GridGlance",
                "Timed out waiting for Rust overlay IPC on 127.0.0.1:19847.")
            return 1

        remote = RemoteOverlay(
            client,
            demo=demo,
            edit_mode=not click_through,
            running=start_now,
        )
        hold["remote"] = remote
        config.on_preset_change(lambda _name: remote.apply_active_preset())
        # Push full live CFG (not sparse disk merge) so show flags match Settings.
        try:
            remote.apply_active_preset()
        except OverlayIpcError:
            pass
        if start_now:
            try:
                remote.start_overlay()
            except OverlayIpcError:
                pass
        if not click_through:
            try:
                remote.set_edit_mode(True)
            except OverlayIpcError:
                pass

        from .config_editor import ConfigEditor
        from .updater import UpdateChecker

        remote._updater = UpdateChecker()
        updater_bridge = _LaunchUpdater(app, remote)
        remote._updater_bridge = updater_bridge
        remote._updater.found.connect(updater_bridge.on_found)
        remote._updater.progress.connect(updater_bridge.on_progress)
        remote._updater.downloaded.connect(updater_bridge.on_downloaded)
        remote._updater.failed.connect(updater_bridge.on_failed)
        if config.CFG.get("check_updates_on_launch", True):
            remote._updater.start()

        def _open_settings() -> None:
            existing = getattr(app, "_settings_window", None)
            if existing is not None and existing.isVisible():
                existing.raise_()
                existing.activateWindow()
                return
            editor = ConfigEditor(overlay=remote)
            app._settings_window = editor
            remote._settings_window = editor

            def _on_close(*_args) -> None:
                # Settings exits; Rust overlay keeps racing.
                pass

            editor.destroyed.connect(_on_close)
            editor.show()

        def _quit_all() -> None:
            _shutdown_overlay()
            app.quit()

        instance.set_activate_callback(_open_settings)

        # Keep the Python process alive after settings closes so the tray can
        # reopen settings / quit. The Rust overlay is the long-lived paint process.
        tray = None
        if icon_path:
            from PyQt6.QtGui import QAction, QIcon
            from PyQt6.QtWidgets import QMenu, QSystemTrayIcon
            tray = QSystemTrayIcon(QIcon(icon_path), app)
            menu = QMenu()
            act_settings = QAction("Settings", menu)
            act_settings.triggered.connect(_open_settings)
            act_quit = QAction("Quit", menu)
            act_quit.triggered.connect(_quit_all)
            menu.addAction(act_settings)
            menu.addAction(act_quit)
            tray.setContextMenu(menu)
            tray.setToolTip("GridGlance")
            tray.show()

        app.setQuitOnLastWindowClosed(tray is None)
        if open_settings or not start_now:
            _open_settings()

        return app.exec()
    finally:
        _shutdown_overlay()



def main() -> int:
    if "--dump-config" in sys.argv:
        path = config.write_template()
        print(f"Wrote default config template to {path}")
        print("Edit it to customize every color, font, size, count and toggle.")
        return 0

    backend = _resolve_backend()
    if backend == "rust":
        return _main_rust()

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
    # waits for you to press "Start Overlay". --start / --demo / Settings toggle
    # start widgets immediately; --no-settings skips opening the settings window.
    start_now = (
        "--start" in sys.argv
        or demo
        or bool(config.CFG.get("start_overlay_on_launch", False))
    )
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

    # Taskbar / desktop double-click should activate the tray app, not spawn
    # another process. Must run before building the HUD.
    from . import single_instance
    instance = single_instance.acquire(app)
    if instance is None:
        return 0

    import signal
    import traceback

    def _excepthook(exc_type, exc, tb) -> None:
        traceback.print_exception(exc_type, exc, tb, file=sys.stderr)
        sys.__excepthook__(exc_type, exc, tb)

    sys.excepthook = _excepthook

    signal.signal(signal.SIGINT, lambda *_: app.quit())
    keepalive = QTimer()
    keepalive.start(200)
    keepalive.timeout.connect(lambda: None)

    hud = AdvancedSimHUD(click_through=click_through, demo=demo,
                         demo_track=demo_track, tracks_dir=tracks_dir)
    instance.set_activate_callback(hud.open_settings)

    # In-app auto-update: check GitHub for a newer release and offer to install.
    # Bound QObject slots (not bare lambdas) so worker-thread signals queue onto
    # the GUI thread — same pattern as the settings-page updater.
    from .updater import UpdateChecker
    hud._updater = UpdateChecker()
    bridge = _LaunchUpdater(app, hud)
    hud._updater_bridge = bridge
    hud._updater.found.connect(bridge.on_found)
    hud._updater.progress.connect(bridge.on_progress)
    hud._updater.downloaded.connect(bridge.on_downloaded)
    hud._updater.failed.connect(bridge.on_failed)
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


class _LaunchUpdater(QObject):
    """Startup/tray update UX: progress, errors, stop overlay, then install."""

    def __init__(self, app: QApplication, hud: AdvancedSimHUD):
        super().__init__(app)
        self._app = app
        self._hud = hud
        self._dl_dialog: QProgressDialog | None = None
        self._dl_canceled = False

    def _parent_window(self):
        w = getattr(self._hud, "_settings_window", None)
        if w is not None and w.isVisible():
            return w
        return None

    @pyqtSlot(dict)
    def on_found(self, info: dict) -> None:
        parent = self._parent_window()
        box = QMessageBox(parent) if parent is not None else QMessageBox()
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
        box.setDefaultButton(QMessageBox.StandardButton.Yes)
        if box.exec() != QMessageBox.StandardButton.Yes:
            return
        url = info.get("url")
        if not url:
            QMessageBox.warning(
                parent, "No installer",
                "That release doesn't have a downloadable installer "
                "for this platform.")
            return
        self._begin_download(url, info.get("version", "?"))

    def _begin_download(self, url: str, ver: str) -> None:
        self._dl_canceled = False
        parent = self._parent_window()
        dlg = QProgressDialog("Downloading update\u2026", "Cancel", 0, 100, parent)
        dlg.setWindowTitle(f"Downloading v{ver}")
        dlg.setWindowModality(Qt.WindowModality.ApplicationModal)
        dlg.setAutoClose(False)
        dlg.setAutoReset(False)
        dlg.setMinimumDuration(0)
        dlg.setValue(0)
        dlg.canceled.connect(self._cancel_download)
        self._dl_dialog = dlg
        dlg.show()
        self._hud._updater.download_async(url)

    @pyqtSlot()
    def _cancel_download(self) -> None:
        self._dl_canceled = True
        if self._dl_dialog is not None:
            self._dl_dialog.close()
            self._dl_dialog = None

    @pyqtSlot(int, int)
    def on_progress(self, done: int, total: int) -> None:
        if self._dl_dialog is None:
            return
        if total > 0:
            self._dl_dialog.setMaximum(100)
            self._dl_dialog.setValue(int(done * 100 / total))
            mb = done / 1_048_576
            tot = total / 1_048_576
            self._dl_dialog.setLabelText(
                f"Downloading update\u2026  {mb:.1f} / {tot:.1f} MB")
        else:
            self._dl_dialog.setMaximum(0)

    @pyqtSlot(str)
    def on_downloaded(self, path: str) -> None:
        if self._dl_canceled:
            return
        if self._dl_dialog is not None:
            self._dl_dialog.close()
            self._dl_dialog = None
        self._launch_installer(path)

    @pyqtSlot(str)
    def on_failed(self, msg: str) -> None:
        if self._dl_dialog is not None:
            self._dl_dialog.close()
            self._dl_dialog = None
        if self._dl_canceled:
            return
        QMessageBox.warning(
            self._parent_window(), "Download failed",
            f"Couldn't download the update.\n\n{msg}")

    def _launch_installer(self, path: str) -> None:
        import subprocess
        parent = self._parent_window()
        try:
            if os.name == "nt":
                subprocess.Popen([path, "/VERYSILENT", "/SUPPRESSMSGBOXES",
                                  "/NORESTART"])
            else:
                subprocess.Popen([path])
        except Exception as exc:  # noqa: BLE001
            QMessageBox.warning(
                parent, "Couldn't start installer",
                f"The update was downloaded to:\n{path}\n\n"
                f"but couldn't be launched automatically:\n{exc}")
            return
        try:
            self._hud.stop_overlay()
        except Exception:
            pass
        self._app.quit()


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
