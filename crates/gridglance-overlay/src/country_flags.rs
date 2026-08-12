//! Country flags from iRacing driver flair / club (session YAML DriverInfo).
//!
//! Prefer `FlairID` (profile nationality flag). Fall back to `ClubName` region.

use egui::{ColorImage, CornerRadius, Image, Rect, TextureHandle, TextureOptions, Ui, Vec2};
use once_cell::sync::Lazy;
use std::collections::HashMap;

/// Resolve ISO2 (lowercase) from flair first, then club name / flair name.
pub fn resolve_country_code(
    flair_id: i32,
    flair_name: &str,
    club_name: &str,
) -> Option<&'static str> {
    if let Some(code) = flair_id_to_country_code(flair_id) {
        return Some(code);
    }
    if let Some(code) = name_to_country_code(flair_name) {
        return Some(code);
    }
    club_to_country_code(club_name)
}

/// ISO2 from iRacing profile FlairID (1 = unaffiliated → none).
pub fn flair_id_to_country_code(flair_id: i32) -> Option<&'static str> {
    match flair_id {
        // Unaffiliated / unknown / iRacing logo flairs — no country flag.
        0 | 1 | 2 => None,
        13 => Some("ar"),
        16 => Some("au"),
        17 => Some("at"),
        23 => Some("be"),
        31 => Some("br"),
        34 => Some("bg"),
        39 => Some("ca"),
        44 => Some("cl"),
        45 => Some("cn"),
        48 => Some("co"),
        52 => Some("hr"),
        54 => Some("cz"),
        56 => Some("dk"),
        70 => Some("fi"),
        71 => Some("fr"),
        77 => Some("de"),
        80 => Some("gr"),
        92 => Some("hk"),
        93 => Some("hu"),
        95 => Some("in"),
        96 => Some("id"),
        98 => Some("ie"),
        100 => Some("il"),
        101 => Some("it"),
        104 => Some("jp"),
        120 => Some("lu"),
        125 => Some("my"),
        134 => Some("mx"),
        146 => Some("nl"),
        149 => Some("nz"),
        156 => Some("no"),
        164 => Some("pe"),
        165 => Some("ph"),
        167 => Some("pl"),
        168 => Some("pt"),
        173 => Some("ro"),
        190 => Some("sg"),
        191 => Some("sk"),
        192 => Some("si"),
        195 => Some("za"),
        197 => Some("kr"),
        198 => Some("es"),
        203 => Some("se"),
        204 => Some("ch"),
        205 => Some("tw"),
        208 => Some("th"),
        215 => Some("tr"),
        220 => Some("ua"),
        221 => Some("ae"),
        222 | 236 | 237 | 238 | 239 => Some("gb"), // UK + home nations
        223 => Some("us"),
        224 => Some("uy"),
        228 => Some("ve"),
        // Broader set (may lack a bundled PNG — paint skips missing art).
        3 => Some("af"),
        5 => Some("al"),
        6 => Some("dz"),
        18 => Some("az"),
        19 => Some("bs"),
        20 => Some("bh"),
        21 => Some("bd"),
        24 => Some("bz"),
        28 => Some("bo"),
        29 => Some("ba"),
        30 => Some("bw"),
        37 => Some("kh"),
        40 => Some("cv"),
        51 => Some("cr"),
        53 => Some("cy"),
        55 => Some("cd"),
        59 => Some("do"),
        60 => Some("ec"),
        61 => Some("eg"),
        62 => Some("sv"),
        65 => Some("ee"),
        76 => Some("ge"),
        94 => Some("is"),
        97 => Some("iq"),
        103 => Some("jm"),
        106 => Some("jo"),
        107 => Some("kz"),
        108 => Some("ke"),
        110 => Some("kw"),
        113 => Some("lv"),
        114 => Some("lb"),
        119 => Some("lt"),
        122 => Some("mk"),
        128 => Some("mt"),
        136 => Some("md"),
        137 => Some("mc"),
        139 => Some("me"),
        141 => Some("ma"),
        145 => Some("np"),
        150 => Some("ni"),
        152 => Some("ng"),
        157 => Some("om"),
        158 => Some("pk"),
        160 => Some("ps"),
        161 => Some("pa"),
        163 => Some("py"),
        169 => Some("pr"),
        170 => Some("qa"),
        185 => Some("sa"),
        187 => Some("rs"),
        199 => Some("lk"),
        214 => Some("tn"),
        219 => Some("ug"),
        225 => Some("uz"),
        229 => Some("vn"),
        233 => Some("ye"),
        234 => Some("zm"),
        235 => Some("zw"),
        _ => None,
    }
}

