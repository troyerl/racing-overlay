//! Leader lap-down projection for stay-out vs pit.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LapDownSnapshot {
    pub laps_until_lapped: Option<f32>,
    pub laps_until_lapped_after_stop: Option<f32>,
    pub lap_down_if_pit: bool,
    pub note: Option<String>,
}

/// Project whether the leader will lap the player, stay-out and after a pit.
pub fn project_lap_down(
    player_lap: i32,
    player_pct: f32,
    player_pace: f32,
    leader_lap: i32,
    leader_pct: f32,
    leader_pace: f32,
    pit_loss_s: f32,
    race_laps_remain: Option<f32>,
) -> LapDownSnapshot {
    if player_lap <= 0 || leader_lap <= 0 || player_pace <= 10.0 || leader_pace <= 10.0 {
        return LapDownSnapshot::default();
    }
    let p_pct = player_pct.clamp(0.0, 0.999);
    let l_pct = leader_pct.clamp(0.0, 0.999);

    let player_dist = player_lap as f32 + p_pct;
    let leader_dist = leader_lap as f32 + l_pct;
    let gap_laps = leader_dist - player_dist;
    if gap_laps < 0.0 {
        return LapDownSnapshot::default();
    }

    let leader_rate = 1.0 / leader_pace;
    let player_rate = 1.0 / player_pace;
    let close_rate = leader_rate - player_rate;

    let mut laps_until = if gap_laps >= 1.0 {
        Some(0.0)
    } else if close_rate > 1e-6 {
        let need = 1.0 - gap_laps;
        Some((need / close_rate) / player_pace)
    } else {
        None
    };

    let loss_laps = (pit_loss_s / player_pace).clamp(0.0, 0.95);
    let gap_after = gap_laps + loss_laps;
    let mut after = if gap_after >= 1.0 {
        Some(0.0)
    } else if close_rate > 1e-6 {
        let need = 1.0 - gap_after;
        Some((need / close_rate) / player_pace)
    } else {
        None
    };

    if let Some(rem) = race_laps_remain {
        if laps_until.map(|u| u > rem).unwrap_or(false) {
            laps_until = None;
        }
        if after.map(|u| u > rem).unwrap_or(false) {
            after = None;
        }
    }

    let mut lap_down_if_pit = gap_after >= 0.95
        || after.map(|a| a < 1.5).unwrap_or(false);
    if let Some(rem) = race_laps_remain {
        if after.map(|a| a > rem).unwrap_or(true) && gap_after < 0.95 {
            lap_down_if_pit = false;
        }
    }

    let note = if lap_down_if_pit {
        Some("Pit risks lap down".into())
    } else if let Some(u) = laps_until.filter(|u| *u < 5.0) {
        Some(format!("Leader laps in ~{u:.1}L"))
    } else {
        None
    };

    LapDownSnapshot {
        laps_until_lapped: laps_until,
        laps_until_lapped_after_stop: after,
        lap_down_if_pit,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pit_can_cause_lap_down() {
        let snap = project_lap_down(10, 0.2, 90.0, 10, 0.9, 89.0, 25.0, Some(20.0));
        assert!(snap.lap_down_if_pit || snap.note.is_some());
    }

    #[test]
    fn no_risk_when_ahead() {
        let snap = project_lap_down(12, 0.5, 90.0, 11, 0.2, 90.0, 25.0, Some(10.0));
        assert!(!snap.lap_down_if_pit);
        assert!(snap.laps_until_lapped.is_none());
    }
}
