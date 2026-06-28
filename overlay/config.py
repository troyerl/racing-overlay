"""
Central configuration for every overlay widget.

Everything visual or behavioral (colors, fonts, sizes, toggles, row counts,
ranges, animation speeds) is defined here as defaults and can be overridden by an
`overlay_config.json` file placed next to the scripts. Only the keys you want to
change need to appear in that file -- it is deep-merged over the defaults.

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

from . import paths

CONFIG_FILE = paths.data_file("overlay_config.json")

# Every column a timing table knows how to draw. Which ones appear, and in what
# order, is controlled per table by its "column_order" list (add/remove/reorder
# from the settings editor). "stripe" is not a column -- it's a sub-toggle of the
# position cell -- so it lives in the table's "columns" dict instead.
TABLE_COLUMNS = ["badge", "position", "car_number", "name", "license",
                 "irating", "pit", "gap", "last_lap", "best_lap"]

DEFAULTS: dict = {
    "font_family": "Segoe UI",
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
    "table": {
        "corner_radius_frac": 0.05,
        "alt_row_shading": True,
        # Cap a row's height to this fraction of the panel height so that, when
        # only a few cars are present, rows don't stretch and the text doesn't
        # look zoomed in (extra space is left empty below). 0 disables the cap.
        "max_row_height_frac": 0.14,
        "font_scale": 0.40,        # row text size (multiple of row height)
        "gap_font_scale": 1.12,
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
            "irating": 1.70,
            "license": 2.00,
            "pit": 2.10,
            "last_lap": 2.90,
            "best_lap": 2.90,
            "gutter": 0.18,
        },
        "colors": {
            # Vertical gradient card matching the dash (top lighter -> bottom dark).
            "bg": "#1b1f26f2",
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "border": "#ffffff20",
            "cell_dark": "#0b0e12",
            "row_alt": "#ffffff0a",
            "player_row": "#8a5a18b4",
            # Lapped-traffic row tints: "threat" (red) = a car a lap ahead that
            # will lap you; "lapped" (blue) = a car a lap down that you're lapping.
            "threat": "#7a1a1adc",
            "lapped": "#13386edc",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "irating_bg": "#eef0f2",
            "irating_text": "#14161a",
            "badge_player": "#ff9416",
            "badge_pit_bg": "#ebeef0",
            "badge_pit_text": "#141414",
            "badge_lap": "#7638c4",
            "badge_empty_border": "#ffffff28",
            "badge_empty_fill": "#00000078",
        },
        "license_colors": {
            "R": "#d34a3c",
            "D": "#e0791a",
            "C": "#d6b400",
            "B": "#3a9b3a",
            "A": "#2f6bd8",
            "P": "#1a1a1a",
        },
    },
    "relative": {
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
        #   session_best, local_time, sim_time, cpu, mem.
        "header": {"left": "sof", "center": "none", "right": "position"},
        "footer": {"left": "race_time", "center": "lap", "right": "incidents"},
        # Per-section: show a Font Awesome icon instead of the text label.
        "header_icons": {"left": False, "center": False, "right": False},
        "footer_icons": {"left": False, "center": False, "right": False},
    },
    "standings": {
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
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # How many of your most recent laps to list (newest at the top).
        "rows": 8,
        "corner_radius_frac": 0.05,
        "alt_row_shading": True,
        "show_header": True,
        # Row text size (multiple of row height) and the header size on top of it.
        "font_scale": 0.42,
        "header_font_scale": 1.0,
        # Show a thermometer icon before the track temperature column.
        "temp_icon": True,
        # What DELTA compares each lap against: "previous" (the lap before it) or
        # "best" (your best lap so far this session).
        "delta_mode": "previous",
        "colors": {
            # Vertical gradient card matching the dash/tables.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "border": "#ffffff20",
            "row_alt": "#ffffff0a",
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
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        "corner_radius_frac": 0.04,
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
        "colors": {
            # Vertical gradient card matching the dash/tables.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "border": "#ffffff20",
            "accent": "#e23b3b",      # thin top bar
            "title": "#f4f6f8",
            "header": "#8b93a1",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
            "row_alt": "#ffffff12",
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
        "show_nose": True,
        "show_axis": True,
        # Draw a rounded card behind the radar (matches the dash panels).
        "show_panel": False,
        "corner_radius_frac": 0.12,
        "sizes": {
            "car_w": 0.13,
            "car_h": 0.20,
            "bar_h": 0.78,
            "glow_w": 0.17,
            "nose_len": 0.16,
        },
        "colors": {
            "car": "#f4f6f8",
            "red": "#e23b3b",
            "yellow": "#ffd23a",
            "axis": "#46df7a3a",
            "nose": "#46df7ae6",
            # Card background gradient + border, matching the dash style.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff20",
        },
    },
    "dash": {
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
        # right, slower = red to the left). delta_bar_range is the seconds at
        # full deflection.
        "show_delta_bar": False,
        "delta_bar_range": 1.0,
        # Every content slot below picks any metric (or "none" to hide it):
        # speed, rpm, gear, position, lap_count, laps_left, lap, fuel,
        # fuel_stack, fuel_laps, tires, incidents, last_lap, best_lap,
        # cur_lap, delta, air_temp, track_temp.
        "top_right": "incidents",       # readout next to the shift bar
        "primary_left": "lap_count",    # small readout, lower-left
        "primary_right": "speed",       # big readout, lower-left
        "stat_left": "tires",           # stacked cell, lower-right
        "stat_right": "fuel_stack",     # stacked cell, lower-right
        "strip_left": "air_temp",       # bottom strip
        "strip_center": "track_temp",
        "strip_right": "last_lap",
        "colors": {
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff20",
            "label": "#8b93a1",
            "value": "#f4f6f8",
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
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff20",
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
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Reference lap the delta is measured against: "session_best" (overall),
        # "best_lap" (your own best) or "optimal" (iRacing's best-sectors lap).
        "mode": "session_best",
        # Seconds of delta at full bar deflection (smaller = more sensitive).
        "range": 1.0,
        # Show the big signed number above the bar.
        "show_value": True,
        "colors": {
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff20",
            "faster": "#46df7a",
            "slower": "#e23b3b",
            "track": "#262b34",
            "center": "#8b93a1",
            "text": "#f4f6f8",
            "muted": "#8b93a1",
        },
    },
    "flags": {
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Text shown when no flag is flying.
        "idle_text": "TRACK CLEAR",
        "colors": {
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff20",
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
    "sector_timing": {
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size, multiplied by the global text_scale.
        "text_scale": 1.0,
        # Fallback sector count when the session provides no sector layout.
        "sectors": 3,
        "colors": {
            "bg_top": "#1b1f26",
            "bg_bottom": "#0f1216",
            "panel_border": "#ffffff20",
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
        },
    },
    "map": {
        # Show or hide this whole widget (its window + all of its per-tick work).
        "show": True,
        # Per-widget text size (corner labels, car numbers), x global text_scale.
        "text_scale": 1.0,
        "asphalt_width": 11,
        "outline_width": 2,
        "dot_radius_frac": 0.05,
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
        # What each car dot shows: "number" (car number) or "position".
        "car_label": "number",
        # Opacity (0..1) of a car's dot while it's on pit road.
        "pit_dot_opacity": 0.45,
        # Show a small wind compass (arrow + speed) in the map's corner.
        "show_wind": True,
        # Draw a rounded card behind the whole map. Off by default so only the
        # infield (the area enclosed by the track loop) is shaded.
        "show_panel": False,
        "corner_radius_frac": 0.08,
        "colors": {
            "asphalt": "#333a42",
            "outline": "#8b93a1",
            "infield": "#0f1216c8",
            "player": "#46df7a",
            "corner_bg": "#0b0d11c8",
            "corner_text": "#d6dce2",
            # Pit-lane highlight (thin red slashed line), its label text, and the
            # over-limit warning color.
            "pit": "#ff4d4d",
            "pit_text": "#ffffff",
            "pit_over": "#ffd23a",
            # Fill for a car's dot while it's on pit road (grayed out).
            "pit_car": "#6e747d",
            # Wind compass arrow + label.
            "wind": "#9fd0ff",
            "wind_text": "#eaf3ff",
            # Card background gradient + border, matching the dash style.
            "bg_top": "#1b1f26f2",
            "bg_bottom": "#0f1216f2",
            "panel_border": "#ffffff20",
        },
        "palette": [
            "#3aa0ff", "#ff5bac", "#46d27a", "#b06bff", "#ffa23a",
            "#ff5b5b", "#36d6d6", "#d6d636", "#7a8cff", "#ff8cce",
            "#5be0a0", "#c0c0c0",
        ],
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


def _read_user() -> dict:
    try:
        with open(CONFIG_FILE, "r", encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return {}


def _split_user(user: dict) -> tuple[dict, dict]:
    """Separate the base config from the sparse garage overrides."""
    user = copy.deepcopy(user or {})
    garage = user.pop(GARAGE_KEY, {}) or {}
    return user, garage


def load() -> dict:
    """Load defaults merged with the base (on-track) overlay_config.json."""
    base_user, _ = _split_user(_read_user())
    return _deep_merge(DEFAULTS, base_user)


def load_garage() -> dict:
    """Load just the sparse garage overrides from disk."""
    _, garage = _split_user(_read_user())
    return garage


# The base ("On track") config and the sparse garage overrides.
BASE: dict = load()
GARAGE: dict = load_garage()

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
_rescan_listeners: list = []


def on_rescan(callback) -> None:
    """Register a callback() fired when the user asks to rescan the track."""
    _rescan_listeners.append(callback)


def request_rescan() -> bool:
    """Tell the running overlay to forget and re-learn the current track.

    Returns True if a live overlay handled it, False if none is listening
    (e.g. the editor was launched standalone with no overlay running).
    """
    handled = False
    for cb in list(_rescan_listeners):
        try:
            cb()
            handled = True
        except Exception:
            pass
    return handled


_rescan_pit_listeners: list = []


def on_rescan_pits(callback) -> None:
    """Register a callback() fired when the user asks to re-learn just the pits."""
    _rescan_pit_listeners.append(callback)


def request_rescan_pits() -> bool:
    """Tell the running overlay to forget and re-learn only the pit lane."""
    handled = False
    for cb in list(_rescan_pit_listeners):
        try:
            cb()
            handled = True
        except Exception:
            pass
    return handled


# The widget section currently painting. Shared font helpers use it to apply the
# right per-widget text_scale without every call site passing a section. Safe
# because Qt painting runs on a single (GUI) thread.
_active_section: str | None = None


def use_section(name: str | None) -> None:
    """Mark which widget section is painting (set at the top of paintEvent)."""
    global _active_section
    _active_section = name


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
    for cb in list(_listeners):
        try:
            cb(CFG)
        except Exception:
            pass


def set_cfg(new_cfg: dict, notify: bool = True) -> dict:
    """Replace the base (on-track) config and recompute the live config.

    Kept for backwards compatibility; treats the supplied dict as the base
    profile. Use apply_edits()/save_profiles() for context-aware editing.
    """
    return apply_base(new_cfg, notify)


def reload() -> dict:
    """Reload both profiles from disk and recompute the live config."""
    global BASE, GARAGE
    BASE = load()
    GARAGE = load_garage()
    _compute_cfg()
    _notify()
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
    ACTIVE_CONTEXT = ctx
    # A live editor preview pin overrides the auto-detected context.
    if _preview_context is None:
        _compute_cfg()
        if notify:
            _notify()
    return CFG


def set_preview_context(ctx: str | None) -> dict:
    """Pin the live config to a context for the editor's preview (None clears)."""
    global _preview_context
    _preview_context = ctx if ctx in CONTEXTS else None
    _compute_cfg()
    _notify()
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
    """Replace the base (on-track) profile with a full config dict."""
    global BASE
    BASE = copy.deepcopy(full_cfg)
    _compute_cfg()
    if notify:
        _notify()
    return CFG


