//! Last-pit tracking for table `pit_mode` column (Python `_update_pit_tracking` / `_pit_text`).

use std::collections::HashMap;

use crate::config::OverlayConfig;

use super::tables::TableRow;
use super::{CarRow, TelemetryFrame};

#[derive(Debug, Clone, Default)]
struct PitCarState {
    on: bool,
    lap: Option<i32>,
    time: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct PitStopTracker {
    cars: HashMap<i32, PitCarState>,
}

impl PitStopTracker {
    /// Rising edge of OnPitRoad (or stall/approaching fallback) records lap + session time.
    pub fn observe(&mut self, cars: &[CarRow], session_time: f64) {
        let mut seen = HashMap::new();
        for c in cars {
            seen.insert(c.car_idx, ());
            let now_on = c.on_pit || c.in_pit;
            let st = self.cars.entry(c.car_idx).or_default();
            if now_on && !st.on {
                st.lap = if c.lap > 0 { Some(c.lap) } else { None };
                st.time = if session_time.is_finite() && session_time >= 0.0 {
                    Some(session_time)
                } else {
                    None
                };
            }
            st.on = now_on;
        }
        self.cars.retain(|k, _| seen.contains_key(k));
    }

    pub fn apply_frame(&self, frame: &mut TelemetryFrame, cfg: &OverlayConfig) {
        let sess = frame.session_time;
        fill_rows(
            &mut frame.relative_cars,
            &self.cars,
            sess,
            cfg.str_key("relative", "pit_mode", "laps_since").as_str(),
        );
        fill_rows(
            &mut frame.standings_cars,
            &self.cars,
            sess,
            cfg.str_key("standings", "pit_mode", "laps_since").as_str(),
        );
    }
}

fn fill_rows(rows: &mut [TableRow], cars: &HashMap<i32, PitCarState>, sess_time: f64, mode: &str) {
    for row in rows.iter_mut() {
        if row.empty {
            continue;
        }
        let Ok(idx) = row.key.parse::<i32>() else {
            row.pit_text.clear();
            continue;
        };
        row.pit_text = pit_text(cars.get(&idx), row.laps, sess_time, mode);
    }
}

fn pit_text(st: Option<&PitCarState>, car_lap: i32, sess_time: f64, mode: &str) -> String {
    let Some(st) = st else {
        return String::new();
    };
    if st.lap.is_none() && st.time.is_none() {
        return String::new();
    }
    match mode {
        "laps_since" => {
            if let Some(pit_lap) = st.lap {
                if car_lap > 0 {
                    return format!("{}L", (car_lap - pit_lap).max(0));
                }
            }
            String::new()
        }
        "time_since" => {
            if let Some(t) = st.time {
                if sess_time.is_finite() {
                    return fmt_clock((sess_time - t).max(0.0));
                }
            }
            String::new()
        }
        "at_lap" => st.lap.map(|l| format!("L{l}")).unwrap_or_default(),
        "at_time" => st.time.map(fmt_clock).unwrap_or_default(),
        _ => String::new(),
    }
}

fn fmt_clock(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "--:--".into();
    }
    let secs = secs as i64;
    let h = secs / 3600;
    let rem = secs % 3600;
    let m = rem / 60;
    let s = rem % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}