/// ISO2 (lowercase) for a ClubName, when we have a bundled flag.
pub fn club_to_country_code(club: &str) -> Option<&'static str> {
    name_to_country_code(club)
}

fn name_to_country_code(name: &str) -> Option<&'static str> {
    let key = normalize_club(name);
    if key.is_empty() {
        return None;
    }
    if let Some(code) = CLUB_TO_ISO.get(key.as_str()).copied() {
        return Some(code);
    }
    COUNTRY_NAME_TO_ISO.get(key.as_str()).copied()
}

/// Shared outer footprint for every country flag (width ÷ height).
///
/// Bundled PNGs vary (US ~1.9:1, CH 1:1). Fitting into this box keeps icons
/// the same on-screen size regardless of source aspect.
pub const FLAG_DISPLAY_ASPECT: f32 = 3.0 / 2.0;

/// Max flag box that fits in `cell_w` × `cell_h` at [`FLAG_DISPLAY_ASPECT`].
pub fn flag_display_box(cell_w: f32, cell_h: f32) -> (f32, f32) {
    let max_h = cell_h * 0.64;
    let max_w = cell_w * 0.88;
    if max_w <= 0.0 || max_h <= 0.0 {
        return (0.0, 0.0);
    }
    if max_w / max_h > FLAG_DISPLAY_ASPECT {
        (max_h * FLAG_DISPLAY_ASPECT, max_h)
    } else {
        (max_w, max_w / FLAG_DISPLAY_ASPECT)
    }
}

/// PNG bytes for an ISO2 code (lowercase), if bundled.
pub fn flag_png_bytes(code: &str) -> Option<&'static [u8]> {
    match code.to_ascii_lowercase().as_str() {
        "ae" => Some(include_bytes!("../../../assets/flags/ae.png")),
        "ar" => Some(include_bytes!("../../../assets/flags/ar.png")),
        "at" => Some(include_bytes!("../../../assets/flags/at.png")),
        "au" => Some(include_bytes!("../../../assets/flags/au.png")),
        "be" => Some(include_bytes!("../../../assets/flags/be.png")),
        "bg" => Some(include_bytes!("../../../assets/flags/bg.png")),
        "br" => Some(include_bytes!("../../../assets/flags/br.png")),
        "ca" => Some(include_bytes!("../../../assets/flags/ca.png")),
        "ch" => Some(include_bytes!("../../../assets/flags/ch.png")),
        "cl" => Some(include_bytes!("../../../assets/flags/cl.png")),
        "cn" => Some(include_bytes!("../../../assets/flags/cn.png")),
        "co" => Some(include_bytes!("../../../assets/flags/co.png")),
        "cz" => Some(include_bytes!("../../../assets/flags/cz.png")),
        "de" => Some(include_bytes!("../../../assets/flags/de.png")),
        "dk" => Some(include_bytes!("../../../assets/flags/dk.png")),
        "es" => Some(include_bytes!("../../../assets/flags/es.png")),
        "fi" => Some(include_bytes!("../../../assets/flags/fi.png")),
        "fr" => Some(include_bytes!("../../../assets/flags/fr.png")),
        "gb" => Some(include_bytes!("../../../assets/flags/gb.png")),
        "gr" => Some(include_bytes!("../../../assets/flags/gr.png")),
        "hk" => Some(include_bytes!("../../../assets/flags/hk.png")),
        "hr" => Some(include_bytes!("../../../assets/flags/hr.png")),
        "hu" => Some(include_bytes!("../../../assets/flags/hu.png")),
        "id" => Some(include_bytes!("../../../assets/flags/id.png")),
        "ie" => Some(include_bytes!("../../../assets/flags/ie.png")),
        "il" => Some(include_bytes!("../../../assets/flags/il.png")),
        "in" => Some(include_bytes!("../../../assets/flags/in.png")),
        "it" => Some(include_bytes!("../../../assets/flags/it.png")),
        "jp" => Some(include_bytes!("../../../assets/flags/jp.png")),
        "kr" => Some(include_bytes!("../../../assets/flags/kr.png")),
        "lu" => Some(include_bytes!("../../../assets/flags/lu.png")),
        "mx" => Some(include_bytes!("../../../assets/flags/mx.png")),
        "my" => Some(include_bytes!("../../../assets/flags/my.png")),
        "nl" => Some(include_bytes!("../../../assets/flags/nl.png")),
        "no" => Some(include_bytes!("../../../assets/flags/no.png")),
        "nz" => Some(include_bytes!("../../../assets/flags/nz.png")),
        "pe" => Some(include_bytes!("../../../assets/flags/pe.png")),
        "ph" => Some(include_bytes!("../../../assets/flags/ph.png")),
        "pl" => Some(include_bytes!("../../../assets/flags/pl.png")),
        "pt" => Some(include_bytes!("../../../assets/flags/pt.png")),
        "ro" => Some(include_bytes!("../../../assets/flags/ro.png")),
        "ru" => Some(include_bytes!("../../../assets/flags/ru.png")),
        "se" => Some(include_bytes!("../../../assets/flags/se.png")),
        "sg" => Some(include_bytes!("../../../assets/flags/sg.png")),
        "si" => Some(include_bytes!("../../../assets/flags/si.png")),
        "sk" => Some(include_bytes!("../../../assets/flags/sk.png")),
        "th" => Some(include_bytes!("../../../assets/flags/th.png")),
        "tr" => Some(include_bytes!("../../../assets/flags/tr.png")),
        "tw" => Some(include_bytes!("../../../assets/flags/tw.png")),
        "ua" => Some(include_bytes!("../../../assets/flags/ua.png")),
        "us" => Some(include_bytes!("../../../assets/flags/us.png")),
        "uy" => Some(include_bytes!("../../../assets/flags/uy.png")),
        "ve" => Some(include_bytes!("../../../assets/flags/ve.png")),
        "za" => Some(include_bytes!("../../../assets/flags/za.png")),
        _ => None,
    }
}

