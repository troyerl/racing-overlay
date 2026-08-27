//! Settings schema: nav order, skip lists, labels, groups (Python `config_editor` parity).

use crate::config::WIDGET_KEYS;
use serde_json::Value;
use std::collections::HashMap;

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
            "pace_caution",
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
            "grow",
            "rows_ahead",
            "rows_behind",
            "show_footer",
            "pit_mode",
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
            "show_sr_projection",
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
                    "rows",
                    "rows_ahead",
                    "rows_behind",
                    "show_footer",
                    "pit_mode",
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
                    "panel_style",
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
                    "panel_style",
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
                    "history_laps",
                    "green_history_laps",
                    "fuel_ema_alpha",
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
                    "panel_style",
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
            (
                "Shift bar",
                &[
                    "show_shift_bar",
                    "shift_blink",
                    "shift_blink_hz",
                    "shift_blink_pct",
                    "shift_blink_max_sec",
                ],
            ),
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
            (
                "Flags",
                &[
                    "show_flags",
                    "start_go_text",
                    "start_set_text",
                    "start_ready_text",
                ],
            ),
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
            ("Safety Rating", &["show_sr_projection"]),
            ("Colors", &["colors"]),
        ],
        "inputs" => vec![
            (
                "Visibility",
                &[
                    "panel_style",
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
            ("Behavior", &["panel_style", "mode", "range", "show_value"]),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "flags" => vec![
            (
                "Content",
                &[
                    "panel_style",
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
            (
                "Content",
                &[
                    "panel_style",
                    "reference_mode",
                    "review_top3",
                    "max_turns",
                    "show_graph",
                    "show_steer_delta",
                    "show_top3",
                    "show_brake_markers",
                    "show_lift_markers",
                ],
            ),
            ("Row layout", &["alt_row_shading"]),
            ("Colors", &["colors"]),
        ],
        "sector_timing" => vec![
            (
                "Content",
                &[
                    "panel_style",
                    "sectors",
                    "show_sector_delta",
                    "show_predicted_lap",
                    "highlight_active_sector_on_map",
                    "text_scale",
                ],
            ),
            ("Row layout", &["row_height_px", "max_row_height_frac"]),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "tire_panel" => vec![
            (
                "Content",
                &[
                    "panel_style",
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
                    "panel_style",
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
                    "panel_style",
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
                    "panel_style",
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
                    "panel_style",
                    "show_title",
                    "title",
                    "show_position",
                    "show_car_number",
                    "show_name",
                    "show_country",
                    "highlight_player",
                    "text_scale",
                ],
            ),
            ("Row layout", &["row_height_px"]),
            ("Layout", &["corner_radius_frac", "panel_opacity"]),
            ("Colors", &["colors"]),
        ],
        "system_panel" => vec![
            (
                "Content",
                &["panel_style", "show_title", "title", "show_icons"],
            ),
            (
                "Metrics",
                &[
                    "show_cpu",
                    "show_mem",
                    "show_gpu",
                    "show_fps",
                    "show_network",
                    "show_ffb",
                ],
            ),
            (
                "Subtasks",
                &[
                    "show_process_breakdown",
                    "show_subtask_iracing",
                    "show_subtask_overlays",
                    "show_subtask_music",
                ],
            ),
            ("Row layout", &["row_height_px"]),
            ("Layout", &["corner_radius_frac", "panel_opacity"]),
            ("Colors", &["colors"]),
        ],
        "pace_caution" => vec![
            (
                "Content",
                &["panel_style", "show_title", "title", "show_delta"],
            ),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "pit_advisor" => vec![
            (
                "Content",
                &[
                    "panel_style",
                    "show_title",
                    "title",
                    "show_only_when_actionable",
                    "show_field_context",
                    "show_tire_inventory",
                    "final_laps_optional_suppress",
                    "low_fuel_laps_threshold",
                    "legal_fuel_buffer_l",
                    "min_stint_laps",
                    "undercut_gap_max_s",
                    "cover_gap_max_s",
                    "pit_loss_seconds",
                    "caution_pit_loss_factor",
                    "caution_fuel_multiplier",
                    "opponent_stint_due_laps",
                    "opponent_splash_pit_max_s",
                    "track_wetness_tire_suppress",
                    "race_tire_sets_total",
                    "tire_sets_reserve",
                    "fuel_mass_laptime_s_per_l",
                    "fuel_fill_rate_lps",
                    "tire_change_4t_s",
                    "tire_change_2t_s",
                    "pace_window_laps",
                    "fcy_horizon_laps",
                ],
            ),
            ("Layout", &["corner_radius_frac"]),
            ("Colors", &["colors"]),
        ],
        "ers_hybrid" => vec![
            (
                "Content",
                &[
                    "panel_style",
                    "show_title",
                    "title",
                    "label_battery",
                    "label_boost",
                    "label_p2p",
                    "empty_text",
                    "show_battery",
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
                    "panel_style",
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
                    "reverse_path",
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
    !matches!(group_title, "Colors" | "Row layout")
}

fn flag(values: &HashMap<String, Value>, key: &str, default: bool) -> bool {
    values.get(key).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// Hide a setting until its parent toggle (or mode) makes it applicable.
pub fn setting_visible(section: &str, key: &str, values: &HashMap<String, Value>) -> bool {
    if key == "title" && values.contains_key("show_title") {
        return flag(values, "show_title", true);
    }
    match (section, key) {
        ("relative" | "standings", "rows") => !flag(values, "center_on_player", true),
        ("relative" | "standings", "rows_ahead" | "rows_behind") => {
            flag(values, "center_on_player", true)
        }
        ("standings", "pin_podium") => flag(values, "center_on_player", true),
        ("relative", "strategy_fuel_pct_thresh" | "undercut_gap_max_s" | "cover_gap_max_s") => {
            flag(values, "show_strategy_hints", true)
        }
        ("relative" | "standings", "footer_font_scale" | "footer" | "footer_icons") => {
            flag(values, "show_footer", true)
        }
        ("fuel_calc", "low_fuel_laps_threshold" | "low_fuel_time_threshold") => {
            flag(values, "show_low_fuel_alert", true)
        }
        ("dash", "shift_blink") => flag(values, "show_shift_bar", true),
        ("dash", "shift_blink_hz" | "shift_blink_pct" | "shift_blink_max_sec") => {
            flag(values, "show_shift_bar", true) && flag(values, "shift_blink", false)
        }
        ("dash", "start_go_text" | "start_set_text" | "start_ready_text") => {
            flag(values, "show_flags", true)
        }
        ("dash", "delta_bar_range") => flag(values, "show_delta_bar", false),
        ("inputs", "brake_threshold") => flag(values, "show_brake_threshold", false),
        ("inputs", "label_text") => flag(values, "show_label", true),
        ("flags", "incident_warn_pct") => flag(values, "show_incident_warning", true),
        ("pit_board", "pit_banner_text") => flag(values, "show_pit_banner", true),
        ("tire_panel", "warn_wear_pct") => flag(values, "show_wear", true),
        (
            "system_panel",
            "show_subtask_iracing" | "show_subtask_overlays" | "show_subtask_music",
        ) => flag(values, "show_process_breakdown", true),
        ("ers_hybrid", "label_battery") => flag(values, "show_battery", true),
        ("ers_hybrid", "label_boost") => flag(values, "show_boost", true),
        ("ers_hybrid", "label_p2p") => flag(values, "show_p2p", true),
        ("map", "show_expanded_weather") => flag(values, "show_wind", true),
        ("map", "show_pit_blends" | "show_pit_speed" | "pit_lane_opacity" | "pit_dot_opacity") => {
            flag(values, "show_pit", true)
        }
        ("map", "marker_hold_seconds") => flag(values, "show_traffic_markers", true),
        _ => true,
    }
}

/// Laptime log columns (toggle / order in Settings).
pub const LAPLOG_COLUMNS: &[&str] = &[
    "lap",
    "time",
    "delta",
    "temp",
    "sectors",
    "fuel",
    "tires",
    "incidents",
    "tag",
];

/// Data columns Relative / Standings can show (order = default insertion order).
pub const TABLE_DATA_COLUMNS: &[&str] = &[
    "badge",
    "position",
    "car_number",
    "country",
    "name",
    "license",
    "irating",
    "gap",
    "gap_ahead",
    "gap_leader",
    "pit",
    "last_lap",
    "best_lap",
    "class_pos",
    "status",
    "car_flag",
    "laps",
    "closing",
    "team",
    "nickname",
];

/// Default width multiplier (× row height) for a table column. `name` is flex.
pub fn default_table_col_width(col: &str) -> f32 {
    match col {
        "badge" => 0.95,
        "position" => 1.25,
        "car_number" => 1.60,
        "gap" | "gap_ahead" | "gap_leader" => 1.70,
        "irating" => 1.20,
        "license" => 1.35,
        "pit" => 2.10,
        "last_lap" | "best_lap" => 2.90,
        "country" => 1.15,
        "class_pos" | "status" | "car_flag" | "laps" => 1.35,
        "closing" => 1.80,
        "team" | "nickname" => 2.20,
        "gutter" => 0.12,
        _ => 1.2,
    }
}

/// Per-widget accent (Python `TAB_COLORS`).
pub fn tab_color(section: &str) -> &'static str {
    match section {
        "__general__" | "__app__" | "__drivers__" | "__lan__" | "__laps__" | "__scan__" => {
            "#9aa3b2"
        }
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
        "pace_caution" => "#ffd23a",
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
        "__general__" | "__app__" | "__drivers__" | "__lan__" | "__laps__" | "__scan__" => {
            TopTab::Settings
        }
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
            "title",
            "show_title",
        ],
        "standings" => &[
            "irating_show_icon",
            "max_row_height_frac",
            "data_font_bold",
            "show_title",
        ],
        "dash" => &[
            "flag_pulse",
            "flag_pulse_seconds",
            "flag_blink_hz",
            "flag_green_seconds",
            "delta_bar_mode",
            "row_dividers",
            "data_font_bold",
            "irating_show_icon",
            "title",
            "show_title",
        ],
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
        "inputs" => &[
            "row_dividers",
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
        "pace_caution" => &["row_dividers", "data_font_bold", "text_scale"],
        "leaderboard_strip" => &[
            "show_class_color",
            "data_font_bold",
            "text_scale",
            "row_dividers",
        ],
        "radio_tower" => &["row_dividers", "max_row_height_frac", "data_font_bold"],
        "ers_hybrid" => &["row_dividers", "data_font_bold", "text_scale"],
        "system_panel" => &[
            "text_scale",
            "max_row_height_frac",
            "row_dividers",
            "data_font_bold",
        ],
        "pit_advisor" => &["row_dividers", "text_scale", "data_font_bold"],
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
        ("__drivers__".into(), "Drivers".into(), String::new()),
        ("__lan__".into(), "LAN telemetry".into(), String::new()),
        ("__laps__".into(), "Race laps".into(), String::new()),
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
    match key {
        "show_cpu" => return "CPU".into(),
        "show_mem" => return "Memory".into(),
        "show_gpu" => return "GPU".into(),
        "show_fps" => return "FPS".into(),
        "show_network" => return "Network".into(),
        "show_ffb" => return "FFB".into(),
        "show_process_breakdown" => return "Show subtasks".into(),
        "show_subtask_iracing" => return "iRacing".into(),
        "show_subtask_overlays" => return "Overlays".into(),
        "show_subtask_music" => return "Music".into(),
        "show_icons" => return "Icons".into(),
        "car_label" => return "Dot number".into(),
        "grow" => return "Grow with field".into(),
        _ => {}
    }
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
    "session_type",
    "race_split",
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
    "session_type",
    "race_split",
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

/// Friendly label for a stored choice value (snake_case → title case, plus overrides).
pub fn choice_label(value: &str) -> String {
    match value {
        "none" => "None".into(),
        "sof" => "SOF".into(),
        "class_sof" => "Class SOF".into(),
        "cpu" => "CPU".into(),
        "mem" => "Memory".into(),
        "gpu" => "GPU".into(),
        "order_pill" => "Order pill".into(),
        "metric" => "Metric".into(),
        "imperial" => "Imperial".into(),
        "data" => "Data".into(),
        "elegant" => "Elegant".into(),
        "number" => "Car number".into(),
        "position" => "Position".into(),
        "laps_since" => "Laps since pit".into(),
        "time_since" => "Time since pit".into(),
        "at_lap" => "Pit lap number".into(),
        "at_time" => "Pit clock time".into(),
        "ring" => "Input ring".into(),
        "pedals" => "Pedals".into(),
        "session_best" => "Session best".into(),
        "best_lap" => "Best lap".into(),
        "optimal" => "Optimal".into(),
        "last_lap" => "Last lap".into(),
        "leader_last" => "Leader last lap".into(),
        "previous" => "Previous lap".into(),
        "best" => "Best lap".into(),
        "personal_best" => "Personal best".into(),
        "entry" => "Entry".into(),
        "road" => "Road".into(),
        "merge" => "Merge".into(),
        "1" => "Lane 1".into(),
        "2" => "Lane 2".into(),
        "lap_count" => "Lap count".into(),
        "laps_left" => "Laps left".into(),
        "car_number" => "Car number".into(),
        "car_flag" => "Session flag".into(),
        "country" => "Country".into(),
        "fuel_laps" => "Fuel laps".into(),
        "fuel_stack" => "Fuel stack".into(),
        "cur_lap" => "Current lap".into(),
        "air_temp" => "Air temp".into(),
        "track_temp" => "Track temp".into(),
        "class_position" => "Class position".into(),
        "session_time" => "Session time".into(),
        "race_time" => "Race time".into(),
        "track_name" => "Track name".into(),
        "my_session_best" => "My session best".into(),
        "local_time" => "Local time".into(),
        "sim_time" => "Sim time".into(),
        "laps_remain" => "Laps remaining".into(),
        "incident_limit" => "Incident limit".into(),
        "fast_repairs" => "Fast repairs".into(),
        "track_wetness" => "Track wetness".into(),
        "session_type" => "Session type".into(),
        "race_split" => "Race split".into(),
        _ => pretty_key(value),
    }
}

/// Keys that may be typed freely (panel titles / display labels). Everything else is a dropdown.
pub fn allows_free_text_setting(key: &str) -> bool {
    matches!(
        key,
        "title"
            | "label_text"
            | "idle_text"
            | "empty_text"
            | "active_text"
            | "start_go_text"
            | "start_set_text"
            | "start_ready_text"
            | "label_battery"
            | "label_boost"
            | "label_p2p"
    ) || key.starts_with("label_")
}

/// Known enum string settings → (stored value, friendly label).
pub fn string_choices(section: &str, key: &str) -> Option<&'static [(&'static str, &'static str)]> {
    match (section, key) {
        (_, "panel_style") => Some(&[("data", "Data"), ("elegant", "Elegant")]),
        ("map", "car_label") => Some(&[("number", "Car number"), ("position", "Position")]),
        ("relative" | "standings", "pit_mode") => Some(&[
            ("laps_since", "Laps since pit"),
            ("time_since", "Time since pit"),
            ("at_lap", "Pit lap number"),
            ("at_time", "Pit clock time"),
        ]),
        ("dash", "center_mode") => Some(&[("ring", "Input ring"), ("pedals", "Pedals")]),
        ("delta_bar", "mode") | ("dash", "delta_bar_mode") => Some(&[
            ("session_best", "Session best"),
            ("best_lap", "Best lap"),
            ("optimal", "Optimal"),
            ("last_lap", "Last lap"),
            ("leader_last", "Leader last lap"),
        ]),
        ("laptime_log", "delta_mode") => Some(&[
            ("previous", "Previous lap"),
            ("best", "Best lap"),
            ("personal_best", "Personal best"),
        ]),
        ("lap_compare", "reference_mode") => Some(&[
            ("race_best", "Race / session best"),
            ("track_pb", "Track personal best"),
            ("best", "My best (session)"),
            ("last", "My last lap"),
        ]),
        ("lap_compare", "review_top3") => Some(&[
            ("-1", "Live lap"),
            ("0", "My #1 lap"),
            ("1", "My #2 lap"),
            ("2", "My #3 lap"),
        ]),
        (
            "dash",
            "top_left" | "top_right" | "primary_left" | "primary_right" | "stat_left"
            | "stat_right" | "strip_left" | "strip_center" | "strip_right",
        ) => Some(DASH_SLOT_CHOICES),
        _ => None,
    }
}

pub const DASH_SLOT_CHOICES: &[(&str, &str)] = &[
    ("none", "None"),
    ("speed", "Speed"),
    ("rpm", "RPM"),
    ("gear", "Gear"),
    ("position", "Position"),
    ("car_number", "Car number"),
    ("lap_count", "Lap count"),
    ("laps_left", "Laps left"),
    ("lap", "Lap"),
    ("fuel", "Fuel"),
    ("fuel_laps", "Fuel laps"),
    ("fuel_stack", "Fuel stack"),
    ("tires", "Tires"),
    ("incidents", "Incidents"),
    ("last_lap", "Last lap"),
    ("best_lap", "Best lap"),
    ("cur_lap", "Current lap"),
    ("delta", "Delta"),
    ("irating", "iRating"),
    ("license", "License / SR"),
    ("air_temp", "Air temp"),
    ("track_temp", "Track temp"),
];

pub const UNITS_CHOICES: &[(&str, &str)] = &[("metric", "Metric"), ("imperial", "Imperial")];

pub const PIT_PHASE_CHOICES: &[(&str, &str)] =
    &[("entry", "Entry"), ("road", "Road"), ("merge", "Merge")];

pub const PIT_LANE_CHOICES: &[(&str, &str)] = &[("1", "Lane 1"), ("2", "Lane 2")];

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
        ("relative" | "standings", "column_order") => {
            Some("Toggle columns and reorder. Order is left-to-right on the table.")
        }
        ("laptime_log", "column_order") => {
            Some("Toggle and reorder lap log columns (left-to-right).")
        }
        ("relative" | "standings", "columns") => {
            Some("Extra column options (class color stripe beside position).")
        }
        ("radar", "sizes") => Some("Relative sizes of radar car, bars, and glow."),
        ("relative" | "standings", "widths") => Some(
            "Width of each visible column as a multiple of row height. Name always fills leftover space.",
        ),
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
        ("__app__", "close_settings_to_tray") => Some(
            "Closing the Settings window hides it to the system tray instead of quitting the app. Use Quit to exit fully.",
        ),
        ("__lan__" | "__app__", "lan_telemetry_enabled") => Some(
            "Start a read-only LAN telemetry server so other devices on your Wi‑Fi can subscribe to live iRacing state. Requires ipc_token on every request. Windows may prompt for Firewall access the first time.",
        ),
        ("__lan__" | "__app__", "lan_telemetry_port") => Some(
            "TCP port for the LAN telemetry API (default 19848). Listens on all interfaces when enabled. Separate from localhost control IPC (19847).",
        ),
        ("__lan__" | "__app__", "lan_telemetry_hz") => Some(
            "How often subscribed clients receive telemetry push frames (5–30 Hz).",
        ),
        ("__laps__" | "__lan__" | "__app__", "upload_race_laps") => Some(
            "Off by default. When enabled and a Mongo write URI is set, upload race best / your top-3 / track PB once after the race finishes (checkered / cooldown). Laps still save locally during the session.",
        ),
        ("lap_compare", "reference_mode") => Some(
            "Race/session best compares you to the fastest lap in this session. Track PB compares to your all-time best on this track and car.",
        ),
        ("lap_compare", "review_top3") => Some(
            "Review one of your three fastest laps from this race instead of the live lap.",
        ),
        ("lap_compare", "show_steer_delta") => {
            Some("Show steering difference vs the reference along the lap.")
        }
        ("lap_compare", "show_top3") => {
            Some("List your three fastest laps and their gap to the reference / track PB.")
        }
        (_, "show") => Some("Show this panel on the overlay."),
        (_, "text_scale") => Some("Per-panel text scale (multiplies global)."),
        (_, "show_panel") => Some("Draw the card background behind this panel."),
        (_, "show_sr_projection") => Some(
            "Estimate Safety Rating change from corners completed and incidents this session (approximate — iRacing's exact history is private).",
        ),
        (_, "show_irating_projection") => {
            Some("Show projected iRating change from the current race order.")
        }
        (_, "panel_style") => Some(
            "Data: dense telemetry layout. Elegant: softer minimal visual layout.",
        ),
        ("map", "car_label") => {
            Some("Number on each car dot (and traffic-marker pills): car number or race position. Shared for on-track and garage profiles.")
        },
        ("relative", "rows") => Some(
            "Total relative rows including you. Rows ahead + rows behind = total − 1.",
        ),
        ("standings", "rows") => Some(
            "Maximum standings rows. With Grow on, the panel height matches the field up to this cap.",
        ),
        ("standings", "grow") => Some(
            "Grow the panel from the bottom to fit the cars on track (up to Total rows). Off pads empty slots to a fixed height.",
        ),
        ("relative", "rows_ahead") => Some(
            "Cars ahead of you. With rows behind, must equal total rows minus your row.",
        ),
        ("relative", "rows_behind") => Some(
            "Cars behind you. With rows ahead, must equal total rows minus your row.",
        ),
        ("standings", "rows_ahead") => {
            Some("Preferred cars ahead when centering on you.")
        }
        ("standings", "rows_behind") => {
            Some("Preferred cars behind when centering on you.")
        }
        (_, "center_on_player") => Some("Keep your row centered in the table."),
        ("standings", "pin_podium") => Some(
            "Always keep P1–P3 at the top, then your surrounding cars below a separator.",
        ),
        ("pace_caution", "show_delta") => {
            Some("Show You−Pace (ΔP) and You−Pit limit (ΔL) columns.")
        }
        ("radio_tower", "show_country") => {
            Some("Show the speaker's country flag (from iRacing club region).")
        }
        ("dash", "start_go_text") => Some("Title on the dash flag bar for StartGo (green light)."),
        ("dash", "start_set_text") => Some("Title on the dash flag bar for StartSet."),
        ("dash", "start_ready_text") => Some("Title on the dash flag bar for StartReady."),
        ("system_panel", "show_cpu") => Some("Show CPU usage."),
        ("system_panel", "show_mem") => Some("Show memory usage."),
        ("system_panel", "show_gpu") => Some("Show GPU usage."),
        ("system_panel", "show_fps") => Some("Show iRacing FPS."),
        ("system_panel", "show_network") => Some("Show connection / channel quality."),
        ("system_panel", "show_ffb") => Some("Show force-feedback torque (%). Warns above 100%."),
        ("system_panel", "show_process_breakdown") => Some(
            "Under CPU, memory, and GPU, show selected process subtasks.",
        ),
        ("system_panel", "show_subtask_iracing") => {
            Some("Show iRacing process usage under CPU, memory, and GPU.")
        }
        ("system_panel", "show_subtask_overlays") => Some(
            "Show GridGlance, RaceLab, and iOverlay process usage under CPU, memory, and GPU.",
        ),
        ("system_panel", "show_subtask_music") => Some(
            "Show Spotify, Apple Music, and YouTube Music process usage under CPU, memory, and GPU.",
        ),
        ("system_panel", "show_icons") => Some("Use icons instead of text labels for each metric."),
        (_, "panel_opacity") => {
            Some("Panel background opacity. 1.0 (100%) is fully solid; lower values fade the fill.")
        }
        _ => None,
    }
}
