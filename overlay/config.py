"""
Central configuration for every overlay widget.

Everything visual or behavioral (colors, fonts, sizes, toggles, row counts,
ranges, animation speeds) is defined here as defaults and can be overridden by an
`overlay_config.json` file in the per-user data folder (see paths.data_dir), kept
separate from the code so app updates never overwrite your settings. Only the keys
you want to change need to appear in that file -- it is deep-merged over the
defaults.

Generate a full, editable template with:
    python3 sim_hud.py --dump-config        # writes overlay_config.json

Colors accept any of: "#RGB", "#RRGGBB", "#RRGGBBAA", "rgba(r,g,b,a)" or
[r, g, b] / [r, g, b, a] lists.
"""

from __future__ import annotations

import copy
import json
import os

from PyQt6.QtGui import QColor

from . import layout_store
from . import paths

CONFIG_FILE = paths.data_file("overlay_config.json")

# Every column a timing table knows how to draw. Which ones appear, and in what
# order, is controlled per table by its "column_order" list (add/remove/reorder
# from the settings editor). "stripe" is not a column -- it's a sub-toggle of the
# position cell -- so it lives in the table's "columns" dict instead.
TABLE_COLUMNS = ["badge", "position", "car_number", "name", "license",
                 "irating", "pit", "gap", "last_lap", "best_lap",
                 "class_pos", "status", "car_flag", "laps",
                 "gap_ahead", "gap_leader", "closing",
                 "qual_pos", "qual_best", "gap_pole",
                 "team", "nickname"]

LAPTIME_LOG_COLUMNS = ("lap", "time", "delta", "temp", "sectors", "fuel",
                       "tires", "incidents", "tag")

# Shared styling defaults for the timing tables. Each table (Relative, Standings)
# gets its *own* copy of these so they can be themed and sized independently from
# the settings editor -- changing one never touches the other.
_TABLE_STYLE: dict = {
    "corner_radius_frac": 0.0,
    "alt_row_shading": True,
    "row_dividers": True,
    "name_font_bold": True,
    "data_font_bold": False,
    "irating_show_icon": True,
    # Fixed row height in pixels. When > 0, rows, text and header keep this
    # size no matter how big the panel is dragged -- resizing the panel just
    # reveals more empty space instead of zooming the table. Set to 0 to fall
    # back to the old "scale to fit" behavior (capped by max_row_height_frac).
    "row_height_px": 36,
    # Cap a row's height to this fraction of the panel height so that, when
    # only a few cars are present, rows don't stretch and the text doesn't
    # look zoomed in (extra space is left empty below). 0 disables the cap.
    # Only used when row_height_px is 0.
    "max_row_height_frac": 0.14,
    "font_scale": 0.40,        # row text size (multiple of row height)
    "gap_font_scale": 1.12,
    # The license pill shows iRating + class (e.g. "1.4k R"). When this is on,
    # iRating is abbreviated ("1.4k"); turn it off to show the full number
    # ("1432"). Also applies to the iRating column and the SOF readouts.
    "irating_abbreviate": True,
    # When on, show projected iRating +/- beside the iRating column (race only).
    "show_irating_projection": False,
    # Header / footer text size, independent of the row font above.
    "header_font_scale": 1.0,
    "footer_font_scale": 1.0,
    "row_ease_tau": 0.16,
    "fade_ease_tau": 0.12,
    "widths": {  # as multiples of row height
        "badge": 0.95,
        "position": 1.25,
        "car_number": 1.60,
        "gap": 1.70,
        "irating": 1.20,
        "license": 1.35,
        "pit": 2.10,
        "last_lap": 2.90,
        "best_lap": 2.90,
        "class_pos": 1.35,
        "status": 1.50,
        "car_flag": 1.35,
        "laps": 1.35,
        "gap_ahead": 1.70,
        "gap_leader": 1.70,
        "closing": 1.80,
        "qual_pos": 1.35,
        "qual_best": 2.90,
        "gap_pole": 1.70,
        "team": 2.20,
        "nickname": 2.20,
        "gutter": 0.18,
    },
    "colors": {
        # Vertical gradient card matching the dash (top lighter -> bottom dark).
        "bg": "#1b1f26f2",
        "bg_top": "#1b1f26f2",
        "bg_bottom": "#0f1216f2",
        "border": "#ffffff28",
        "cell_dark": "#0b0e12",
        "row_alt": "#ffffff14",
        "player_row": "#ff941658",
        "header_bg": "#0b0e12bb",
        "footer_bg": "#0f1216",
        "pit_row": "#8b93a118",
        "inactive_row": "#8b93a128",
        # Lapped-traffic row tints: "threat" (red) = a car a lap ahead that
        # will lap you; "lapped" (blue) = a car a lap down that you're lapping.
        # Rendered as a soft left-to-right gradient wash in the relative table.
        "threat": "#ff505060",
        "lapped": "#4a8cff60",
        "speaking_row": "#22c55e50",
        "text": "#f4f6f8",
        "muted": "#8b93a1",
        "irating_bg": "#0b0d11cc",
        "irating_border": "#ffffff20",
        "irating_text": "#f4f6f8",
        "irating_delta_up": "#46df7a",
        "irating_delta_down": "#ff5050",
        "badge_player": "#ff9416",
        "badge_pit_bg": "#ebeef0",
        "badge_pit_text": "#141414",
        "badge_lap": "#7638c4",
        "badge_speaking_bg": "#22c55e",
        "badge_speaking_text": "#ffffff",
        "badge_speaking_border": "#ffffffcc",
        "badge_empty_border": "#ffffff28",
        "badge_empty_fill": "#00000078",
        "pro_name": "#f5c542",
        "pro_badge": "#f5c542",
        "flag_black": "#1a1a1acc",
        "flag_black_text": "#ffffff",
        "flag_meatball": "#ff9416cc",
        "flag_meatball_text": "#141414",
        "flag_dq": "#ff5050cc",
        "flag_dq_text": "#ffffff",
        "flag_furled": "#ffd23acc",
        "flag_furled_text": "#141414",
    },
    "license_colors": {
        "R": "#d34a3c",
        "D": "#e0791a",
        "C": "#d6b400",
        "B": "#3a9b3a",
        "A": "#2f6bd8",
        "P": "#1a1a1a",
    },
}

# Shared polish keys merged into list-style / card widgets (not dash).
_WIDGET_CHROME: dict = {
    "row_dividers": True,
    "data_font_bold": False,
    "corner_radius_frac": 0.0,
}
_WIDGET_CHROME_COLORS: dict = {
    "header_bg": "#0b0e12bb",
    "footer_bg": "#0f1216",
    "cell_dark": "#0b0e12",
    "cell_border": "#ffffff20",
    "row_alt": "#ffffff14",
}

