"""Help text for every settings-editor row (tooltips + ? popups)."""

from __future__ import annotations

WIDGET_NAMES: dict[str, str] = {
    "relative": "Relative table",
    "standings": "Standings table",
    "laptime_log": "Lap time log",
    "fuel_calc": "Fuel calculator",
    "radar": "Proximity radar",
    "dash": "Dash",
    "inputs": "Inputs trace",
    "delta_bar": "Delta bar",
    "flags": "Flags",
    "sector_timing": "Sector timing",
    "lap_compare": "Lap compare",
    "map": "Track map",
    "tire_panel": "Tire panel",
    "pit_board": "Pit board",
    "weather_panel": "Weather panel",
    "leaderboard_strip": "Leaderboard strip",
    "radio_tower": "Radio tower",
    "system_panel": "System panel",
    "pit_advisor": "Pit engineer",
    "ers_hybrid": "ERS / hybrid panel",
}

COLUMN_WIDTH_HELP: dict[str, str] = {
    "badge": "How wide the badge column is, as a multiple of row height. "
    "Holds icons, flags, and speaking indicators in the {widget}.",
    "position": "How wide the race-position column is, as a multiple of row height "
    "in the {widget}.",
    "car_number": "How wide the car-number column is, as a multiple of row height "
    "in the {widget}.",
    "gap": "How wide the main gap/interval column is, as a multiple of row height "
    "in the {widget}. Raise if gap text feels cramped.",
    "irating": "How wide the iRating column is, as a multiple of row height "
    "in the {widget}.",
    "license": "How wide the license-class pill column is, as a multiple of row height "
    "in the {widget}.",
    "pit": "How wide the pit-info column is, as a multiple of row height "
    "in the {widget}. Content depends on Pit mode.",
    "last_lap": "How wide the last-lap time column is, as a multiple of row height "
    "in the {widget}.",
    "best_lap": "How wide the best-lap time column is, as a multiple of row height "
    "in the {widget}.",
    "class_pos": "How wide the class-position column is, as a multiple of row height "
    "in the {widget}.",
    "status": "How wide the status column is, as a multiple of row height "
    "in the {widget}.",
    "car_flag": "How wide the per-car flag column is, as a multiple of row height "
    "in the {widget}.",
    "laps": "How wide the laps-completed column is, as a multiple of row height "
    "in the {widget}.",
    "gap_ahead": "How wide the gap-to-car-ahead column is, as a multiple of row height "
    "in the {widget}.",
    "gap_leader": "How wide the gap-to-leader column is, as a multiple of row height "
    "in the {widget}.",
    "closing": "How wide the closing-rate column is, as a multiple of row height "
    "in the {widget}.",
    "qual_pos": "How wide the qualifying-position column is, as a multiple of row height "
    "in the {widget}.",
    "qual_best": "How wide the qualifying-best-lap column is, as a multiple of row height "
    "in the {widget}.",
    "gap_pole": "How wide the gap-to-pole column is, as a multiple of row height "
    "in the {widget}.",
    "team": "How wide the team-name column is, as a multiple of row height "
    "in the {widget}.",
    "nickname": "How wide the driver nickname column is, as a multiple of row height "
    "in the {widget}.",
    "gutter": "Left/right padding inside each row, as a multiple of row height "
    "in the {widget}.",
}

LICENSE_CLASS_HELP: dict[str, str] = {
    "R": "Pill color for Rookie (R) license class drivers in the {widget}.",
    "D": "Pill color for D-class license drivers in the {widget}.",
    "C": "Pill color for C-class license drivers in the {widget}.",
    "B": "Pill color for B-class license drivers in the {widget}.",
    "A": "Pill color for A-class license drivers in the {widget}.",
    "P": "Pill color for Pro/WC license drivers in the {widget}.",
}

DASH_SLOT_HELP: dict[str, str] = {
    "center_mode": "What the center medallion shows: gear ring with input arcs, "
    "or vertical throttle/brake/clutch bars.",
    "top_right": "Small readout beside the shift bar. Pick speed, incidents, fuel, "
    "lap times, temps, or None to hide the slot.",
    "primary_left": "Small readout at lower-left. Pick lap count, fuel, delta, "
    "or another metric, or None to hide.",
    "primary_right": "Large primary readout at lower-left (usually speed or RPM). "
    "Pick any metric or None to hide.",
    "stat_left": "Upper stacked stat cell at lower-right (tire wear, fuel, etc.).",
    "stat_right": "Lower stacked stat cell at lower-right.",
    "strip_left": "Left cell of the bottom telemetry strip (air temp, track temp, etc.).",
    "strip_center": "Center cell of the bottom telemetry strip.",
    "strip_right": "Right cell of the bottom telemetry strip.",
}

RADAR_SIZE_HELP: dict[str, str] = {
    "car_w": "Your car icon width on the radar, as a fraction of panel width.",
    "car_h": "Your car icon height on the radar, as a fraction of panel height.",
    "bar_h": "Height of the front/rear proximity bars, as a fraction of panel height.",
    "glow_w": "Width of the proximity glow, as a fraction of panel width.",
    "nose_len": "Length of the forward-pointing nose marker, as a fraction of panel height.",
}