/// Draw a country flag centered in `rect` (egui). No-op if unknown / decode fails.
pub fn paint_egui(ui: &mut Ui, code: &str, rect: Rect) {
    let Some(tex) = texture(ui, code) else {
        return;
    };
    let size = tex.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 {
        return;
    }
    let (box_w, box_h) = flag_display_box(rect.width(), rect.height());
    if box_w <= 0.0 || box_h <= 0.0 {
        return;
    }
    // Contain within the shared footprint so wide assets (US) match others.
    let scale = (box_w / size.x).min(box_h / size.y);
    let w = size.x * scale;
    let h = size.y * scale;
    let dest = Rect::from_center_size(rect.center(), Vec2::new(w, h));
    let radius = (h * 0.18).clamp(1.5, 3.5);
    Image::from_texture((tex.id(), size))
        .corner_radius(CornerRadius::same(radius.round() as u8))
        .paint_at(ui, dest);
}

fn texture(ui: &mut Ui, code: &str) -> Option<TextureHandle> {
    let code = code.to_ascii_lowercase();
    let id = egui::Id::new(("country_flag_tex", code.clone()));
    if let Some(tex) = ui.ctx().data(|d| d.get_temp::<TextureHandle>(id)) {
        return Some(tex);
    }
    let bytes = flag_png_bytes(&code)?;
    let img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    let color = ColorImage::from_rgba_unmultiplied(size, img.as_raw());
    let tex = ui.ctx().load_texture(
        format!("country_flag_{code}"),
        color,
        TextureOptions::LINEAR,
    );
    ui.ctx().data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

fn normalize_club(club: &str) -> String {
    club.trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

static CLUB_TO_ISO: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    // US regional clubs
    for name in [
        "alabama",
        "alaska",
        "arizona",
        "arkansas",
        "california",
        "carolinas",
        "chicago",
        "colorado",
        "connecticut",
        "delaware",
        "florida",
        "georgia",
        "hawaii",
        "idaho",
        "illinois",
        "indiana",
        "iowa",
        "kansas",
        "kentucky",
        "louisiana",
        "maine",
        "maryland",
        "massachusetts",
        "michigan",
        "midwest",
        "minnesota",
        "mississippi",
        "missouri",
        "montana",
        "nebraska",
        "nevada",
        "new england",
        "new hampshire",
        "new jersey",
        "new mexico",
        "new york",
        "north carolina",
        "north dakota",
        "ohio",
        "oklahoma",
        "oregon",
        "pennsylvania",
        "rhode island",
        "south carolina",
        "south dakota",
        "tennessee",
        "texas",
        "utah",
        "vermont",
        "virginia",
        "washington",
        "west virginia",
        "wisconsin",
        "wyoming",
        "atlantic",
        "plains",
        "rocky mountain",
        "america",
        "usa",
        "united states",
    ] {
        m.insert(name, "us");
    }
    // Canada regions
    for name in [
        "canada",
        "ontario",
        "quebec",
        "british columbia",
        "alberta",
        "manitoba",
        "saskatchewan",
        "atlantic canada",
    ] {
        m.insert(name, "ca");
    }
    // Multi-country clubs (pick a representative flag)
    m.insert("benelux", "nl");
    m.insert("de at ch", "de");
    m.insert("deatsch", "de");
    m.insert("iberia", "es");
    m.insert("celtic", "ie");
    m.insert("scandinavia", "se");
    m.insert("hispanic latam", "mx");
    m.insert("latam", "mx");
    m.insert("australia nz", "au");
    m.insert("australia and new zealand", "au");
    m.insert("uk", "gb");
    m.insert("united kingdom", "gb");
    m.insert("great britain", "gb");
    m.insert("england", "gb");
    m.insert("scotland", "gb");
    m.insert("wales", "gb");
    m.insert("ireland", "ie");
    m.insert("northern ireland", "gb");
    m
});