DEFAULTS: dict = {
    "font_family": "Segoe UI",
    # Monospace override for gap / lap-time columns (empty = same as font_family).
    "tabular_font_family": "",
    # Global multiplier applied to every text size in every widget. Raise it to
    # make all text bigger, lower it to make everything smaller. Each widget also
    # has its own "text_scale" that multiplies on top of this one.
    "text_scale": 1.20,
    # Unit system for speed, temperature and fuel: "metric" (km/h, C, L) or
    # "imperial" (mph, F, gal). Affects the unit-aware "speed", temperature and
    # fuel readouts. (speed_kph / speed_mph stay fixed to their named unit.)
    "units": "metric",
    # Check GitHub for a newer release when the app launches (silent unless an
    # update is found). The "Check for Updates" button works regardless.
    "check_updates_on_launch": True,
    # When True, start overlay widgets immediately on app launch (same as --start).
    # Default False preserves Settings-first launch for desktop / Start Menu.
    "start_overlay_on_launch": False,
    # When True on Windows, place a Startup-folder shortcut so GridGlance runs at
    # login. The OS toggle is applied via overlay.autostart; this flag is the
    # remembered preference (synced from the shortcut on the Settings App page).
    "start_at_login": False,
    "relative": {
        **copy.deepcopy(_TABLE_STYLE),
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "rows_ahead": 3,
        "rows_behind": 3,
        "center_on_player": True,
        "show_footer": True,
        # pit_mode: one of "laps_since" (laps out since last stop),
        # "time_since" (time out since last stop), "at_lap" (lap they pitted on),
        # "at_time" (race clock when they pitted).
        "pit_mode": "laps_since",
        # Which columns appear and in what order (left to right). Add, remove and
        # reorder them from the settings editor. The "name" column always
        # stretches to fill the leftover space.
        "column_order": ["badge", "position", "name", "license",
                         "irating", "gap"],
        # The position cell's class-color stripe (not a column of its own).
        "columns": {"stripe": True},
        # Header / footer are each split into three sections (left/center/right);
        # pick which item goes in each (or "none"). Any item works in any slot:
        #   sof, class_sof, position, class_position, session_time, race_time,
        #   lap, incidents, track_name, track_temp, air_temp, best_lap,
        #   session_best, local_time, sim_time, cpu, mem, laps_remain,
        #   incident_limit, fast_repairs, weather, track_wetness, session_type.
        "header": {"left": "sof", "center": "none", "right": "position"},
        "footer": {"left": "race_time", "center": "lap", "right": "incidents"},
        # Per-section: show a Font Awesome icon instead of the text label.
        "header_icons": {"left": False, "center": False, "right": False},
        "footer_icons": {"left": False, "center": False, "right": False},
    },
    "standings": {
        **copy.deepcopy(_TABLE_STYLE),
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "rows": 10,  # how many to show in top-N mode (center_on_player off)
        # When centered, the window is rows_ahead + you + rows_behind.
        "rows_ahead": 4,
        "rows_behind": 5,
        # When true, show a window of the running order centered on the player
        # instead of the top N positions.
        "center_on_player": True,
        # When center_on_player is on, pin P1–P3 in the first three rows and
        # show the player-centered window in the remaining rows (same total height).
        "pin_podium": False,
        "show_footer": True,
        "title": "Standings",
        "pit_mode": "laps_since",
        # Which columns appear and in what order (left to right). Add, remove and
        # reorder them from the settings editor. The "name" column always
        # stretches to fill the leftover space.
        "column_order": ["badge", "position", "name", "license",
                         "irating", "gap"],
        # The position cell's class-color stripe (not a column of its own).
        "columns": {"stripe": True},
        # Header / footer each have three sections; pick the item for each (or
        # "none"). order_pill / title / count are standings-specific; every
        # other item (sof, class_sof, position, class_position, session_time,
        # race_time, lap, incidents, track_name, track_temp, air_temp, best_lap,
        # session_best, local_time, sim_time, cpu, mem) works in any slot too.
        "header": {"left": "order_pill", "center": "title", "right": "count"},
        "footer": {"left": "track_temp", "center": "session_time",
                   "right": "air_temp"},
        # Per-section: show a Font Awesome icon instead of the text label.
        "header_icons": {"left": False, "center": False, "right": False},
        "footer_icons": {"left": False, "center": False, "right": False},
    },
    "laptime_log": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # How many of your most recent laps to list (newest at the top).
        "rows": 8,
        "alt_row_shading": True,
        "show_header": True,
        # Row text size (multiple of row height) and the header size on top of it.
        "font_scale": 0.42,
        "header_font_scale": 1.0,
        # Fixed row height in pixels (0 = scale rows to fill the panel).
        "row_height_px": 0,
        "max_row_height_frac": 0.14,
        # Show a thermometer icon before the track temperature column.
        "temp_icon": True,
        # What DELTA compares each lap against: "previous" (the lap before it),
        # "best" (session best in log), or "personal_best" (LapBestLapTime).
        "delta_mode": "previous",
        # Visible columns in order (optional extras default off).
        "column_order": ["lap", "time", "delta", "temp"],
        "colors": {
            **_WIDGET_CHROME_COLORS,
            # Vertical gradient card matching the dash/tables.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "border": "#ffffff28",
            "panel_border": "#ffffff28",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            # Column headers (LAP / TIME / DELTA / TEMP.).
            "header": "#ffd23a",
            # Delta colors: faster (improved) vs slower than the baseline.
            "faster": "#46df7a",
            "slower": "#e23b3b",
        },
    },
    "fuel_calc": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "title": "FUEL CALCULATOR",
        # How many recent laps of fuel use to average for the projections.
        "history_laps": 10,
        # Toggle each feature on/off; hidden sections collapse and the rest
        # reflow to fill the panel.
        "show_title": True,
        "show_pill": True,       # pit-window status pill
        "show_add": True,        # big "add fuel to finish" box
        "show_gauge": True,      # fuel level gauge
        "show_stats": True,      # AVG / MAX / MIN usage grid
        "show_strip": True,      # PIT lap-timeline strip
        "show_time": True,       # TIME UNTIL EMPTY box
        "show_laps": True,       # LAPS UNTIL EMPTY box
        "show_live_burn": False,
        "show_tank_pct": False,
        "show_stints": False,
        "show_low_fuel_alert": True,
        "show_pit_compare": False,
        "pit_loss_seconds": 25.0,
        "stint_laps": 15,
        "legal_fuel_buffer_l": 2.0,
        "low_fuel_laps_threshold": 2.0,
        "low_fuel_time_threshold": 120.0,
        # Stats grid row height (0 = scale to fit the stats block).
        "row_height_px": 0,
        "max_row_height_frac": 0.14,
        # Stats grid text size, independent of the widget text_scale above.
        "stats_header_font_scale": 1.0,
        "stats_row_font_scale": 1.0,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            # Vertical gradient card matching the dash/tables.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "border": "#ffffff28",
            "panel_border": "#ffffff28",
            "accent": "#e23b3b",      # thin top bar
            "title": "#f4f6f8",
            "header": "#8b93a1",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "row_alt": "#ffffff14",
            "cell_dark": "#0b0e12",
            # Pit-window status pill + the big "add fuel" box.
            "pill_open": "#46df7a",
            "pill_closed": "#6e747d",
            "pill_text": "#06210f",
            "add_bg": "#0b0e12",
            "add_text": "#f4f6f8",
            # Fuel level gauge.
            "gauge_fill": "#f4f6f8",
            "gauge_bg": "#0b0e12",
            "gauge_border": "#ffffff30",
            # The two summary boxes (time / laps until empty).
            "box_border": "#46df7a",
            "box_value": "#f4f6f8",
            "box_warn": "#e23b3b",
            # PIT lap-timeline strip.
            "strip_none": "#333a42",
            "strip_window": "#46df7a",
            "strip_now": "#ffd23a",
        },
    },
    "radar": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        "range_pct": 0.03,
        "ease_side_tau": 0.10,
        "ease_glow_tau": 0.13,
        # Front/rear proximity sensing. Turn either off to hide the ahead/behind
        # glow and skip its detection (e.g. if you only want blind-spot warnings).
        "show_front": True,
        "show_rear": True,
        # The side warning is a moving marker: a car alongside slides from the
        # bottom (level with your rear bumper) up to the top (your front bumper)
        # as it pulls forward. This is how far ahead/behind, as a fraction of a
        # lap, maps to the marker reaching the very top/bottom of the radar.
        "side_span_pct": 0.0045,
        # When on, the side marker fades yellow->red by fore/aft overlap: red when
        # a car is dead alongside you, yellowing as it slides to your front/rear
        # bumper. This is an approximation (iRacing gives no true sideways
        # distance), so it's off by default -- the marker is plain red otherwise.
        "side_proximity_color": False,
        # Car number on the side marker (uses EstTime + lap-% alongside zone).
        "show_side_labels": False,
        # Tint side markers by EstTime closing rate (yellow -> red).
        "closing_rate_color": False,
        # Closing rate (m/s) that maps to full red tint.
        "closing_rate_full": 1.5,
        # Show seconds since blind spot last cleared (CarLeftRight clear).
        "show_clear_timer": False,
        # Lap-% window treated as "alongside" for side car correlation.
        "alongside_zone_pct": 0.004,
        "show_nose": True,
        "show_axis": True,
        # Draw a rounded card behind the radar (matches the dash panels).
        "show_panel": False,
        "sizes": {
            "car_w": 0.13,
            "car_h": 0.20,
            "bar_h": 0.78,
            "glow_w": 0.17,
            "nose_len": 0.16,
        },
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "car": "#f4f6f8",
            "red": "#e23b3b",
            "yellow": "#ffd23a",
            "axis": "#46df7a3a",
            "nose": "#46df7ae6",
            # Card background gradient + border, matching the dash style.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
        },
    },
    "dash": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "shift_segments": 20,
        "shift_red_frac": 0.16,
        "shift_yellow_frac": 0.24,
        # Flash the whole shift bar once RPM reaches the top of the range,
        # signalling "shift now". shift_blink_hz is the flash rate.
        "shift_blink": True,
        "shift_blink_hz": 7.0,
        # RPM (as a fraction of the car's redline) at which the bar starts to
        # flash. Raise it toward 1.0 if the blink feels too early, lower it for
        # an earlier warning. Used in preference to iRacing's shift-light RPMs.
        "shift_blink_pct": 0.99,
        # After this many seconds of continuous blink-eligible RPM, stop
        # flashing until RPM drops below the threshold (then blink again).
        "shift_blink_max_sec": 3.0,
        "ring_segments": 16,
        # Which driver inputs the center medallion shows. These apply to BOTH
        # center modes: in "ring" each selected input is a concentric arc
        # (outer->inner: throttle, brake, clutch); in "pedals" each is a vertical
        # bar. Pick any combination (e.g. throttle only, or throttle + brake).
        "show_throttle": True,
        "show_brake": True,
        "show_clutch": False,
        "show_shift_bar": True,
        "show_ring": True,
        # Center medallion content: "ring" (gear + input ring) or "pedals"
        # (throttle / brake / clutch bars with an ABS highlight).
        "center_mode": "ring",
        "show_position": True,
        # Flag indicator (yellow / black / green) driven by SessionFlags. Green
        # only flashes briefly when racing resumes out of a yellow, then clears
        # after flag_green_seconds. flag_blink_hz is the wave/flash rate.
        "show_flags": True,
        "flag_green_seconds": 3.0,
        # The flag bar flashes ("pulses") when a flag appears, then holds steady.
        # flag_pulse turns the flash on/off, flag_pulse_seconds is how long it
        # flashes for, and flag_blink_hz is the flash rate.
        "flag_pulse": True,
        "flag_pulse_seconds": 1.5,
        "flag_blink_hz": 2.5,
        # A thin horizontal delta bar across the top (faster = green to the
        # right, slower = red to the left). delta_bar_mode is independent of
        # the standalone Delta Bar widget. delta_bar_range is the seconds at
        # full deflection.
        "show_delta_bar": False,
        "delta_bar_mode": "session_best",
        "delta_bar_range": 1.0,
        # Every content slot below picks any metric (or "none" to hide it):
        # speed, rpm, gear, position, car_number, lap_count, laps_left, lap, fuel,
        # fuel_stack, fuel_laps, tires, incidents, last_lap, best_lap,
        # cur_lap, delta, irating, air_temp, track_temp.
        "top_right": "incidents",       # readout next to the shift bar
        "primary_left": "lap_count",    # small readout, lower-left
        "primary_right": "speed",       # big readout, lower-left
        "stat_left": "tires",           # stacked cell, lower-right
        "stat_right": "fuel_stack",     # stacked cell, lower-right
        "strip_left": "air_temp",       # bottom strip
        "strip_center": "track_temp",
        "strip_right": "last_lap",
        # iRating: assign "irating" to any slot; toggle show_irating_projection
        # to draw the projected change inline next to the value.
        "irating_abbreviate": True,
        "show_irating_projection": False,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "border": "#ffffff28",
            "panel_border": "#ffffff28",
            "label": "#8b93a1",
            "value": "#f4f6f8",
            "muted": "#8b93a1",
            "irating_bg": "#0b0d11cc",
            "irating_border": "#ffffff20",
            "irating_text": "#f4f6f8",
            "irating_delta_up": "#46df7a",
            "irating_delta_down": "#ff5050",
            "gear": "#ffffff",
            "green": "#46df7a",
            "ring_track": "#333a42",
            "orange": "#ff9416",
            "warn": "#e0a93a",
            "shift_green": "#46df7a",
            "shift_yellow": "#ffd23a",
            "shift_red": "#e23b3b",
            "shift_off": "#333a42",
            "pill_bg": "#0b0d11ee",
            "pill_border": "#ffffff20",
            # Border around the floating gear/throttle medallion so it stands out.
            "medallion_border": "#46df7a",
            # Pedal bars (throttle / brake / clutch) + ABS highlight + track.
            "pedal_throttle": "#46df7a",
            "pedal_brake": "#e23b3b",
            "pedal_clutch": "#3aa0ff",
            "pedal_track": "#333a42",
            "abs": "#ffd23a",
            # Delta bar: faster (negative delta) vs slower, plus its track.
            "delta_faster": "#46df7a",
            "delta_slower": "#e23b3b",
            "delta_bar_track": "#333a42",
            # Flag banner backgrounds + text.
            "flag_yellow": "#ffd23a",
            "flag_yellow_text": "#1a1400",
            "flag_black": "#0a0a0a",
            "flag_black_text": "#ffffff",
            # Meatball (mechanical black flag, must repair) - orange disc style.
            "flag_meatball": "#ff7a1a",
            "flag_meatball_text": "#1a0d00",
            # Furled/rolled black flag = warning.
            "flag_furled": "#caa23a",
            "flag_furled_text": "#1a1400",
            # Disqualified.
            "flag_dq": "#c0392b",
            "flag_dq_text": "#ffffff",
            "flag_green": "#46df7a",
            "flag_green_text": "#06210f",
            # White flag (final lap) - shows briefly like the green flag.
            "flag_white_bg": "#eef1f4",
            "flag_white_text": "#14171c",
            # Red flag - session stopped.
            "flag_red": "#d11f2d",
            "flag_red_text": "#ffffff",
            # Blue flag - faster car behind, let it by.
            "flag_blue": "#2f6bd8",
            "flag_blue_text": "#ffffff",
            # Debris on track - warning.
            "flag_debris": "#e0a72e",
            "flag_debris_text": "#1a1400",
            # Crossed flag - halfway point of the race.
            "flag_crossed": "#2a2f38",
            "flag_crossed_text": "#f4f6f8",
            # Checkered (session finished) - dark bar with a black/white weave.
            "flag_checker_bg": "#14171c",
            "flag_checker_text": "#f4f6f8",
        },
    },
    "inputs": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Seconds of pedal history shown by the scrolling trace (its width).
        "history_seconds": 6.0,
        # Which inputs appear in the trace graph (throttle/brake/clutch also get a
        # value bar; steering is trace-only, swinging around the centre line).
        "show_throttle": True,
        "show_brake": True,
        "show_clutch": False,
        "show_steering": False,
        "show_handbrake": False,
        "show_steering_torque": False,
        "show_tc_abs": False,
        "show_shift_markers": False,
        # A horizontal "trail-braking" reference line. When the brake trace climbs
        # above brake_threshold percent (0..100), that part turns the over color.
        "show_brake_threshold": False,
        "brake_threshold": 85,
        # Layout sections (each can be hidden; the rest reflow to fill the panel):
        # the vertical title tab, the scrolling trace, the value bars and the
        # gear/speed medallion.
        "show_label": True,
        "show_graph": True,
        "show_bars": True,
        "show_gauge": True,
        "label_text": "TELEMETRY",
        # Trace line thickness in pixels.
        "line_width": 2.4,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff28",
            # The vertical accent bar beside the title.
            "accent": "#e23b3b",
            "label": "#cdd3db",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            # Trace well background + gridlines.
            "graph_bg": "#0b0d11",
            "grid": "#ffffff14",
            # Input channels (also used for the value bars).
            "throttle": "#46df7a",
            "brake": "#e23b3b",
            "clutch": "#3aa0ff",
            "steering": "#c08bff",
            # Brake line turns this color while ABS is active...
            "brake_abs": "#ffd23a",
            # ...and this color where it's above the brake threshold line.
            "brake_over": "#ff7a1a",
            # The brake threshold reference line itself.
            "threshold": "#ffffff66",
            "bar_track": "#262b34",
            # Gear/speed medallion.
            "gauge_bg": "#0b0d11",
            "gauge_ring": "#333a42",
        },
    },
    "delta_bar": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Reference lap: session_best, best_lap, optimal, last_lap, leader_last.
        "mode": "session_best",
        # Seconds of delta at full bar deflection (smaller = more sensitive).
        "range": 1.0,
        # Show the big signed number above the bar.
        "show_value": True,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff28",
            "faster": "#46df7a",
            "slower": "#e23b3b",
            "track": "#262b34",
            "center": "#8b93a1",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
        },
    },
    "flags": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Text shown when no flag is flying.
        "idle_text": "TRACK CLEAR",
        "show_incident_warning": True,
        "incident_warn_pct": 0.75,
        "show_blue_detail": True,
        "show_pit_limiter": True,
        "show_finish_position": True,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff28",
            # Calm "no flag" state.
            "idle_bg": "#1f242c",
            "idle_text": "#9fb0a4",
            # Flag banner backgrounds + text (mirrors the dash flag colors).
            "flag_yellow": "#ffd23a",
            "flag_yellow_text": "#1a1400",
            "flag_black": "#0a0a0a",
            "flag_black_text": "#ffffff",
            "flag_meatball": "#ff7a1a",
            "flag_meatball_text": "#1a0d00",
            "flag_furled": "#caa23a",
            "flag_furled_text": "#1a1400",
            "flag_dq": "#c0392b",
            "flag_dq_text": "#ffffff",
            "flag_green": "#46df7a",
            "flag_green_text": "#06210f",
            "flag_white_bg": "#eef1f4",
            "flag_white_text": "#14171c",
            "flag_red": "#d11f2d",
            "flag_red_text": "#ffffff",
            "flag_blue": "#2f6bd8",
            "flag_blue_text": "#ffffff",
            "flag_debris": "#e0a72e",
            "flag_debris_text": "#1a1400",
            "flag_crossed": "#2a2f38",
            "flag_crossed_text": "#f4f6f8",
            "flag_checker_bg": "#14171c",
            "flag_checker_text": "#f4f6f8",
        },
    },
    "lap_compare": {
        **_WIDGET_CHROME,
        "alt_row_shading": True,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Max number of corner rows listed (worst-first).
        "max_turns": 6,
        # Fixed row height for corner rows (0 = scale to fit).
        "row_height_px": 0,
        "max_row_height_frac": 0.14,
        # Only list corners where you gained/lost at least this many seconds.
        "min_time_loss": 0.03,
        # Show the big live delta-to-best (vs the last completed lap's delta).
        "show_live_delta": True,
        # Show the delta-over-distance sparkline.
        "show_graph": True,
        "reference_mode": "best",
        "show_brake_markers": False,
        "show_lift_markers": False,
        "show_gear_rpm": False,
        "exclude_wet_laps": True,
        "wetness_delta_threshold": 5.0,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff28",
            "accent": "#e23b3b",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "faster": "#46df7a",
            "slower": "#e23b3b",
            "chip_bg": "#0b0d11cc",
            "chip_border": "#ffffff20",
            "graph_bg": "#0b0d11",
            "grid": "#ffffff1f",
            "graph_line": "#ffd23a",
        },
    },
    "sector_timing": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "row_height_px": 0,
        "max_row_height_frac": 0.0,
        # Fallback sector count when the session provides no sector layout.
        "sectors": 3,
        "show_sector_delta": False,
        "show_predicted_lap": False,
        "highlight_active_sector_on_map": False,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff28",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            # Sector cell fills by state: matched your best (purple), completed,
            # the in-progress sector, and not-yet-run.
            "sec_best": "#6b39c8",
            "sec_done": "#22303f",
            "sec_running": "#1d3a2a",
            "sec_running_edge": "#46df7a",
            "sec_idle": "#161a20",
            "sec_text": "#dfe3ea",
            "faster": "#46df7a",
            "slower": "#e23b3b",
        },
    },
    "map": {
        **_WIDGET_CHROME,
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size (corner labels, car numbers), x global text_scale.
        "text_scale": 1.0,
        "asphalt_width": 12,
        "outline_width": 6,
        # Car dot sizes. 0.05 is the default; raise/lower to scale. Player and
        # field cars are independent (player glow follows player size).
        "dot_radius_frac": 0.05,
        "other_dot_radius_frac": 0.05,
        "show_infield": True,
        "show_corners": True,
        # Auto-number corners from the track shape when the track file has no
        # corner data (learned tracks). Detected by curvature, numbered in
        # driving order from the start/finish line.
        "auto_corners": True,
        "show_start_finish": True,
        # Orient the map to taste: rotate in 90-degree steps (0/90/180/270) and
        # optionally mirror it horizontally. Everything (track, cars, corners,
        # pit, start/finish) rotates together; the wind compass stays north-up.
        "rotation": 0,
        "mirror": False,
        # Highlight the stretch of track the pit lane runs alongside (learned by
        # driving through the pits once) and show the pit speed limit + your live
        # pit-lane speed.
        "show_pit": True,
        # Draw the pit entry/exit "blend" lines (the commit lanes joining the
        # track to pit road and back). When off, the pit lane itself still shows
        # and a car only appears in the pits while it's actually on pit road,
        # snapping back to the track the moment it leaves.
        "show_pit_blends": True,
        # Show the static pit speed-limit badge on the map (independent of the
        # blend lines). Only relevant when the pit lane is shown.
        "show_pit_speed": True,
        # Opacity (0..1) of the drawn pit lane and its entry/exit blend lines.
        # Lower it to make the whole pit route fade back behind the track.
        "pit_lane_opacity": 1.0,
        # What each car dot shows: "number" (car number) or "position".
        "car_label": "number",
        # Opacity (0..1) of a car's dot while it's on pit road.
        "pit_dot_opacity": 0.45,
        # Lap-distance window (fraction of a lap) for red "lapping you" tint on
        # the map when a car is ~one lap ahead but not yet a full lap clear.
        "lap_proximity_pct": 0.04,
        # Show a small wind compass (arrow + speed) in the map's corner.
        "show_wind": True,
        # Rain / track wetness readout under the wind compass.
        "show_expanded_weather": False,
        # Small status badge on car dots (pit / off / flags).
        "show_car_status": True,
        # Optional DRS / push-to-pass zone highlights (track JSON drs_zones / p2p_zones).
        "show_drs_zones": False,
        "show_p2p_zones": False,
        # Pace car dot (CarIsPaceCar) and sector / traffic marker overlays.
        "show_pace_car": True,
        "show_sector_boundaries": True,
        "show_traffic_markers": True,
        # Seconds a new ahead/behind/leader candidate must hold before the icon
        # moves (prevents flicker when two cars are side-by-side).
        "marker_hold_seconds": 3.0,
        # Draw a rounded card behind the whole map. Off by default so only the
        # infield (the area enclosed by the track loop) is shaded.
        "show_panel": False,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "asphalt": "#333a42",
            "outline": "#8b93a1",
            "infield": "#0f1216c8",
            "player": "#46df7a",
            "competitor": "#b06bff",
            "lapped": "#4a8cff",
            "lapping": "#ff5050",
            "corner_bg": "#0b0d11cc",
            "corner_border": "#ffffff20",
            "corner_text": "#d6dce2",
            "scan_bg": "#000000c8",
            "hint_bg": "#ff9416e6",
            "hint_text": "#14161a",
            # Pit-lane highlight (thin red slashed line), its label text, and the
            # over-limit warning color.
            "pit": "#ff4d4d",
            "pit_text": "#ffffff",
            "pit_over": "#ffd23a",
            # Pit entry/exit blend lines (the "commit" lanes that join the pit
            # road to the track), drawn as dashed slashes. Entry is yellow, exit
            # is blue so the two ends read apart at a glance.
            "pit_blend": "#ffd23a",
            "pit_blend_out": "#3aa0ff",
            # Fill for a car's dot while it's on pit road (grayed out).
            "pit_car": "#6e747d",
            # Wind compass arrow + label.
            "wind": "#9fd0ff",
            "wind_text": "#eaf3ff",
            "pace_car": "#0b0e12",
            "pace_car_text": "#ffffff",
            "sector_line": "#a78bfa",
            "sector_text": "#c4b5fd",
            "marker_leader": "#ffd23a",
            "marker_ahead": "#46df7a",
            "marker_behind": "#ff5050",
            "marker_line": "#ffffff40",
            "speaking_ring": "#46df7a",
            "speaking_glow": "#46df7a55",
            "speaking_badge_bg": "#22c55e",
            "speaking_badge_text": "#ffffff",
            "status_pit": "#ffd23a",
            "status_off": "#ff5050",
            "status_garage": "#8b93a1",
            "status_black": "#1a1a1a",
            "status_meatball": "#ff9416",
            "status_dq": "#ff5050",
            "status_furled": "#ffd23a",
            "drs_zone": "#46df7a88",
            "p2p_zone": "#3aa0ff88",
            "active_sector": "#ffd23a66",
            # Card background gradient + border, matching the dash style.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
        },
        "palette": [
            "#3aa0ff", "#ff5bac", "#46d27a", "#b06bff", "#ffa23a",
            "#ff5b5b", "#36d6d6", "#d6d636", "#7a8cff", "#ff8cce",
            "#5be0a0", "#c0c0c0",
        ],
    },
    "tire_panel": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "show_title": True,
        "title": "TIRES",
        "show_wear": True,
        "show_temp": True,
        "show_pressure": False,
        "warn_wear_pct": 30.0,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "header": "#ffd23a",
            "wear": "#46df7a",
            "warn": "#e23b3b",
            "bar_bg": "#0b0e12",
        },
    },
    "pit_board": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "show_title": True,
        "title": "PIT SERVICES",
        "show_pit_banner": True,
        "pit_banner_text": "PIT STOP ACTIVE",
        "row_height_px": 0,
        "max_row_height_frac": 0.0,
        "show_pressures": False,
        "show_fast_repairs": True,
        "show_compound": True,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
            "title": "#f4f6f8",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "checked": "#46df7a",
            "active_bg": "#ffd23a",
            "active_text": "#141414",
        },
    },
    "weather_panel": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "show_title": True,
        "title": "WEATHER",
        "row_height_px": 0,
        "max_row_height_frac": 0.0,
        "show_skies": True,
        "show_rain": True,
        "show_temps": True,
        "show_wind": False,
        "show_trend": True,
        "trend_window_seconds": 300.0,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "header": "#9fd0ff",
            "wind": "#9fd0ff",
        },
    },
    "leaderboard_strip": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "rows": 0,
        "row_height_px": 0,
        "max_row_height_frac": 0.0,
        "show_position": True,
        "show_name": False,
        "show_car_number": True,
        "show_gap": False,
        "show_lap": False,
        "show_mph": False,
        "show_class_color": False,
        "highlight_player": True,
        "row_dividers": False,
        "corner_radius_frac": 0.0,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "pylon_bg": "#000000",
            "bg_top": "#000000f2",
            "bg_bottom": "#000000f2",
            "panel_border": "#ffffff18",
            "header": "#d8d8d8",
            "text": "#b8b8b8",
            "muted": "#707070",
            "pos": "#ffffff",
            "car_number": "#ff8c00",
            "player_row": "#ffffff14",
            "faster": "#46df7a",
            "slower": "#ff6a3a",
        },
    },
    "radio_tower": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "show_title": True,
        "title": "RADIO",
        "show_position": True,
        "show_car_number": True,
        "show_name": True,
        "highlight_player": True,
        "row_height_px": 0,
        "max_row_height_frac": 0.0,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
            "header": "#d8d8d8",
            "text": "#d8d8d8",
            "muted": "#707070",
            "pos": "#ffffff",
            "car_number": "#ff8c00",
            "player_row": "#ffffff14",
            "speaking_row": "#22c55e50",
            "badge_speaking_bg": "#22c55e",
            "badge_speaking_text": "#ffffff",
            "badge_speaking_border": "#ffffffcc",
        },
    },
    "ers_hybrid": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "show_title": True,
        "title": "HYBRID",
        "label_battery": "ERS",
        "label_lap": "LAP",
        "label_boost": "BOOST",
        "label_p2p": "P2P",
        "empty_text": "No hybrid data",
        "show_battery": True,
        "show_lap_energy": True,
        "show_boost": True,
        "show_p2p": True,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "gauge_fill": "#46df7a",
            "gauge_bg": "#0b0e12",
            "pill": "#ffd23a",
        },
    },
    "system_panel": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "show_title": True,
        "title": "PERFORMANCE",
        "show_icons": False,
        "row_height_px": 0,
        "max_row_height_frac": 0.0,
        "show_cpu": True,
        "show_mem": True,
        "show_gpu": True,
        "show_fps": True,
        "show_network": True,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "header": "#9fd0ff",
            "gauge_bg": "#0b0e12",
            "gauge_fill": "#46df7a",
        },
    },
    "pit_advisor": {
        **_WIDGET_CHROME,
        "show": False,
        "text_scale": 1.0,
        "show_title": True,
        "title": "PIT ENGINEER",
        "show_only_when_actionable": True,
        "pit_loss_seconds": 28.0,
        "legal_fuel_buffer_l": 2.0,
        "low_fuel_laps_threshold": 2.0,
        "undercut_gap_max_s": 12.0,
        "cover_gap_max_s": 8.0,
        "caution_fuel_multiplier": 0.85,
        "top_positions_stay_out": 5,
        "field_pit_follow_threshold": 0.45,
        "caution_pit_pra_threshold": 0.60,
        "caution_pit_lead_loss_max": 3,
        "recent_pit_laps_window": 3,
        "green_run_caution_bias_laps": 15,
        "post_pit_quiet_min_laps": 6,
        "lapped_danger_fuel_min_laps": 1.5,
        "reentry_window_pct": 0.035,
        "show_field_context": True,
        "show_tire_inventory": True,
        "tire_warn_wear_pct": 35.0,
        "tire_critical_wear_pct": 25.0,
        "low_tire_laps_threshold": 3.0,
        "min_stint_laps": 4,
        "tire_sets_reserve": 1,
        "race_tire_sets_total": 0,
        "ahead_scan_positions": 5,
        "ahead_pace_delta_s": 0.3,
        "fresh_tire_lap_delta": 3,
        "caution_overdue_ratio": 1.15,
        "field_chaos_high_threshold": 0.25,
        "caution_wait_min_fuel_laps": 3.0,
        "cover_closing_min_rate": 0.15,
        "green_pos_lost_max": 2,
        "caution_prb_stay_out_threshold": 0.50,
        "caution_prb_pit_threshold": 0.35,
        "final_laps_optional_suppress": 3,
        "track_wetness_tire_suppress": 0.1,
        "use_measured_pit_loss": True,
        "pit_loss_ema_alpha": 0.35,
        "pit_loss_measured_min_s": 15.0,
        "pit_loss_measured_max_s": 90.0,
        "pit_menu_hard_gate": True,
        "opponent_tire_inference_enabled": True,
        "ahead_profile_scan_positions": 15,
        "strategic_pit_min_net_positions": 3,
        "opponent_splash_pit_max_s": 0,
        "opponent_stint_due_laps": 25,
        "green_pos_tradeoff_override": True,
        "caution_bankrupt_ahead_min": 3,
        "colors": {
            **_WIDGET_CHROME_COLORS,
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff28",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "header": "#9fd0ff",
            "active_bg": "#46df7a",
            "active_text": "#141414",
            "cell_dark": "#3a4048",
        },
    },
}