# Hand-written explanations keyed by dotted path or bare leaf key.
HELP_OVERRIDES: dict[str, str] = {
    "show": "Show or hide this entire widget window and its telemetry work.",
    "text_scale": "Multiplies text size in this widget on top of the global Text scale.",
    "corner_radius_frac": "Rounds panel corners (0 = square, 1 = very round). "
    "Affects the card background shape only.",
    "row_dividers": "Draws thin horizontal lines between rows.",
    "data_font_bold": "Uses bold weight for numeric cells (position, gap, lap times).",
    "alt_row_shading": "Alternates a subtle background tint on even rows.",
    "font_family": "Primary UI font for labels, names, headers, and (unless "
    "Tabular font is set) numeric values across all widgets.",
    "tabular_font_family": "Optional monospace override for gap / lap-time / "
    "numeric columns. Empty uses the same family as Font.",
    "text_scale_global": "Global multiplier for every widget's text size. "
    "Raise to enlarge all overlay text; lower to shrink it.",
    "units": "Metric (km/h, °C, L) or imperial (mph, °F, gal) for speed, "
    "temperature and fuel readouts.",
    "check_updates_on_launch": "Checks GitHub for a newer GridGlance release when "
    "the app starts.",
    "start_overlay_on_launch": "Starts the overlay widgets as soon as GridGlance "
    "opens, without waiting for Start Overlay. Settings still open unless launch "
    "uses --no-settings (e.g. the login Startup shortcut).",
    "start_at_login": "On Windows, runs GridGlance when you sign in (Startup folder "
    "shortcut). If overlay-on-launch is on, login skips opening Settings.",
    "row_height_px": "Fixed row height in pixels. 0 scales rows to fill the panel.",
    "max_row_height_frac": "Caps row height as a fraction of panel height when "
    "row height is 0 (stops rows looking zoomed with few entries).",
    "irating_abbreviate": "Shows iRating as 1.4k instead of the full number.",
    "show_irating_projection": "Shows projected iRating +/- during races. Uses registered field size (including DNS) and is an estimate until results are final.",
    "font_scale": "Row text size as a multiple of row height.",
    "gap_font_scale": "Extra scale applied to gap / interval columns only.",
    "name_font_bold": "Renders driver name cells in bold.",
    "irating_show_icon": "Shows a small chart icon beside iRating values.",
    "header_font_scale": "Header band text size, independent of row text.",
    "footer_font_scale": "Footer band text size, independent of row text.",
    "row_ease_tau": "How quickly row reorder animations ease (lower = snappier).",
    "fade_ease_tau": "How quickly row fade-in/out animations ease.",
    "map.asphalt_width": "Thickness of the track surface line on the map. "
    "Higher = thicker racing line.",
    "map.outline_width": "Thickness of the track outline stroke. "
    "Higher = bolder edge.",
    "map.rotation": "Rotates the entire map (track, cars, corners, pit) in "
    "90° steps. Does not rotate the wind compass.",
    "map.mirror": "Flips the map horizontally. Useful when the imported track "
    "orientation does not match your preference.",
    "map.dot_radius_frac": "Your car's dot size relative to the map panel. "
    "Does not change other cars.",
    "map.other_dot_radius_frac": "Other cars' (and pace car) dot size relative "
    "to the map panel. Does not change your car.",
    "map.show_infield": "Fills the area inside the track loop with the infield color.",
    "map.show_corners": "Shows numbered corner labels on the track map.",
    "map.auto_corners": "Auto-detects corners from track shape when the file "
    "has no corner data.",
    "map.show_start_finish": "Draws the start/finish line across the track.",
    "map.show_pit": "Shows pit lane geometry and related overlays.",
    "show_pit_blends": "Draws dashed entry/exit blend lines between pit road "
    "and the racing line.",
    "show_pit_speed": "Shows the pit speed limit badge on the map.",
    "pit_lane_opacity": "Opacity of pit lane and blend lines (0 = invisible, "
    "1 = solid).",
    "pit_dot_opacity": "Opacity of car dots while they are on pit road.",
    "map.show_wind": "Shows wind direction and speed in a map corner.",
    "map.show_expanded_weather": "Shows rain / track wetness under the wind compass.",
    "map.show_car_status": "Small status badges on car dots (pit, off-track, flags).",
    "show_pace_car": "Highlights the pace car (PC) on the map.",
    "show_sector_boundaries": "Draws sector split markers on the track loop.",
    "show_traffic_markers": "Shows ahead / behind / leader icons on car dots.",
    "marker_hold_seconds": "Seconds a traffic marker must hold before switching "
    "to another car (reduces flicker).",
    "map.show_panel": "Draws a rounded card behind the whole map widget.",
    "map.show_drs_zones": "Highlights DRS zones from track JSON (if present).",
    "map.show_p2p_zones": "Highlights push-to-pass zones from track JSON.",
    "map.show_pace_car": "Shows the pace car dot when CarIsPaceCar is set.",
    "map.show_pace_safety_line": "Under caution, draws the pit-exit mark and a "
    "moving pace-car safety line so you can see if rejoining would put you a lap down.",
    "map.car_label": "What each car dot displays: car number or race position.",
    "map.lap_proximity_pct": "Lap-distance window for red 'lapping you' tint "
    "when a car is ~one lap ahead.",
    "relative.center_on_player": "Keeps your car centered with rows above/below. "
    "Off shows a fixed top-N list instead.",
    "relative.rows_ahead": "How many cars to show above you in centered mode.",
    "relative.rows_behind": "How many cars to show below you in centered mode.",
    "relative.show_strategy_hints": "When the fuel pit window is open, tint nearby "
    "cars for undercut (ahead) or cover (behind) risk.",
    "relative.strategy_fuel_pct_thresh": "Fuel fraction that also opens strategy "
    "hints when the formal pit window is closed (e.g. 0.18 = 18%).",
    "relative.undercut_gap_max_s": "Max gap ahead (seconds) to tag as undercut opportunity.",
    "relative.cover_gap_max_s": "Max gap behind (seconds) to tag as cover risk.",
    "relative.pit_loss_seconds": "Assumed pit-stop time loss (seconds); reserved for strategy math.",
    "standings.center_on_player": "Scrolls the table to keep your row visible.",
    "standings.pin_podium": "Always keeps P1–P3 in the first three rows when centered.",
    "standings.rows": "Number of rows when not using center-on-player mode.",
    "standings.rows_ahead": "Rows above your car when centered.",
    "standings.rows_behind": "Rows below your car when centered.",
    "radar.show_front": "Enables front blind-spot sensing bar.",
    "radar.show_rear": "Enables rear blind-spot sensing bar.",
    "radar.side_span_pct": "How far side markers travel along the radar bar.",
    "radar.side_proximity_color": "Fades side markers yellow→red by fore/aft overlap.",
    "radar.show_side_labels": "Shows car numbers on side markers.",
    "radar.closing_rate_color": "Tints side markers by closing speed.",
    "radar.closing_rate_full": "Closing speed (m/s) that maps to full red tint.",
    "radar.show_clear_timer": "Shows seconds since blind spot last cleared.",
    "radar.alongside_zone_pct": "Lap-% window used to correlate side cars.",
    "inputs.history_seconds": "How many seconds of pedal history the trace shows.",
    "inputs.show_graph": "Shows the scrolling throttle/brake trace.",
    "inputs.show_bars": "Shows live value bars beside the trace.",
    "inputs.show_gauge": "Shows the gear / speed medallion.",
    "fuel_calc.history_laps": "Number of recent laps averaged for fuel projections.",
    "fuel_calc.show_gauge": "Shows the vertical fuel level gauge.",
    "fuel_calc.show_strip": "Shows the pit-window timeline strip.",
    "delta_bar.mode": "Which lap the delta bar compares against. Live delta on the out lap begins at the first sector line after leaving pits.",
    "delta_bar.range": "Seconds of delta that fill the bar to full width.",
    "delta_bar.show_value": "Shows the numeric delta above the bar.",
    "lap_compare.max_turns": "Maximum corners listed in the compare panel.",
    "lap_compare.min_time_loss": "Minimum time loss (s) before a corner is listed.",
    "lap_compare.show_live_delta": "Shows live delta while on track.",
    "lap_compare.show_graph": "Shows delta-over-distance trace graph.",
    "sector_timing.sectors": "Sector count when the session provides no layout.",
    "laptime_log.rows": "How many recent laps to list (newest first).",
    "laptime_log.delta_mode": "What each lap's delta column compares against.",
    "laptime_log.temp_icon": "Shows a thermometer icon in the temp column.",
    # --- relative / standings ---
    "relative.show_footer": "When on, shows the footer band with race clock, lap, and incidents. "
    "When off, hides the footer to save vertical space.",
    "standings.show_footer": "When on, shows the footer band with temps and session time. "
    "When off, hides the footer.",
    "relative.pit_mode": "What the pit column displays: laps since stop, time since stop, "
    "lap pitted on, or race time when they pitted.",
    "standings.pit_mode": "What the pit column displays: laps since stop, time since stop, "
    "lap pitted on, or race time when they pitted.",
    "standings.title": "Title text in the standings header (shown when Title is in a header slot).",
    "relative.columns.stripe": "When on, draws the class-color stripe inside the position cell. "
    "When off, hides the stripe.",
    "standings.columns.stripe": "When on, draws the class-color stripe inside the position cell. "
    "When off, hides the stripe.",
    "laptime_log.show_header": "When on, shows column headers (LAP, TIME, DELTA, TEMP). When off, hides them.",
    # --- fuel calc ---
    "fuel_calc.title": "Title text shown in the fuel calculator header bar.",
    "fuel_calc.show_title": "When on, shows the title bar across the top. When off, hides it.",
    "fuel_calc.show_pill": "When on, shows the pit-window open/closed status pill. When off, hides it.",
    "fuel_calc.show_add": "When on, shows the large fuel-to-add box. When off, hides it.",
    "fuel_calc.show_stats": "When on, shows the AVG/MAX/MIN burn grid (MAX = "
    "worst burn, MIN = best economy). When off, hides it.",
    "fuel_calc.show_time": "When on, shows the time-until-empty summary box. When off, hides it.",
    "fuel_calc.show_laps": "When on, shows the laps-until-empty summary box. When off, hides it.",
    "fuel_calc.show_live_burn": "When on, shows live fuel burn rate. When off, uses lap averages only.",
    "fuel_calc.show_tank_pct": "When on, shows tank fill as a percentage. When off, hides it.",
    "fuel_calc.show_stints": "When on, shows stint lap counts. When off, hides stint info.",
    "fuel_calc.show_low_fuel_alert": "When on, highlights boxes red when fuel is critically low.",
    "fuel_calc.show_pit_compare": "When on, compares pit-stop fuel options. When off, hides comparison.",
    "fuel_calc.pit_loss_seconds": "Assumed time lost per pit stop (seconds) used in fuel strategy math.",
    "fuel_calc.stint_laps": "Expected laps per stint for pit-window calculations.",
    "fuel_calc.legal_fuel_buffer_l": "Extra fuel (liters) kept as a safety buffer in finish calculations.",
    "fuel_calc.low_fuel_laps_threshold": "Laps of fuel remaining that triggers a low-fuel warning.",
    "fuel_calc.low_fuel_time_threshold": "Seconds of fuel remaining that triggers a low-fuel warning.",
    "fuel_calc.stats_header_font_scale": "Text size for the AVG/MAX/MIN grid headers.",
    "fuel_calc.stats_row_font_scale": "Text size for values in the AVG/MAX/MIN grid.",
    # --- radar ---
    "radar.range_pct": "Lap-distance window (fraction of a lap) for front/rear proximity detection. "
    "Smaller = shorter range; larger = longer range.",
    "radar.ease_side_tau": "How smoothly side markers slide along the bar (lower = snappier).",
    "radar.ease_glow_tau": "How smoothly the front/rear glow fades (lower = snappier).",
    "radar.show_nose": "When on, shows the forward-pointing nose on your car icon. When off, hides it.",
    "radar.show_axis": "When on, shows the center axis line. When off, hides it.",
    "radar.show_panel": "When on, draws a rounded card behind the radar. When off, shows radar only.",
    # --- dash ---
    "dash.shift_segments": "Number of segments in the RPM shift bar. More = smoother gradient.",
    "dash.shift_red_frac": "Fraction of the shift bar reserved for the red shift-now zone.",
    "dash.shift_yellow_frac": "Fraction of the shift bar for the yellow warning zone.",
    "dash.shift_blink": "When on, the whole shift bar flashes at redline to signal shift now.",
    "dash.shift_blink_hz": "Flash rate (Hz) when the shift bar blinks at redline.",
    "dash.shift_blink_pct": "RPM fraction of redline where blinking starts (0.99 = very late).",
    "dash.shift_blink_max_sec": "Stop flashing after this many seconds at redline; "
    "resumes the next time RPM drops below the threshold and climbs back.",
    "dash.ring_segments": "Number of segments in the throttle/brake input ring.",
    "dash.show_throttle": "When on, shows throttle in the center medallion (ring arc or pedal bar).",
    "dash.show_brake": "When on, shows brake in the center medallion.",
    "dash.show_clutch": "When on, shows clutch in the center medallion.",
    "dash.show_shift_bar": "When on, shows the horizontal RPM shift bar. When off, hides it.",
    "dash.show_ring": "When on, shows the center medallion (gear ring or pedals). When off, hides it.",
    "dash.show_position": "When on, shows race position on the dash. When off, hides it.",
    "dash.show_flags": "When on, shows the session flag banner. When off, hides flag alerts.",
    "dash.flag_green_seconds": "How long the green flag stays visible after a yellow clears (seconds).",
    "dash.flag_pulse": "When on, new flags pulse/flash briefly before holding steady.",
    "dash.flag_pulse_seconds": "How long the flag banner pulses when a new flag appears (seconds).",
    "dash.flag_blink_hz": "Flash rate (Hz) for flag pulse and green-flag wave.",
    "dash.show_delta_bar": "When on, shows the thin live delta bar across the top of the dash. Live delta on the out lap begins at the first sector line after leaving pits.",
    "dash.delta_bar_mode": "Which lap the dash delta bar (and delta metric slot) compares against. Independent of the standalone Delta Bar widget. Live delta on the out lap begins at the first sector line after leaving pits.",
    "dash.delta_bar_range": "Seconds of delta that push the dash delta bar to full width.",
    "dash.center_mode": DASH_SLOT_HELP["center_mode"],
    "dash.top_right": DASH_SLOT_HELP["top_right"],
    "dash.primary_left": DASH_SLOT_HELP["primary_left"],
    "dash.primary_right": DASH_SLOT_HELP["primary_right"],
    "dash.stat_left": DASH_SLOT_HELP["stat_left"],
    "dash.stat_right": DASH_SLOT_HELP["stat_right"],
    "dash.strip_left": DASH_SLOT_HELP["strip_left"],
    "dash.strip_center": DASH_SLOT_HELP["strip_center"],
    "dash.strip_right": DASH_SLOT_HELP["strip_right"],
    # --- inputs ---
    "inputs.show_throttle": "When on, draws the throttle trace and value bar.",
    "inputs.show_brake": "When on, draws the brake trace and value bar.",
    "inputs.show_clutch": "When on, draws the clutch trace.",
    "inputs.show_steering": "When on, draws the steering trace (centered around zero).",
    "inputs.show_handbrake": "When on, draws the handbrake trace.",
    "inputs.show_steering_torque": "When on, draws steering torque in the trace.",
    "inputs.show_tc_abs": "When on, shows traction-control and ABS activity in the trace.",
    "inputs.show_shift_markers": "When on, marks gear shifts on the trace.",
    "inputs.show_brake_threshold": "When on, draws the trail-braking threshold line on the brake trace.",
    "inputs.brake_threshold": "Brake pedal % where the trace switches to heavy-braking color (0–100).",
    "inputs.show_label": "When on, shows the vertical title tab. When off, hides it.",
    "inputs.label_text": "Text on the vertical title tab (default TELEMETRY).",
    "inputs.line_width": "Thickness of trace lines in pixels.",
    # --- flags ---
    "flags.idle_text": "Message shown when no session flag is active (default TRACK CLEAR).",
    "flags.show_incident_warning": "When on, warns as incident points approach the limit.",
    "flags.incident_warn_pct": "Incident limit fraction (0–1) where the warning appears.",
    "flags.show_blue_detail": "When on, shows which car triggered a blue flag.",
    "flags.show_pit_limiter": "When on, shows pit speed limiter status in the flags panel.",
    "flags.show_finish_position": "When on, shows your finish position when the checkered flag falls.",
    # --- lap compare ---
    "lap_compare.reference_mode": "Reference lap for corner deltas: personal best or last lap.",
    "lap_compare.show_brake_markers": "When on, marks braking points on the delta graph.",
    "lap_compare.show_lift_markers": "When on, marks lift/coast points on the delta graph.",
    "lap_compare.show_gear_rpm": "When on, shows gear and RPM on corner rows.",
    "lap_compare.wetness_delta_threshold": "Track wetness % above which lap deltas are ignored.",
    "lap_compare.exclude_wet_laps": "When on, skips wet laps from corner comparison.",
    # --- sector timing ---
    "sector_timing.show_sector_delta": "When on, shows delta vs best for each sector.",
    "sector_timing.show_predicted_lap": "When on, shows a predicted lap time based on sector pace.",
    "sector_timing.highlight_active_sector_on_map": "When on, highlights your current sector on the track map.",
    # --- tire panel ---
    "tire_panel.show_title": "When on, shows the title bar. When off, hides it.",
    "tire_panel.title": "Title text in the tire panel header.",
    "tire_panel.show_wear": "When on, shows tire wear bars per corner.",
    "tire_panel.show_temp": "When on, shows tire temperatures.",
    "tire_panel.show_pressure": "When on, shows tire pressures.",
    "tire_panel.warn_wear_pct": "Wear % below which tire bars turn warning red.",
    # --- pit board ---
    "pit_board.show_title": "When on, shows the title bar. When off, hides it.",
    "pit_board.title": "Title text in the pit board header.",
    "pit_board.show_pit_banner": "When on, shows the active pit-stop banner.",
    "pit_board.pit_banner_text": "Text on the active pit-stop banner.",
    "pit_board.show_pressures": "When on, shows tire pressure adjustment options.",
    "pit_board.show_fast_repairs": "When on, shows fast-repair availability.",
    "pit_board.show_compound": "When on, shows tire compound selection.",
    # --- weather ---
    "weather_panel.show_title": "When on, shows the title bar. When off, hides it.",
    "weather_panel.title": "Title text in the weather panel header.",
    "weather_panel.show_skies": "When on, shows sky/cloud conditions.",
    "weather_panel.show_rain": "When on, shows rain and track wetness.",
    "weather_panel.show_temps": "When on, shows air and track temperatures.",
    "weather_panel.show_wind": "When on, shows wind direction and speed.",
    "weather_panel.show_trend": "When on, shows temperature trend arrows.",
    "weather_panel.trend_window_seconds": "Seconds of history used to compute temperature trends.",
    "system_panel.show_title": "When on, shows the title bar. When off, hides it.",
    "system_panel.title": "Title text in the system panel header.",
    "system_panel.show_icons": "When on, shows Font Awesome icons instead of text "
    "labels for each metric row.",
    "system_panel.show_cpu": "When on, shows this machine's CPU usage.",
    "system_panel.show_mem": "When on, shows this machine's memory usage.",
    "system_panel.show_gpu": "When on, shows GPU usage (Windows PDH or nvidia-smi).",
    "system_panel.show_fps": "When on, shows iRacing average frame rate.",
    "system_panel.show_network": "When on, shows iRacing channel quality/latency "
    "during online sessions, otherwise OS WiFi signal when available.",
    # --- pit engineer ---
    "pit_advisor.show_title": "When on, shows the title bar. When off, hides it.",
    "pit_advisor.title": "Title text in the pit engineer header.",
    "pit_advisor.show_only_when_actionable": "When on, hides the panel when fuel is "
    "comfortable and the recommendation is stay out (reduces clutter).",
    "pit_advisor.pit_loss_seconds": "Assumed time lost per pit stop (seconds) for undercut math.",
    "pit_advisor.legal_fuel_buffer_l": "Extra fuel (liters) kept as a safety buffer in finish calculations.",
    "pit_advisor.low_fuel_laps_threshold": "Laps of fuel margin that triggers a critical pit call.",
    "pit_advisor.undercut_gap_max_s": "Maximum gap ahead (seconds) to suggest pitting next lap for an undercut.",
    "pit_advisor.cover_gap_max_s": "Maximum gap behind (seconds) to suggest pitting now to cover.",
    "pit_advisor.caution_fuel_multiplier": "Scales fuel burn estimate under caution (e.g. 0.85 = 15% less burn).",
    "pit_advisor.top_positions_stay_out": "Under caution with an open window, stay out when position is this good or better.",
    "pit_advisor.field_pit_follow_threshold": "Fraction of cars ahead on pit road that triggers a join-the-cycle call.",
    "pit_advisor.caution_pit_pra_threshold": "Ahead pitting ratio that triggers a caution boxing pit call.",
    "pit_advisor.caution_pit_lead_loss_max": "Maximum estimated lead-lap positions lost before pitting under yellow.",
    "pit_advisor.recent_pit_laps_window": "Laps since pit that count a car as recently pitted.",
    "pit_advisor.green_run_caution_bias_laps": "Green-flag laps before suggesting another caution is possible.",
    "pit_advisor.post_pit_quiet_min_laps": "Laps after your pit stop before pit alerts resume (fuel, tires, and caution).",
    "pit_advisor.lapped_danger_fuel_min_laps": "Minimum fuel laps left to defer a pit that risks going a lap down.",
    "pit_advisor.reentry_window_pct": "Lap-distance fraction for merge-traffic density check after a pit.",
    "pit_advisor.show_field_context": "When on, shows caution count, field pitting %, and reentry verdict on the second line.",
    "pit_advisor.show_tire_inventory": "When on, shows tire set count (e.g. Set 2 of 4) on the secondary line.",
    "pit_advisor.tire_warn_wear_pct": "Minimum tread % before the tire stop window opens.",
    "pit_advisor.tire_critical_wear_pct": "Tread % that forces an immediate pit call.",
    "pit_advisor.low_tire_laps_threshold": "Projected laps to critical wear that triggers a pit call.",
    "pit_advisor.min_stint_laps": "Minimum laps on a stint before optional tire stops (unless critical).",
    "pit_advisor.tire_sets_reserve": "Sets to keep in reserve when possible (delays optional tire stops).",
    "pit_advisor.race_tire_sets_total": "Manual total dry sets when iRacing does not report inventory (0 = auto).",
    "pit_advisor.ahead_scan_positions": "How many cars ahead to scan for pace and stint age.",
    "pit_advisor.ahead_pace_delta_s": "Seconds faster than your pace to count a car as quicker ahead.",
    "pit_advisor.fresh_tire_lap_delta": "Laps fresher than you to treat an ahead car as on new tires.",
    "pit_advisor.caution_overdue_ratio": "Green-run length vs avg caution gap before yellow is considered due.",
    "pit_advisor.field_chaos_high_threshold": "Field off-track/flag fraction that raises caution likelihood.",
    "pit_advisor.caution_wait_min_fuel_laps": "Minimum fuel margin before suggesting wait-for-yellow.",
    "pit_advisor.cover_closing_min_rate": "Closing speed (s/s) from behind that triggers a cover pit.",
    "pit_advisor.green_pos_lost_max": "Positions likely lost on a green pit stop before downgrading to marginal.",
    "pit_advisor.caution_prb_stay_out_threshold": "Fraction of cars behind pitting that favors staying out on yellow.",
    "pit_advisor.caution_prb_pit_threshold": "Fraction of cars behind still out that favors pitting on yellow.",
    "pit_advisor.final_laps_optional_suppress": "Laps to go before optional pit advice is suppressed (critical only).",
    "pit_advisor.track_wetness_tire_suppress": "Track wetness above this disables dry-tire wear logic.",
    "pit_advisor.use_measured_pit_loss": "When on, uses your measured pit stop duration instead of the fixed default.",
    "pit_advisor.pit_loss_ema_alpha": "Blend weight for new pit stop samples (higher = more responsive).",
    "pit_advisor.pit_menu_hard_gate": "When on, blocks PIT NOW until required fuel/tires are queued on the pit menu.",
    "pit_advisor.opponent_tire_inference_enabled": "When on, infers opponent tire-set usage from pit stops and stint length.",
    "pit_advisor.ahead_profile_scan_positions": "How many cars ahead to profile for tire-set inference.",
    "pit_advisor.strategic_pit_min_net_positions": "Minimum net positions gained before recommending a strategic early pit.",
    "pit_advisor.opponent_splash_pit_max_s": "Pit stop shorter than this (seconds) counts as fuel-only, not a tire change (0 = disabled).",
    "pit_advisor.opponent_stint_due_laps": "Stint length after which an opponent is considered due for tires.",
    "pit_advisor.green_pos_tradeoff_override": "When on, a positive net position gain can override green-flag pit downgrade.",
    "pit_advisor.caution_bankrupt_ahead_min": "Minimum cars ahead out of tires to bias toward pitting under caution.",
    # --- leaderboard strip ---
    "leaderboard_strip.rows": "How many drivers to show (0 = entire field; positive = top N only).",
    "leaderboard_strip.show_position": "When on, shows race position (1, 2, …).",
    "leaderboard_strip.show_name": "When on, shows driver names below each row.",
    "leaderboard_strip.show_car_number": "When on, shows car numbers in orange LED style.",
    "leaderboard_strip.show_lap": "Optional LAP column (off by default; core layout is position + car number).",
    "leaderboard_strip.show_mph": "Optional MPH column (off by default; core layout is position + car number).",
    "leaderboard_strip.show_gap": "When on, shows gap to the car ahead below each row.",
    "leaderboard_strip.show_class_color": "Legacy option; class stripes are not drawn in pylon style.",
    "leaderboard_strip.highlight_player": "When on, highlights your row in the strip.",
    # --- Radio tower ---
    "radio_tower.show_title": "When on, shows the title bar. When off, hides it.",
    "radio_tower.title": "Title text in the radio tower header.",
    "radio_tower.show_position": "When on, shows the speaker's race position in a left column.",
    "radio_tower.show_car_number": "When on, appends the car number to the driver label (e.g. Name #12).",
    "radio_tower.show_name": "When on, shows the driver name in the label line beside position.",
    "radio_tower.highlight_player": "When on, highlights your row when you are on radio.",
    # --- ERS / hybrid ---
    "ers_hybrid.show_title": "When on, shows the title bar. When off, hides it.",
    "ers_hybrid.title": "Title text in the hybrid panel header.",
    "ers_hybrid.label_battery": "Label for the battery/ERS charge row.",
    "ers_hybrid.label_lap": "Label for the per-lap energy row.",
    "ers_hybrid.label_boost": "Label on the boost deployment chip.",
    "ers_hybrid.label_p2p": "Label on the push-to-pass chip.",
    "ers_hybrid.empty_text": "Message when no hybrid telemetry is available.",
    "ers_hybrid.show_battery": "When on, shows battery/ERS charge level.",
    "ers_hybrid.show_lap_energy": "When on, shows per-lap energy use.",
    "ers_hybrid.show_boost": "When on, shows boost deployment status.",
    "ers_hybrid.show_p2p": "When on, shows push-to-pass availability.",
}

