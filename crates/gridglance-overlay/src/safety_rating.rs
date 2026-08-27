//! Safety Rating projection from live incidents + corners.
//!
//! iRacing does not publish the exact SR formula. This module uses a community-
//! validated approximation fitted against official result `old_cpi` / `new_cpi`
//! pairs:
//!
//! 1. Map displayed SR → estimated CPI (`ln(cpi) = P[lic_level] + sr / K`)
//! 2. Roll the corner window: `new_cpi = H / ((H - C) / old_cpi + I)`
//! 3. Map CPI → SR with the same scale (`sr = K · (ln(cpi) − P)`), apply the
//!    ±0.40 whole-number bump, then ΔSR = round(new) − round(displayed old).
//!    Using the displayed SR as the addend (`old + K·ln(ratio)`) is wrong when
//!    the board hundredth was rounded away from the CPI-implied value.
//!
//! `lic_level = 4·class_idx + floor(sr)` matches DriverInfo `LicLevel`
//! (e.g. D 3.37 → 7). P is fitted per LicLevel because the displayed SR scale
//! (with whole-number buffers) is not linear across a full class.
//!
//! Exactness is limited without the driver's private CPI history. When live
//! `old_cpi` is known (results / member API), pass it via [`project_delta_with_cpi`].

/// CPI ↔ SR scale: `sr ≈ K · (ln(cpi) − P)` — tuned so board hundredths match
/// official `old_sub_level` / `new_sub_level` on recent road results.
const K: f64 = 1.70;

/// Corner-history sizes by license class index (R=0 … P=5).
const HISTORY: [f64; 6] = [1_500.0, 1_700.0, 1_900.0, 2_135.0, 2_735.0, 3_000.0];

/// Fallback intercept by class when a LicLevel band has no samples.
const LN_CPI_P_CLASS: [f64; 6] = [
    0.851_9, // Rookie
    1.026_8, // D
    1.403_7, // C
    1.769_7, // B
    2.145_5, // A
    2.200_0, // Pro / WC
];