def _deep_merge(base: dict, override: dict) -> dict:
    out = copy.deepcopy(base)
    for k, v in (override or {}).items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _deep_merge(out[k], v)
        else:
            out[k] = v
    return out


# --- Context profiles ------------------------------------------------------
# The overlay can behave differently when you're in the garage vs out on track.
# The base config is the "On track" profile; a sparse set of overrides stored
# under the reserved "garage" key in overlay_config.json is layered on top when
# the garage context is active. Only values that differ from the base are kept.
GARAGE_KEY = "garage"
CONTEXTS = ("race", "garage")
CONTEXT_LABELS = {"race": "On track", "garage": "In garage"}

# Presets are named config sets. Each one bundles its own on-track + in-garage
# config, on-track widget layout + optional sparse garage layout overrides, and
# an optional list of cars that auto-activate it. One preset is active at a
# time; the race/garage profiles still auto-switch within it. The settings file
# is "schema 2": {active_preset, presets:{...}}.
DEFAULT_PRESET = "Default"


def _migrate_table_split(user: dict) -> dict:
    """Fold a legacy shared "table" section into per-table settings.

    Older configs stored all table styling under one "table" key shared by both
    tables. Tables are now themed independently, so copy those overrides into
    both "relative" and "standings" (anything already set on a table wins) and
    drop the old key. Handled for the base config and the nested garage diff.
    """
    if not isinstance(user, dict):
        return user
    user = copy.deepcopy(user)

    def fold(d: dict) -> None:
        tbl = d.pop("table", None)
        if not isinstance(tbl, dict):
            return
        for sec in ("relative", "standings"):
            existing = d.get(sec) if isinstance(d.get(sec), dict) else {}
            d[sec] = _deep_merge(tbl, existing)

    fold(user)
    if isinstance(user.get(GARAGE_KEY), dict):
        fold(user[GARAGE_KEY])
    return user


