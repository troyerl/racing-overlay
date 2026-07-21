//! Lap-history accumulator for the laptime log widget.

use super::format;
use super::{LapLogRow, TelemetryFrame};
use crate::config::OverlayConfig;

const MAX_STORED: usize = 60;

/// Raw completed lap used to rebuild display rows when CFG changes.
#[derive(Debug, Clone)]
pub struct CompletedLap {
    pub lap: i32,
    pub secs: f64,
    pub temp_c: Option<f32>,
    pub fuel_l: Option<f32>,
    pub tires: Option<i32>,
    pub incidents: Option<i32>,
    pub tag: Option<String>,
    pub personal_best: Option<f64>,
}

/// Tracks completed player laps (newest first) for the laptime log.
#[derive(Debug, Clone, Default)]
pub struct LapLogAccum {
    pub laps: Vec<CompletedLap>,
    prev_lap: Option<i32>,
    lap_start_incidents: i32,
    lap_tag: Option<String>,
    lap_on_pit: bool,
}

impl LapLogAccum {
    pub fn new() -> Self {
        Self::default()
    }

    /// Detect a lap transition and record the completed lap when `current_lap`
    /// advances. Returns `true` when a new entry was inserted.
    pub fn observe(
        &mut self,
        current_lap: i32,
        last_lap_s: Option<f64>,
        track_temp_c: Option<f32>,
        extras: LapExtras,
    ) -> bool {
        // Track pit/out tag across the lap (Python `_ll_lap_tag`).
        if extras.on_pit {
            self.lap_tag = Some("PIT".into());
        } else if self.lap_on_pit {
            self.lap_tag = Some("OUT".into());
        }
        self.lap_on_pit = extras.on_pit;

        match self.prev_lap {
            None => {
                self.prev_lap = Some(current_lap);
                self.lap_start_incidents = extras.incidents.unwrap_or(0);
                false
            }
            Some(prev) if current_lap > prev => {
                let completed = current_lap - 1;
                let mut inserted = false;
                if let Some(secs) = last_lap_s.filter(|s| *s > 0.0) {
                    let already = self.laps.first().map(|l| l.lap) == Some(completed);
                    if !already {
                        let start = self.lap_start_incidents;
                        let now = extras.incidents.unwrap_or(start);
                        let delta = (now - start).max(0);
                        self.laps.insert(
                            0,
                            CompletedLap {
                                lap: completed,
                                secs,
                                temp_c: track_temp_c,
                                fuel_l: extras.fuel_l,
                                tires: extras.tires,
                                incidents: Some(delta),
                                tag: self.lap_tag.clone(),
                                personal_best: extras.personal_best,
                            },
                        );
                        self.laps.truncate(MAX_STORED);
                        inserted = true;
                    }
                }
                self.prev_lap = Some(current_lap);
                self.lap_tag = None;
                self.lap_start_incidents = extras.incidents.unwrap_or(0);
                inserted
            }
            Some(prev) if current_lap < prev => {
                self.laps.clear();
                self.prev_lap = Some(current_lap);
                self.lap_tag = None;
                self.lap_start_incidents = extras.incidents.unwrap_or(0);
                true
            }
            _ => false,
        }
    }

    /// Manually push a completed lap (newest first).
    #[allow(dead_code)]
    pub fn push_completed(&mut self, lap: CompletedLap) {
        if self.laps.first().map(|l| l.lap) == Some(lap.lap) {
            return;
        }
        self.laps.insert(0, lap);
        self.laps.truncate(MAX_STORED);
    }

    /// Seed a few historical laps so the laptime log isn't empty at demo start.
    pub fn seed_demo(&mut self, current_lap: i32) {
        if !self.laps.is_empty() {
            return;
        }
        let best = 87.90;
        let seeds: &[(i32, f64, Option<&str>)] = &[
            (current_lap - 1, 88.11, None),
            (current_lap - 2, 88.40, None),
            (current_lap - 3, 88.23, None),
            (current_lap - 4, 88.51, Some("OUT")),
            (current_lap - 5, 87.90, Some("PIT")),
            (current_lap - 6, 88.31, None),
        ];
        for (lap, secs, tag) in seeds {
            if *lap < 1 {
                continue;
            }
            self.laps.push(CompletedLap {
                lap: *lap,
                secs: *secs,
                temp_c: Some(31.5),
                fuel_l: Some(2.4),
                tires: Some(92),
                incidents: Some(0),
                tag: tag.map(|s| s.to_string()),
                personal_best: Some(best),
            });
        }
        self.prev_lap = Some(current_lap);
    }

