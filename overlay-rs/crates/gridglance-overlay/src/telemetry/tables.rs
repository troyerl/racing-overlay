//! Relative / standings row builders + radar state (Python parity helpers).

use crate::config::OverlayConfig;
use serde::{Deserialize, Serialize};

use super::{CarRow, TelemetryFrame};

/// One painted table row (relative or standings).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableRow {
    pub key: String,
    pub empty: bool,
    pub position: i32,
    pub car_number: String,
    pub name: String,
    pub lic_class: String,
    pub sr: String,
    pub irating: i32,
    pub irating_delta: Option<i32>,
    pub class_color: String,
    /// Signed gap seconds for relative (ahead > 0). Standings leaves None.
    pub gap_secs: Option<f32>,
    /// Preformatted gap for standings (e.g. "+1.2", "—", "-1L").
    pub gap_text: String,
    pub last_lap: String,
    pub best_lap: String,
    pub is_player: bool,
    pub in_pit: bool,
    pub on_pit: bool,
    pub lapping: bool,
    pub lap_ahead: bool,
    pub inactive: bool,
    pub is_speaking: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableSlots {
    pub header_left: String,
    pub header_center: String,
    pub header_right: String,
    pub footer_left: String,
    pub footer_center: String,
    pub footer_right: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RadarState {
    pub left: bool,
    pub right: bool,
    pub left2: bool,
    pub right2: bool,
    pub left_pos: f32,
    pub right_pos: f32,
    pub ahead: Option<f32>,
    pub behind: Option<f32>,
    pub left_label: String,
    pub right_label: String,
    pub clear_secs: Option<f32>,
}

pub fn empty_row(tag: &str) -> TableRow {
    TableRow {
        key: format!("_empty_{tag}"),
        empty: true,
        ..Default::default()
    }
}

/// Wrap EstTime delta into (-half, half].
pub fn wrap_est_delta(delta: f32, lap_est: f32) -> f32 {
    if lap_est <= 0.0 {
        return delta;
    }
    let half = lap_est * 0.5;
    let mut d = delta;
    if d > half {
        d -= lap_est;
    } else if d < -half {
        d += lap_est;
    }
    d
}

pub fn wrap_lap_delta(pct: f32, me: f32) -> f32 {
    let mut d = pct - me;
    if d > 0.5 {
        d -= 1.0;
    } else if d < -0.5 {
        d += 1.0;
    }
    d
}

fn fmt_irating(ir: i32) -> String {
    if ir <= 0 {
        return String::new();
    }
    if ir >= 1000 {
        format!("{:.1}k", ir as f32 / 1000.0)
    } else {
        ir.to_string()
    }
}

fn parse_license(s: &str) -> (String, String) {
    // e.g. "A 3.42" or "A"
    let mut parts = s.split_whitespace();
    let cls = parts.next().unwrap_or("").to_string();
    let sr = parts.next().unwrap_or("").to_string();
    (cls, sr)
}

impl TableRow {
    pub fn from_car(c: &CarRow, gap_secs: Option<f32>, gap_text: String, inactive: bool) -> Self {
        let (lic_class, sr) = parse_license(&c.license);
        Self {
            key: c.car_idx.to_string(),
            empty: false,
            position: c.position,
            car_number: c.car_number.clone(),
            name: c.name.clone(),
            lic_class,
            sr,
            irating: c.irating,
            irating_delta: c.irating_delta,
            class_color: c.class_color.clone(),
            gap_secs,
            gap_text,
            last_lap: c.last_lap.clone(),
            best_lap: c.best_lap.clone(),
            is_player: c.is_player,
            in_pit: c.in_pit,
            on_pit: c.on_pit,
            lapping: c.lapping,
            lap_ahead: c.lap_ahead,
            inactive,
            is_speaking: c.is_speaking,
        }
    }
}

/// Build relative rows centered on the player (Python `_update_relative`).
pub fn build_relative(
    cars: &[CarRow],
    cfg: &OverlayConfig,
    lap_est_hint: f32,
) -> (Vec<TableRow>, TableSlots) {
    let n_ahead = cfg.f64_key("relative", "rows_ahead", 3.0) as usize;
    let n_behind = cfg.f64_key("relative", "rows_behind", 3.0) as usize;
    let center = cfg.bool_key("relative", "center_on_player", true);

    let Some(player) = cars.iter().find(|c| c.is_player) else {
        return (Vec::new(), default_relative_slots(cars));
    };
    let me = player.est_time;
    let lap_est = if lap_est_hint > 10.0 {
        lap_est_hint
    } else {
        estimate_lap_est(cars)
    };

    let mut rels: Vec<(f32, &CarRow)> = Vec::new();
    for c in cars {
        if c.is_player || c.is_pace_car {
            continue;
        }
        if !relative_include(c, player) {
            continue;
        }
        let delta = wrap_est_delta(c.est_time - me, lap_est);
        rels.push((delta, c));
    }

    let mut ahead: Vec<_> = rels.iter().copied().filter(|(d, _)| *d > 0.0).collect();
    ahead.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    ahead.truncate(n_ahead);

    let mut behind: Vec<_> = rels.iter().copied().filter(|(d, _)| *d <= 0.0).collect();
    behind.sort_by(|a, b| (-a.0).partial_cmp(&-b.0).unwrap_or(std::cmp::Ordering::Equal));
    behind.truncate(n_behind);

    let mut rows = Vec::new();
    if center {
        for k in 0..(n_ahead.saturating_sub(ahead.len())) {
            rows.push(empty_row(&format!("rel_top{k}")));
        }
    }
    for (delta, c) in ahead.iter().rev() {
        rows.push(TableRow::from_car(c, Some(*delta), format!("{:.1}", delta.abs()), false));
    }
    rows.push(TableRow::from_car(player, Some(0.0), "0.0".into(), false));
    for (delta, c) in &behind {
        rows.push(TableRow::from_car(c, Some(*delta), format!("{:.1}", delta.abs()), false));
    }
    if center {
        for k in 0..(n_behind.saturating_sub(behind.len())) {
            rows.push(empty_row(&format!("rel_bot{k}")));
        }
    }

    (rows, default_relative_slots(cars))
}

fn relative_include(c: &CarRow, _player: &CarRow) -> bool {
    // Race: on-track or pit; practice-ish: prefer on-track (surface unknown → include).
    !c.is_pace_car && (c.on_track || c.in_pit || c.on_pit || c.position > 0)
}

fn estimate_lap_est(cars: &[CarRow]) -> f32 {
    let mut times: Vec<f32> = cars
        .iter()
        .filter(|c| c.est_time > 1.0)
        .map(|c| c.est_time)
        .collect();
    if times.len() < 2 {
        return 90.0;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Rough: span between min and max as proxy; Python uses telemetry LapEstTime.
    let span = times.last().copied().unwrap_or(90.0) - times.first().copied().unwrap_or(0.0);
    if span > 20.0 && span < 300.0 {
        span
    } else {
        90.0
    }
}

fn default_relative_slots(cars: &[CarRow]) -> TableSlots {
    let sof = {
        let irs: Vec<i32> = cars.iter().map(|c| c.irating).filter(|i| *i > 0).collect();
        if irs.is_empty() {
            "--".into()
        } else {
            let avg = irs.iter().sum::<i32>() / irs.len() as i32;
            fmt_irating(avg)
        }
    };
    let player = cars.iter().find(|c| c.is_player);
    TableSlots {
        header_left: format!("SOF {sof}"),
        header_center: String::new(),
        header_right: player
            .map(|p| format!("P{}", p.position))
            .unwrap_or_default(),
        footer_left: String::new(),
        footer_center: String::new(),
        footer_right: String::new(),
    }
}

/// Standings windowing (Python `standings_row_list`).
pub fn build_standings(cars: &[CarRow], cfg: &OverlayConfig) -> (Vec<TableRow>, TableSlots) {
    let mut ranked: Vec<&CarRow> = cars
        .iter()
        .filter(|c| !c.is_pace_car && c.position > 0)
        .collect();
    ranked.sort_by_key(|c| c.position);

    let center = cfg.bool_key("standings", "center_on_player", true);
    let pin_podium = cfg.bool_key("standings", "pin_podium", false);
    let rows_n = cfg.f64_key("standings", "rows", 10.0) as usize;
    let rows_ahead = cfg.f64_key("standings", "rows_ahead", 4.0) as usize;
    let rows_behind = cfg.f64_key("standings", "rows_behind", 5.0) as usize;

    let leader_f2 = ranked.first().map(|c| c.f2_time).unwrap_or(0.0);

    let build = |c: &CarRow| -> TableRow {
        let gap_text = if c.position <= 1 {
            "—".into()
        } else if leader_f2 > 0.0 && c.f2_time > 0.0 {
            let g = c.f2_time - leader_f2;
            if g > 0.0 {
                format!("+{g:.1}")
            } else {
                format!("{g:.1}")
            }
        } else {
            c.gap.clone()
        };
        let inactive = c.inactive || (!c.on_track && !c.in_pit && !c.on_pit && c.lap_dist_pct < 0.0);
        TableRow::from_car(c, None, gap_text, inactive)
    };

    let idxs: Vec<usize> = (0..ranked.len()).collect();
    let player_pos = ranked.iter().position(|c| c.is_player);

    let out: Vec<TableRow> = if !center || player_pos.is_none() {
        ranked.iter().take(rows_n).map(|c| build(c)).collect()
    } else {
        let pidx = player_pos.unwrap();
        let player = pidx;
        let above = rows_ahead;
        let below = rows_behind;
        let window = above + 1 + below;

        if pin_podium {
            let mut podium = Vec::new();
            for slot in 0..3 {
                if slot < ranked.len() {
                    podium.push(build(ranked[slot]));
                } else {
                    podium.push(empty_row(&format!("podium{slot}")));
                }
            }
            let podium_set: std::collections::HashSet<usize> =
                (0..ranked.len().min(3)).collect();
            let on_podium = podium_set.contains(&player);
            let mut slots = window.saturating_sub(3);
            if !on_podium {
                slots = slots.max(1);
            }
            let picked = pick_context_indices(&idxs, pidx, player, &podium_set, slots, above, below);
            let mut context: Vec<TableRow> = picked.iter().map(|&i| build(ranked[i])).collect();
            for i in context.len()..slots {
                context.push(empty_row(&format!("ctx{i}")));
            }
            podium.extend(context);
            podium
        } else {
            let mut out = Vec::new();
            let start = pidx as i32 - above as i32;
            for i in 0..window {
                let slot = start + i as i32;
                if slot >= 0 && (slot as usize) < ranked.len() {
                    out.push(build(ranked[slot as usize]));
                } else {
                    out.push(empty_row(&format!("win{i}")));
                }
            }
            out
        }
    };

    let shown = out.iter().filter(|r| !r.empty).count();
    let total = ranked.len();
    let title = cfg.str_key("standings", "title", "Standings");
    let slots = TableSlots {
        header_left: "ORDER".into(),
        header_center: title,
        header_right: format!("{shown}/{total}"),
        footer_left: String::new(),
        footer_center: String::new(),
        footer_right: String::new(),
    };
    (out, slots)
}

fn pick_context_indices(
    ranked: &[usize],
    pidx: usize,
    player: usize,
    podium_idxs: &std::collections::HashSet<usize>,
    limit: usize,
    rows_ahead: usize,
    rows_behind: usize,
) -> Vec<usize> {
    if limit == 0 {
        return Vec::new();
    }
    let total = ranked.len();
    let on_podium = podium_idxs.contains(&player);

    if on_podium {
        let mut chosen = Vec::new();
        let mut off = 1i32;
        while chosen.len() < limit {
            let mut added = false;
            for delta in [-off, off] {
                let slot = pidx as i32 + delta;
                if slot >= 0 && (slot as usize) < total {
                    let idx = ranked[slot as usize];
                    if !podium_idxs.contains(&idx) && !chosen.contains(&idx) {
                        chosen.push(idx);
                        added = true;
                        if chosen.len() >= limit {
                            break;
                        }
                    }
                }
            }
            if !added {
                break;
            }
            off += 1;
        }
        chosen.sort_by_key(|i| ranked.iter().position(|x| x == i).unwrap_or(0));
        return chosen;
    }

    let need = limit.saturating_sub(1);
    let mut above = rows_ahead.min(need);
    let below = rows_behind.min(need.saturating_sub(above));
    above = above.min(need.saturating_sub(below));

    let mut chosen = vec![player];
    let start = pidx as i32 - above as i32;
    let end = pidx + below;
    for slot in start..=(end as i32) {
        if slot == pidx as i32 {
            continue;
        }
        if slot >= 0 && (slot as usize) < total {
            let idx = ranked[slot as usize];
            if !podium_idxs.contains(&idx) && !chosen.contains(&idx) {
                chosen.push(idx);
            }
        }
    }

    let mut off = above.max(below) as i32 + 1;
    while chosen.len() < limit {
        let mut added = false;
        for delta in [-off, off] {
            let slot = pidx as i32 + delta;
            if slot >= 0 && (slot as usize) < total {
                let idx = ranked[slot as usize];
                if !podium_idxs.contains(&idx) && !chosen.contains(&idx) {
                    chosen.push(idx);
                    added = true;
                    if chosen.len() >= limit {
                        break;
                    }
                }
            }
        }
        if !added {
            break;
        }
        off += 1;
    }
    chosen.sort_by_key(|i| ranked.iter().position(|x| x == i).unwrap_or(0));
    chosen
}

/// Fill relative_cars / standings_cars and enrich radar from cars + cfg.
pub fn finalize_frame(frame: &mut TelemetryFrame, cfg: &OverlayConfig) {
    let (rel, mut rel_slots) = build_relative(&frame.cars, cfg, frame.lap_est_time);
    enrich_slots(frame, &mut rel_slots, "relative");
    let (std, mut std_slots) = build_standings(&frame.cars, cfg);
    enrich_slots(frame, &mut std_slots, "standings");
    frame.relative_cars = rel;
    frame.relative_slots = rel_slots;
    frame.standings_cars = std;
    frame.standings_slots = std_slots;

    // Merge proximity / side-pos from lap % when IRSDK only set L/R bools (or demo).
    let enriched = build_radar(
        &frame.cars,
        cfg,
        frame.radar.left,
        frame.radar.right,
        frame.radar.left2,
        frame.radar.right2,
        frame.player_lap_dist_pct,
    );
    // Keep explicit clear_secs / labels from IRSDK when present.
    if frame.radar.ahead.is_none() {
        frame.radar.ahead = enriched.ahead;
    }
    if frame.radar.behind.is_none() {
        frame.radar.behind = enriched.behind;
    }
    if frame.radar.left || frame.radar.right {
        if frame.radar.left_pos == 0.0 && enriched.left_pos != 0.0 {
            frame.radar.left_pos = enriched.left_pos;
        }
        if frame.radar.right_pos == 0.0 && enriched.right_pos != 0.0 {
            frame.radar.right_pos = enriched.right_pos;
        }
        if frame.radar.left_label.is_empty() {
            frame.radar.left_label = enriched.left_label;
        }
        if frame.radar.right_label.is_empty() {
            frame.radar.right_label = enriched.right_label;
        }
    }
    frame.radar_left = frame.radar.left;
    frame.radar_right = frame.radar.right;
}

fn enrich_slots(frame: &TelemetryFrame, slots: &mut TableSlots, section: &str) {
    // Fill common footer/header strings the painters display as plain text.
    if slots.footer_left.is_empty() && section == "relative" {
        let mins = (frame.session_time / 60.0).floor() as i32;
        let secs = (frame.session_time % 60.0).floor() as i32;
        slots.footer_left = format!("{mins:02}:{secs:02}");
    }
    if slots.footer_center.is_empty() {
        if frame.laps_total > 0 {
            slots.footer_center = format!("L{}/{}", frame.lap, frame.laps_total);
        } else if frame.lap > 0 {
            slots.footer_center = format!("L{}", frame.lap);
        }
    }
    if slots.footer_right.is_empty() && section == "relative" {
        slots.footer_right = format!("x{}", frame.incidents);
    }
    if slots.footer_left.is_empty() && section == "standings" {
        if let Some(tt) = frame.track_temp {
            slots.footer_left = format!("TRK {tt:.0}°");
        }
    }
    if slots.footer_right.is_empty() && section == "standings" {
        if let Some(at) = frame.air_temp {
            slots.footer_right = format!("AIR {at:.0}°");
        }
    }
    if slots.footer_center.is_empty() && section == "standings" {
        let mins = (frame.session_time / 60.0).floor() as i32;
        let secs = (frame.session_time % 60.0).floor() as i32;
        slots.footer_center = format!("{mins:02}:{secs:02}");
    }
}

/// Radar proximity + side fore/aft from CarLeftRight flags + LapDistPct.
pub fn build_radar(
    cars: &[CarRow],
    cfg: &OverlayConfig,
    left: bool,
    right: bool,
    left2: bool,
    right2: bool,
    player_pct: f32,
) -> RadarState {
    let range = cfg.f64_key("radar", "range_pct", 0.03) as f32;
    let zone = cfg.f64_key("radar", "alongside_zone_pct", 0.004) as f32;
    let span = cfg.f64_key("radar", "side_span_pct", 0.0045) as f32;
    let want_front = cfg.bool_key("radar", "show_front", true);
    let want_rear = cfg.bool_key("radar", "show_rear", true);
    let want_labels = cfg.bool_key("radar", "show_side_labels", false);

    let mut nearest_ahead = None;
    let mut nearest_behind = None;
    let mut left_delta = None;
    let mut right_delta = None;
    let mut left_label = String::new();
    let mut right_label = String::new();

    let me = player_pct;
    // Collect alongside candidates sorted by |delta|.
    let mut alongside: Vec<(f32, &CarRow)> = Vec::new();
    for c in cars {
        if c.is_player || c.is_pace_car || c.lap_dist_pct < 0.0 {
            continue;
        }
        if !(c.on_track || c.in_pit || c.on_pit) {
            continue;
        }
        let d = wrap_lap_delta(c.lap_dist_pct, me);
        if want_front && zone < d && d <= range {
            nearest_ahead = Some(nearest_ahead.map_or(d, |a: f32| a.min(d)));
        } else if want_rear && -range <= d && d < -zone {
            nearest_behind = Some(nearest_behind.map_or(d, |b: f32| b.max(d)));
        }
        if d.abs() <= zone * 3.0 {
            alongside.push((d, c));
        }
    }
    alongside.sort_by(|a, b| a.0.abs().partial_cmp(&b.0.abs()).unwrap_or(std::cmp::Ordering::Equal));

    if left {
        if let Some((d, c)) = alongside.first() {
            left_delta = Some(*d);
            if want_labels {
                left_label = c.car_number.clone();
            }
        }
    }
    if right {
        let skip = if left { 1 } else { 0 };
        if let Some((d, c)) = alongside.get(skip) {
            right_delta = Some(*d);
            if want_labels {
                right_label = c.car_number.clone();
            }
        } else if let Some((d, c)) = alongside.first() {
            right_delta = Some(*d);
            if want_labels {
                right_label = c.car_number.clone();
            }
        }
    }

    let side_pos = |delta: Option<f32>| -> f32 {
        match delta {
            Some(d) if span > 0.0 => (d / span).clamp(-1.0, 1.0),
            _ => 0.0,
        }
    };
    let closeness = |delta: Option<f32>| {
        delta.map(|d| (1.0 - d.abs() / range).clamp(0.0, 1.0))
    };

    RadarState {
        left,
        right,
        left2,
        right2,
        left_pos: side_pos(left_delta),
        right_pos: side_pos(right_delta),
        ahead: if want_front {
            closeness(nearest_ahead)
        } else {
            None
        },
        behind: if want_rear {
            closeness(nearest_behind)
        } else {
            None
        },
        left_label,
        right_label,
        clear_secs: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OverlayConfig;

    #[test]
    fn wrap_est_crosses_s_f() {
        let d = wrap_est_delta(80.0, 90.0);
        assert!((d - (-10.0)).abs() < 0.01);
        let d2 = wrap_est_delta(-80.0, 90.0);
        assert!((d2 - 10.0).abs() < 0.01);
    }

    #[test]
    fn relative_centers_player() {
        let mut cars = Vec::new();
        for i in 0..7 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                est_time: 10.0 + i as f32 * 2.0,
                is_player: i == 3,
                on_track: true,
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let (rows, _) = build_relative(&cars, &cfg, 90.0);
        let player_i = rows.iter().position(|r| r.is_player).unwrap();
        // With 3 ahead / 3 behind and centering, player is in the middle slot.
        assert_eq!(player_i, 3);
        assert_eq!(rows.len(), 7);
    }

    #[test]
    fn standings_window_includes_player() {
        let mut cars = Vec::new();
        for i in 0..12 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                f2_time: i as f32 * 0.5,
                is_player: i == 7,
                on_track: true,
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let (rows, _) = build_standings(&cars, &cfg);
        assert!(rows.iter().any(|r| r.is_player));
        assert!(!rows.is_empty());
    }
}
