//! Last-pit tracking + opponent splash / stint-due inference.

use std::collections::HashMap;

use crate::config::OverlayConfig;

use super::tables::TableRow;
use super::{CarRow, TelemetryFrame};

#[derive(Debug, Clone, Default)]
struct PitCarState {
    on: bool,
    lap: Option<i32>,
    time: Option<f64>,
    /// Session time when this visit started.
    entry_time: Option<f64>,
    /// Last completed stop duration (seconds).
    last_duration_s: Option<f32>,
    /// Inferred: splash | full | tires
    last_kind: Option<&'static str>,
}

#[derive(Debug, Clone, Default)]
pub struct PitStopTracker {
    cars: HashMap<i32, PitCarState>,
}

#[derive(Debug, Clone, Default)]
pub struct OpponentFieldNote {
    pub note: Option<String>,
    pub ahead_due: bool,
    pub ahead_splash: bool,
}

impl PitStopTracker {
    /// Rising edge of OnPitRoad records lap + session time; falling edge measures duration.
    pub fn observe(&mut self, cars: &[CarRow], session_time: f64, splash_max_s: f32) {
        let splash_max = splash_max_s.clamp(5.0, 40.0);
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
                st.entry_time = st.time;
            } else if !now_on && st.on {
                if let (Some(entry), true) = (st.entry_time.take(), session_time.is_finite()) {
                    let dt = (session_time - entry) as f32;
                    if (4.0..150.0).contains(&dt) {
                        st.last_duration_s = Some(dt);
                        st.last_kind = Some(if dt <= splash_max {
                            "splash"
                        } else if dt >= splash_max + 8.0 {
                            "tires"
                        } else {
                            "full"
                        });
                    }
                }
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

    /// Field context vs nearest car ahead for undercut pressure.
    pub fn field_note(
        &self,
        cars: &[CarRow],
        player_idx: i32,
        stint_due_laps: i32,
        show: bool,
    ) -> OpponentFieldNote {
        if !show {
            return OpponentFieldNote::default();
        }
        let player = cars.iter().find(|c| c.car_idx == player_idx);
        let Some(player) = player else {
            return OpponentFieldNote::default();
        };
        // Nearest ahead by lap distance among same-class-ish live cars.
        let mut best: Option<(&CarRow, f32)> = None;
        for c in cars {
            if c.car_idx == player_idx || !c.is_live_competitor() || c.on_pit {
                continue;
            }
            let mut d = c.lap_dist_pct - player.lap_dist_pct;
            if d <= 0.0 {
                d += 1.0;
            }
            if d > 0.55 {
                continue; // too far ahead on track
            }
            if best.map(|(_, bd)| d < bd).unwrap_or(true) {
                best = Some((c, d));
            }
        }
        let Some((ahead, _)) = best else {
            return OpponentFieldNote::default();
        };
        let st = self.cars.get(&ahead.car_idx);
        let laps_since = st
            .and_then(|s| s.lap)
            .map(|pl| (ahead.lap - pl).max(0))
            .unwrap_or(0);
        let due = stint_due_laps > 0 && laps_since >= stint_due_laps;
        let splash = st
            .and_then(|s| s.last_kind)
            .map(|k| k == "splash")
            .unwrap_or(false)
            && laps_since <= 3;

        let note = if due {
            Some(format!("#{} due for stop", ahead.car_number))
        } else if splash {
            Some(format!("#{} splash", ahead.car_number))
        } else if laps_since > 0 {
            Some(format!("#{} {}L since pit", ahead.car_number, laps_since))
        } else {
            None
        };
        OpponentFieldNote {
            note,
            ahead_due: due,
            ahead_splash: splash,
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_splash_vs_full() {
        let mut t = PitStopTracker::default();
        let mut cars = vec![CarRow {
            car_idx: 1,
            on_pit: false,
            lap: 10,
            ..Default::default()
        }];
        t.observe(&cars, 100.0, 12.0);
        cars[0].on_pit = true;
        t.observe(&cars, 200.0, 12.0);
        cars[0].on_pit = false;
        t.observe(&cars, 208.0, 12.0); // 8s splash
        assert_eq!(t.cars[&1].last_kind, Some("splash"));
        cars[0].on_pit = true;
        t.observe(&cars, 300.0, 12.0);
        cars[0].on_pit = false;
        t.observe(&cars, 330.0, 12.0); // 30s
        assert_eq!(t.cars[&1].last_kind, Some("tires"));
    }
}
