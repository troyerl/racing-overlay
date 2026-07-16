//! Settings schema: nav order, skip lists, labels, groups (Python `config_editor` parity).

use crate::config::WIDGET_KEYS;

/// Top-level Settings tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TopTab {
    #[default]
    Widgets,
    Settings,
}

impl TopTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Widgets => "Widgets",
            Self::Settings => "Settings",
        }
    }
}

/// Left-nav widget groups (Python `WIDGET_NAV_GROUPS`).
pub const WIDGET_NAV_GROUPS: &[(&str, &[&str])] = &[
    ("Standings", &["relative", "standings", "leaderboard_strip"]),
    (
        "Timing",
        &["laptime_log", "sector_timing", "delta_bar", "lap_compare"],
    ),
    ("Driving", &["dash", "inputs", "fuel_calc", "tire_panel"]),
    (
        "Session",
        &[
            "flags",
            "weather_panel",
            "pit_board",
            "radio_tower",
            "ers_hybrid",
            "system_panel",
            "pit_advisor",
        ],
    ),
    ("Awareness", &["map", "radar"]),
];

const TABLE_GROUPS: &[(&str, &[&str])] = &[
    (
        "Content",
        &[
            "title",
            "center_on_player",
            "pin_podium",
            "rows",
            "rows_ahead",
            "rows_behind",
            "show_footer",
            "text_scale",
        ],
    ),
    (
        "Typography",
        &[
            "font_scale",
            "gap_font_scale",
            "header_font_scale",
            "footer_font_scale",
            "name_font_bold",
            "irating_abbreviate",
            "show_irating_projection",
        ],
    ),
    (
        "Row layout",
        &[
            "row_height_px",
            "row_dividers",
            "alt_row_shading",
            "corner_radius_frac",
            "row_ease_tau",
            "fade_ease_tau",
        ],
    ),
    (
        "Header & footer",
        &["header", "footer", "header_icons", "footer_icons"],
    ),
    ("Columns", &["column_order", "columns"]),
    ("Sizing", &["widths"]),
    ("Colors", &["colors", "license_colors"]),
];