_DASH_IRATING_ALIASES = {"irating_delta": "irating", "irating_stack": "irating"}
_DASH_SLOT_KEYS = (
    "top_right", "primary_left", "primary_right",
    "stat_left", "stat_right",
    "strip_left", "strip_center", "strip_right",
)


def _migrate_dash_irating(user: dict) -> dict:
    """Fold legacy dash iRating slot keys into a single "irating" metric."""
    if not isinstance(user, dict):
        return user
    user = copy.deepcopy(user)

    def fix(d: dict) -> None:
        dash = d.get("dash")
        if not isinstance(dash, dict):
            return
        for key in _DASH_SLOT_KEYS:
            slot = dash.get(key)
            if slot not in _DASH_IRATING_ALIASES:
                continue
            dash[key] = "irating"
            if slot == "irating_delta":
                dash["show_irating_projection"] = True

    fix(user)
    if isinstance(user.get(GARAGE_KEY), dict):
        fix(user[GARAGE_KEY])
    return user


def _to_v2(raw: dict) -> dict:
    """Upgrade a legacy single-config file to the multi-preset v2 layout.

    A v1 file is the whole config (base diff + optional "garage"); it becomes the
    "Default" preset, pulling in the old standalone overlay_layout.json as that
    preset's layout. A v2 file (already has "presets") is returned unchanged.
    """
    if not isinstance(raw, dict):
        return {}
    if isinstance(raw.get("presets"), dict):
        return raw
    return {
        "schema": 2,
        "active_preset": DEFAULT_PRESET,
        "auto_switch_by_car": True,
        "presets": {
            DEFAULT_PRESET: {
                "config": raw,
                "layout": layout_store.load_layout(),
                "cars": [],
            }
        },
    }


