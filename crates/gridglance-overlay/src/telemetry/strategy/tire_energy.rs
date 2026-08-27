//! Proxy tire energy from lateral load + thermal urgency.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TireEnergySnapshot {
    pub energy_index: f32,
    pub tire_urgent: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TireEnergyTracker {
    energy: f32,
    stint_laps: f32,
    prev_session_time: Option<f64>,
    prev_on_pit: bool,
    had_tire_service: bool,
    last_wear_min: Option<f32>,
}

impl TireEnergyTracker {
    pub fn tick(
        &mut self,
        session_time: f64,
        on_track: bool,
        on_pit: bool,
        caution: bool,
        speed_mps: f32,
        lat_accel: f32,
        steering: f32,
        yaw_rate: f32,
        tire_temps: &[f32; 4],
        tire_wear_min: Option<f32>,
        track_temp: Option<f32>,
        wetness: Option<f32>,
        wet_suppress: f32,
        tire_bits_selected: bool,
    ) -> TireEnergySnapshot {
        // Detect pit exit after tire service / wear jump.
        if self.prev_on_pit && !on_pit {
            let wear_jump = match (self.last_wear_min, tire_wear_min) {
                (Some(prev), Some(now)) => now > prev + 0.08,
                _ => false,
            };
            if self.had_tire_service || wear_jump || tire_bits_selected {
                self.energy = 0.0;
                self.stint_laps = 0.0;
            }
            self.had_tire_service = false;
        }
        if on_pit && tire_bits_selected {
            self.had_tire_service = true;
        }
        self.prev_on_pit = on_pit;

        let dt = match self.prev_session_time {
            Some(prev) if session_time > prev => (session_time - prev) as f32,
            _ => 0.0,
        };
        self.prev_session_time = Some(session_time);
        if dt > 0.0 && dt < 0.25 && on_track && !on_pit && !caution {
            let lat = lat_accel.abs().min(40.0);
            let scrub = (steering.abs() * 0.15 - yaw_rate.abs()).max(0.0);
            self.energy += (lat * speed_mps.max(0.0) + scrub * 80.0) * dt;
            self.stint_laps += dt / 90.0; // rough; refined by lap observes
        }
        if let Some(w) = tire_wear_min {
            self.last_wear_min = Some(w);
        }

        let per_lap = if self.stint_laps > 0.5 {
            self.energy / self.stint_laps.max(0.5)
        } else {
            self.energy
        };
        // Normalize to a 0..2-ish index (empirical scale).
        let index = (per_lap / 2500.0).clamp(0.0, 3.0);

        let mut temp_hot = false;
        let mut temp_spread = 0.0f32;
        let temps: Vec<f32> = tire_temps.iter().copied().filter(|t| *t > 20.0).collect();
        if temps.len() >= 2 {
            let max = temps.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let min = temps.iter().copied().fold(f32::INFINITY, f32::min);
            temp_spread = max - min;
            let track = track_temp.unwrap_or(25.0);
            temp_hot = max > track + 55.0 || max > 105.0;
        }

        let wet = wetness.unwrap_or(0.0);
        let suppress = wet >= wet_suppress && wet_suppress > 0.0;
        let worn = tire_wear_min.map(|w| w > 0.0 && w < 0.45).unwrap_or(false);

        let tire_urgent = !suppress && (index > 1.15 || temp_hot || temp_spread > 18.0 || worn);
        let note = if tire_urgent {
            Some("Tires stressed".into())
        } else if index > 0.7 {
            Some(format!("Tire load {index:.1}"))
        } else {
            None
        };

        TireEnergySnapshot {
            energy_index: index,
            tire_urgent,
            note,
        }
    }

    pub fn on_lap_complete(&mut self, eligible: bool) {
        if eligible {
            self.stint_laps = (self.stint_laps.floor() + 1.0).max(self.stint_laps);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulates_and_resets_on_service() {
        let mut t = TireEnergyTracker::default();
        let temps = [95.0f32; 4];
        for i in 0..20 {
            t.tick(
                i as f64 * 0.05,
                true,
                false,
                false,
                50.0,
                12.0,
                0.2,
                0.1,
                &temps,
                Some(0.8),
                Some(30.0),
                Some(0.0),
                40.0,
                false,
            );
        }
        assert!(t.energy > 0.0);
        // Enter pit with tires selected, then exit → reset.
        t.tick(
            1.0,
            false,
            true,
            false,
            10.0,
            0.0,
            0.0,
            0.0,
            &temps,
            Some(0.8),
            Some(30.0),
            Some(0.0),
            40.0,
            true,
        );
        t.tick(
            1.1,
            true,
            false,
            false,
            40.0,
            5.0,
            0.0,
            0.0,
            &temps,
            Some(0.95),
            Some(30.0),
            Some(0.0),
            40.0,
            false,
        );
        assert!(t.energy < 500.0);
    }
}