/// Purpose-based setting groups per widget section (Python `SETTING_GROUPS`).
pub fn setting_groups(section: &str) -> Vec<(&'static str, &'static [&'static str])> {
    match section {
        "relative" => vec![
            (
                "Content",
                &[
                    "center_on_player",
                    "rows_ahead",
                    "rows_behind",
                    "show_footer",
                    "text_scale",
                    "show_strategy_hints",
                    "strategy_fuel_pct_thresh",
                    "undercut_gap_max_s",
                    "cover_gap_max_s",
                ],
            ),
            TABLE_GROUPS[1],
            TABLE_GROUPS[2],
            TABLE_GROUPS[3],
            TABLE_GROUPS[4],
            TABLE_GROUPS[5],
            TABLE_GROUPS[6],
        ],
        "standings" => TABLE_GROUPS.to_vec(),
        "laptime_log" => vec![
            (
                "Content",
                &[
                    "rows",
                    "delta_mode",
                    "column_order",
                    "show_header",
                    "temp_icon",
                    "text_scale",
                ],
            ),
            (
                "Typography",
                &["font_scale", "header_font_scale", "row_dividers"],
            ),
            (
                "Row layout",
                &[
                    "row_height_px",
                    "max_row_height_frac",
                    "alt_row_shading",
                    "corner_radius_frac",
                ],
            ),
            ("Colors", &["colors"]),
        ],
        "fuel_calc" => vec![
            (
                "Visibility",
                &[
                    "show_title",
                    "show_pill",
                    "show_add",
                    "show_gauge",
                    "show_stats",
                    "show_strip",
                    "show_time",
                    "show_laps",
                    "show_live_burn",
                    "show_tank_pct",
                    "show_low_fuel_alert",
                    "show_pit_compare",
                ],
            ),
            (
                "Content",
                &[
                    "title",
                    "pit_loss_seconds",
                    "low_fuel_laps_threshold",
                    "low_fuel_time_threshold",
                ],
            ),
            (
                "Row layout",
                &[
                    "row_height_px",
                    "max_row_height_frac",
                    "stats_header_font_scale",
                    "stats_row_font_scale",
                    "corner_radius_frac",
                    "row_dividers",
                ],
            ),
            ("Colors", &["colors"]),
        ],
        "radar" => vec![
            (
                "Behavior",
                &[
                    "range_pct",
                    "show_front",
                    "show_rear",
                    "side_span_pct",
                    "side_proximity_color",
                    "show_side_labels",
                    "show_clear_timer",
                    "alongside_zone_pct",
                    "ease_side_tau",
                    "ease_glow_tau",
                ],
            ),
            (
                "Display",
                &["show_nose", "show_axis", "show_panel", "text_scale"],
            ),
            ("Layout", &["corner_radius_frac", "sizes"]),
            ("Colors", &["colors"]),
        ],
        "dash" => vec![
            (
                "Layout",
                &[
                    "corner_radius_frac",
                    "shift_segments",
                    "shift_red_frac",
                    "shift_yellow_frac",
                    "ring_segments",
                    "text_scale",
                ],
            ),
            ("Shift bar", &["show_shift_bar"]),
            (
                "Center medallion",
                &[
                    "center_mode",
                    "show_ring",
                    "show_throttle",
                    "show_brake",
                    "show_clutch",
                ],
            ),
            ("Flags", &["show_flags"]),
            ("Delta bar", &["show_delta_bar", "delta_bar_range"]),
            (
                "Metrics & slots",
                &[
                    "show_position",
                    "top_right",
                    "primary_left",
                    "primary_right",
                    "stat_left",
                    "stat_right",
                    "strip_left",
                    "strip_center",
                    "strip_right",
                ],
            ),
            (
                "iRating",
                &["irating_abbreviate", "show_irating_projection"],
            ),
            ("Colors", &["colors"]),
        ],
        "inputs" => vec![
            (
                "Visibility",
                &[
                    "show_label",
                    "show_graph",
                    "show_bars",
                    "show_gauge",
                    "label_text",
                ],
            ),
            (
                "Trace",
                &[
                    "history_seconds",
                    "show_throttle",
                    "show_brake",
                    "show_clutch",
                    "show_steering",
                    "show_shift_markers",
                    "show_brake_threshold",
                    "brake_threshold",
                    "line_width",
                ],
            ),
            ("Colors", &["colors"]),
        ],
        "delta_bar" => vec![
            ("Behavior", &["mode", "range", "show_value"]),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "flags" => vec![
            (
                "Content",
                &[
                    "idle_text",
                    "show_incident_warning",
                    "incident_warn_pct",
                    "show_blue_detail",
                    "show_pit_limiter",
                    "show_finish_position",
                ],
            ),
            ("Colors", &["colors"]),
        ],
        "lap_compare" => vec![
            ("Content", &["max_turns", "show_graph"]),
            ("Row layout", &["alt_row_shading"]),
            ("Colors", &["colors"]),
        ],
        "sector_timing" => vec![
            (
                "Content",
                &["show_sector_delta", "show_predicted_lap", "text_scale"],
            ),
            ("Row layout", &["row_height_px", "max_row_height_frac"]),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "tire_panel" => vec![
            (
                "Content",
                &[
                    "show_title",
                    "title",
                    "show_wear",
                    "show_temp",
                    "show_pressure",
                    "warn_wear_pct",
                ],
            ),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "pit_board" => vec![
            (
                "Content",
                &[
                    "show_title",
                    "title",
                    "show_pit_banner",
                    "pit_banner_text",
                    "show_fast_repairs",
                    "show_compound",
                ],
            ),
            ("Row layout", &["row_height_px", "max_row_height_frac"]),
            ("Layout", &["corner_radius_frac", "row_dividers"]),
            ("Colors", &["colors"]),
        ],
        "weather_panel" => vec![
            (
                "Content",
                &[
                    "show_title",
                    "title",
                    "show_skies",
                    "show_rain",
                    "show_temps",
                    "show_wind",
                ],
            ),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "leaderboard_strip" => vec![
            (
                "Content",
                &[
                    "rows",
                    "show_position",
                    "show_car_number",
                    "show_lap",
                    "show_mph",
                    "show_name",
                    "show_gap",
                    "highlight_player",
                ],
            ),
            ("Row layout", &["row_height_px", "max_row_height_frac"]),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "radio_tower" => vec![
            (
                "Content",
                &[
                    "show_title",
                    "title",
                    "show_position",
                    "show_car_number",
                    "show_name",
                    "highlight_player",
                    "text_scale",
                ],
            ),
            ("Row layout", &["row_height_px"]),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "system_panel" => vec![
            (
                "Content",
                &[
                    "show_title",
                    "title",
                    "show_cpu",
                    "show_mem",
                    "show_gpu",
                    "show_fps",
                    "show_network",
                ],
            ),
            ("Row layout", &["row_height_px"]),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "pit_advisor" => vec![
            (
                "Content",
                &[
                    "show_title",
                    "title",
                    "show_only_when_actionable",
                    "low_fuel_laps_threshold",
                    "undercut_gap_max_s",
                    "cover_gap_max_s",
                    "pit_loss_seconds",
                ],
            ),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "ers_hybrid" => vec![
            (
                "Content",
                &[
                    "show_title",
                    "title",
                    "label_battery",
                    "label_lap",
                    "label_boost",
                    "label_p2p",
                    "empty_text",
                    "show_battery",
                    "show_lap_energy",
                    "show_boost",
                    "show_p2p",
                ],
            ),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "map" => vec![
            (
                "Display",
                &[
                    "show_infield",
                    "show_corners",
                    "show_start_finish",
                    "show_wind",
                    "show_expanded_weather",
                    "show_car_status",
                    "show_drs_zones",
                    "show_p2p_zones",
                    "show_panel",
                    "show_pace_safety_line",
                    "show_sector_boundaries",
                    "show_traffic_markers",
                ],
            ),
            (
                "Traffic & markers",
                &[
                    "marker_hold_seconds",
                    "car_label",
                    "dot_radius_frac",
                    "other_dot_radius_frac",
                ],
            ),
            (
                "Pit lane",
                &[
                    "show_pit",
                    "show_pit_blends",
                    "show_pit_speed",
                    "pit_lane_opacity",
                    "pit_dot_opacity",
                ],
            ),
            (
                "Layout",
                &[
                    "rotation",
                    "mirror",
                    "asphalt_width",
                    "outline_width",
                    "corner_radius_frac",
                    "text_scale",
                ],
            ),
            ("Colors", &["colors"]),
        ],
        _ => vec![],
    }
}

pub fn group_default_open(group_title: &str) -> bool {
    !matches!(group_title, "Colors" | "Sizing" | "Row layout")
}

/// Per-widget accent (Python `TAB_COLORS`).
pub fn tab_color(section: &str) -> &'static str {
    match section {
        "__general__" | "__app__" | "__scan__" => "#9aa3b2",
        "__widgets__" => "#9aa3b2",
        "relative" => "#2fe0b0",
        "standings" => "#a98bff",
        "laptime_log" => "#ffd23a",
        "fuel_calc" => "#ff9416",
        "radar" => "#ff5b5b",
        "dash" => "#46df7a",
        "inputs" => "#28cfe0",
        "delta_bar" => "#9ee84b",
        "flags" => "#ff7ec2",
        "lap_compare" => "#ffb43a",
        "sector_timing" => "#e07bff",
        "map" => "#5aa9ff",
        "tire_panel" => "#ff9416",
        "pit_board" => "#ffd23a",
        "weather_panel" => "#5aa9ff",
        "leaderboard_strip" => "#a98bff",
        "radio_tower" => "#9aa3b2",
        "ers_hybrid" => "#46df7a",
        "system_panel" => "#9aa3b2",
        "pit_advisor" => "#46df7a",
        _ => "#9aa3b2",
    }
}

pub fn top_tab_for(section: &str) -> TopTab {
    match section {
        "__general__" | "__app__" | "__scan__" => TopTab::Settings,
        _ => TopTab::Widgets,
    }
}

/// Keys hidden from Settings (still kept in DEFAULTS / merges).
pub fn section_skip(section: &str) -> &'static [&'static str] {
    match section {
        "relative" => &[
            "irating_show_icon",
            "max_row_height_frac",
            "data_font_bold",
            "pit_loss_seconds",
        ],
        "standings" => &["irating_show_icon", "max_row_height_frac", "data_font_bold"],
        "laptime_log" => &["data_font_bold"],
        "fuel_calc" => &[
            "show_stints",
            "stint_laps",
            "legal_fuel_buffer_l",
            "data_font_bold",
            "text_scale",
        ],
        "radar" => &[
            "row_dividers",
            "closing_rate_color",
            "closing_rate_full",
            "data_font_bold",
        ],
        "dash" => &[
            "flag_pulse",
            "flag_pulse_seconds",
            "flag_blink_hz",
            "flag_green_seconds",
            "delta_bar_mode",
            "row_dividers",
            "data_font_bold",
        ],
        "inputs" => &[
            "row_dividers",
            "show_handbrake",
            "show_steering_torque",
            "show_tc_abs",
            "data_font_bold",
            "text_scale",
        ],
        "delta_bar" => &["row_dividers", "data_font_bold", "text_scale"],
        "flags" => &["row_dividers", "data_font_bold", "text_scale"],
        "lap_compare" => &[
            "min_time_loss",
            "show_live_delta",
            "show_gear_rpm",
            "exclude_wet_laps",
            "wetness_delta_threshold",
            "row_height_px",
            "max_row_height_frac",
            "row_dividers",
            "data_font_bold",
            "text_scale",
        ],
        "sector_timing" => &["row_dividers", "data_font_bold"],
        "map" => &[
            "auto_corners",
            "row_dividers",
            "data_font_bold",
            "palette",
            "lap_proximity_pct",
            "show_pace_car",
        ],
        "tire_panel" => &["row_dividers", "data_font_bold", "text_scale"],
        "pit_board" => &["show_pressures", "data_font_bold", "text_scale"],
        "weather_panel" => &[
            "show_trend",
            "trend_window_seconds",
            "row_height_px",
            "max_row_height_frac",
            "row_dividers",
            "data_font_bold",
            "text_scale",
        ],
        "leaderboard_strip" => &[
            "show_class_color",
            "data_font_bold",
            "text_scale",
            "row_dividers",
        ],
        "radio_tower" => &["row_dividers", "max_row_height_frac", "data_font_bold"],
        "ers_hybrid" => &["row_dividers", "data_font_bold", "text_scale"],
        "system_panel" => &[
            "show_icons",
            "text_scale",
            "max_row_height_frac",
            "row_dividers",
            "data_font_bold",
        ],
        "pit_advisor" => &[
            "row_dividers",
            "race_tire_sets_total",
            "tire_sets_reserve",
            "min_stint_laps",
            "legal_fuel_buffer_l",
            "caution_fuel_multiplier",
            "track_wetness_tire_suppress",
            "opponent_stint_due_laps",
            "opponent_splash_pit_max_s",
            "final_laps_optional_suppress",
            "show_field_context",
            "show_tire_inventory",
            "text_scale",
            "data_font_bold",
        ],
        _ => &[],
    }
}

pub fn is_skipped(section: &str, key: &str) -> bool {
    section_skip(section).contains(&key)
}

/// Nav pages: (section_key, title, group_or_empty).
pub fn ordered_sections() -> Vec<(String, String, String)> {
    let mut out = vec![
        ("__general__".into(), "General".into(), String::new()),
        ("__app__".into(), "App".into(), String::new()),
    ];
    if crate::cloud::can_write() {
        out.push(("__scan__".into(), "Track Scan".into(), String::new()));
    }
    let mut seen = std::collections::HashSet::new();
    for (group, keys) in WIDGET_NAV_GROUPS {
        for key in *keys {
            if WIDGET_KEYS.contains(key) && seen.insert(*key) {
                out.push(((*key).into(), pretty_key(key), (*group).into()));
            }
        }
    }
    for key in WIDGET_KEYS {
        if seen.insert(*key) {
            out.push(((*key).into(), pretty_key(key), "Other".into()));
        }
    }
    out
}

pub fn nav_for_tab(tab: TopTab) -> Vec<(String, String, String)> {
    ordered_sections()
        .into_iter()
        .filter(|(key, _, _)| top_tab_for(key) == tab)
        .collect()
}

pub fn pretty_key(key: &str) -> String {
    key.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Header/footer slot keys for Relative (Python `_SLOT_COMMON`).
pub const TABLE_SLOT_COMMON: &[&str] = &[
    "none",
    "sof",
    "class_sof",
    "position",
    "class_position",
    "session_time",
    "race_time",
    "lap",
    "incidents",
    "track_name",
    "track_temp",
    "air_temp",
    "best_lap",
    "my_session_best",
    "session_best",
    "local_time",
    "sim_time",
    "cpu",
    "mem",
    "gpu",
    "laps_remain",
    "incident_limit",
    "fast_repairs",
    "weather",
    "track_wetness",
];

/// Standings adds order_pill / title / count (Python `_SLOT_STANDINGS`).
pub const TABLE_SLOT_STANDINGS: &[&str] = &[
    "none",
    "sof",
    "class_sof",
    "position",
    "class_position",
    "session_time",
    "race_time",
    "lap",
    "incidents",
    "track_name",
    "track_temp",
    "air_temp",
    "best_lap",
    "my_session_best",
    "session_best",
    "local_time",
    "sim_time",
    "cpu",
    "mem",
    "gpu",
    "laps_remain",
    "incident_limit",
    "fast_repairs",
    "weather",
    "track_wetness",
    "order_pill",
    "title",
    "count",
];

pub fn table_slot_options(section: &str) -> &'static [&'static str] {
    if section == "standings" {
        TABLE_SLOT_STANDINGS
    } else {
        TABLE_SLOT_COMMON
    }
}

pub fn matches_search(section: &str, key: &str, query: &str) -> bool {
    if query.trim().is_empty() {
        return true;
    }
    let q = query.to_ascii_lowercase();
    pretty_key(key).to_ascii_lowercase().contains(&q)
        || key.to_ascii_lowercase().contains(&q)
        || section.to_ascii_lowercase().contains(&q)
        || pretty_key(section).to_ascii_lowercase().contains(&q)
}

/// Short help for common keys (subset of Python `setting_help`).
pub fn help_text(section: &str, key: &str) -> Option<&'static str> {
    match (section, key) {
        ("__general__", "units") => Some("Metric or imperial display units."),
        ("__general__", "text_scale") => Some("Global UI text scale multiplier."),
        ("__general__", "start_overlay_on_launch") => {
            Some("Show overlay panels when the app starts.")
        }
        ("__app__", "start_overlay_on_launch") => Some("Show overlay panels when the app starts."),
        ("__app__", "start_at_login") => {
            Some("Launch GridGlance automatically when you sign in to Windows.")
        }
        ("__app__", "check_updates_on_launch") => {
            Some("Silently check GitHub for a newer release when the app starts.")
        }
        (_, "show") => Some("Show this panel on the overlay."),
        (_, "text_scale") => Some("Per-panel text scale (multiplies global)."),
        (_, "show_panel") => Some("Draw the card background behind this panel."),
        (_, "rows_ahead") => Some("How many cars to list ahead of you."),
        (_, "rows_behind") => Some("How many cars to list behind you."),
        (_, "center_on_player") => Some("Keep your row centered in the table."),
        _ => None,
    }
}
