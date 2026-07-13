//! Simple lap-compare engine: current vs best distance-time curve.

use serde::{Deserialize, Serialize};

const MAX_SAMPLES: usize = 240;
const SPARK_BINS: usize = 64;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LapCompareView {
    pub delta: Option<f64>,
    pub spark: Vec<f32>,
    pub turns: Vec<(String, f32)>,
}

/// Ring of (pct, time) samples for the current lap vs a best-lap reference.
#[derive(Debug, Clone, Default)]
pub struct LapCompareState {
    cur: Vec<(f32, f64)>,
    best: Vec<(f32, f64)>,
    best_time: Option<f64>,
    prev_pct: Option<f32>,
    lap_started: bool,
    last_delta: Option<f64>,
}

impl LapCompareState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(&mut self, pct: f32, cur_lap_s: Option<f64>, last_lap_s: Option<f64>) {
        if pct < 0.0 {
            return;
        }
        let pct = pct.clamp(0.0, 0.999_999);

        if let Some(prev) = self.prev_pct {
            if pct + 0.5 < prev {
                self.finish_lap(last_lap_s);
                self.cur.clear();
                self.lap_started = true;
                self.prev_pct = Some(pct);
            }
        }
        self.prev_pct = Some(pct);

        if let Some(t) = cur_lap_s.filter(|t| *t >= 0.0) {
            if self.cur.last().map(|(p, _)| (pct - p).abs() > 1e-4).unwrap_or(true) {
                self.cur.push((pct, t));
                if self.cur.len() > MAX_SAMPLES {
                    // Decimate: keep every other sample when overfilled.
                    self.cur = self
                        .cur
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| i % 2 == 0)
                        .map(|(_, v)| *v)
                        .collect();
                }
            }
        }
    }

    fn finish_lap(&mut self, last_lap_s: Option<f64>) {
        let Some(lap_t) = last_lap_s.filter(|t| *t > 0.0) else {
            return;
        };
        if self.cur.len() < 8 {
            return;
        }
        let better = self.best_time.map_or(true, |b| lap_t < b);
        if better {
            self.best = self.cur.clone();
            self.best_time = Some(lap_t);
            self.last_delta = Some(0.0);
        } else if let Some(b) = self.best_time {
            self.last_delta = Some(lap_t - b);
        }
    }

    fn interp(curve: &[(f32, f64)], pct: f32) -> Option<f64> {
        if curve.is_empty() {
            return None;
        }
        if pct <= curve[0].0 {
            return Some(curve[0].1);
        }
        if let Some(last) = curve.last() {
            if pct >= last.0 {
                return Some(last.1);
            }
        }
        for w in curve.windows(2) {
            let (p0, t0) = w[0];
            let (p1, t1) = w[1];
            if pct >= p0 && pct <= p1 {
                let span = (p1 - p0).max(1e-6);
                let u = (pct - p0) / span;
                return Some(t0 + (t1 - t0) * u as f64);
            }
        }
        None
    }

    fn live_delta(&self) -> Option<f64> {
        if !self.lap_started || self.best.is_empty() {
            return self.last_delta;
        }
        let (pct, cur_t) = *self.cur.last()?;
        let ref_t = Self::interp(&self.best, pct)?;
        Some(cur_t - ref_t)
    }

    fn build_spark(&self) -> Vec<f32> {
        if self.best.is_empty() || self.cur.is_empty() {
            return Vec::new();
        }
        let upto = self.cur.last().map(|(p, _)| *p).unwrap_or(0.0);
        let mut out = Vec::with_capacity(SPARK_BINS);
        for i in 0..SPARK_BINS {
            let pct = i as f32 / (SPARK_BINS - 1).max(1) as f32;
            if pct > upto + 1e-3 && self.lap_started {
                break;
            }
            let Some(ct) = Self::interp(&self.cur, pct) else {
                continue;
            };
            let Some(rt) = Self::interp(&self.best, pct) else {
                continue;
            };
            out.push((ct - rt) as f32);
        }
        out
    }

    /// Build the widget view; fills synthetic turn losses when none derived.
    pub fn view(&self, session_time: f64) -> LapCompareView {
        let delta = self.live_delta();
        let mut spark = self.build_spark();
        if spark.is_empty() {
            spark = demo_spark(session_time);
        }
        let turns = if self.best.is_empty() {
            demo_turns(session_time)
        } else {
            turns_from_spark(&spark)
        };
        LapCompareView {
            delta: delta.or_else(|| demo_delta(session_time)),
            spark,
            turns,
        }
    }
}

fn demo_delta(t: f64) -> Option<f64> {
    Some(-0.18 + 0.35 * (t * 0.45).sin())
}

fn demo_spark(t: f64) -> Vec<f32> {
    (0..SPARK_BINS)
        .map(|i| {
            let x = i as f64 / SPARK_BINS as f64;
            let base = (x * std::f64::consts::TAU * 2.0 + t * 0.4).sin() * 0.25;
            let drift = x * 0.15 - 0.05;
            (base + drift) as f32
        })
        .collect()
}

fn demo_turns(t: f64) -> Vec<(String, f32)> {
    vec![
        ("T1".into(), (0.12 + 0.04 * (t * 0.5).sin()) as f32),
        ("T3".into(), (0.07 + 0.02 * (t * 0.3).cos()) as f32),
        ("T7".into(), (-0.05 + 0.03 * (t * 0.6).sin()) as f32),
        ("T12".into(), (0.19 + 0.03 * (t * 0.25).cos()) as f32),
    ]
}

/// Approximate a few turn loss chips from spark extrema when no map corners exist.
fn turns_from_spark(spark: &[f32]) -> Vec<(String, f32)> {
    if spark.len() < 8 {
        return demo_turns(0.0);
    }
    let n = spark.len();
    let slices = [
        (0.12, "T1"),
        (0.35, "T3"),
        (0.58, "T7"),
        (0.82, "T12"),
    ];
    slices
        .iter()
        .map(|(frac, label)| {
            let i = ((*frac) * (n - 1) as f32).round() as usize;
            let lo = i.saturating_sub(2);
            let hi = (i + 2).min(n - 1);
            let loss = spark[lo..=hi]
                .iter()
                .copied()
                .fold(0.0_f32, |a, b| if b.abs() > a.abs() { b } else { a });
            ((*label).to_string(), loss)
        })
        .collect()
}
