//! Green-lap pace degradation with fuel-mass normalization.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PaceSnapshot {
    pub loss_per_lap: Option<f32>,
    pub window_open: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PaceModel {
    prev_lap: Option<i32>,
    /// Newest-first normalized green lap times (seconds).
    norms: Vec<f32>,
    fuel_at_lap: Vec<f32>,
}

impl PaceModel {
    pub fn observe(
        &mut self,
        lap: i32,
        last_lap_s: Option<f64>,
        fuel_l: f32,
        eligible: bool,
        mass_s_per_l: f32,
        history_n: usize,
        pit_loss_s: f32,
        laps_remain: Option<f32>,
    ) -> PaceSnapshot {
        if lap <= 0 {
            return PaceSnapshot::default();
        }
        match self.prev_lap {
            None => self.prev_lap = Some(lap),
            Some(prev) if lap > prev => {
                if eligible {
                    if let Some(raw) = last_lap_s.map(|s| s as f32).filter(|s| *s > 10.0) {
                        // Normalize toward empty-tank pace: subtract mass penalty of current fuel.
                        let norm = raw - fuel_l.max(0.0) * mass_s_per_l.max(0.0);
                        if norm > 10.0 {
                            self.norms.insert(0, norm);
                            self.fuel_at_lap.insert(0, fuel_l);
                            let n = history_n.max(3);
                            if self.norms.len() > n {
                                self.norms.truncate(n);
                                self.fuel_at_lap.truncate(n);
                            }
                        }
                    }
                }
                self.prev_lap = Some(lap);
            }
            Some(prev) if lap < prev => {
                self.norms.clear();
                self.fuel_at_lap.clear();
                self.prev_lap = Some(lap);
            }
            _ => {}
        }
        self.snapshot(pit_loss_s, laps_remain)
    }

    fn snapshot(&self, pit_loss_s: f32, laps_remain: Option<f32>) -> PaceSnapshot {
        let loss = slope_per_lap(&self.norms);
        let Some(loss) = loss.filter(|l| *l > 0.005) else {
            return PaceSnapshot::default();
        };
        let rem = laps_remain.unwrap_or(10.0).max(1.0);
        // Cumulative extra time if we stay on these tires for rem laps.
        let cum = loss * rem * (rem + 1.0) * 0.5;
        let window_open = cum > pit_loss_s * 0.85;
        let note = Some(format!("Pace drop {loss:.2}s/L"));
        PaceSnapshot {
            loss_per_lap: Some(loss),
            window_open,
            note,
        }
    }
}

/// Simple linear regression slope of times vs lap index (0 = newest).
fn slope_per_lap(norms: &[f32]) -> Option<f32> {
    if norms.len() < 3 {
        return None;
    }
    // Oldest → newest as x=0..n-1 for positive degradation slope.
    let n = norms.len() as f32;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;
    for (i, &y) in norms.iter().rev().enumerate() {
        let x = i as f32;
        sum_x += x;
        sum_y += y;
        sum_xy += x * y;
        sum_xx += x * x;
    }
    let denom = n * sum_xx - sum_x * sum_x;
    if denom.abs() < 1e-6 {
        return None;
    }
    Some((n * sum_xy - sum_x * sum_y) / denom)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_positive_degradation() {
        let mut m = PaceModel::default();
        // Rising raw times with constant fuel → positive slope.
        let times = [90.0, 90.2, 90.5, 90.9, 91.4];
        for (i, t) in times.iter().enumerate() {
            m.observe(
                (i + 1) as i32,
                Some(*t),
                40.0,
                true,
                0.02,
                8,
                25.0,
                Some(15.0),
            );
        }
        let snap = m.observe(6, Some(91.8), 40.0, true, 0.02, 8, 25.0, Some(15.0));
        assert!(snap.loss_per_lap.unwrap_or(0.0) > 0.1);
        assert!(snap.window_open);
    }
}