def _atomic_write(path: str, data: dict) -> None:
    """Write JSON via a temp file + rename so a crash can't corrupt the file."""
    tmp = f"{path}.tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2)
    os.replace(tmp, path)


def _read_user() -> dict:
    try:
        with open(CONFIG_FILE, "r", encoding="utf-8") as fh:
            return _to_v2(json.load(fh))
    except (OSError, ValueError):
        return {}


def _split_user(user: dict) -> tuple[dict, dict]:
    """Separate the base config from the sparse garage overrides."""
    user = copy.deepcopy(user or {})
    garage = user.pop(GARAGE_KEY, {}) or {}
    return user, garage


def _widget_sections() -> list[str]:
    """Top-level config sections that represent a toggleable widget panel."""
    return [k for k, v in DEFAULTS.items()
            if isinstance(v, dict) and "show" in v]


def ensure_user_config() -> None:
    """On first launch (no config file yet) write one with every widget hidden.

    The settings file lives in the per-user data folder (see paths.data_dir),
    separate from the code, so app updates never overwrite it. A clean install
    starts with all overlays off; you turn on the ones you want in Settings.
    """
    if os.path.exists(CONFIG_FILE):
        return
    off = {k: {"show": False} for k in _widget_sections()}
    data = {
        "schema": 2,
        "active_preset": DEFAULT_PRESET,
        "auto_switch_by_league": True,
        "auto_switch_by_car": True,
        "auto_switch_to_default": True,
        "cloud_tracks": True,
        "presets": {DEFAULT_PRESET: {"config": off, "layout": {}, "cars": [],
                                     "leagues": [], "default": True}},
    }
    try:
        os.makedirs(os.path.dirname(CONFIG_FILE), exist_ok=True)
        _atomic_write(CONFIG_FILE, data)
    except OSError:
        pass


def _int_list(values) -> list:
    """Coerce an iterable to a list of ints, dropping anything non-numeric."""
    out: list = []
    for v in (values or []):
        try:
            out.append(int(v))
        except (TypeError, ValueError):
            pass
    return out


def _blank_preset() -> dict:
    """A fresh preset: defaults, but with every widget toggled off so you opt in."""
    base = copy.deepcopy(DEFAULTS)
    for key in _widget_sections():
        base[key]["show"] = False
    return {"base": base, "garage": {}, "layout": {}, "layout_garage": {},
            "cars": [], "leagues": [], "default": False}


def _deserialize_preset(entry: dict) -> dict:
    """Build a live preset (base + garage + layouts + cars + leagues) from disk."""
    cfg = _migrate_dash_irating(_migrate_table_split(entry.get("config") or {}))
    base_user, garage = _split_user(cfg)
    return {
        "base": _deep_merge(DEFAULTS, base_user),
        "garage": garage,
        "layout": dict(entry.get("layout") or {}),
        "layout_garage": dict(entry.get("layout_garage") or {}),
        "cars": [str(c) for c in (entry.get("cars") or [])],
        "leagues": _int_list(entry.get("leagues")),
        "default": bool(entry.get("default", False)),
    }


def _ensure_one_default() -> None:
    """Maintain the invariant that exactly one preset is marked as the default.

    If none (or several) are flagged on disk, keep the first flagged one, else
    promote the active preset (falling back to the first) so the default-fallback
    auto-switch always resolves to a real preset.
    """
    if not _PRESETS:
        return
    flagged = [n for n, p in _PRESETS.items() if p.get("default")]
    if len(flagged) == 1:
        return
    keep = flagged[0] if flagged else (
        ACTIVE_PRESET if ACTIVE_PRESET in _PRESETS else next(iter(_PRESETS)))
    for name, p in _PRESETS.items():
        p["default"] = (name == keep)


