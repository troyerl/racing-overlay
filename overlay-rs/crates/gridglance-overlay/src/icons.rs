//! Font Awesome 6 Free Solid glyphs (parity with Python `overlay.widgets.icons`).
//! Also registers Noto Sans Regular/Bold for HUD labels.

use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

/// Family name registered with egui.
pub const FAMILY: &str = "fa-solid";

/// Named family for bold HUD text (`chrome::label` when bold).
pub const BOLD_FAMILY: &str = "Bold";

const FA_TTF: &[u8] = include_bytes!("../../../../assets/fonts/fa-solid-900.ttf");
const NOTO_REGULAR: &[u8] = include_bytes!("../../../../assets/fonts/NotoSans-Regular.ttf");
const NOTO_BOLD: &[u8] = include_bytes!("../../../../assets/fonts/NotoSans-Bold.ttf");

/// Metric / label name → Font Awesome 6 Free Solid codepoint.
/// Keep in sync with `overlay/widgets/icons.py` `_CODEPOINTS`.
const CODEPOINTS: &[(&str, u32)] = &[
    // dash speed / engine metrics
    ("speed", 0xF625),     // gauge-high
    ("speed_kph", 0xF625), // gauge-high
    ("speed_mph", 0xF625), // gauge-high
    ("rpm", 0xF624),       // gauge
    ("gear", 0xF013),      // gear (cog)
    ("position", 0xF292),  // hashtag
    ("class_position", 0xF292),
    ("class_pos", 0xF292),
    ("car_number", 0xF1B9), // car
    // lap / fuel / tire metrics
    ("lap", 0xF11E),        // flag-checkered
    ("lap_count", 0xF11E),  // flag-checkered
    ("laps_left", 0xF11E),  // flag-checkered
    ("laps_remain", 0xF11E),
    ("fuel", 0xF52F),       // gas-pump
    ("fuel_laps", 0xF52F),  // gas-pump
    ("fuel_stack", 0xF52F), // gas-pump
    ("fuel_pct", 0xF52F),
    ("fuel_burn", 0xF52F),
    ("tires", 0xF1CD), // life-ring (tire-like)
    ("tires_4", 0xF1CD),
    ("tire_temp", 0xF2C9),
    // timing metrics
    ("last_lap", 0xF2F2), // stopwatch
    ("best_lap", 0xF091), // trophy
    ("cur_lap", 0xF2F2),  // stopwatch
    ("delta", 0xF252),    // hourglass-half
    ("delta_best", 0xF091),
    ("delta_optimal", 0xF252),
    ("time_remain", 0xF253), // hourglass-end
    ("incidents", 0xF071),   // triangle-exclamation
    ("incident_limit", 0xF071),
    ("incidents_limit", 0xF071),
    ("incidents_team", 0xF0C0),
    // environment
    ("track_temp", 0xF2C9), // temperature-half
    ("air_temp", 0xF72E),   // wind
    ("weather", 0xF0C2),    // cloud
    ("track_wetness", 0xF043), // droplet
    ("water_temp", 0xF043),
    ("oil_temp", 0xF613), // oil-can
    // decorative / driver groups
    ("sparkle", 0xF005),    // star
    ("pro_driver", 0xF005), // star
    ("league", 0xF0C0),     // users
    ("flag", 0xF024),       // flag
    ("trophy", 0xF091),     // trophy
    ("shield", 0xF3ED),     // shield-halved
    ("crown", 0xF521),      // crown
    ("bolt", 0xF0E7),       // bolt
    ("voltage", 0xF0E7),
    // table header / footer
    ("sof", 0xF0C0),       // users
    ("class_sof", 0xF0C0), // users
    ("session_time", 0xF017), // clock
    ("race_time", 0xF017),
    ("session_best", 0xF091), // trophy
    ("my_session_best", 0xF2F2), // stopwatch
    ("local_time", 0xF017),
    ("sim_time", 0xF185), // sun
    ("session_type", 0xF0AE), // tasks / session
    ("race_split", 0xF0CB), // list-ol
    ("order_pill", 0xF0CB),
    ("title", 0xF091),
    ("count", 0xF0C0),
    ("track_name", 0xF018), // road
    ("fast_repairs", 0xF0AD), // wrench
    // system panel
    ("cpu", 0xF2DB),     // microchip
    ("mem", 0xF538),     // memory
    ("gpu", 0xF390),     // display
    ("fps", 0xF624),     // gauge
    ("network", 0xF1EB), // wifi
    // radio / map traffic
    ("speaking", 0xF028), // volume-high
    ("radio", 0xF130),    // microphone
    ("leader", 0xF521),   // crown
    ("car_ahead", 0xF062), // arrow-up
    ("car_behind", 0xF063), // arrow-down
    ("gap_ahead", 0xF062),
    ("gap_behind", 0xF063),
    ("irating", 0xF201), // chart-line
    ("irating_up", 0xF062),
    ("irating_down", 0xF063),
    // car controls / warnings
    ("dc_brake_bias", 0xF1DE), // sliders
    ("dc_tc", 0xF3ED),
    ("dc_abs", 0xF3ED),
    ("dc_fuel_mix", 0xF0C3), // flask
    ("dc_tire_set", 0xF1CD),
    ("engine_warn", 0xF06A), // circle-exclamation
    ("lap_corners", 0xF5EB), // turn-right
    // settings accordion
    ("chevron_right", 0xF054), // chevron-right
    ("chevron_down", 0xF078),  // chevron-down
];

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts
        .font_data
        .insert(FAMILY.to_owned(), Arc::new(FontData::from_static(FA_TTF)));
    fonts.font_data.insert(
        "NotoSans-Regular".to_owned(),
        Arc::new(FontData::from_static(NOTO_REGULAR)),
    );
    fonts.font_data.insert(
        "NotoSans-Bold".to_owned(),
        Arc::new(FontData::from_static(NOTO_BOLD)),
    );
    fonts
        .families
        .insert(FontFamily::Name(FAMILY.into()), vec![FAMILY.to_owned()]);
    fonts.families.insert(
        FontFamily::Name(BOLD_FAMILY.into()),
        vec!["NotoSans-Bold".to_owned()],
    );
    // Prefer Noto Sans for HUD body text; keep egui builtins as fallback.
    if let Some(prop) = fonts.families.get_mut(&FontFamily::Proportional) {
        prop.insert(0, "NotoSans-Regular".to_owned());
    }
    ctx.set_fonts(fonts);
}

