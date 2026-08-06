//! iRating projection (parity with Python `overlay.irating_calc`).

use crate::telemetry::CarRow;
use std::collections::HashMap;

const BR1: f64 = 1600.0 / std::f64::consts::LN_2;

fn chance(a: f64, b: f64) -> f64 {
    let ea = (-a / BR1).exp();
    let eb = (-b / BR1).exp();
    (1.0 - ea) * eb / ((1.0 - eb) * ea + (1.0 - ea) * eb)
}

fn round_half_away(x: f64) -> i32 {
    if x >= 0.0 {
        (x + 0.5).floor() as i32
    } else {
        (x - 0.5).ceil() as i32
    }
}

fn delta_from_change(start_ir: i32, change: f64) -> i32 {
    round_half_away(start_ir as f64 + change) - start_ir
}

/// `entries`: finish-ordered `(start_ir, started)` pairs.
pub fn calculate_deltas(entries: &[(i32, bool)]) -> Vec<i32> {
    let n = entries.len();
    if n < 2 {
        return vec![0; n];
    }
    let ratings: Vec<f64> = entries.iter().map(|(ir, _)| *ir as f64).collect();
    let chances: Vec<Vec<f64>> = ratings
        .iter()
        .map(|a| ratings.iter().map(|b| chance(*a, *b)).collect())
        .collect();
    let expected: Vec<f64> = chances
        .iter()
        .map(|row| row.iter().sum::<f64>() - 0.5)
        .collect();

    let num_reg = n as f64;
    let num_starters = entries.iter().filter(|(_, s)| *s).count() as f64;
    let num_non = num_reg - num_starters;
    if num_starters < 1.0 {
        return vec![0; n];
    }

    let mut fudge = Vec::with_capacity(n);
    for (rank, (_, started)) in entries.iter().enumerate() {
        let rank = (rank + 1) as f64;
        if !*started {
            fudge.push(0.0);
        } else {
            let x = num_reg - num_non / 2.0;
            fudge.push((x / 2.0 - rank) / 100.0);
        }
    }

    let mut changes_starters: Vec<Option<f64>> = Vec::with_capacity(n);
    for (rank, ((_, started), exp, fud)) in entries
        .iter()
        .zip(expected.iter())
        .zip(fudge.iter())
        .map(|((e, exp), fud)| (e, exp, fud))
        .enumerate()
    {
        let rank = (rank + 1) as f64;
        if !started {
            changes_starters.push(None);
        } else {
            changes_starters.push(Some((num_reg - rank - exp - fud) * 200.0 / num_starters));
        }
    }

    let sum_starters: f64 = changes_starters.iter().flatten().sum();

    let exp_non: Vec<Option<f64>> = entries
        .iter()
        .zip(expected.iter())
        .map(|((_, started), exp)| if !started { Some(*exp) } else { None })
        .collect();
    let sum_exp_non: f64 = exp_non.iter().flatten().copied().sum();

    let changes_non: Vec<Option<f64>> = exp_non
        .iter()
        .map(|exp| match exp {
            None => None,
            Some(_) if num_non <= 0.0 || sum_exp_non <= 0.0 => Some(0.0),
            Some(exp) => Some(-sum_starters * exp / sum_exp_non),
        })
        .collect();

    entries
        .iter()
        .zip(changes_starters.iter())
        .zip(changes_non.iter())
        .map(|(((ir, _), cs), cn)| {
            let change = cs.or(*cn).unwrap_or(0.0);
            delta_from_change(*ir, change)
        })
        .collect()
}

fn car_rank(c: &CarRow) -> i32 {
    if c.class_position > 0 {
        c.class_position
    } else if c.position > 0 {
        c.position
    } else {
        0
    }
}

fn car_started(c: &CarRow) -> bool {
    car_rank(c) > 0 || c.laps_completed > 0
}

/// Rating used in the field matrix. Missing/zero DriverInfo iRating still counts
/// in official results (often rookies with `oldi_rating: -1`); fill those seats
/// with the class mean so field size / SOF stay honest.
fn field_rating(ir: i32, fallback: i32) -> i32 {
    if ir > 0 {
        ir
    } else {
        fallback
    }
}

fn class_ir_fallback(cars: &[&CarRow]) -> i32 {
    let known: Vec<i32> = cars.iter().map(|c| c.irating).filter(|&ir| ir > 0).collect();
    if known.is_empty() {
        // iRacing's default starting iRating when nothing else is known.
        return 1350;
    }
    let sum: f64 = known.iter().map(|&x| x as f64).sum();
    round_half_away(sum / known.len() as f64)
}

