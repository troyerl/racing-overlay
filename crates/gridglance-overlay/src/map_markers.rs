//! Map traffic-marker selection and hold-before-switch (Python `map_markers.py`).
//!
//! Ahead/behind/leader **targets** and position **labels** on dots follow live
//! race progress (`laps_completed + lap_dist_pct`) so a pass updates with the
//! car order on the map. Standings / dash keep their own shared ranks (including
//! pit freeze); the map must not lag behind what the dots show spatially.

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

/// Clear ahead/behind/leader hold so the next resolve reacquires immediately.
pub fn clear_hold_states(hold_states: &mut HoldStates) {
    for slot in MARKER_SLOTS {
        if let Some(state) = hold_states.get_mut(*slot) {
            *state = MarkerHoldSlot::default();
        }
    }
}

/// True when session_type looks like a race (same rule as Relative/Standings).
pub fn session_is_race(session_type: Option<&str>) -> bool {
    session_type
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("race")
}

/// Car can host a traffic marker wherever it is on the map — racing line or
/// pits — as long as it has a placeable lap %. Garage / not-in-world hide.
pub fn marker_car_valid(car: &CarRow) -> bool {
    if car.is_pace_car {
        return false;
    }
    car.lap_dist_pct >= 0.0 && (car.on_track || car.on_pit || car.in_pit)
}

/// Race distance proxy used for mid-lap map order.
pub fn live_race_progress(car: &CarRow) -> Option<f64> {
    if !marker_car_valid(car) {
        return None;
    }
    let laps = car.laps_completed.max(0) as f64;
    let pct = (car.lap_dist_pct as f64).clamp(0.0, 1.0);
    Some(laps + pct)
}

fn official_rank(car: &CarRow) -> Option<i32> {
    let pos = if car.position > 0 {
        car.position
    } else {
        car.class_position
    };
    (pos > 0).then_some(pos)
}

/// Map ranks: live progress in race when enough cars have pct; else official
/// Position / ClassPosition.
pub fn live_map_ranks(cars: &[CarRow], is_race: bool) -> HashMap<i32, i32> {
    if is_race {
        let mut scored: Vec<(i32, f64)> = cars
            .iter()
            .filter_map(|c| live_race_progress(c).map(|p| (c.car_idx, p)))
            .collect();
        if scored.len() >= 2 {
            scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            return scored
                .into_iter()
                .enumerate()
                .map(|(i, (idx, _))| (idx, (i + 1) as i32))
                .collect();
        }
    }
    cars.iter()
        .filter_map(|c| official_rank(c).map(|r| (c.car_idx, r)))
        .collect()
}

/// Text drawn on map dots / traffic-marker pills (`map.car_label`).
///
/// Position mode prefers live race-progress rank (matches on-map order). Falls
/// back to `CarRow.position` / `class_position` when live ranks are unavailable
/// (qual/practice, missing pct).
pub fn car_dot_label(car: &CarRow, mode: &str, live_rank: Option<i32>) -> String {
    if car.is_pace_car {
        return "PC".into();
    }
    if mode.eq_ignore_ascii_case("position") {
        if let Some(r) = live_rank.filter(|r| *r > 0) {
            return r.to_string();
        }
        if let Some(r) = official_rank(car) {
            return r.to_string();
        }
    }
    if !car.car_number.is_empty() {
        return car.car_number.clone();
    }
    "?".into()
}

/// Focus car crossed start/finish (large backward jump in lap %).
pub fn focus_crossed_sf(prev_pct: Option<f32>, now_pct: f32) -> bool {
    let Some(prev) = prev_pct else {
        return false;
    };
    if prev < 0.0 || now_pct < 0.0 {
        return false;
    }
    (prev - now_pct) > 0.5
}

/// Locked car for this slot was passed (progress order flipped vs focus).
fn clear_pass_of_locked(slot: &str, focus: &CarRow, locked: &CarRow) -> bool {
    let Some(fp) = live_race_progress(focus) else {
        return false;
    };
    let Some(lp) = live_race_progress(locked) else {
        return false;
    };
    match slot {
        // Ahead marker: locked should still be ahead; if not, we passed them.
        "ahead" => lp < fp,
        // Behind marker: locked should still be behind; if not, they passed us.
        "behind" => lp > fp,
        _ => false,
    }
}