def _load_all() -> None:
    """(Re)load every preset + the active selection from disk into memory."""
    global _PRESETS, ACTIVE_PRESET, BASE, GARAGE
    global AUTO_SWITCH_BY_LEAGUE, AUTO_SWITCH_BY_CAR, AUTO_SWITCH_TO_DEFAULT
    global CLOUD_TRACKS
    raw = _read_user()
    presets: dict = {}
    for name, entry in (raw.get("presets") or {}).items():
        if isinstance(entry, dict):
            presets[str(name)] = _deserialize_preset(entry)
    if not presets:
        presets = {DEFAULT_PRESET: _blank_preset()}
    _PRESETS = presets
    AUTO_SWITCH_BY_LEAGUE = bool(raw.get("auto_switch_by_league", True))
    AUTO_SWITCH_BY_CAR = bool(raw.get("auto_switch_by_car", True))
    AUTO_SWITCH_TO_DEFAULT = bool(raw.get("auto_switch_to_default", True))
    CLOUD_TRACKS = True  # always on; legacy cloud_tracks:false in profiles is ignored
    active = raw.get("active_preset")
    ACTIVE_PRESET = active if active in _PRESETS else next(iter(_PRESETS))
    _ensure_one_default()
    BASE = _PRESETS[ACTIVE_PRESET]["base"]
    GARAGE = _PRESETS[ACTIVE_PRESET]["garage"]


# Create a fresh, all-widgets-off config on first launch, then load every preset.
ensure_user_config()

# Named config sets and which one is active. BASE/GARAGE always alias the active
# preset's on-track config + sparse garage override (kept for the rest of the app).
_PRESETS: dict = {}
ACTIVE_PRESET: str = DEFAULT_PRESET
AUTO_SWITCH_BY_LEAGUE: bool = True
AUTO_SWITCH_BY_CAR: bool = True
AUTO_SWITCH_TO_DEFAULT: bool = True
CLOUD_TRACKS: bool = True
BASE: dict = {}
GARAGE: dict = {}
_load_all()

# The active context (set from telemetry) and an optional editor preview pin
# that overrides it so the settings UI can show the profile being edited.
ACTIVE_CONTEXT: str = "race"
_preview_context: str | None = None


def _ctx() -> str:
    return _preview_context or ACTIVE_CONTEXT


def _compute_cfg() -> dict:
    global CFG
    if _ctx() == "garage" and GARAGE:
        CFG = _deep_merge(BASE, GARAGE)
    else:
        CFG = copy.deepcopy(BASE)
    return CFG


# The live, merged configuration for the active context. Reloadable via reload().
CFG: dict = _compute_cfg()

# Callbacks invoked whenever the live config changes (e.g. from the editor UI),
# so the running overlay can repaint immediately.
_listeners: list = []


def on_change(callback) -> None:
    """Register a callback(cfg) fired whenever the live config is replaced."""
    _listeners.append(callback)


# One-shot "rescan the track now" action, wired from the settings UI to the
# running overlay. Kept separate from the persistent config so it never gets
# written to overlay_config.json.

# The widget section currently painting. Shared font helpers use it to apply the
# right per-widget text_scale without every call site passing a section. Safe
# because Qt painting runs on a single (GUI) thread.
_active_section: str | None = None


def use_section(name: str | None) -> None:
    """Mark which widget section is painting (set at the top of paintEvent)."""
    global _active_section
    _active_section = name


def active_section() -> str | None:
    """The widget section currently painting (set via use_section)."""
    return _active_section


def text_scale_for(section: str | None = None) -> float:
    """Global text_scale times the given (or active) widget's own text_scale."""
    g = float(CFG.get("text_scale", 1.0) or 1.0)
    if section is None:
        section = _active_section
    if section:
        widget = CFG.get(section)
        if isinstance(widget, dict):
            return g * float(widget.get("text_scale", 1.0) or 1.0)
    return g


def _notify() -> None:
    try:
        from .widgets import fonts
        fonts.clear_font_cache()
    except Exception:
        pass
    for cb in list(_listeners):
        try:
            cb(CFG)
        except Exception:
            pass


# Callbacks fired when the *active preset* changes (so the overlay can reapply
# that preset's saved window layout). Separate from _listeners (config content).
_preset_listeners: list = []


def on_preset_change(callback) -> None:
    """Register a callback(name) fired when the active preset changes."""
    _preset_listeners.append(callback)


def _notify_preset() -> None:
    for cb in list(_preset_listeners):
        try:
            cb(ACTIVE_PRESET)
        except Exception:
            pass


# Callbacks fired when the effective race/garage context changes (so the
# overlay can swap to that context's widget layout).
_context_listeners: list = []


def on_context_change(callback) -> None:
    """Register a callback(ctx) fired when effective_context() changes."""
    _context_listeners.append(callback)


def _notify_context() -> None:
    ctx = _ctx()
    for cb in list(_context_listeners):
        try:
            cb(ctx)
        except Exception:
            pass


def set_cfg(new_cfg: dict, notify: bool = True) -> dict:
    """Replace the base (on-track) config and recompute the live config.

    Kept for backwards compatibility; treats the supplied dict as the base
    profile. Use apply_edits()/save_profiles() for context-aware editing.
    """
    return apply_base(new_cfg, notify)


def reload() -> dict:
    """Reload every preset from disk and recompute the live config."""
    _load_all()
    _compute_cfg()
    _clear_column_caches()
    _notify()
    _notify_preset()
    return CFG


# --- context + profile API -------------------------------------------------

def contexts() -> tuple:
    return CONTEXTS


def active_context() -> str:
    return ACTIVE_CONTEXT


def effective_context() -> str:
    """The context actually driving the live config (preview pin wins)."""
    return _ctx()


def set_context(ctx: str, notify: bool = True) -> dict:
    """Switch the active context (called from telemetry: garage vs on track)."""
    global ACTIVE_CONTEXT
    if ctx not in CONTEXTS:
        return CFG
    prev = _ctx()
    ACTIVE_CONTEXT = ctx
    # A live editor preview pin overrides the auto-detected context.
    if _preview_context is None:
        _compute_cfg()
        if notify:
            _notify()
        if _ctx() != prev:
            _notify_context()
    return CFG


def set_preview_context(ctx: str | None) -> dict:
    """Pin the live config to a context for the editor's preview (None clears)."""
    global _preview_context
    prev = _ctx()
    _preview_context = ctx if ctx in CONTEXTS else None
    _compute_cfg()
    _notify()
    if _ctx() != prev:
        _notify_context()
    return CFG


def base_cfg() -> dict:
    return copy.deepcopy(BASE)


def garage_overrides() -> dict:
    return copy.deepcopy(GARAGE)


def editor_full(ctx: str) -> dict:
    """The full config dict the editor should edit for a given context."""
    if ctx == "garage":
        return _deep_merge(BASE, GARAGE)
    return copy.deepcopy(BASE)


def apply_base(full_cfg: dict, notify: bool = True) -> dict:
    """Replace the active preset's base (on-track) profile with a full config."""
    global BASE
    BASE = copy.deepcopy(full_cfg)
    _PRESETS[ACTIVE_PRESET]["base"] = BASE
    _compute_cfg()
    _clear_column_caches()
    if notify:
        _notify()
    return CFG


def apply_garage(full_cfg: dict, notify: bool = True) -> dict:
    """Set the active preset's garage profile, keeping only the diff vs base."""
    global GARAGE
    GARAGE = diff_from_defaults(full_cfg, BASE)
    _PRESETS[ACTIVE_PRESET]["garage"] = GARAGE
    _compute_cfg()
    _clear_column_caches()
    if notify:
        _notify()
    return CFG


def apply_edits(ctx: str, full_cfg: dict, notify: bool = True) -> dict:
    if ctx == "garage":
        return apply_garage(full_cfg, notify)
    return apply_base(full_cfg, notify)


def clear_garage(notify: bool = True) -> dict:
    """Drop the active preset's garage overrides (it mirrors on-track again)."""
    global GARAGE
    GARAGE = {}
    _PRESETS[ACTIVE_PRESET]["garage"] = GARAGE
    _compute_cfg()
    if notify:
        _notify()
    return CFG


def _sync_active() -> None:
    """Push the live BASE/GARAGE back into the active preset record."""
    _PRESETS[ACTIVE_PRESET]["base"] = BASE
    _PRESETS[ACTIVE_PRESET]["garage"] = GARAGE


def _serialize() -> dict:
    """The full schema-2 document: every preset's sparse config + layout + cars."""
    _sync_active()
    presets: dict = {}
    for name, p in _PRESETS.items():
        cfg = diff_from_defaults(p["base"])
        garage_sparse = diff_from_defaults(_deep_merge(p["base"], p["garage"]),
                                           p["base"])
        if garage_sparse:
            cfg[GARAGE_KEY] = garage_sparse
        entry: dict = {"config": cfg}
        if p.get("layout"):
            entry["layout"] = p["layout"]
        if p.get("layout_garage"):
            entry["layout_garage"] = p["layout_garage"]
        if p.get("cars"):
            entry["cars"] = p["cars"]
        if p.get("leagues"):
            entry["leagues"] = p["leagues"]
        if p.get("default"):
            entry["default"] = True
        presets[name] = entry
    return {
        "schema": 2,
        "active_preset": ACTIVE_PRESET,
        "auto_switch_by_league": AUTO_SWITCH_BY_LEAGUE,
        "auto_switch_by_car": AUTO_SWITCH_BY_CAR,
        "auto_switch_to_default": AUTO_SWITCH_TO_DEFAULT,
        "presets": presets,
    }


def save_profiles(path: str = CONFIG_FILE) -> dict:
    """Persist every preset (active selection, configs, layouts, car bindings)."""
    data = _serialize()
    _atomic_write(path, data)
    return data


