//! Shared lap/time/session formatting and SessionInfo YAML parsers.

#![cfg_attr(not(windows), allow(dead_code))] // session parsers consumed by irsdk (Windows)

// CarIdxSessionFlags bits used for per-car flag labels (match irsdk / Python).
const FLAG_BLUE: i32 = 0x0000_0020;
const FLAG_BLACK: i32 = 0x0001_0000;
const FLAG_DQ: i32 = 0x0002_0000;
const FLAG_FURLED: i32 = 0x0008_0000;
const FLAG_REPAIR: i32 = 0x0010_0000;

/// Format lap time for table slots / headers (`empty` when invalid, e.g. `"—"`).
pub fn fmt_laptime(secs: f64, empty: &str) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return empty.into();
    }
    let m = (secs / 60.0).floor() as i32;
    let s = secs - (m as f64) * 60.0;
    format!("{m}:{s:06.3}")
}

/// Display car number with a leading `#` (idempotent if already prefixed).
pub fn format_car_number(num: &str) -> String {
    let t = num.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.starts_with('#') {
        t.to_string()
    } else {
        format!("#{t}")
    }
}

/// Format lap time for the laptime log widget (zero-padded minutes).
pub fn fmt_laptime_log(secs: f64) -> String {
    if secs <= 0.0 {
        return "—".into();
    }
    let m = (secs / 60.0).floor() as i32;
    let s = secs - m as f64 * 60.0;
    format!("{m:02}:{s:06.3}")
}

/// Sim clock from `SessionTimeOfDay` (seconds since midnight).
pub fn fmt_tod(secs: f32) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "—".into();
    }
    let secs = (secs as i64).rem_euclid(86_400);
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    format!("{h:02}:{m:02}")
}

pub fn format_session_type(raw: String) -> String {
    let st = raw.to_ascii_lowercase();
    if st.contains("qualif") || st == "qual" {
        "Qualifying".into()
    } else if st.contains("race") {
        "Race".into()
    } else if st.contains("practice") || st == "open" {
        "Practice".into()
    } else {
        raw.replace('_', " ")
    }
}

/// iRacing uses ~32767 for unlimited / time-only sessions. Treat those as no lap total.
pub fn finite_laps_total(n: i32) -> Option<i32> {
    if n > 0 && n < 32_000 {
        Some(n)
    } else {
        None
    }
}

pub fn fmt_car_gap(position: i32, f2: f32) -> String {
    if position == 1 {
        return "LEADER".into();
    }
    if !f2.is_finite() || f2 <= 0.0 {
        return String::new();
    }
    format!("+{f2:.1}")
}

/// Session flag label for the table `car_flag` column (blue / meatball / …).
pub fn map_session_flag_label(car_flags: i32) -> Option<String> {
    if car_flags & FLAG_REPAIR != 0 {
        return Some("meatball".into());
    }
    if car_flags & FLAG_BLACK != 0 {
        return Some("black".into());
    }
    if car_flags & FLAG_DQ != 0 {
        return Some("dq".into());
    }
    if car_flags & FLAG_FURLED != 0 {
        return Some("furled".into());
    }
    if car_flags & FLAG_BLUE != 0 {
        return Some("blue".into());
    }
    None
}

/// Parse `Sessions:` blocks for `SessionType` values (indexed by SessionNum).
pub fn parse_session_types(yaml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut in_sessions = false;
    for line in yaml.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("Sessions:") {
            in_sessions = true;
            continue;
        }
        if in_sessions {
            let indent = line.len() - line.trim_start().len();
            if !trimmed.is_empty() && indent == 0 && !trimmed.starts_with('-') {
                break;
            }
            if trimmed.starts_with("SessionType:") {
                let v = trimmed
                    .trim_start_matches("SessionType:")
                    .trim()
                    .trim_matches('"')
                    .to_string();
                out.push(v);
            }
        }
    }
    out
}

/// Registration split from WeekendInfo YAML when present.
pub fn parse_race_split(yaml: &str) -> Option<i32> {
    const KEYS: &[&str] = &[
        "RaceSplit",
        "SplitNum",
        "SplitNumber",
        "SessionSplit",
        "SessionSplitNum",
        "EventSplit",
    ];
    for key in KEYS {
        if let Some(v) = yaml_i32(yaml, key) {
            if v > 0 {
                return Some(v);
            }
        }
    }
    for line in yaml.lines() {
        let trimmed = line.trim();
        let Some((key, rest)) = trimmed.split_once(':') else {
            continue;
        };
        let lower = key.to_ascii_lowercase();
        if !lower.contains("split")
            || lower.contains("total")
            || lower.contains("count")
            || lower == "numsplits"
        {
            continue;
        }
        if let Ok(v) = rest.trim().trim_matches('"').parse::<i32>() {
            if v > 0 {
                return Some(v);
            }
        }
    }
    None
}

/// Total registration splits when a provider adds it to WeekendInfo.
pub fn parse_race_split_total(yaml: &str) -> Option<i32> {
    for key in [
        "RaceSplitTotal",
        "TotalSplits",
        "NumSplits",
        "SplitCount",
        "SessionSplitCount",
    ] {
        if let Some(v) = yaml_i32(yaml, key) {
            if v > 0 {
                return Some(v);
            }
        }
    }
    None
}

fn yaml_i32(yaml: &str, key: &str) -> Option<i32> {
    let needle = format!("{key}:");
    for line in yaml.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&needle) {
            return rest.trim().trim_matches('"').parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_session_type_labels() {
        assert_eq!(format_session_type("practice".into()), "Practice");
        assert_eq!(format_session_type("open".into()), "Practice");
        assert_eq!(format_session_type("qualifying".into()), "Qualifying");
        assert_eq!(format_session_type("race".into()), "Race");
    }

    #[test]
    fn finite_laps_total_rejects_iracing_unlimited() {
        assert_eq!(finite_laps_total(50), Some(50));
        assert_eq!(finite_laps_total(0), None);
        assert_eq!(finite_laps_total(32_767), None);
        assert_eq!(finite_laps_total(100_000), None);
    }

    #[test]
    fn fmt_tod_wraps_midnight() {
        assert_eq!(fmt_tod(86_400.0), "00:00");
        assert_eq!(fmt_tod(14.0 * 3600.0 + 30.0 * 60.0), "14:30");
        assert_eq!(fmt_tod(-1.0), "—");
    }

    #[test]
    fn fmt_car_gap_leader_and_trailing() {
        assert_eq!(fmt_car_gap(1, 0.0), "LEADER");
        assert_eq!(fmt_car_gap(2, 1.25), "+1.2");
        assert_eq!(fmt_car_gap(3, 0.0), "");
    }

    #[test]
    fn map_session_flag_priority() {
        assert_eq!(
            map_session_flag_label(FLAG_REPAIR | FLAG_BLUE),
            Some("meatball".into())
        );
        assert_eq!(map_session_flag_label(FLAG_BLUE), Some("blue".into()));
    }

    #[test]
    fn parse_session_types_yaml() {
        let yaml = r#"
Sessions:
  - SessionNum: 0
    SessionType: Practice
  - SessionNum: 1
    SessionType: Race
"#;
        let types = parse_session_types(yaml);
        assert_eq!(types.len(), 2);
        assert_eq!(types[0], "Practice");
        assert_eq!(types[1], "Race");
    }

    #[test]
    fn parse_race_split_yaml() {
        let yaml = "RaceSplit: 2\nRaceSplitTotal: 5\nOther: 1\n";
        assert_eq!(parse_race_split(yaml), Some(2));
        assert_eq!(parse_race_split_total(yaml), Some(5));
    }
}
