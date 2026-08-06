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

/// Car can host a traffic marker wherever it is on the map — racing line or
/// pits — as long as it has a placeable lap %. Garage / not-in-world hide.
pub fn marker_car_valid(car: &CarRow) -> bool {
    if car.is_pace_car {
        return false;
    }
    car.lap_dist_pct >= 0.0 && (car.on_track || car.on_pit || car.in_pit)
}

/// Text drawn on map dots / traffic-marker pills (`map.car_label`).
pub fn car_dot_label(car: &CarRow, mode: &str) -> String {
    if car.is_pace_car {
        return "PC".into();
    }
    if mode.eq_ignore_ascii_case("position") {
        let pos = if car.position > 0 {
            car.position
        } else {
            car.class_position
        };
        if pos > 0 {
            return pos.to_string();
        }
    }
    if !car.car_number.is_empty() {
        return car.car_number.clone();
    }
    "?".into()
}

/// Raw CarIdx targets for ahead / behind / leader (no hold debounce).
///
/// Ahead/behind are race-position neighbors (P−1 / P+1) wherever they are on
/// the map — including pits. No proximity gate and no fallthrough to the next
/// position. Leader is overall P1 when placeable, never the focus car. Ahead
/// is omitted when it is the same car as leader (crown alone).
///
/// `focus_idx` is the car the markers are relative to — your live race car when
/// still competing (even if the camera wanders), else the spectated car for a
/// pure spectator.
pub fn select_marker_candidates(
    cars: &[CarRow],
    focus_idx: Option<i32>,
) -> HashMap<&'static str, Option<i32>> {
    let mut out: HashMap<&'static str, Option<i32>> =
        MARKER_SLOTS.iter().map(|s| (*s, None)).collect();

    let Some(player) = focus_idx
        .and_then(|idx| cars.iter().find(|c| c.car_idx == idx))
        .or_else(|| cars.iter().find(|c| c.is_player))
    else {
        return out;
    };
    // Focus needs a race position; they need not be on a valid lap % for
    // neighbors to resolve (e.g. brief telem glitch) — still require position.
    if player.position < 1 {
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
            // Found that race position — placeable or hide (no fallthrough).
            return if marker_car_valid(c) {
                Some(c.car_idx)
            } else {
                None
            };
        }
        None
    };

    let mut ahead = idx_at_pos(my_pos - 1);
    let behind = idx_at_pos(my_pos + 1);

    let mut leader = None;
    for c in cars {
        if c.position != 1 || c.car_idx == player_idx {
            continue;
        }
        if marker_car_valid(c) {
            leader = Some(c.car_idx);
        }
        break;
    }

    // P2: ahead is the leader — keep the crown only, drop the green ahead icon.
    if ahead.is_some() && ahead == leader {
        ahead = None;
    }

    out.insert("ahead", ahead);
    out.insert("behind", behind);
    out.insert("leader", leader);
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
        // No eligible target — clear hold so a later car starts fresh.
        state.locked = None;
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
        // First acquire: show immediately (don't wait a full hold with nothing up).
        state.locked = Some(candidate_idx);
        state.pending = None;
        state.pending_since = None;
        Some(candidate_idx)
    }
}

/// Apply hold debounce; return idx + lap-% + label for each marker slot.
pub fn resolve_traffic_markers(
    hold_states: &mut HoldStates,
    cars: &[CarRow],
    now: f64,
    hold_sec: f64,
    focus_idx: Option<i32>,
    label_mode: &str,
) -> HashMap<&'static str, Option<TrafficMarker>> {
    let candidates = select_marker_candidates(cars, focus_idx);
    let mut out: HashMap<&'static str, Option<TrafficMarker>> =
        MARKER_SLOTS.iter().map(|s| (*s, None)).collect();

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
                            label: car_dot_label(c, label_mode),
                        }),
                    );
                }
            }
        }
    }

    // Final pass: never draw two floating icons on the same car.
    dedupe_marker_slots(&mut out);
    out
}

/// Prefer leader > ahead > behind when the same car would claim multiple slots.
fn dedupe_marker_slots(markers: &mut HashMap<&'static str, Option<TrafficMarker>>) {
    let leader = markers
        .get("leader")
        .and_then(|m| m.as_ref().map(|t| t.idx));
    let ahead = markers
        .get("ahead")
        .and_then(|m| m.as_ref().map(|t| t.idx));
    if let Some(li) = leader {
        if ahead == Some(li) {
            markers.insert("ahead", None);
        }
        if markers
            .get("behind")
            .and_then(|m| m.as_ref().map(|t| t.idx))
            == Some(li)
        {
            markers.insert("behind", None);
        }
    }
    let ahead = markers
        .get("ahead")
        .and_then(|m| m.as_ref().map(|t| t.idx));
    if let Some(ai) = ahead {
        if markers
            .get("behind")
            .and_then(|m| m.as_ref().map(|t| t.idx))
            == Some(ai)
        {
            markers.insert("behind", None);
        }
    }
}