/// Raw CarIdx targets for ahead / behind / leader (no hold debounce).
///
/// In a race, ahead/behind are **live progress** neighbors (rank−1 / rank+1)
/// wherever they are on the map — including pits. Outside a race (or when
/// progress is unavailable) ranks fall back to official Position.
/// Leader is overall rank 1 when placeable, never the focus car. Ahead is
/// omitted when it is the same car as leader (crown alone).
///
/// `focus_idx` is the car the markers are relative to — your live race car when
/// still competing (even if the camera wanders), else the spectated car for a
/// pure spectator.
pub fn select_marker_candidates(
    cars: &[CarRow],
    focus_idx: Option<i32>,
    is_race: bool,
) -> HashMap<&'static str, Option<i32>> {
    let mut out: HashMap<&'static str, Option<i32>> =
        MARKER_SLOTS.iter().map(|s| (*s, None)).collect();

    let Some(player) = focus_idx
        .and_then(|idx| cars.iter().find(|c| c.car_idx == idx))
        .or_else(|| cars.iter().find(|c| c.is_player))
    else {
        return out;
    };
    let ranks = live_map_ranks(cars, is_race);
    let Some(&my_pos) = ranks.get(&player.car_idx) else {
        return out;
    };
    let player_idx = player.car_idx;

    let idx_at_rank = |target: i32| -> Option<i32> {
        if target < 1 {
            return None;
        }
        for c in cars {
            if ranks.get(&c.car_idx) != Some(&target) || c.car_idx == player_idx {
                continue;
            }
            // Found that rank — placeable or hide (no fallthrough).
            return if marker_car_valid(c) {
                Some(c.car_idx)
            } else {
                None
            };
        }
        None
    };

    let mut ahead = idx_at_rank(my_pos - 1);
    let behind = idx_at_rank(my_pos + 1);

    let mut leader = None;
    for c in cars {
        if ranks.get(&c.car_idx) != Some(&1) || c.car_idx == player_idx {
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
///
/// `focus_prev_pct` tracks the focus car's previous lap % so an S/F wrap can
/// clear holds and re-resolve against the post-line field.
pub fn resolve_traffic_markers(
    hold_states: &mut HoldStates,
    cars: &[CarRow],
    now: f64,
    hold_sec: f64,
    focus_idx: Option<i32>,
    label_mode: &str,
    is_race: bool,
    focus_prev_pct: &mut Option<f32>,
) -> HashMap<&'static str, Option<TrafficMarker>> {
    let focus = focus_idx
        .and_then(|idx| cars.iter().find(|c| c.car_idx == idx))
        .or_else(|| cars.iter().find(|c| c.is_player));
    let focus_pct = focus.map(|c| c.lap_dist_pct).unwrap_or(-1.0);
    if focus_crossed_sf(*focus_prev_pct, focus_pct) {
        clear_hold_states(hold_states);
    }
    if focus_pct >= 0.0 {
        *focus_prev_pct = Some(focus_pct);
    }

    let ranks = live_map_ranks(cars, is_race);
    let candidates = select_marker_candidates(cars, focus_idx, is_race);

    // A clear pass of the locked ahead/behind car drops both neighbor holds so
    // the field can reacquire immediately (passed car becomes the new behind).
    let pass_flush = focus.is_some_and(|f| {
        ["ahead", "behind"].iter().any(|slot| {
            hold_states
                .get(*slot)
                .and_then(|s| s.locked)
                .and_then(|idx| cars.iter().find(|c| c.car_idx == idx))
                .is_some_and(|l| marker_car_valid(l) && clear_pass_of_locked(slot, f, l))
        })
    });
    if pass_flush {
        for slot in ["ahead", "behind"] {
            if let Some(state) = hold_states.get_mut(slot) {
                *state = MarkerHoldSlot::default();
            }
        }
    }

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
                    let live_rank = ranks.get(&c.car_idx).copied();
                    out.insert(
                        slot,
                        Some(TrafficMarker {
                            idx,
                            pct: c.lap_dist_pct,
                            label: car_dot_label(c, label_mode, live_rank),
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

    /// Official position `pos` with matching live progress (higher progress = better).
    fn car(idx: i32, pos: i32, player: bool) -> CarRow {
        CarRow {
            car_idx: idx,
            position: pos,
            is_player: player,
            on_track: true,
            laps_completed: 5,
            lap_dist_pct: 0.90 - 0.05 * (pos - 1) as f32,
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
        let c = select_marker_candidates(&cars, None, true);
        assert_eq!(c["leader"], Some(0));
        assert_eq!(c["ahead"], Some(1));
        assert_eq!(c["behind"], Some(3));
    }

    #[test]
    fn p2_keeps_leader_crown_without_ahead_duplicate() {
        let cars = vec![car(0, 1, false), car(1, 2, true), car(2, 3, false)];
        let c = select_marker_candidates(&cars, None, true);
        assert_eq!(c["leader"], Some(0));
        assert_eq!(c["ahead"], None, "ahead is the leader — crown only");
        assert_eq!(c["behind"], Some(2));
    }

    #[test]
    fn leader_marker_skips_focus_when_you_lead() {
        let cars = vec![car(0, 1, true), car(1, 2, false), car(2, 3, false)];
        let c = select_marker_candidates(&cars, None, true);
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
        // Live neighbors stay marked even when far apart on the drawing.
        cars[0].lap_dist_pct = 0.90;
        cars[1].lap_dist_pct = 0.85; // ahead of player, opposite side of map
        cars[2].lap_dist_pct = 0.15;
        cars[3].lap_dist_pct = 0.10;
        let c = select_marker_candidates(&cars, None, true);
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
        let c = select_marker_candidates(&cars, None, true);
        assert_eq!(c["ahead"], Some(1), "pitting ahead car must stay marked");
        assert_eq!(c["leader"], Some(0));
    }

    #[test]
    fn hides_ahead_in_garage_without_lap_pct() {
        let mut cars = vec![car(0, 1, false), car(1, 2, false), car(2, 3, true)];
        cars[1].on_track = false;
        cars[1].on_pit = false;
        cars[1].lap_dist_pct = -1.0;
        let c = select_marker_candidates(&cars, None, true);
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
        assert_eq!(select_marker_candidates(&cars, None, true)["ahead"], None);

        let c = select_marker_candidates(&cars, Some(2), true);
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
            select_marker_candidates(&cars, Some(2), true),
            select_marker_candidates(&cars, None, true)
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
        let mut prev = None;
        let m = resolve_traffic_markers(
            &mut hold,
            &cars,
            10.0,
            0.75,
            None,
            "number",
            true,
            &mut prev,
        );
        assert!(m["ahead"].is_some());
        assert!(m["behind"].is_some());
        assert!(m["leader"].is_some());
    }

    #[test]
    fn car_dot_label_uses_position_mode() {
        let mut c = car(5, 3, false);
        c.car_number = "48".into();
        assert_eq!(car_dot_label(&c, "position", None), "3");
        // Live map rank wins over standings/SDK position so dots match track order.
        assert_eq!(car_dot_label(&c, "position", Some(2)), "2");
        assert_eq!(car_dot_label(&c, "number", Some(2)), "48");
        c.position = 0;
        c.class_position = 2;
        assert_eq!(car_dot_label(&c, "position", None), "2");
        assert_eq!(car_dot_label(&c, "position", Some(4)), "4");
        c.class_position = 0;
        assert_eq!(car_dot_label(&c, "position", Some(4)), "4");
    }

    #[test]
    fn live_pass_flips_ahead_behind_despite_stale_official_position() {
        // Official positions still say player is P3 behind car1, but progress
        // shows the player has already passed them.
        let mut cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        cars[1].lap_dist_pct = 0.40; // official P2, now behind on track
        cars[2].lap_dist_pct = 0.55; // player ahead of car1 by progress
        cars[3].lap_dist_pct = 0.30;
        let c = select_marker_candidates(&cars, None, true);
        assert_eq!(c["leader"], Some(0));
        assert_eq!(c["behind"], Some(1), "passed car becomes behind");
        // P2 crown: only the leader is ahead → suppress green ahead icon.
        assert_eq!(c["ahead"], None);
    }

    #[test]
    fn hold_bypasses_on_clear_pass() {
        let mut cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        let mut hold = fresh_hold_states();
        let mut prev = None;
        let m = resolve_traffic_markers(
            &mut hold, &cars, 1.0, 3.0, None, "number", true, &mut prev,
        );
        assert_eq!(m["ahead"].as_ref().map(|t| t.idx), Some(1));

        // Pass car1: progress flips, official positions unchanged.
        cars[1].lap_dist_pct = 0.40;
        cars[2].lap_dist_pct = 0.55;
        cars[3].lap_dist_pct = 0.30;
        let m2 = resolve_traffic_markers(
            &mut hold, &cars, 1.1, 3.0, None, "number", true, &mut prev,
        );
        // Hold would keep old ahead for 3s — pass flush must reacquire immediately.
        assert_eq!(m2["behind"].as_ref().map(|t| t.idx), Some(1));
        assert_eq!(m2["ahead"].as_ref().map(|t| t.idx), None);
        assert_eq!(hold["behind"].locked, Some(1));
    }

    #[test]
    fn hold_keeps_locked_without_progress_swap() {
        let cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
            car(4, 5, false),
        ];
        let mut hold = fresh_hold_states();
        let mut prev = None;
        let _ = resolve_traffic_markers(
            &mut hold, &cars, 1.0, 3.0, None, "number", true, &mut prev,
        );
        assert_eq!(hold["behind"].locked, Some(3));

        // Flicker: candidate briefly becomes car4 while car3 is still behind
        // the player by progress — hold must keep car3.
        let mut flickered = cars.clone();
        flickered[3].laps_completed = 4; // drop car3 out of live top ranks
        flickered[3].lap_dist_pct = 0.10;
        flickered[4].laps_completed = 5;
        flickered[4].lap_dist_pct = 0.70; // still behind player (0.80)
        let m = resolve_traffic_markers(
            &mut hold, &flickered, 1.2, 3.0, None, "number", true, &mut prev,
        );
        assert_eq!(
            m["behind"].as_ref().map(|t| t.idx),
            Some(3),
            "hold keeps prior behind without a pass"
        );
    }

    #[test]
    fn sf_wrap_clears_hold_and_reresolves() {
        let cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        let mut hold = fresh_hold_states();
        let mut prev = None;
        let _ = resolve_traffic_markers(
            &mut hold, &cars, 1.0, 3.0, None, "number", true, &mut prev,
        );
        assert_eq!(hold["ahead"].locked, Some(1));
        assert!(prev.is_some());

        // Approach S/F then wrap.
        let mut wrapping = cars.clone();
        wrapping[2].lap_dist_pct = 0.98;
        let _ = resolve_traffic_markers(
            &mut hold, &wrapping, 2.0, 3.0, None, "number", true, &mut prev,
        );
        wrapping[2].lap_dist_pct = 0.02;
        wrapping[2].laps_completed = 6;
        // After wrap, reorder so a different car is ahead.
        wrapping[1].laps_completed = 5;
        wrapping[1].lap_dist_pct = 0.50;
        wrapping[0].laps_completed = 6;
        wrapping[0].lap_dist_pct = 0.10;
        let m = resolve_traffic_markers(
            &mut hold, &wrapping, 2.1, 3.0, None, "number", true, &mut prev,
        );
        // Hold cleared on wrap — new ahead acquires immediately (leader crown).
        assert_eq!(m["leader"].as_ref().map(|t| t.idx), Some(0));
        assert_eq!(m["ahead"].as_ref().map(|t| t.idx), None);
    }

    #[test]
    fn non_race_uses_official_position() {
        let mut cars = vec![
            car(0, 1, false),
            car(1, 2, false),
            car(2, 3, true),
            car(3, 4, false),
        ];
        // Progress would put player first; official order must win outside race.
        cars[2].laps_completed = 99;
        cars[2].lap_dist_pct = 0.99;
        let c = select_marker_candidates(&cars, None, false);
        assert_eq!(c["leader"], Some(0));
        assert_eq!(c["ahead"], Some(1));
        assert_eq!(c["behind"], Some(3));
    }

}
