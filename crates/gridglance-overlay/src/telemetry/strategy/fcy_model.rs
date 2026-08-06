//! Crude FCY probability from yellow frequency + incident rate.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FcySnapshot {
    pub p_fcy: f32,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FcyModel {
    yellow_events: u32,
    green_laps: u32,
    prev_caution: bool,
    prev_lap: Option<i32>,
    prev_incidents: i32,
    incident_deltas: u32,
}

impl FcyModel {
    pub fn observe(
        &mut self,
        lap: i32,
        caution: bool,
        incidents: i32,
        horizon_laps: f32,
    ) -> FcySnapshot {
        if caution && !self.prev_caution {
            self.yellow_events = self.yellow_events.saturating_add(1);
        }
        self.prev_caution = caution;

        match self.prev_lap {
            None => self.prev_lap = Some(lap),
            Some(prev) if lap > prev => {
                if !caution {
                    self.green_laps = self.green_laps.saturating_add(1);
                }
                if incidents > self.prev_incidents {
                    self.incident_deltas = self.incident_deltas.saturating_add(
                        (incidents - self.prev_incidents).max(0) as u32,
                    );
                }
                self.prev_incidents = incidents;
                self.prev_lap = Some(lap);
            }
            Some(prev) if lap < prev => {
                *self = Self {
                    prev_caution: caution,
                    prev_lap: Some(lap),
                    prev_incidents: incidents,
                    ..Default::default()
                };
            }
            _ => {
                self.prev_incidents = incidents.max(self.prev_incidents);
            }
        }

        let base = if self.green_laps + self.yellow_events > 0 {
            self.yellow_events as f32 / (self.green_laps as f32 + self.yellow_events as f32).max(1.0)
        } else {
            0.08
        };
        let incident_boost = (self.incident_deltas as f32 * 0.02).min(0.25);
        let per_lap = (base * 0.35 + incident_boost).clamp(0.02, 0.45);
        let h = horizon_laps.clamp(1.0, 20.0);
        // P(at least one FCY in horizon) ≈ 1 - (1-p)^h
        let p = 1.0 - (1.0 - per_lap).powf(h);
        let p = p.clamp(0.0, 0.85);
        let note = if p >= 0.18 {
            Some(format!("FCY ~{:.0}%", p * 100.0))
        } else {
            None
        };
        FcySnapshot { p_fcy: p, note }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yellows_raise_probability() {
        let mut m = FcyModel::default();
        let mut p0 = 0.0;
        for lap in 1..10 {
            let s = m.observe(lap, false, 0, 5.0);
            p0 = s.p_fcy;
        }
        m.observe(10, true, 0, 5.0);
        m.observe(11, false, 0, 5.0);
        m.observe(12, true, 2, 5.0);
        let s = m.observe(13, false, 2, 5.0);
        assert!(s.p_fcy >= p0);
    }
}
