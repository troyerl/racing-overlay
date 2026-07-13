//! Lap-history accumulator for the laptime log widget.
//!
//! Host / IRSDK can call [`LapLogAccum::push_completed`] when a player lap
//! finishes, then [`LapLogAccum::build_rows`] to fill `TelemetryFrame::lap_log`.

#![allow(dead_code)] // wired by host later; keep for demo / IRSDK integration

use super::LapLogRow;
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
        match self.prev_lap {
            None => {
                self.prev_lap = Some(current_lap);
                false
            }
            Some(prev) if current_lap > prev => {
                let completed = current_lap - 1;
                let mut inserted = false;
                if let Some(secs) = last_lap_s.filter(|s| *s > 0.0) {
                    let already = self.laps.first().map(|l| l.lap) == Some(completed);
                    if !already {
                        self.laps.insert(
                            0,
                            CompletedLap {
                                lap: completed,
                                secs,
                                temp_c: track_temp_c,
                                fuel_l: extras.fuel_l,
                                tires: extras.tires,
                                incidents: extras.incidents,
                                tag: extras.tag,
                                personal_best: extras.personal_best,
                            },
                        );
                        self.laps.truncate(MAX_STORED);
                        inserted = true;
                    }
                }
                self.prev_lap = Some(current_lap);
                inserted
            }
            Some(prev) if current_lap < prev => {
                self.laps.clear();
                self.prev_lap = Some(current_lap);
                true
            }
            _ => false,
        }
    }

    /// Manually push a completed lap (newest first).
    pub fn push_completed(&mut self, lap: CompletedLap) {
        if self.laps.first().map(|l| l.lap) == Some(lap.lap) {
            return;
        }
        self.laps.insert(0, lap);
        self.laps.truncate(MAX_STORED);
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
                    fmt_lap_time(l.secs),
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
    pub tires: Option<i32>,
    pub incidents: Option<i32>,
    pub tag: Option<String>,
    pub personal_best: Option<f64>,
}

fn fmt_lap_time(secs: f64) -> String {
    if secs <= 0.0 {
        return "—".into();
    }
    let m = (secs / 60.0).floor() as i32;
    let s = secs - m as f64 * 60.0;
    format!("{m:02}:{s:06.3}")
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
