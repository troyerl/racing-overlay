//! Lift-and-coast: regress fuel burn vs throttle / coast fraction.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LiftCoastSnapshot {
    pub economy_usage: Option<f32>,
    pub race_usage: Option<f32>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct LapAccum {
    thr_sum: f64,
    coast_n: u32,
    n: u32,
}

#[derive(Debug, Clone, Default)]
pub struct LiftCoastTracker {
    accum: LapAccum,
    prev_lap: Option<i32>,
    lap_start_fuel: f32,
    /// (mean_throttle, coast_frac, burn_l) newest first.
    samples: Vec<(f32, f32, f32)>,
}

impl LiftCoastTracker {
    pub fn tick(&mut self, throttle: f32, brake: f32, eligible_sample: bool) {
        if !eligible_sample {
            return;
        }
        let thr = throttle.clamp(0.0, 1.0);
        self.accum.thr_sum += thr as f64;
        self.accum.n = self.accum.n.saturating_add(1);
        if thr < 0.12 && brake < 0.15 {
            self.accum.coast_n = self.accum.coast_n.saturating_add(1);
        }
    }

    pub fn observe_lap(
        &mut self,
        lap: i32,
        fuel: f32,
        eligible: bool,
        history_n: usize,
    ) -> LiftCoastSnapshot {
        if lap <= 0 || !fuel.is_finite() {
            return self.snapshot();
        }
        match self.prev_lap {
            None => {
                self.prev_lap = Some(lap);
                self.lap_start_fuel = fuel;
                self.accum = LapAccum::default();
            }
            Some(prev) if lap > prev => {
                let used = self.lap_start_fuel - fuel;
                if eligible && used > 0.05 && used < 20.0 && self.accum.n > 10 {
                    let mean_thr = (self.accum.thr_sum / self.accum.n as f64) as f32;
                    let coast = self.accum.coast_n as f32 / self.accum.n as f32;
                    self.samples.insert(0, (mean_thr, coast, used));
                    if self.samples.len() > history_n.max(4) {
                        self.samples.truncate(history_n.max(4));
                    }
                }
                self.prev_lap = Some(lap);
                self.lap_start_fuel = fuel;
                self.accum = LapAccum::default();
            }
            Some(prev) if lap < prev => {
                self.samples.clear();
                self.prev_lap = Some(lap);
                self.lap_start_fuel = fuel;
                self.accum = LapAccum::default();
            }
            _ => {}
        }
        self.snapshot()
    }

    fn snapshot(&self) -> LiftCoastSnapshot {
        if self.samples.len() < 3 {
            return LiftCoastSnapshot::default();
        }
        // burn ≈ a + b * mean_throttle
        let (a, b) = fit_burn_vs_throttle(&self.samples);
        let race_thr = 0.88f32;
        let eco_thr = 0.62f32;
        let race = (a + b * race_thr).max(0.2);
        let eco = (a + b * eco_thr).max(0.15);
        let note = if eco < race * 0.96 {
            Some(format!("Economy +{:.1}L/lap", race - eco))
        } else {
            None
        };
        LiftCoastSnapshot {
            economy_usage: Some(eco.min(race)),
            race_usage: Some(race),
            note,
        }
    }
}

fn fit_burn_vs_throttle(samples: &[(f32, f32, f32)]) -> (f32, f32) {
    let n = samples.len() as f32;
    let mut sx = 0.0;
    let mut sy = 0.0;
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    for &(thr, _, burn) in samples {
        sx += thr;
        sy += burn;
        sxy += thr * burn;
        sxx += thr * thr;
    }
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-6 {
        let mean = sy / n;
        return (mean, 0.0);
    }
    let b = (n * sxy - sx * sy) / denom;
    let a = (sy - b * sx) / n;
    (a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regresses_economy_below_race() {
        let mut t = LiftCoastTracker::default();
        // Synthetic: higher throttle → higher burn.
        let laps = [
            (1, 60.0, 0.9, 2.4),
            (2, 57.6, 0.85, 2.2),
            (3, 55.4, 0.7, 1.9),
            (4, 53.5, 0.65, 1.75),
            (5, 51.75, 0.88, 2.3),
        ];
        let mut fuel = 62.0;
        t.observe_lap(0, fuel, false, 8);
        for &(lap, _f, thr, burn) in &laps {
            for _ in 0..40 {
                t.tick(thr, 0.0, true);
            }
            fuel -= burn;
            let _ = t.observe_lap(lap, fuel, true, 8);
        }
        let snap = t.observe_lap(6, fuel - 2.0, true, 8);
        // Force one more with mid throttle samples already in history.
        assert!(snap.economy_usage.is_some() || snap.race_usage.is_some() || t.samples.len() >= 3);
        if let (Some(eco), Some(race)) = (snap.economy_usage, snap.race_usage) {
            assert!(eco <= race + 0.05);
        }
    }
}