COLOR_HELP: dict[str, str] = {
    "bg": "Main panel background fill for {widget}.",
    "bg_top": "Top color of the panel gradient for {widget}.",
    "bg_bottom": "Bottom color of the panel gradient for {widget}.",
    "border": "Outer panel border color for {widget}.",
    "panel_border": "Outer panel border color for {widget}.",
    "text": "Primary text color in {widget}.",
    "muted": "Secondary / de-emphasized text in {widget}.",
    "header": "Column or section header text in {widget}.",
    "header_bg": "Header band background in {widget}.",
    "footer_bg": "Footer band background in {widget}.",
    "cell_dark": "Dark cell or pill fill in {widget}.",
    "cell_border": "Border around dark cells / pills in {widget}.",
    "row_alt": "Alternating row background tint in {widget}.",
    "player_row": "Highlight wash for your row or car in {widget}.",
    "player": "Your car dot color on the track map.",
    "competitor": "Other cars' dot color on the track map.",
    "lapped": "Tint for cars a lap down (or lapped-traffic rows).",
    "lapping": "Tint for cars a lap ahead that may lap you.",
    "pit": "Pit lane line color on the map.",
    "pit_blend": "Pit entry blend line color on the map.",
    "pit_blend_out": "Pit exit blend line color on the map.",
    "pit_row": "Row background when a driver is in the pits.",
    "inactive_row": "Standings row background when a driver is in the garage or disconnected.",
    "pit_text": "Pit speed limit label text on the map.",
    "pit_over": "Color when exceeding pit speed limit.",
    "pit_car": "Car dot fill while on pit road.",
    "asphalt": "Track surface line color on the map.",
    "outline": "Track outline stroke color on the map.",
    "infield": "Fill inside the track loop on the map.",
    "corner_bg": "Corner number pill background on the map.",
    "corner_border": "Corner number pill border on the map.",
    "corner_text": "Corner number text on the map.",
    "threat": "Row tint for cars a lap ahead in the relative table.",
    "faster": "Color for faster / improved deltas or gaps.",
    "slower": "Color for slower / worse deltas or gaps.",
    "scan_bg": "Background for track-scan hint badges.",
    "hint_bg": "Transient flash-hint banner background.",
    "hint_text": "Transient flash-hint banner text.",
    "irating_bg": "iRating pill background.",
    "irating_border": "iRating pill border.",
    "irating_text": "iRating pill text.",
    "irating_delta_up": "Positive iRating projection color.",
    "irating_delta_down": "Negative iRating projection color.",
    "accent": "Accent stripe or highlight color.",
    "label": "Title or label text color.",
    "graph_bg": "Trace graph background in the inputs widget.",
    "grid": "Grid line color in graphs.",
    "throttle": "Throttle trace and bar color.",
    "brake": "Brake trace and bar color.",
    "clutch": "Clutch trace color.",
    "steering": "Steering trace color.",
    "wind": "Wind compass arrow and text on the map.",
    "drs_zone": "DRS zone highlight on the map.",
    "p2p_zone": "Push-to-pass zone highlight on the map.",
    "abs": "ABS active highlight on brake inputs in the {widget}.",
    "active_bg": "Background when a pit-board row is active/selected in the {widget}.",
    "active_sector": "Highlight for the sector you are currently driving on the map.",
    "active_text": "Text on an active pit-board row in the {widget}.",
    "add_bg": "Background of the big fuel-to-add box in the fuel calculator.",
    "add_text": "Text in the fuel-to-add box.",
    "axis": "Center axis line on the proximity radar.",
    "badge_empty_border": "Border around an empty badge slot in table rows.",
    "badge_empty_fill": "Fill behind an empty badge slot in table rows.",
    "badge_lap": "Lap-count badge color in table rows.",
    "badge_pit_bg": "Background of the in-pits badge on table rows.",
    "badge_pit_text": "Text on the in-pits badge.",
    "badge_player": "Accent color on your row's player badge.",
    "badge_speaking_bg": "Background when a driver is speaking on team radio.",
    "badge_speaking_border": "Border on the speaking/voice-activity badge.",
    "badge_speaking_text": "Text on the speaking/voice-activity badge.",
    "pro_name": "Accent color for professional-driver names in Relative and Standings.",
    "pro_badge": "Star/badge color marking professional drivers in Relative and Standings.",
    "pylon_bg": "Background behind leaderboard-strip pylon markers.",
    "car_number": "Car-number text color on the leaderboard strip or radio tower.",
    "bar_bg": "Background track behind tire wear bars.",
    "bar_track": "Background track behind input value bars.",
    "box_border": "Border around fuel summary boxes (time/laps until empty).",
    "box_value": "Large numeric value in fuel summary boxes.",
    "box_warn": "Warning color when fuel is critically low.",
    "brake_abs": "Brake trace color while ABS is active.",
    "brake_over": "Brake trace color above the threshold line (heavy braking).",
    "car": "Your car icon fill on the proximity radar.",
    "center": "Center zero line on the delta bar.",
    "checked": "Checkmark color for completed pit services.",
    "chip_bg": "Background of corner chips in lap compare.",
    "chip_border": "Border around corner chips in lap compare.",
    "delta_bar_track": "Neutral track behind the dash delta bar.",
    "delta_faster": "Faster-than-reference side of the dash delta bar.",
    "delta_slower": "Slower-than-reference side of the dash delta bar.",
    "flag_black": "Black-flag banner background.",
    "flag_black_text": "Text on the black-flag banner.",
    "flag_blue": "Blue-flag banner background (faster car behind).",
    "flag_blue_text": "Text on the blue-flag banner.",
    "flag_checker_bg": "Checkered-flag / session-finished banner background.",
    "flag_checker_text": "Text on the checkered-flag banner.",
    "flag_crossed": "Crossed-flags (halfway) banner background.",
    "flag_crossed_text": "Text on the crossed-flags banner.",
    "flag_debris": "Debris-on-track warning banner background.",
    "flag_debris_text": "Text on the debris warning banner.",
    "flag_dq": "Disqualification banner background.",
    "flag_dq_text": "Text on the disqualification banner.",
    "flag_furled": "Furled black-flag (warning) banner background.",
    "flag_furled_text": "Text on the furled black-flag banner.",
    "flag_green": "Green-flag (racing resumed) banner background.",
    "flag_green_text": "Text on the green-flag banner.",
    "flag_meatball": "Meatball / mechanical black-flag banner background.",
    "flag_meatball_text": "Text on the meatball flag banner.",
    "flag_red": "Red-flag (session stopped) banner background.",
    "flag_red_text": "Text on the red-flag banner.",
    "flag_white_bg": "White-flag (final lap) banner background.",
    "flag_white_text": "Text on the white-flag banner.",
    "flag_yellow": "Yellow-flag banner background.",
    "flag_yellow_text": "Text on the yellow-flag banner.",
    "gauge_bg": "Background of a circular or vertical gauge.",
    "gauge_border": "Border around a fuel or hybrid gauge.",
    "gauge_fill": "Filled portion of a gauge (fuel level, ERS charge, etc.).",
    "gauge_ring": "Outer ring of the gear/speed medallion.",
    "gear": "Gear number text in the dash medallion.",
    "graph_line": "Delta-over-distance trace line in lap compare.",
    "green": "Positive / good-state accent green in the dash.",
    "idle_bg": "Background when no flag is flying in the flags widget.",
    "idle_text": "Text shown when no flag is flying.",
    "marker_ahead": "Ahead-traffic marker on car dots.",
    "marker_behind": "Behind-traffic marker on car dots.",
    "marker_leader": "Race-leader marker on car dots.",
    "marker_line": "Connector line for traffic markers on the map.",
    "medallion_border": "Border around the dash gear/throttle medallion.",
    "nose": "Forward nose marker on the proximity radar.",
    "orange": "Warning orange accent in the dash.",
    "pace_car": "Pace-car dot fill on the map.",
    "pace_car_text": "Pace-car label text on the map.",
    "pedal_brake": "Brake bar/arc color in the dash medallion.",
    "pedal_clutch": "Clutch bar/arc color in the dash medallion.",
    "pedal_throttle": "Throttle bar/arc color in the dash medallion.",
    "pedal_track": "Empty track behind pedal bars in the dash.",
    "pill": "Hybrid boost / P2P chip fill in the ERS panel.",
    "pill_bg": "Background of small info pills in the dash.",
    "pill_border": "Border around small info pills.",
    "pill_closed": "Pit window closed color on the fuel status pill.",
    "pill_open": "Pit window open color on the fuel status pill.",
    "pill_text": "Text on the fuel pit-window status pill.",
    "pos": "Position text (P1, P2, …) in the leaderboard strip.",
    "red": "Proximity-threat red on the radar.",
    "ring_track": "Background track behind the dash input ring.",
    "sec_best": "Sector cell when you matched your personal best sector time.",
    "sec_done": "Sector cell after you have completed that sector.",
    "sec_idle": "Sector cell not yet started this lap.",
    "sec_running": "Fill for the sector you are currently driving.",
    "sec_running_edge": "Bright edge on the active sector cell.",
    "sec_text": "Sector time text in the sector timing panel.",
    "sector_line": "Sector boundary line drawn on the track map.",
    "sector_text": "Sector label text on the track map.",
    "shift_green": "Lower RPM range on the shift bar (safe).",
    "shift_off": "Inactive segment color on the shift bar.",
    "shift_red": "Redline / shift-now zone on the shift bar.",
    "shift_yellow": "Upper RPM warning zone on the shift bar.",
    "speaking_badge_bg": "Voice-activity badge background on the map.",
    "speaking_badge_text": "Voice-activity badge text on the map.",
    "speaking_glow": "Glow ring when a driver is speaking on the map.",
    "speaking_ring": "Outer ring for voice-activity on car dots.",
    "speaking_row": "Row highlight when a driver is on team radio.",
    "status_black": "Black-flag status badge on car dots.",
    "status_dq": "Disqualified status badge on car dots.",
    "status_furled": "Furled-flag status badge on car dots.",
    "status_garage": "In-garage status badge on car dots.",
    "status_meatball": "Meatball-flag status badge on car dots.",
    "status_off": "Off-track status badge on car dots.",
    "status_pit": "In-pits status badge on car dots.",
    "strip_none": "Timeline segment with no pit window in fuel calc.",
    "strip_now": "Current lap marker on the fuel pit-window strip.",
    "strip_window": "Open pit-window segment on the fuel timeline.",
    "threshold": "Brake threshold reference line in the inputs trace.",
    "title": "Title bar text color in the {widget}.",
    "track": "Neutral track behind the standalone delta bar.",
    "value": "Large numeric readout text in the dash.",
    "warn": "Warning color (low tire wear, alerts, etc.) in the {widget}.",
    "wear": "Tire wear bar fill when wear is healthy.",
    "wind_text": "Wind speed/direction label text on the map.",
    "yellow": "Caution yellow on the proximity radar.",
}


