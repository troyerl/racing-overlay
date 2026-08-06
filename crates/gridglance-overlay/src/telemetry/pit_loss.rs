//! Measured player pit-lane time-loss (session-time delta on pit-road edge).

/// Tracks player pit-road entry/exit and maintains an EMA of stop duration.
#[derive(Debug, Clone, Default)]
pub struct PitLossTracker {
    on_pit: bool,
    entry_session_time: Option<f64>,
    /// EMA of measured pit-lane durations (seconds).
    pub ema_loss_s: Option<f32>,
}

impl PitLossTracker {
    /// Observe player pit-road state. Rising edge starts a timer; falling edge
    /// records duration into the EMA when the stop looks like a real service.
    pub fn observe(
        &mut self,
        on_pit: bool,
        session_time: f64,
        alpha: f32,
    ) {
        let alpha = alpha.clamp(0.05, 0.95);
        if on_pit && !self.on_pit {
            if session_time.is_finite() && session_time >= 0.0 {
                self.entry_session_time = Some(session_time);
            }
        } else if !on_pit && self.on_pit {
            if let Some(entry) = self.entry_session_time.take() {
                if session_time.is_finite() {
                    let dt = (session_time - entry) as f32;
                    // Real stops are typically 12–90s; ignore drive-through blips / tows.
                    if (12.0..120.0).contains(&dt) {
                        self.ema_loss_s = Some(match self.ema_loss_s {
                            Some(prev) => alpha * dt + (1.0 - alpha) * prev,
                            None => dt,
                        });
                    }
                }
            }
        }
        self.on_pit = on_pit;
    }

}

/// Effective pit loss for strategy: measured EMA or config fallback, scaled
/// under caution / FCY.
pub fn effective_loss_s(
    measured: Option<f32>,
    fallback: f32,
    caution: bool,
    caution_factor: f32,
) -> f32 {
    let base = measured.unwrap_or(fallback).clamp(5.0, 120.0);
    if caution {
        (base * caution_factor.clamp(0.1, 1.0)).max(3.0)
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measures_pit_stop_ema() {
        let mut t = PitLossTracker::default();
        t.observe(false, 100.0, 0.4);
        t.observe(true, 200.0, 0.4);
        t.observe(true, 220.0, 0.4);
        t.observe(false, 230.0, 0.4); // 30s stop
        assert!((t.ema_loss_s.unwrap() - 30.0).abs() < 1e-3);
        assert!((effective_loss_s(t.ema_loss_s, 25.0, false, 0.4) - 30.0).abs() < 1e-3);
        assert!((effective_loss_s(t.ema_loss_s, 25.0, true, 0.4) - 12.0).abs() < 1e-3);
    }

    #[test]
    fn ignores_blip_stops() {
        let mut t = PitLossTracker::default();
        t.observe(true, 10.0, 0.4);
        t.observe(false, 15.0, 0.4); // 5s — too short
        assert!(t.ema_loss_s.is_none());
    }
}
