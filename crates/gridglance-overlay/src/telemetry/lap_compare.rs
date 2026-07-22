//! Lap-compare engine: current vs best/last distance-time curve + input markers.

use serde::{Deserialize, Serialize};

const MAX_SAMPLES: usize = 240;
const SPARK_BINS: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MarkerKind {
    Brake,
    Lift,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareMarker {
    /// Lap distance 0..1.
    pub pct: f32,
    pub kind: MarkerKind,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LapCompareView {
    pub delta: Option<f64>,
    pub spark: Vec<f32>,
    pub turns: Vec<(String, f32)>,
    /// "VS BEST" / "VS LAST".
    #[serde(default)]
    pub ref_label: String,
    #[serde(default)]
    pub markers: Vec<CompareMarker>,
}

/// Ring of (pct, time, brake, throttle) samples for the current lap vs a reference.
#[derive(Debug, Clone, Default)]
pub struct LapCompareState {
    cur: Vec<(f32, f64, f32, f32)>,
    best: Vec<(f32, f64, f32, f32)>,
    last: Vec<(f32, f64, f32, f32)>,
    best_time: Option<f64>,
    prev_pct: Option<f32>,
    lap_started: bool,
    last_delta: Option<f64>,
    prev_brake: f32,
    prev_throttle: f32,
}

impl LapCompareState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(
        &mut self,
        pct: f32,
        cur_lap_s: Option<f64>,
        last_lap_s: Option<f64>,
        brake: f32,
        throttle: f32,
        _ref_mode: &str,
    ) {
        if pct < 0.0 {
            return;
        }
        let pct = pct.clamp(0.0, 0.999_999);
        let brake = brake.clamp(0.0, 1.0);
        let throttle = throttle.clamp(0.0, 1.0);

        if let Some(prev) = self.prev_pct {
            if pct + 0.5 < prev {
                self.finish_lap(last_lap_s);
                self.cur.clear();
                self.lap_started = true;
                self.prev_pct = Some(pct);
                self.prev_brake = brake;
                self.prev_throttle = throttle;
                return;
            }
        }
        self.prev_pct = Some(pct);

        if let Some(t) = cur_lap_s.filter(|t| *t >= 0.0) {
            if self
                .cur
                .last()
                .map(|(p, _, _, _)| (pct - p).abs() > 1e-4)
                .unwrap_or(true)
            {
                self.cur.push((pct, t, brake, throttle));
                if self.cur.len() > MAX_SAMPLES {
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
        self.prev_brake = brake;
        self.prev_throttle = throttle;
    }

    fn finish_lap(&mut self, last_lap_s: Option<f64>) {
        let Some(lap_t) = last_lap_s.filter(|t| *t > 0.0) else {
            return;
        };
        if self.cur.len() < 8 {
            return;
        }
        self.last = self.cur.clone();
        let better = self.best_time.is_none_or(|b| lap_t < b);
        if better {
            self.best = self.cur.clone();
            self.best_time = Some(lap_t);
            self.last_delta = Some(0.0);
        } else if let Some(b) = self.best_time {
            self.last_delta = Some(lap_t - b);
        }
    }

    fn ref_curve<'a>(&'a self, mode: &str) -> &'a [(f32, f64, f32, f32)] {
        if (mode.eq_ignore_ascii_case("last_lap") || mode.eq_ignore_ascii_case("last"))
            && !self.last.is_empty()
        {
            return &self.last;
        }
        &self.best
    }

    fn interp(curve: &[(f32, f64, f32, f32)], pct: f32) -> Option<f64> {
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
            let (p0, t0, _, _) = w[0];
            let (p1, t1, _, _) = w[1];
            if pct >= p0 && pct <= p1 {
                let span = (p1 - p0).max(1e-6);
                let u = (pct - p0) / span;
                return Some(t0 + (t1 - t0) * u as f64);
            }
        }
        None
    }

    fn live_delta(&self, mode: &str) -> Option<f64> {
        let reference = self.ref_curve(mode);
        if !self.lap_started || reference.is_empty() {
            return self.last_delta;
        }
        let (pct, cur_t, _, _) = *self.cur.last()?;
        let ref_t = Self::interp(reference, pct)?;
        Some(cur_t - ref_t)
    }

    fn build_spark(&self, mode: &str) -> Vec<f32> {
        let reference = self.ref_curve(mode);
        if reference.is_empty() || self.cur.is_empty() {
            return Vec::new();
        }
        let upto = self.cur.last().map(|(p, _, _, _)| *p).unwrap_or(0.0);
        let mut out = Vec::with_capacity(SPARK_BINS);
        for i in 0..SPARK_BINS {
            let pct = i as f32 / (SPARK_BINS - 1).max(1) as f32;
            if pct > upto + 1e-3 && self.lap_started {
                break;
            }
            let Some(ct) = Self::interp(&self.cur, pct) else {
                continue;
            };
            let Some(rt) = Self::interp(reference, pct) else {
                continue;
            };
            out.push((ct - rt) as f32);
        }
        out
    }

    fn build_markers(&self) -> Vec<CompareMarker> {
        let mut out = Vec::new();
        let mut prev_brk = 0.0_f32;
        let mut prev_thr = 0.0_f32;
        for &(pct, _, brk, thr) in &self.cur {
            // Brake onset: 0 → ≥0.15
            if prev_brk < 0.05 && brk >= 0.15 {
                out.push(CompareMarker {
                    pct,
                    kind: MarkerKind::Brake,
                });
            }
            // Lift: throttle drops while no significant brake.
            if prev_thr >= 0.40 && thr <= prev_thr - 0.20 && brk < 0.10 {
                out.push(CompareMarker {
                    pct,
                    kind: MarkerKind::Lift,
                });
            }
            prev_brk = brk;
            prev_thr = thr;
        }
        // Cap density.
        if out.len() > 24 {
            let step = out.len() / 24;
            out = out.into_iter().step_by(step.max(1)).collect();
        }
        out
    }

    /// Build the widget view; fills synthetic turn losses when none derived.
    /// When `allow_demo` is false (live iRacing), empty/missing data stays empty.
    pub fn view(&self, session_time: f64, ref_mode: &str, allow_demo: bool) -> LapCompareView {
        let use_last =
            ref_mode.eq_ignore_ascii_case("last_lap") || ref_mode.eq_ignore_ascii_case("last");
        let ref_label = if use_last {
            "VS LAST".into()
        } else {
            "VS BEST".into()
        };
        let delta = self.live_delta(ref_mode);
        let mut spark = self.build_spark(ref_mode);
        if spark.is_empty() && allow_demo {
            spark = demo_spark(session_time);
        }
        let turns = if self.ref_curve(ref_mode).is_empty() {
            if allow_demo {
                demo_turns(session_time)
            } else {
                Vec::new()
            }
        } else {
            turns_from_spark(&spark, allow_demo)
        };
        let mut markers = self.build_markers();
        if markers.is_empty() && self.cur.is_empty() && allow_demo {
            markers = demo_markers(session_time);
        }
        LapCompareView {
            delta: delta.or_else(|| {
                if allow_demo {
                    demo_delta(session_time)
                } else {
                    None
                }
            }),
            spark,
            turns,
            ref_label,
            markers,
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

fn demo_markers(t: f64) -> Vec<CompareMarker> {
    vec![
        CompareMarker {
            pct: (0.18 + 0.02 * (t * 0.2).sin()) as f32,
            kind: MarkerKind::Brake,
        },
        CompareMarker {
            pct: (0.42 + 0.01 * (t * 0.3).cos()) as f32,
            kind: MarkerKind::Lift,
        },
        CompareMarker {
            pct: 0.71,
            kind: MarkerKind::Brake,
        },
    ]
}

/// Approximate a few turn loss chips from spark extrema when no map corners exist.
fn turns_from_spark(spark: &[f32], allow_demo: bool) -> Vec<(String, f32)> {
    if spark.len() < 8 {
        return if allow_demo {
            demo_turns(0.0)
        } else {
            Vec::new()
        };
    }
    let n = spark.len();
    let slices = [(0.12, "T1"), (0.35, "T3"), (0.58, "T7"), (0.82, "T12")];
    slices
        .iter()
        .map(|(frac, label)| {
            let i = ((*frac) * (n - 1) as f32).round() as usize;
            let lo = i.saturating_sub(2);
            let hi = (i + 2).min(n - 1);
            let loss =
                spark[lo..=hi]
                    .iter()
                    .copied()
                    .fold(0.0_f32, |a, b| if b.abs() > a.abs() { b } else { a });
            ((*label).into(), loss)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_view_skips_demo_fallback() {
        let state = LapCompareState::new();
        let view = state.view(10.0, "best", false);
        assert!(view.spark.is_empty());
        assert!(view.turns.is_empty());
        assert!(view.delta.is_none());
        assert!(view.markers.is_empty());
    }
}
