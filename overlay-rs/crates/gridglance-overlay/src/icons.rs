//! Font Awesome 6 Free Solid glyphs (parity with Python `overlay.widgets.icons`).

use egui::{FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

/// Family name registered with egui.
pub const FAMILY: &str = "fa-solid";

const FA_TTF: &[u8] = include_bytes!("../../../../assets/fonts/fa-solid-900.ttf");

const CODEPOINTS: &[(&str, u32)] = &[
    ("speed", 0xF625),
    ("speed_kph", 0xF625),
    ("speed_mph", 0xF625),
    ("rpm", 0xF624),
    ("gear", 0xF013),
    ("position", 0xF292),
    ("car_number", 0xF1B9),
    ("lap", 0xF11E),
    ("lap_count", 0xF11E),
    ("laps_left", 0xF11E),
    ("fuel", 0xF52F),
    ("fuel_laps", 0xF52F),
    ("fuel_stack", 0xF52F),
    ("tires", 0xF1CD),
    ("last_lap", 0xF2F2),
    ("best_lap", 0xF091),
    ("cur_lap", 0xF2F2),
    ("delta", 0xF252),
    ("incidents", 0xF071),
    ("track_temp", 0xF2C9),
    ("air_temp", 0xF72E),
    ("irating", 0xF201),
    ("irating_up", 0xF062),
    ("irating_down", 0xF063),
    ("pro_driver", 0xF005),
    ("league", 0xF0C0),
    ("flag", 0xF024),
    ("speaking", 0xF028),
];

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        FAMILY.to_owned(),
        Arc::new(FontData::from_static(FA_TTF)),
    );
    fonts
        .families
        .insert(FontFamily::Name(FAMILY.into()), vec![FAMILY.to_owned()]);
    // Keep proportional/monospace defaults; icons use the named family only.
    ctx.set_fonts(fonts);
}

pub fn glyph(name: &str) -> Option<String> {
    CODEPOINTS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, cp)| char::from_u32(*cp).unwrap_or('\u{FFFD}').to_string())
}

pub fn font_id(size: f32) -> FontId {
    FontId::new(size.max(6.0), FontFamily::Name(FAMILY.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incidents_glyph() {
        assert_eq!(glyph("incidents").unwrap().chars().next().unwrap() as u32, 0xF071);
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
        ] {
            assert!(glyph(name).is_some(), "missing glyph {name}");
        }
    }
}