/// `(LicLevel, P)` medians: `ln(cpi) = P + sr / K`.
const LN_CPI_P_LEVEL: &[(i32, f64)] = &[
    (2, 0.851_9),  // R 2.xx
    (6, 1.229_5),  // D 2.xx
    (7, 1.026_8),  // D 3.xx
    (8, 0.815_7),  // D 4.xx
    (9, 1.818_6),  // C 1.xx
    (10, 1.622_5), // C 2.xx
    (11, 1.403_7), // C 3.xx
    (13, 2.207_9), // B 1.xx
    (14, 1.992_2), // B 2.xx
    (15, 1.769_7), // B 3.xx
    (16, 1.728_8), // B 4.xx
    (17, 2.583_5), // A 1.xx
    (18, 2.361_7), // A 2.xx
    (19, 2.145_5), // A 3.xx
    (20, 1.943_7), // A 4.xx
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SrProjection {
    /// Projected SR change in hundredths (e.g. `+16` → `+0.16`).
    pub delta_hundredths: i32,
}

/// Map license letter / name → class index (Rookie=0 … Pro=5).
pub fn class_idx_from_license(lic: &str) -> usize {
    let s = lic.trim();
    let letter = s
        .chars()
        .next()
        .map(|c| c.to_ascii_uppercase())
        .unwrap_or('R');
    match letter {
        'R' => 0,
        'D' => 1,
        'C' => 2,
        'B' => 3,
        'A' => 4,
        'P' | 'W' => 5, // Pro / WC
        _ => 4,
    }
}

/// DriverInfo-style license level: `4·class_idx + floor(sr)`.
pub fn lic_level(class_idx: usize, sr: f64) -> i32 {
    let floor = sr.clamp(0.0, 4.99).floor() as i32;
    (4 * class_idx as i32) + floor
}

fn session_incident_weight(session_type: &str) -> f64 {
    let s = session_type.to_ascii_lowercase();
    if s.contains("race") || s.contains("heat") || s.contains("feature") || s.contains("consol") {
        1.0
    } else if s.contains("qual") || s.contains("time trial") || s.contains("timetrial") || s == "tt"
    {
        0.35
    } else if s.contains("practice") || s.contains("warm") || s.contains("lone") {
        0.50
    } else {
        // Unknown / hosted — treat like race so the live number still moves.
        1.0
    }
}

fn ln_cpi_p(class_idx: usize, sr: f64) -> f64 {
    let level = lic_level(class_idx, sr);
    if let Some((_, p)) = LN_CPI_P_LEVEL.iter().find(|(l, _)| *l == level) {
        return *p;
    }
    // Nearest known level in the same class band (4 levels).
    let class_base = 4 * class_idx as i32;
    let mut best: Option<(i32, f64)> = None;
    for &(l, p) in LN_CPI_P_LEVEL {
        if l < class_base || l >= class_base + 4 {
            continue;
        }
        let dist = (l - level).abs();
        if best.is_none_or(|(d, _)| dist < d) {
            best = Some((dist, p));
        }
    }
    best.map(|(_, p)| p)
        .unwrap_or(LN_CPI_P_CLASS[class_idx.min(5)])
}

fn cpi_from_sr(class_idx: usize, sr: f64) -> f64 {
    (ln_cpi_p(class_idx, sr) + sr / K).exp()
}

fn apply_whole_number_bump(old_sr: f64, new_sr: f64) -> f64 {
    let old_floor = old_sr.floor();
    let new_floor = new_sr.floor();
    if new_floor > old_floor {
        new_sr + 0.40
    } else if new_floor < old_floor {
        new_sr - 0.40
    } else {
        new_sr
    }
}

fn clamp_sr(sr: f64) -> f64 {
    sr.clamp(0.0, 4.99)
}

fn hundredths_delta(old_sr: f64, new_sr: f64) -> i32 {
    let old_h = (old_sr * 100.0).round() as i32;
    let new_h = (new_sr * 100.0).round() as i32;
    new_h - old_h
}

fn project_from_cpi(
    old_sr: f64,
    old_cpi: f64,
    corners_completed: f64,
    incidents: f64,
    session_type: &str,
    class_idx: usize,
) -> Option<SrProjection> {
    let old_sr = old_sr.clamp(0.0, 4.99);
    let old_cpi = old_cpi.max(1e-3);
    let c = corners_completed.max(0.0);
    let i = (incidents.max(0.0) * session_incident_weight(session_type)).max(0.0);
    let h = HISTORY[class_idx.min(5)].max(c + 50.0);
    let den = (h - c) / old_cpi + i;
    if den <= 1e-9 {
        return None;
    }
    let new_cpi = h / den;
    if new_cpi <= 1e-9 {
        return None;
    }
    // Absolute CPI→SR (not `displayed_sr + K·ln(ratio)`): the board hundredth can
    // sit a few thousandths off the CPI-implied SR, and that bias flips rounding.
    let p = ln_cpi_p(class_idx, old_sr);
    let new_sr_from_cpi = K * (new_cpi.ln() - p);
    let mut new_sr = apply_whole_number_bump(old_sr, new_sr_from_cpi);
    new_sr = clamp_sr(new_sr);
    Some(SrProjection {
        delta_hundredths: hundredths_delta(old_sr, new_sr),
    })
}

/// Project SR change for the current session (CPI estimated from displayed SR).
///
/// `corners_completed` should be `laps × TrackNumTurns` (fractional laps OK).
/// `incidents` is the raw iRacing incident point total for the session.
pub fn project_delta(
    class_idx: usize,
    safety_rating: f64,
    corners_completed: f64,
    incidents: f64,
    session_type: &str,
) -> Option<SrProjection> {
    if !(0.0..=5.0).contains(&safety_rating) || corners_completed < 1.0 {
        return None;
    }
    let class_idx = class_idx.min(5);
    let old_cpi = cpi_from_sr(class_idx, safety_rating.clamp(0.0, 4.99));
    project_delta_with_cpi(
        class_idx,
        safety_rating,
        old_cpi,
        corners_completed,
        incidents,
        session_type,
    )
}

/// Like [`project_delta`], but uses a known `old_cpi` (from results / member API).
pub fn project_delta_with_cpi(
    class_idx: usize,
    safety_rating: f64,
    old_cpi: f64,
    corners_completed: f64,
    incidents: f64,
    session_type: &str,
) -> Option<SrProjection> {
    if !(0.0..=5.0).contains(&safety_rating) || corners_completed < 1.0 || old_cpi <= 0.0 {
        return None;
    }
    project_from_cpi(
        safety_rating,
        old_cpi,
        corners_completed,
        incidents,
        session_type,
        class_idx.min(5),
    )
}

/// Parse `"A 3.42"` / `"B 2.99"` → `(class_idx, sr)`.
pub fn parse_license_sr(license: &str) -> Option<(usize, f64)> {
    let mut parts = license.split_whitespace();
    let cls = parts.next()?;
    let sr_s = parts.next()?;
    let sr: f64 = sr_s.parse().ok()?;
    if !(0.0..=5.0).contains(&sr) {
        return None;
    }
    Some((class_idx_from_license(cls), sr))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_oval_race_gains() {
        // Logan: B 3.43, 54 laps × 4 corners, 0x → about +0.16
        let p = project_delta(3, 3.43, 216.0, 0.0, "Race").unwrap();
        assert!(
            (14..=20).contains(&p.delta_hundredths),
            "got {}",
            p.delta_hundredths
        );
    }

    #[test]
    fn whole_number_bump_on_clean_run() {
        // Crossing 4.00 adds the +0.40 buffer.
        let p = project_delta(3, 3.96, 200.0, 0.0, "Race").unwrap();
        assert!(
            p.delta_hundredths >= 50,
            "expected bump, got {}",
            p.delta_hundredths
        );
    }

    #[test]
    fn incidents_can_cost_sr() {
        let p = project_delta(3, 3.54, 172.0, 4.0, "Race").unwrap();
        assert!(p.delta_hundredths < 0, "got {}", p.delta_hundredths);
    }

    #[test]
    fn practice_incidents_weighted_down() {
        let race = project_delta(4, 3.50, 100.0, 4.0, "Race")
            .unwrap()
            .delta_hundredths;
        let practice = project_delta(4, 3.50, 100.0, 4.0, "Practice")
            .unwrap()
            .delta_hundredths;
        assert!(practice > race, "practice={practice} race={race}");
    }

    #[test]
    fn parse_license() {
        assert_eq!(parse_license_sr("A 3.42"), Some((4, 3.42)));
        assert_eq!(parse_license_sr("R 2.50"), Some((0, 2.50)));
    }

    #[test]
    fn lime_rock_mx5_d337_six_x() {
        // eventresult-87887429: D 3.37, Lime Rock GP (9 corners), 12 laps, 6x → −0.01
        let p = project_delta(1, 3.37, 108.0, 6.0, "Race").unwrap();
        assert_eq!(p.delta_hundredths, -1, "got {}", p.delta_hundredths);
    }

    #[test]
    fn lic_level_d337() {
        assert_eq!(lic_level(1, 3.37), 7);
    }

    fn load_sr_fixture(name: &str) -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(name);
        let raw = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {name}: {e}"))
    }

    fn project_fixture(v: &serde_json::Value) -> (i32, i32, f64, f64) {
        let corners_per_lap = v["corners_per_lap"].as_i64().unwrap() as f64;
        let d = &v["driver"];
        let laps = d["laps_complete"].as_i64().unwrap() as f64;
        let incidents = d["incidents"].as_f64().unwrap();
        let old_sub = d["old_sub_level"].as_i64().unwrap() as i32;
        let new_sub = d["new_sub_level"].as_i64().unwrap() as i32;
        let old_lic = d["old_license_level"].as_i64().unwrap() as i32;
        let class_idx = ((old_lic - 1) / 4) as usize;
        let old_sr = old_sub as f64 / 100.0;
        let corners = laps * corners_per_lap;
        let p = project_delta(class_idx, old_sr, corners, incidents, "Race").unwrap();
        (p.delta_hundredths, new_sub - old_sub, old_sr, corners)
    }

    fn project_fixture_with_true_cpi(v: &serde_json::Value) -> (i32, i32) {
        let corners_per_lap = v["corners_per_lap"].as_i64().unwrap() as f64;
        let d = &v["driver"];
        let laps = d["laps_complete"].as_i64().unwrap() as f64;
        let incidents = d["incidents"].as_f64().unwrap();
        let old_sub = d["old_sub_level"].as_i64().unwrap() as i32;
        let new_sub = d["new_sub_level"].as_i64().unwrap() as i32;
        let old_lic = d["old_license_level"].as_i64().unwrap() as i32;
        let old_cpi = d["old_cpi"].as_f64().unwrap();
        let class_idx = ((old_lic - 1) / 4) as usize;
        let old_sr = old_sub as f64 / 100.0;
        let corners = laps * corners_per_lap;
        let p =
            project_delta_with_cpi(class_idx, old_sr, old_cpi, corners, incidents, "Race").unwrap();
        (p.delta_hundredths, new_sub - old_sub)
    }

    /// Full fixture from `eventresult-87887429.json` (Lime Rock GP MX-5).
    #[test]
    fn eventresult_87887429_logan_troyer() {
        let v = load_sr_fixture("sr_eventresult_87887429.json");
        let (proj, actual, old_sr, corners) = project_fixture(&v);
        assert!((corners - 108.0).abs() < 1e-6);
        assert_eq!(
            proj, actual,
            "projected {:+} vs official {:+} (SR {:.2})",
            proj, actual, old_sr
        );
        let (proj_cpi, _) = project_fixture_with_true_cpi(&v);
        assert_eq!(proj_cpi, actual, "true-CPI path");
    }

    /// Full fixture from `eventresult-87886371.json` (Lime Rock GP MX-5).
    #[test]
    fn eventresult_87886371_logan_troyer() {
        let v = load_sr_fixture("sr_eventresult_87886371.json");
        let (proj, actual, old_sr, corners) = project_fixture(&v);
        assert!((corners - 99.0).abs() < 1e-6);
        assert_eq!(
            proj, actual,
            "projected {:+} vs official {:+} (SR {:.2})",
            proj, actual, old_sr
        );
        let (proj_cpi, _) = project_fixture_with_true_cpi(&v);
        assert_eq!(proj_cpi, actual, "true-CPI path");
    }

    /// Full fixture from `eventresult-87871599.json` (Lime Rock GP MX-5).
    ///
    /// Displayed D 3.44 sits slightly above the CPI-implied SR; the true-CPI path
    /// recovers the official −0.02. The live (estimated-CPI) path cannot see that
    /// offset and may land 0.01 away until member-API CPI is available.
    #[test]
    fn eventresult_87871599_logan_troyer() {
        let v = load_sr_fixture("sr_eventresult_87871599.json");
        let (proj_est, actual, old_sr, corners) = project_fixture(&v);
        assert!((corners - 99.0).abs() < 1e-6);
        assert_eq!(actual, -2, "fixture official delta");
        let (proj_cpi, _) = project_fixture_with_true_cpi(&v);
        assert_eq!(
            proj_cpi, actual,
            "true-CPI path: projected {:+} vs official {:+} (SR {:.2})",
            proj_cpi, actual, old_sr
        );
        assert!(
            (proj_est - actual).abs() <= 1,
            "est-CPI path within 0.01: projected {:+} vs official {:+}",
            proj_est,
            actual
        );
    }
}