/// CarIdx → projected iRating change, computed per `class_id` (Python parity).
pub fn project_deltas_by_class(cars: &[CarRow]) -> HashMap<i32, i32> {
    let mut by_class: HashMap<i32, Vec<&CarRow>> = HashMap::new();
    for c in cars {
        if c.is_pace_car {
            continue;
        }
        // Keep unrated entrants that actually joined (position / laps). Skipping
        // them shrinks the field and under-projects gains (e.g. +21 vs +27).
        if c.irating <= 0 && !car_started(c) {
            continue;
        }
        by_class.entry(c.class_id).or_default().push(c);
    }

    let mut result = HashMap::new();
    for idxs in by_class.values() {
        let fallback = class_ir_fallback(idxs);
        let mut starters: Vec<&CarRow> = Vec::new();
        let mut non_starters: Vec<&CarRow> = Vec::new();
        for c in idxs {
            if car_started(c) {
                starters.push(c);
            } else {
                non_starters.push(c);
            }
        }
        starters.sort_by_key(|c| {
            let r = car_rank(c);
            if r <= 0 {
                10_000
            } else {
                r
            }
        });
        let mut ordered: Vec<(i32, bool)> = starters
            .iter()
            .map(|c| (field_rating(c.irating, fallback), true))
            .collect();
        ordered.extend(
            non_starters
                .iter()
                .map(|c| (field_rating(c.irating, fallback), false)),
        );
        if ordered.len() < 2 {
            continue;
        }
        let deltas = calculate_deltas(&ordered);
        let order_idxs: Vec<i32> = starters
            .iter()
            .map(|c| c.car_idx)
            .chain(non_starters.iter().map(|c| c.car_idx))
            .collect();
        for (idx, delta) in order_idxs.into_iter().zip(deltas) {
            result.insert(idx, delta);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_equal_field_zeroish() {
        let d = calculate_deltas(&[(1500, true), (1500, true)]);
        assert_eq!(d.len(), 2);
        // Winner gains, loser loses roughly equal magnitude.
        assert!(d[0] > 0);
        assert!(d[1] < 0);
    }

    #[test]
    fn round_half_away_neg() {
        assert_eq!(round_half_away(-66.5), -67);
        assert_eq!(round_half_away(66.5), 67);
    }

    #[test]
    fn project_by_class_assigns_player() {
        let cars = vec![
            CarRow {
                car_idx: 0,
                position: 1,
                class_position: 1,
                irating: 2000,
                class_id: 1,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                position: 2,
                class_position: 2,
                irating: 1800,
                class_id: 1,
                is_player: true,
                ..Default::default()
            },
        ];
        let d = project_deltas_by_class(&cars);
        assert!(d.contains_key(&1));
        assert!(d[&0] > 0);
        assert!(d[&1] < 0);
    }

    #[test]
    fn project_started_via_laps_completed() {
        // Position 0 but completed a lap → starter (not DNS).
        let cars = vec![
            CarRow {
                car_idx: 0,
                position: 1,
                class_position: 1,
                irating: 2000,
                class_id: 1,
                laps_completed: 10,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                position: 0,
                class_position: 0,
                irating: 1800,
                class_id: 1,
                laps_completed: 8,
                is_player: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 0,
                class_position: 0,
                irating: 1600,
                class_id: 1,
                laps_completed: 0,
                ..Default::default()
            },
        ];
        let d = project_deltas_by_class(&cars);
        assert!(d.contains_key(&0));
        assert!(d.contains_key(&1));
        assert!(d.contains_key(&2));
        // DNS (car 2) still in field; two starters ordered by rank (car0 first).
        assert!(d[&0] > d[&1]);
    }

    #[test]
    fn calculate_deltas_reference_p5() {
        let entries = [
            (3203, true),
            (3922, true),
            (2974, true),
            (1739, true),
            (1250, true),
            (2588, false),
        ];
        let d = calculate_deltas(&entries);
        assert_eq!(d[4], -7);
    }

    /// Regression: subsession 87743952 — Logan Troyer P5 gained +27. Dropping the
    /// three unrated rookies left an 11-car field and projected +21.
    #[test]
    fn unrated_rookies_keep_field_size() {
        let finish: &[(i32, i32, i32)] = &[
            (0, 1, 1059),
            (1, 2, 1100),
            (2, 3, 1105),
            (3, 4, 1098),
            (4, 5, 1097), // player
            (5, 6, 1107),
            (6, 7, 0),    // Marc Cooper2 — results oldi_rating -1
            (7, 8, 1095),
            (8, 9, 0),    // Darcy Roulston4
            (9, 10, 0),   // Victor Zamorano
            (10, 11, 1131),
            (11, 12, 1114),
        ];
        let cars: Vec<CarRow> = finish
            .iter()
            .map(|&(idx, pos, ir)| CarRow {
                car_idx: idx,
                position: pos,
                class_position: pos,
                irating: ir,
                class_id: 74,
                is_player: idx == 4,
                laps_completed: 1,
                ..Default::default()
            })
            .collect();

        let d = project_deltas_by_class(&cars);
        let player = d[&4];
        // Full 12-car field with class-mean fill lands near the official +27
        // (exact ghost IRs are unknown; mean fallback is within a couple points).
        assert!(
            (25..=29).contains(&player),
            "expected ~+27 with unrated seats filled, got {player}"
        );

        // Old bug: skip ir<=0 → 9-car field → +0 at P5 among rated-only order,
        // or 11-car if only one rookie is missing → +21 with 1350 fill.
        let rated_only: Vec<(i32, bool)> = finish
            .iter()
            .filter(|(_, _, ir)| *ir > 0)
            .map(|(_, _, ir)| (*ir, true))
            .collect();
        assert_eq!(calculate_deltas(&rated_only)[4], 0);
    }
}