def _widget_name(path: list) -> str:
    if path and str(path[0]) in WIDGET_NAMES:
        return WIDGET_NAMES[str(path[0])]
    if path:
        return str(path[0]).replace("_", " ").title()
    return "this widget"


def _lookup_help(dotted: str, key: str) -> str | None:
    return HELP_OVERRIDES.get(dotted) or HELP_OVERRIDES.get(key)


def _pattern_help(path: list, key: str) -> str | None:
    """Structured config paths with shared help templates."""
    if len(path) >= 2 and path[-2] == "widths":
        template = COLUMN_WIDTH_HELP.get(key)
        if template:
            return template.format(widget=_widget_name(path[:-2]))

    if len(path) >= 3 and path[-2] in ("header", "footer"):
        slot = str(path[-1])
        band = str(path[-2])
        widget = _widget_name(path[:1])
        return (
            f"Which live telemetry value appears in the {slot} {band} "
            f"of the {widget}. Choose any supported metric (SOF, lap, temps, "
            f"race time, etc.) or None to leave the slot empty."
        )

    if len(path) >= 3 and path[-2] in ("header_icons", "footer_icons"):
        slot = str(path[-1])
        band = "header" if path[-2] == "header_icons" else "footer"
        widget = _widget_name(path[:1])
        return (
            f"When on, shows a small icon instead of text in the {slot} "
            f"{band} slot of the {widget}. When off, shows the text label."
        )

    if len(path) >= 2 and path[-2] == "license_colors":
        template = LICENSE_CLASS_HELP.get(key)
        if template:
            return template.format(widget=_widget_name(path[:-2]))

    if len(path) >= 2 and path[-2] == "sizes" and path[0] == "radar":
        return RADAR_SIZE_HELP.get(key)

    return None