    /// Build widget rows from stored laps using CFG `rows` / `delta_mode`.
    pub fn build_rows(&self, cfg: &OverlayConfig) -> Vec<LapLogRow> {
        let n = cfg.f64_key("laptime_log", "rows", 8.0).max(1.0) as usize;
        let mode = cfg.str_key("laptime_log", "delta_mode", "previous");
        let best = self
            .laps
            .iter()
            .filter(|l| l.secs > 0.0)
            .map(|l| l.secs)
            .fold(None, |acc: Option<f64>, s| {
                Some(acc.map_or(s, |b| b.min(s)))
            });

        self.laps
            .iter()
            .take(n)
            .enumerate()
            .map(|(i, l)| {
                let delta_s = match mode.as_str() {
                    "best" => best
                        .filter(|_| l.secs > 0.0)
                        .map(|b| (l.secs - b) as f32)
                        .filter(|d| d.abs() >= 1e-4),
                    "personal_best" => l
                        .personal_best
                        .filter(|pb| *pb > 0.0 && l.secs > 0.0)
                        .map(|pb| (l.secs - pb) as f32)
                        .filter(|d| d.abs() >= 1e-4),
                    _ => self
                        .laps
                        .get(i + 1)
                        .filter(|p| p.secs > 0.0 && l.secs > 0.0)
                        .map(|p| (l.secs - p.secs) as f32),
                };
                LapLogRow::from_parts(
                    l.lap,
                    format::fmt_laptime_log(l.secs),
                    delta_s,
                    fmt_temp(cfg, l.temp_c),
                    l.fuel_l.map(|f| format!("{f:.1}")),
                    l.tires.map(|t| t.to_string()),
                    l.incidents.map(|n| n.to_string()),
                    l.tag.clone(),
                )
            })
            .collect()
    }
}

/// Optional per-lap extras passed into [`LapLogAccum::observe`].
#[derive(Debug, Clone, Default)]
pub struct LapExtras {
    pub fuel_l: Option<f32>,
    /// Tire wear percent (0–100) snapshot at lap completion, when known.
    pub tires: Option<i32>,
    /// Running incident total; accumulator stores the per-lap delta.
    pub incidents: Option<i32>,
    pub personal_best: Option<f64>,
    /// Player currently on pit road (drives PIT/OUT tag).
    pub on_pit: bool,
}

impl LapExtras {
    pub fn from_frame(frame: &TelemetryFrame) -> Self {
        let on_pit = frame
            .cars
            .iter()
            .find(|c| c.is_player)
            .map(|c| c.on_pit || c.in_pit)
            .unwrap_or(false);
        let tires = tire_wear_pct_snapshot(frame);
        Self {
            fuel_l: Some(frame.fuel_l),
            tires,
            incidents: Some(frame.incidents),
            personal_best: frame.best_lap_s,
            on_pit,
        }
    }
}

fn tire_wear_pct_snapshot(frame: &TelemetryFrame) -> Option<i32> {
    let wears: Vec<f32> = frame
        .tire_corners
        .iter()
        .filter_map(|c| c.wear)
        .filter(|w| w.is_finite() && *w >= 0.0)
        .map(|w| if w <= 1.0 { w * 100.0 } else { w })
        .collect();
    if wears.is_empty() {
        None
    } else {
        let min = wears.iter().cloned().fold(f32::INFINITY, f32::min);
        Some(min.round() as i32)
    }
}

fn fmt_temp(cfg: &OverlayConfig, c: Option<f32>) -> String {
    match c {
        Some(c) => {
            let t = cfg.conv_temp(c);
            format!("{t:.1}{}", cfg.temp_unit())
        }
        None => "—".into(),
    }
}

/// Format a signed delta for display (1 decimal place, Python parity).
pub fn signed_delta_1(d: f32) -> String {
    let sign = if d < 0.0 { '-' } else { '+' };
    format!("{sign}{:.1}", d.abs())
}

/// Parse a display delta string (`"+0.17"`, `"-0.1"`) into seconds.
pub fn parse_delta_str(s: &str) -> Option<f32> {
    let t = s.trim();
    if t.is_empty() || t == "—" || t == "–" || t == "-" || t == "--" {
        return None;
    }
    t.parse::<f32>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pit_out_tag_on_lap_complete() {
        let mut acc = LapLogAccum::new();
        acc.observe(
            1,
            None,
            None,
            LapExtras {
                incidents: Some(0),
                ..Default::default()
            },
        );
        assert!(acc.observe(
            2,
            Some(88.0),
            None,
            LapExtras {
                on_pit: true,
                incidents: Some(0),
                ..Default::default()
            }
        ));
        assert_eq!(acc.laps[0].tag.as_deref(), Some("PIT"));

        assert!(acc.observe(
            3,
            Some(88.5),
            None,
            LapExtras {
                on_pit: false,
                incidents: Some(0),
                ..Default::default()
            }
        ));
        assert_eq!(acc.laps[0].tag.as_deref(), Some("OUT"));
    }

    #[test]
    fn incident_delta_per_lap() {
        let mut acc = LapLogAccum::new();
        acc.observe(
            1,
            None,
            None,
            LapExtras {
                incidents: Some(2),
                ..Default::default()
            },
        );
        assert!(acc.observe(
            2,
            Some(87.0),
            None,
            LapExtras {
                incidents: Some(5),
                ..Default::default()
            }
        ));
        assert_eq!(acc.laps[0].incidents, Some(3));
    }
}