static COUNTRY_NAME_TO_ISO: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        ("argentina", "ar"),
        ("australia", "au"),
        ("austria", "at"),
        ("belgium", "be"),
        ("brazil", "br"),
        ("bulgaria", "bg"),
        ("chile", "cl"),
        ("china", "cn"),
        ("colombia", "co"),
        ("croatia", "hr"),
        ("czech republic", "cz"),
        ("czechia", "cz"),
        ("denmark", "dk"),
        ("finland", "fi"),
        ("france", "fr"),
        ("germany", "de"),
        ("greece", "gr"),
        ("hong kong", "hk"),
        ("hungary", "hu"),
        ("india", "in"),
        ("indonesia", "id"),
        ("israel", "il"),
        ("italy", "it"),
        ("japan", "jp"),
        ("korea", "kr"),
        ("south korea", "kr"),
        ("luxembourg", "lu"),
        ("malaysia", "my"),
        ("mexico", "mx"),
        ("netherlands", "nl"),
        ("new zealand", "nz"),
        ("norway", "no"),
        ("peru", "pe"),
        ("philippines", "ph"),
        ("poland", "pl"),
        ("portugal", "pt"),
        ("romania", "ro"),
        ("russia", "ru"),
        ("singapore", "sg"),
        ("slovakia", "sk"),
        ("slovenia", "si"),
        ("south africa", "za"),
        ("spain", "es"),
        ("sweden", "se"),
        ("switzerland", "ch"),
        ("taiwan", "tw"),
        ("thailand", "th"),
        ("turkey", "tr"),
        ("ukraine", "ua"),
        ("united arab emirates", "ae"),
        ("uae", "ae"),
        ("uruguay", "uy"),
        ("venezuela", "ve"),
    ])
});

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_us_and_country_clubs() {
        assert_eq!(club_to_country_code("Florida"), Some("us"));
        assert_eq!(club_to_country_code("Brazil"), Some("br"));
        assert_eq!(club_to_country_code("DE-AT-CH"), Some("de"));
        assert_eq!(club_to_country_code("Benelux"), Some("nl"));
        assert_eq!(club_to_country_code("International"), None);
        assert_eq!(club_to_country_code(""), None);
    }

    #[test]
    fn maps_flair_ids() {
        assert_eq!(flair_id_to_country_code(223), Some("us"));
        assert_eq!(flair_id_to_country_code(222), Some("gb"));
        assert_eq!(flair_id_to_country_code(31), Some("br"));
        assert_eq!(flair_id_to_country_code(1), None);
        assert_eq!(
            resolve_country_code(223, "", "Florida"),
            Some("us")
        );
        assert_eq!(
            resolve_country_code(0, "", "Brazil"),
            Some("br")
        );
    }

    #[test]
    fn bundles_common_flags() {
        assert!(flag_png_bytes("us").is_some());
        assert!(flag_png_bytes("gb").is_some());
        assert!(flag_png_bytes("xx").is_none());
    }
}