def _color_help(path: list, key: str) -> str | None:
    if len(path) < 2 or path[-2] != "colors":
        return None
    template = COLOR_HELP.get(key)
    if not template:
        return None
    widget = _widget_name(path[:-2] if len(path) > 2 else path[:1])
    if "{widget}" in template:
        return template.format(widget=widget, key=key.replace("_", " "))
    return template


def _bool_help(path: list, key: str, label: str) -> str:
    widget = _widget_name(path[:-1] if len(path) > 1 else path)
    low = label.lower()
    if key == "show" or key.startswith("show_"):
        feat = low.removeprefix("show ").strip() or low
        return (f"When on, shows {feat} in the {widget}. "
                f"When off, hides it.")
    return f"When on, enables {low} in the {widget}. When off, disables it."


def _number_help(path: list, key: str, label: str, default) -> str:
    widget = _widget_name(path[:-1] if len(path) > 1 else path)
    low = label.lower()
    if "frac" in key or key.endswith("_pct") or "opacity" in key:
        return (f"Controls {low} in the {widget} on a 0–1 scale. "
                f"Higher values increase the effect; lower values reduce it.")
    if "scale" in key:
        return (f"Controls {low} in the {widget}. "
                f"Higher = larger; lower = smaller.")
    if "width" in key or key.endswith("_px"):
        return (f"Controls {low} in the {widget}. "
                f"Higher = thicker or taller; lower = thinner or shorter.")
    if "seconds" in key or key.endswith("_hz") or "tau" in key:
        return (f"Controls {low} in the {widget}. "
                f"Higher = slower or longer; lower = faster or shorter.")
    if isinstance(default, float):
        return f"Controls {low} in the {widget}. Default is {default}."
    return f"Controls {low} in the {widget}. Default is {default}."


def _enum_help(path: list, key: str, label: str, default) -> str:
    widget = _widget_name(path[:-1] if len(path) > 1 else path)
    return (
        f"Selects {label.lower()} for the {widget}. "
        f"The default is {default!r}. Change this to switch what appears "
        f"or how the widget behaves."
    )


def help_for(path: list, default_val, label: str) -> str:
    """Return non-empty help text for a config leaf row."""
    if not path:
        return "Configuration option."
    dotted = ".".join(str(p) for p in path)
    key = str(path[-1])

    if key == "text_scale" and len(path) == 1:
        return HELP_OVERRIDES["text_scale_global"]

    explicit = _lookup_help(dotted, key)
    if explicit:
        return explicit

    patterned = _pattern_help(path, key)
    if patterned:
        return patterned

    color = _color_help(path, key)
    if color:
        return color

    if isinstance(default_val, bool):
        return _bool_help(path, key, label)

    if isinstance(default_val, (int, float)) and not isinstance(default_val, bool):
        return _number_help(path, key, label, default_val)

    if isinstance(default_val, str):
        return _enum_help(path, key, label, default_val)

    return f"Setting for {label.lower()} in {_widget_name(path[:-1])}."