# --- preset management -----------------------------------------------------

def presets() -> list:
    """Names of all presets, in order."""
    return list(_PRESETS.keys())


def active_preset() -> str:
    return ACTIVE_PRESET


def set_active_preset(name: str, notify: bool = True, persist: bool = True) -> dict:
    """Switch the active preset, swapping in its on-track/garage config."""
    global ACTIVE_PRESET, BASE, GARAGE
    if name not in _PRESETS or name == ACTIVE_PRESET:
        return CFG
    _sync_active()
    ACTIVE_PRESET = name
    BASE = _PRESETS[name]["base"]
    GARAGE = _PRESETS[name]["garage"]
    _compute_cfg()
    if persist:
        save_profiles()
    if notify:
        _notify()
        _notify_preset()
    return CFG


def create_preset(name: str, copy_from: str | None = None,
                  activate: bool = True) -> bool:
    """Create a new preset (blank, or copied from an existing one)."""
    name = (name or "").strip()
    if not name or name in _PRESETS:
        return False
    if copy_from and copy_from in _PRESETS:
        if copy_from == ACTIVE_PRESET:
            _sync_active()
        src = _PRESETS[copy_from]
        _PRESETS[name] = {
            "base": copy.deepcopy(src["base"]),
            "garage": copy.deepcopy(src["garage"]),
            "layout": copy.deepcopy(src["layout"]),
            "layout_garage": copy.deepcopy(src.get("layout_garage") or {}),
            # Car/league bindings + default flag are unique per preset; don't copy.
            "cars": [],
            "leagues": [],
            "default": False,
        }
    else:
        _PRESETS[name] = _blank_preset()
    if activate:
        set_active_preset(name)
    else:
        save_profiles()
    return True


def duplicate_preset(src: str, name: str) -> bool:
    return create_preset(name, copy_from=src)


_PRESET_EXPORT_KIND = "gridglance.preset"
_PRESET_EXPORT_VERSION = 1


def export_preset(name: str | None = None) -> dict | None:
    """Serialize one preset for sharing. Returns None if the name is unknown."""
    _sync_active()
    name = name or ACTIVE_PRESET
    if name not in _PRESETS:
        return None
    p = _PRESETS[name]
    cfg = diff_from_defaults(p["base"])
    garage_sparse = diff_from_defaults(_deep_merge(p["base"], p["garage"]),
                                       p["base"])
    if garage_sparse:
        cfg[GARAGE_KEY] = garage_sparse
    entry: dict = {"config": cfg}
    if p.get("layout"):
        entry["layout"] = p["layout"]
    if p.get("layout_garage"):
        entry["layout_garage"] = p["layout_garage"]
    if p.get("cars"):
        entry["cars"] = list(p["cars"])
    if p.get("leagues"):
        entry["leagues"] = list(p["leagues"])
    # Never export default=True — importing must not steal the user's default.
    return {
        "kind": _PRESET_EXPORT_KIND,
        "version": _PRESET_EXPORT_VERSION,
        "name": name,
        "preset": entry,
    }


def import_preset(payload: dict, name: str | None = None,
                  *, overwrite: bool = False, activate: bool = True) -> str:
    """Import a shared preset payload. Returns the preset name used.

    Raises ValueError on malformed payload or name collision when overwrite
    is False.
    """
    global BASE, GARAGE
    if not isinstance(payload, dict):
        raise ValueError("Preset file must be a JSON object")
    kind = payload.get("kind")
    if kind not in (None, _PRESET_EXPORT_KIND):
        raise ValueError(f"Unrecognized preset file kind: {kind!r}")
    entry = payload.get("preset")
    if entry is None and isinstance(payload.get("config"), dict):
        # Allow a bare preset entry (schema-2 style) without the wrapper.
        entry = {k: payload[k] for k in (
            "config", "layout", "layout_garage", "cars", "leagues")
            if k in payload}
    if not isinstance(entry, dict) or not isinstance(entry.get("config"), dict):
        # Also accept a full overlay_config with a single named preset.
        presets = payload.get("presets")
        if isinstance(presets, dict) and len(presets) == 1:
            only_name, only_entry = next(iter(presets.items()))
            if name is None:
                name = str(only_name)
            entry = only_entry if isinstance(only_entry, dict) else None
        elif isinstance(presets, dict) and payload.get("name") in presets:
            entry = presets[payload["name"]]
    if not isinstance(entry, dict):
        raise ValueError("Preset file is missing a preset payload")
    if not isinstance(entry.get("config"), dict):
        # Treat whole entry as config if it looks like a settings tree.
        if any(k in entry for k in ("dash", "map", "relative", "standings")):
            entry = {"config": entry}
        else:
            raise ValueError("Preset file is missing a config section")

    dest = (name or payload.get("name") or "Imported").strip()
    if not dest:
        dest = "Imported"
    if dest in _PRESETS and not overwrite:
        raise ValueError(f"A preset named “{dest}” already exists")

    live = _deserialize_preset(entry)
    live["default"] = False
    if dest in _PRESETS and overwrite:
        live["default"] = bool(_PRESETS[dest].get("default"))
        _PRESETS[dest] = live
        if dest == ACTIVE_PRESET:
            BASE = live["base"]
            GARAGE = live["garage"]
            _compute_cfg()
            save_profiles()
            _notify()
            _notify_preset()
        elif activate:
            set_active_preset(dest)
        else:
            save_profiles()
        return dest

    _PRESETS[dest] = live
    if activate:
        set_active_preset(dest)
    else:
        save_profiles()
    return dest


def rename_preset(old: str, new: str) -> bool:
    global ACTIVE_PRESET
    new = (new or "").strip()
    if old not in _PRESETS or not new or new == old or new in _PRESETS:
        return False
    renamed = {(new if k == old else k): v for k, v in _PRESETS.items()}
    _PRESETS.clear()
    _PRESETS.update(renamed)
    if ACTIVE_PRESET == old:
        ACTIVE_PRESET = new
    save_profiles()
    return True


def delete_preset(name: str) -> bool:
    """Delete a preset; refuses to remove the last one. Falls back to another."""
    global ACTIVE_PRESET, BASE, GARAGE
    if name not in _PRESETS or len(_PRESETS) <= 1:
        return False
    was_active = name == ACTIVE_PRESET
    del _PRESETS[name]
    if was_active:
        ACTIVE_PRESET = next(iter(_PRESETS))
        BASE = _PRESETS[ACTIVE_PRESET]["base"]
        GARAGE = _PRESETS[ACTIVE_PRESET]["garage"]
        _compute_cfg()
    # Deleting the default preset must promote another so one always remains.
    _ensure_one_default()
    save_profiles()
    if was_active:
        _notify()
        _notify_preset()
    return True


def preset_cars(name: str | None = None) -> list:
    p = _PRESETS.get(name or ACTIVE_PRESET)
    return list(p["cars"]) if p else []


def set_preset_cars(name: str, cars) -> None:
    if name in _PRESETS:
        _PRESETS[name]["cars"] = [str(c) for c in cars]
        save_profiles()


def preset_for_car(car_path: str) -> str | None:
    """The first preset bound to this car path (for auto-switching), or None."""
    if not car_path:
        return None
    for name, p in _PRESETS.items():
        if car_path in (p.get("cars") or []):
            return name
    return None


def preset_leagues(name: str | None = None) -> list:
    p = _PRESETS.get(name or ACTIVE_PRESET)
    return list(p["leagues"]) if p else []


def set_preset_leagues(name: str, leagues) -> None:
    if name in _PRESETS:
        _PRESETS[name]["leagues"] = _int_list(leagues)
        save_profiles()


def preset_for_league(league_id) -> str | None:
    """The first preset bound to this iRacing LeagueID, or None."""
    try:
        league_id = int(league_id)
    except (TypeError, ValueError):
        return None
    if league_id <= 0:
        return None
    for name, p in _PRESETS.items():
        if league_id in (p.get("leagues") or []):
            return name
    return None


def default_preset() -> str | None:
    """The name of the preset flagged as the default (always one once loaded)."""
    for name, p in _PRESETS.items():
        if p.get("default"):
            return name
    return None


def set_default_preset(name: str) -> None:
    """Mark one preset as the default, clearing the flag on all others (radio)."""
    if name not in _PRESETS:
        return
    for n, p in _PRESETS.items():
        p["default"] = (n == name)
    save_profiles()


def preset_for_session(league_id, car_path: str) -> str | None:
    """Resolve which preset a session should use, honoring each auto-switch toggle.

    Priority: a bound league wins over a bound car, which wins over the default.
    """
    if AUTO_SWITCH_BY_LEAGUE:
        name = preset_for_league(league_id)
        if name:
            return name
    if AUTO_SWITCH_BY_CAR:
        name = preset_for_car(car_path)
        if name:
            return name
    if AUTO_SWITCH_TO_DEFAULT:
        return default_preset()
    return None


def auto_switch_enabled() -> bool:
    """True if any auto-switch rule is active (so the overlay should evaluate)."""
    return AUTO_SWITCH_BY_LEAGUE or AUTO_SWITCH_BY_CAR or AUTO_SWITCH_TO_DEFAULT


def auto_switch_by_league() -> bool:
    return AUTO_SWITCH_BY_LEAGUE


def set_auto_switch_by_league(value: bool) -> None:
    global AUTO_SWITCH_BY_LEAGUE
    AUTO_SWITCH_BY_LEAGUE = bool(value)
    save_profiles()