pub fn glyph(name: &str) -> Option<String> {
    CODEPOINTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, cp)| char::from_u32(*cp).unwrap_or('\u{FFFD}').to_string())
}

pub fn has(name: &str) -> bool {
    CODEPOINTS.iter().any(|(n, _)| *n == name)
}

pub fn font_id(size: f32) -> FontId {
    FontId::new(size.max(6.0), FontFamily::Name(FAMILY.into()))
}

/// Human label for a known glyph key (settings pickers).
pub fn label(name: &str) -> &str {
    match name {
        "league" => "League",
        "flag" => "Flag",
        "trophy" => "Trophy",
        "shield" => "Shield",
        "crown" => "Crown",
        "bolt" => "Bolt",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incidents_glyph() {
        assert_eq!(
            glyph("incidents").unwrap().chars().next().unwrap() as u32,
            0xF071
        );
    }

    #[test]
    fn driver_group_icons_resolve() {
        for name in ["league", "flag", "trophy", "shield", "crown", "bolt"] {
            assert!(has(name), "missing driver-group glyph {name}");
            assert!(glyph(name).is_some());
        }
    }

    #[test]
    fn codepoints_cover_dash_slots() {
        for name in [
            "speed",
            "laps_left",
            "incidents",
            "tires",
            "fuel_stack",
            "car_number",
            "lap_count",
            "irating",
            "irating_up",
            "irating_down",
            "leader",
            "car_ahead",
            "car_behind",
            "fps",
            "network",
            "radio",
        ] {
            assert!(glyph(name).is_some(), "missing glyph {name}");
        }
    }

    #[test]
    fn codepoints_cover_table_slots() {
        for name in [
            "sof",
            "class_sof",
            "session_time",
            "race_time",
            "position",
            "class_position",
            "track_temp",
            "air_temp",
            "local_time",
            "cpu",
            "weather",
            "track_wetness",
        ] {
            assert!(glyph(name).is_some(), "missing glyph {name}");
        }
    }

    #[test]
    fn no_duplicate_names() {
        let mut seen = std::collections::HashSet::new();
        for (name, _) in CODEPOINTS {
            assert!(seen.insert(*name), "duplicate icon name {name}");
        }
    }
}