/// Slot name for a car idx. Leader wins over ahead/behind if anything slips through.
pub fn marker_slots_by_idx(
    markers: &HashMap<&'static str, Option<TrafficMarker>>,
) -> HashMap<i32, &'static str> {
    let mut out = HashMap::new();
    for &slot in &["behind", "ahead", "leader"] {
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
            lap_dist_pct: 0.40 + 0.02 * (idx as f32 - 2.0),
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
        let c = select_marker_candidates(&cars, None);
        assert_eq!(c["leader"], Some(0));
        assert_eq!(c["ahead"], Some(1));
        assert_eq!(c["behind"], Some(3));
    }

    #[test]
    fn p2_keeps_leader_crown_without_ahead_duplicate() {
        let cars = vec![car(0, 1, false), car(1, 2, true), car(2, 3, false)];
        let c = select_marker_candidates(&cars, None);
        assert_eq!(c["leader"], Some(0));
        assert_eq!(c["ahead"], None, "ahead is the leader — crown only");
        assert_eq!(c["behind"], Some(2));
    }

    #[test]
    fn leader_marker_skips_focus_when_you_lead() {
        let cars = vec![car(0, 1, true), car(1, 2, false), car(2, 3, false)];
        let c = select_marker_candidates(&cars, None);
        assert_eq!(c["leader"], None);
        assert_eq!(c["ahead"], None);
        assert_eq!(c["behind"], Some(1));
    }

    #[test]
    fn shows_ahead_anywhere_on_the_map() {
        let mut cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        // Far around the circuit — still mark them.
        cars[1].lap_dist_pct = (cars[2].lap_dist_pct + 0.5).rem_euclid(1.0);
        let c = select_marker_candidates(&cars, None);
        assert_eq!(c["ahead"], Some(1));
        assert_eq!(c["behind"], Some(3));
        assert_eq!(c["leader"], Some(0));
    }

    #[test]
    fn shows_ahead_when_in_pits() {
        let mut cars = vec![car(0, 1, false), car(1, 2, false), car(2, 3, true)];
        cars[1].on_pit = true;
        cars[1].in_pit = true;
        cars[1].on_track = false;
        let c = select_marker_candidates(&cars, None);
        assert_eq!(c["ahead"], Some(1), "pitting ahead car must stay marked");
        assert_eq!(c["leader"], Some(0));
    }

    #[test]
    fn hides_ahead_in_garage_without_lap_pct() {
        let mut cars = vec![car(0, 1, false), car(1, 2, false), car(2, 3, true)];
        cars[1].on_track = false;
        cars[1].on_pit = false;
        cars[1].lap_dist_pct = -1.0;
        let c = select_marker_candidates(&cars, None);
        assert_eq!(c["ahead"], None);
        assert_eq!(c["leader"], Some(0));
    }

    /// Spectating: the seated player's ghost has no position, so markers must
    /// hang off the camera car instead of collapsing to nothing.
    #[test]
    fn markers_follow_the_spectated_car() {
        let mut cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, false),
            car(3, 4, false),
        ];
        // Seated player is spectating: on no lap, no race position.
        cars.push(CarRow {
            car_idx: 9,
            position: 0,
            is_player: true,
            on_track: false,
            lap_dist_pct: -1.0,
            ..Default::default()
        });
        assert_eq!(select_marker_candidates(&cars, None)["ahead"], None);

        let c = select_marker_candidates(&cars, Some(2));
        assert_eq!(
            c["ahead"],
            Some(1),
            "car in P2 is ahead of the spectated P3"
        );
        assert_eq!(c["behind"], Some(3));
        assert_eq!(c["leader"], Some(0));
    }

    /// Camera on the seated player is the normal racing case and must be
    /// indistinguishable from passing no focus at all.
    #[test]
    fn camera_on_own_car_matches_seated_behaviour() {
        let cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        assert_eq!(
            select_marker_candidates(&cars, Some(2)),
            select_marker_candidates(&cars, None)
        );
    }

    #[test]
    fn first_acquire_shows_immediately() {
        let cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        let mut hold = fresh_hold_states();
        let m = resolve_traffic_markers(&mut hold, &cars, 10.0, 3.0, None, "number");
        assert!(m["ahead"].is_some());
        assert!(m["behind"].is_some());
        assert!(m["leader"].is_some());
    }

    #[test]
    fn car_dot_label_uses_position_mode() {
        let mut c = car(5, 3, false);
        c.car_number = "48".into();
        assert_eq!(car_dot_label(&c, "position"), "3");
        assert_eq!(car_dot_label(&c, "number"), "48");
        c.position = 0;
        c.class_position = 2;
        assert_eq!(car_dot_label(&c, "position"), "2");
    }
}