def apply_garage(full_cfg: dict, notify: bool = True) -> dict:
    """Set the garage profile from a full dict, keeping only the diff vs base."""
    global GARAGE
    GARAGE = diff_from_defaults(full_cfg, BASE)
    _compute_cfg()
    if notify:
        _notify()
    return CFG


def apply_edits(ctx: str, full_cfg: dict, notify: bool = True) -> dict:
    if ctx == "garage":
        return apply_garage(full_cfg, notify)
    return apply_base(full_cfg, notify)


def clear_garage(notify: bool = True) -> dict:
    """Drop all garage overrides (garage then mirrors the on-track profile)."""
    global GARAGE
    GARAGE = {}
    _compute_cfg()
    if notify:
        _notify()
    return CFG


def save_profiles(path: str = CONFIG_FILE) -> dict:
    """Persist the base profile (minimal) plus any sparse garage overrides."""
    data = diff_from_defaults(BASE)
    garage_sparse = diff_from_defaults(_deep_merge(BASE, GARAGE), BASE)
    if garage_sparse:
        data[GARAGE_KEY] = garage_sparse
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2)
    return data


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


def table_column_order(section: str) -> list:
    """Normalized list of visible columns (in order) for a table section.

    Unknown keys are dropped and duplicates removed. If a section has no
    configured order at all, every known column is shown.
    """
    order = CFG.get(section, {}).get("column_order")
    if not order:
        return list(TABLE_COLUMNS)
    result = []
    for k in order:
        if k in TABLE_COLUMNS and k not in result:
            result.append(k)
    return result or list(TABLE_COLUMNS)


def has_column(section: str, key: str) -> bool:
    """True if the given column is currently visible in a table section."""
    return key in table_column_order(section)


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