def auto_switch_by_car() -> bool:
    return AUTO_SWITCH_BY_CAR


def set_auto_switch_by_car(value: bool) -> None:
    global AUTO_SWITCH_BY_CAR
    AUTO_SWITCH_BY_CAR = bool(value)
    save_profiles()


def auto_switch_to_default() -> bool:
    return AUTO_SWITCH_TO_DEFAULT


def set_auto_switch_to_default(value: bool) -> None:
    global AUTO_SWITCH_TO_DEFAULT
    AUTO_SWITCH_TO_DEFAULT = bool(value)
    save_profiles()


def cloud_tracks() -> bool:
    """Shared track maps from MongoDB are always enabled."""
    return True


def set_cloud_tracks(value: bool) -> None:
    """No-op — cloud track sync cannot be disabled."""
    del value


def _geom_equal(a, b) -> bool:
    """True if two [x,y,w,h] layouts match (ints compared as ints)."""
    if a is None or b is None:
        return a is b
    try:
        return [int(v) for v in a] == [int(v) for v in b]
    except (TypeError, ValueError):
        return list(a) == list(b)


def active_layout(ctx: str | None = None) -> dict:
    """Effective window layout for a context (key -> [x,y,w,h]).

    Race uses the preset's ``layout``. Garage starts from that map and overlays
    any sparse ``layout_garage`` entries — widgets never moved in garage keep
    their on-track position.
    """
    preset = _PRESETS[ACTIVE_PRESET]
    race = copy.deepcopy(preset.get("layout") or {})
    use = ctx if ctx in CONTEXTS else _ctx()
    if use != "garage":
        return race
    out = race
    for key, geom in (preset.get("layout_garage") or {}).items():
        if geom is not None:
            out[key] = list(geom)
    return out


def save_active_layout(layout: dict, ctx: str | None = None) -> None:
    """Store + persist the active preset's layout for the given context.

    Race writes the full ``layout`` map. Garage writes a sparse ``layout_garage``
    of widgets whose geometry differs from the race layout.
    """
    use = ctx if ctx in CONTEXTS else _ctx()
    preset = _PRESETS[ACTIVE_PRESET]
    if use == "garage":
        race = preset.get("layout") or {}
        sparse: dict = {}
        for key, geom in (layout or {}).items():
            if geom is None:
                continue
            if not _geom_equal(geom, race.get(key)):
                sparse[key] = list(geom)
        preset["layout_garage"] = sparse
    else:
        preset["layout"] = copy.deepcopy(layout)
    save_profiles()


_MISSING = object()


def diff_from_defaults(cfg: dict, base: dict | None = None) -> dict:
    """Return only the keys in cfg that differ from the defaults (minimal)."""
    base = DEFAULTS if base is None else base
    out: dict = {}
    for k, v in cfg.items():
        b = base.get(k, _MISSING)
        if isinstance(v, dict) and isinstance(b, dict):
            sub = diff_from_defaults(v, b)
            if sub:
                out[k] = sub
        elif b is _MISSING or v != b:
            out[k] = v
    return out


def save(cfg: dict, path: str = CONFIG_FILE, minimal: bool = True) -> dict:
    """Persist a full config dict, writing only the diff from defaults by default."""
    data = diff_from_defaults(cfg) if minimal else cfg
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2)
    return data


def full_defaults() -> dict:
    return copy.deepcopy(DEFAULTS)


def write_template(path: str = CONFIG_FILE) -> str:
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(DEFAULTS, fh, indent=2)
    return path


_col_order_cache: dict[str, list] = {}
_col_order_sig: dict[str, object] = {}


def _clear_column_caches() -> None:
    _col_order_cache.clear()
    _col_order_sig.clear()


def laptime_log_column_order() -> list:
    """Visible laptime-log columns in display order."""
    order = CFG.get("laptime_log", {}).get("column_order")
    if not order:
        return ["lap", "time", "delta", "temp"]
    result = []
    for k in order:
        if k in LAPTIME_LOG_COLUMNS and k not in result:
            result.append(k)
    return result or ["lap", "time", "delta", "temp"]


def laptime_log_has_column(key: str) -> bool:
    return key in laptime_log_column_order()


def table_column_order(section: str) -> list:
    """Normalized list of visible columns (in order) for a table section.

    Unknown keys are dropped and duplicates removed. If a section has no
    configured order at all, every known column is shown.
    """
    sig = tuple(CFG.get(section, {}).get("column_order") or ())
    if _col_order_sig.get(section) == sig and section in _col_order_cache:
        return _col_order_cache[section]
    order = CFG.get(section, {}).get("column_order")
    if not order:
        result = list(TABLE_COLUMNS)
    else:
        result = []
        for k in order:
            if k in TABLE_COLUMNS and k not in result:
                result.append(k)
        if not result:
            result = list(TABLE_COLUMNS)
    _col_order_cache[section] = result
    _col_order_sig[section] = sig
    return result


def has_column(section: str, key: str) -> bool:
    """True if the given column is currently visible in a table section."""
    return key in table_column_order(section)


def any_table_column(*keys: str, sections=("relative", "standings")) -> bool:
    """True if any listed column is visible in relative or standings."""
    return any(has_column(s, k) for s in sections for k in keys)


def dash_active_slots() -> set:
    """Dash content slot metric keys currently in use."""
    dc = CFG.get("dash", {})
    return {dc.get(k) for k in (
        "top_right", "primary_left", "primary_right",
        "stat_left", "stat_right",
        "strip_left", "strip_center", "strip_right")}


def dash_metric_in_use(key: str) -> bool:
    """True if a dash content slot is set to the given metric key."""
    return key in dash_active_slots()


def dash_uses_any(*keys: str) -> bool:
    return any(dash_metric_in_use(k) for k in keys)


def table_slot_items(section: str) -> set:
    """The header/footer items actually displayed for a table section.

    Footer items are excluded when that table's footer is hidden, so values for
    a hidden footer are never computed. Used to drive lazy calculation.
    """
    cfg = CFG.get(section, {})
    groups = ["header"]
    if cfg.get("show_footer", True):
        groups.append("footer")
    items = set()
    for grp in groups:
        for v in cfg.get(grp, {}).values():
            if v and v != "none":
                items.add(v)
    return items


def slot_in_use(key: str, sections=("relative", "standings")) -> bool:
    """True if any table currently displays the given header/footer item."""
    return any(key in table_slot_items(s) for s in sections)


def units() -> str:
    """Active unit system: 'metric' or 'imperial'."""
    u = str(CFG.get("units", "metric")).strip().lower()
    return "imperial" if u.startswith("imp") else "metric"


def is_imperial() -> bool:
    return units() == "imperial"


def conv_speed(ms):
    """m/s -> km/h (metric) or mph (imperial)."""
    if ms is None:
        return None
    return ms * (2.2369362921 if is_imperial() else 3.6)


def speed_unit() -> str:
    return "MPH" if is_imperial() else "KPH"


def conv_temp(c):
    """Celsius -> Celsius (metric) or Fahrenheit (imperial)."""
    if c is None:
        return None
    return c * 9.0 / 5.0 + 32.0 if is_imperial() else c


def temp_unit() -> str:
    return "\u00b0F" if is_imperial() else "\u00b0C"


def conv_fuel(litres):
    """Litres -> litres (metric) or US gallons (imperial)."""
    if litres is None:
        return None
    return litres * 0.2641720524 if is_imperial() else litres


def fuel_unit() -> str:
    return "Gal" if is_imperial() else "L"


# Parsing a color string is surprisingly hot (every pen/brush, every frame), so
# cache the parsed QColor keyed by the original spec. Keys are the literal config
# values, so a changed value is simply a new key -- no invalidation needed. The
# returned QColor must not be mutated in place (callers only read/copy it).
_COLOR_CACHE: dict = {}


def _parse_color(value) -> QColor:
    if isinstance(value, (list, tuple)):
        return QColor(*[int(x) for x in value])
    s = str(value).strip()
    if s.startswith("rgba(") or s.startswith("rgb("):
        nums = s[s.index("(") + 1: s.index(")")].split(",")
        parts = [float(n) for n in nums]
        r, g, b = (int(parts[0]), int(parts[1]), int(parts[2]))
        a = int(parts[3] * 255) if len(parts) > 3 and parts[3] <= 1 else (
            int(parts[3]) if len(parts) > 3 else 255)
        return QColor(r, g, b, a)
    if s.startswith("#"):
        hexs = s[1:]
        if len(hexs) == 3:
            r, g, b = (int(c * 2, 16) for c in hexs)
            return QColor(r, g, b)
        if len(hexs) == 6:
            return QColor(int(hexs[0:2], 16), int(hexs[2:4], 16), int(hexs[4:6], 16))
        if len(hexs) == 8:
            return QColor(int(hexs[0:2], 16), int(hexs[2:4], 16),
                          int(hexs[4:6], 16), int(hexs[6:8], 16))
    c = QColor(s)
    return c if c.isValid() else QColor(255, 0, 255)


def qcolor(value) -> QColor:
    """Parse a color spec into a QColor (cached). Do not mutate the result."""
    if isinstance(value, QColor):
        return value
    key = tuple(value) if isinstance(value, (list, tuple)) else value
    cached = _COLOR_CACHE.get(key)
    if cached is not None:
        return cached
    result = _parse_color(value)
    if len(_COLOR_CACHE) > 1024:
        _COLOR_CACHE.clear()
    _COLOR_CACHE[key] = result
    return result
