//! Map traffic-marker selection and hold-before-switch (Python `map_markers.py`).

use crate::telemetry::CarRow;
use std::collections::HashMap;

pub const MARKER_SLOTS: &[&str] = &["ahead", "behind", "leader"];

#[derive(Debug, Clone, Default)]
pub struct MarkerHoldSlot {
    pub locked: Option<i32>,
    pub pending: Option<i32>,
    pub pending_since: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct TrafficMarker {
    pub idx: i32,
    pub pct: f32,
    pub label: String,
}

pub type HoldStates = HashMap<String, MarkerHoldSlot>;

pub fn fresh_hold_states() -> HoldStates {
    MARKER_SLOTS
        .iter()
        .map(|s| ((*s).to_string(), MarkerHoldSlot::default()))
        .collect()
}

pub fn marker_car_valid(car: &CarRow) -> bool {
    if car.is_pace_car {
        return false;
    }
    if car.on_pit || car.in_pit {
        return false;
    }
    car.on_track && car.lap_dist_pct >= 0.0
}

/// Raw CarIdx targets for ahead / behind / leader (no hold debounce).
/// Ahead/behind are race-position neighbors; no fallthrough if invalid.
pub fn select_marker_candidates(cars: &[CarRow]) -> HashMap<&'static str, Option<i32>> {
    let mut out: HashMap<&'static str, Option<i32>> = MARKER_SLOTS
        .iter()
        .map(|s| (*s, None))
        .collect();

    let Some(player) = cars.iter().find(|c| c.is_player) else {
        return out;
    };
    if player.lap_dist_pct < 0.0 || player.position < 1 {
        return out;
    }
    let my_pos = player.position;
    let player_idx = player.car_idx;

    let idx_at_pos = |target: i32| -> Option<i32> {
        if target < 1 {
            return None;
        }
        for c in cars {
            if c.position != target || c.car_idx == player_idx {
                continue;
            }
            // Found that race position — valid or hide (no fallthrough).
            return if marker_car_valid(c) {
                Some(c.car_idx)
            } else {
                None
            };
        }
        None
    };

    out.insert("ahead", idx_at_pos(my_pos - 1));
    out.insert("behind", idx_at_pos(my_pos + 1));

    for c in cars {
        if c.position != 1 {
            continue;
        }
        if marker_car_valid(c) {
            out.insert("leader", Some(c.car_idx));
        }
        break;
    }
    out
}

fn apply_marker_hold(
    state: &mut MarkerHoldSlot,
    candidate_idx: Option<i32>,
    now: f64,
    hold_sec: f64,
    locked_valid: bool,
) -> Option<i32> {
    if !locked_valid {
        state.locked = None;
    }

    let Some(candidate_idx) = candidate_idx else {
        state.pending = None;
        state.pending_since = None;
        return None;
    };

    if state.locked == Some(candidate_idx) {
        state.pending = None;
        state.pending_since = None;
        return state.locked;
    }

    if state.pending != Some(candidate_idx) {
        state.pending = Some(candidate_idx);
        state.pending_since = Some(now);
    }

    if let Some(since) = state.pending_since {
        if (now - since) >= hold_sec {
            state.locked = Some(candidate_idx);
            state.pending = None;
            state.pending_since = None;
            return Some(candidate_idx);
        }
    }

    if locked_valid {
        state.locked
    } else {
        None
    }
}

/// Apply hold debounce; return idx + lap-% + label for each marker slot.
pub fn resolve_traffic_markers(
    hold_states: &mut HoldStates,
    cars: &[CarRow],
    now: f64,
    hold_sec: f64,
) -> HashMap<&'static str, Option<TrafficMarker>> {
    let candidates = select_marker_candidates(cars);
    let mut out: HashMap<&'static str, Option<TrafficMarker>> = MARKER_SLOTS
        .iter()
        .map(|s| (*s, None))
        .collect();

    for &slot in MARKER_SLOTS {
        let state = hold_states.entry(slot.to_string()).or_default();
        let candidate = candidates.get(slot).copied().flatten();
        let locked = state.locked;
        let locked_valid = locked
            .and_then(|idx| cars.iter().find(|c| c.car_idx == idx))
            .is_some_and(marker_car_valid);
        let idx = apply_marker_hold(state, candidate, now, hold_sec, locked_valid);
        if let Some(idx) = idx {
            if let Some(c) = cars.iter().find(|c| c.car_idx == idx) {
                if c.lap_dist_pct >= 0.0 {
                    out.insert(
                        slot,
                        Some(TrafficMarker {
                            idx,
                            pct: c.lap_dist_pct,
                            label: c.car_number.clone(),
                        }),
                    );
                }
            }
        }
    }
    out
}

/// Slot name for a car idx (leader → ahead → behind; later overwrites).
pub fn marker_slots_by_idx(
    markers: &HashMap<&'static str, Option<TrafficMarker>>,
) -> HashMap<i32, &'static str> {
    let mut out = HashMap::new();
    for &slot in &["leader", "ahead", "behind"] {
        if let Some(Some(m)) = markers.get(slot) {
            out.insert(m.idx, slot);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(idx: i32, pos: i32, player: bool) -> CarRow {
        CarRow {
            car_idx: idx,
            position: pos,
            is_player: player,
            on_track: true,
            lap_dist_pct: 0.1 * idx as f32,
            car_number: format!("{idx}"),
            ..Default::default()
        }
    }

    #[test]
    fn selects_position_neighbors() {
        let cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        let c = select_marker_candidates(&cars);
        assert_eq!(c["leader"], Some(0));
        assert_eq!(c["ahead"], Some(1));
        assert_eq!(c["behind"], Some(3));
    }

    #[test]
    fn no_fallthrough_when_ahead_in_pit() {
        let mut cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
        ];
        cars[1].on_pit = true;
        cars[1].on_track = false;
        let c = select_marker_candidates(&cars);
        assert_eq!(c["ahead"], None);
        assert_eq!(c["leader"], Some(0));
    }
}
