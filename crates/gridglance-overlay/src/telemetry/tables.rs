//! Relative / standings row builders + radar state (Python parity helpers).

use crate::config::OverlayConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{format, CarRow, TelemetryFrame};

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
    /// Preformatted gap for standings (e.g. "+1.2", "--", "-1L").
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
    /// Holds the session-fastest best lap (purple time + trophy badge).
    #[serde(default)]
    pub session_best: bool,
    /// Relative strategy cue: `"undercut"` | `"cover"` when fuel window is open.
    pub strat_tag: Option<String>,
    pub class_position: i32,
    pub status_kind: Option<String>,
    /// Session flag label (blue / meatball / black / ...) for the `car_flag` column.
    #[serde(default)]
    pub car_flag: Option<String>,
    pub closing: Option<f32>,
    pub team: String,
    pub nickname: String,
    pub laps: i32,
    /// Pit column history text from `pit_mode` (empty -> paint as "--"; in-pit still "PIT").
    #[serde(default)]
    pub pit_text: String,
    /// Cloud professional-driver list match.
    #[serde(default)]
    pub is_pro: bool,
    /// Personal driver-group icon key (empty = none).
    #[serde(default)]
    pub group_icon: String,
    /// Personal driver-group accent color hex.
    #[serde(default)]
    pub group_color: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableSlotItem {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TableSlots {
    pub header_left: TableSlotItem,
    pub header_center: TableSlotItem,
    pub header_right: TableSlotItem,
    pub footer_left: TableSlotItem,
    pub footer_center: TableSlotItem,
    pub footer_right: TableSlotItem,
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
    pub fn from_car(
        c: &CarRow,
        gap_secs: Option<f32>,
        gap_text: String,
        inactive: bool,
        app: &serde_json::Value,
    ) -> Self {
        let (lic_class, sr) = parse_license(&c.license);
        let is_pro = crate::cloud::is_pro_driver(&c.name, app);
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
            session_best: false,
            strat_tag: None,
            class_position: c.class_position,
            status_kind: c.status_kind.clone(),
            car_flag: c.car_flag.clone(),
            closing: None,
            team: String::new(),
            nickname: String::new(),
            laps: c.lap,
            pit_text: String::new(),
            is_pro,
            group_icon: String::new(),
            group_color: String::new(),
        }
    }

    pub fn apply_driver_group(&mut self, groups: &serde_json::Value) {
        if self.is_pro || self.name.is_empty() {
            return;
        }
        if let Some(g) = crate::driver_groups::driver_group_for_name(&self.name, groups) {
            self.group_icon = g.icon;
            self.group_color = g.color;
        }
    }
}

/// Sticky Relative neighbor order so noisy gaps do not reshuffle every SDK
/// tick (which restarts row slides and looks like low FPS).
#[derive(Debug, Clone, Default)]
pub struct RelativeOrderHysteresis {
    ahead_idxs: Vec<i32>,
    behind_idxs: Vec<i32>,
    /// Last projected iRating deltas (car_idx → delta), held through cooldown.
    irating_deltas: HashMap<i32, i32>,
    irating_player_delta: Option<i32>,
}

/// Seconds of gap required to overturn the previous Relative order.
const REL_ORDER_MARGIN_S: f32 = 0.25;

fn sticky_sort_by_delta(items: &mut Vec<(f32, &CarRow)>, prev: &[i32], ascending: bool) {
    items.sort_by(|a, b| {
        let da = a.0;
        let db = b.0;
        let cmp_delta = if ascending {
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        } else {
            (-da).partial_cmp(&-db).unwrap_or(std::cmp::Ordering::Equal)
        };
        if (da - db).abs() < REL_ORDER_MARGIN_S {
            let ia = prev.iter().position(|&x| x == a.1.car_idx);
            let ib = prev.iter().position(|&x| x == b.1.car_idx);
            match (ia, ib) {
                (Some(ia), Some(ib)) if ia != ib => return ia.cmp(&ib),
                _ => {}
            }
        }
        cmp_delta
    });
}

/// Mark exactly one table focus for Relative/Standings.
///
/// While your seated car is still a live competitor, keep focus on you even if
/// the iRacing camera is on someone else. Pure spectators (ghost seated entry)
/// center on the camera car, then seated, then race leader.
fn apply_table_focus(cars: &mut [CarRow], camera_car_idx: Option<i32>) {
    let seated = cars.iter().position(|c| c.is_player);
    let seated_live = seated
        .map(|i| cars[i].is_live_competitor())
        .unwrap_or(false);
    let focus = if seated_live {
        seated
    } else {
        camera_car_idx
            .and_then(|idx| cars.iter().position(|c| c.car_idx == idx))
            .or(seated)
            .or_else(|| {
                cars.iter()
                    .enumerate()
                    .filter(|(_, c)| !c.is_pace_car && c.position > 0)
                    .min_by_key(|(_, c)| c.position)
                    .map(|(i, _)| i)
            })
    };
    if let Some(fi) = focus {
        for (i, car) in cars.iter_mut().enumerate() {
            car.is_player = i == fi;
        }
    }
}

/// Apply lapped-traffic tint relative to a focus car.
///
/// Only applied in race sessions — practice/qualifying ignore lap-down
/// blue/red row coloring. iRacing Relative convention: red = car lapping
/// you (`lap_ahead`), blue = traffic you're lapping.
///
/// For a one-lap difference, tint only when the faster car is within five
/// seconds behind and approaching the slower car. A difference of two or more
/// completed laps is always tinted.
///
/// `focus_car_idx` overrides `is_player` when set (map uses camera/live focus
/// while tables use the marked table-focus car).
fn apply_lap_tints(
    cars: &mut [CarRow],
    lap_est_hint: f32,
    session_type: Option<&str>,
    focus_car_idx: Option<i32>,
) {
    for car in cars.iter_mut() {
        car.lapping = false;
        car.lap_ahead = false;
    }
    if !is_race_session(session_type) {
        return;
    }
    let Some(focus_idx) = focus_car_idx
        .and_then(|idx| cars.iter().position(|c| c.car_idx == idx))
        .or_else(|| cars.iter().position(|c| c.is_player))
    else {
        return;
    };
    let focus_lap = cars[focus_idx].lap;
    let focus_est = cars[focus_idx].est_time;
    let focus_pct = cars[focus_idx].lap_dist_pct;
    let focus_on_route = {
        let f = &cars[focus_idx];
        f.on_track || f.in_pit || f.on_pit
    };
    let lap_est = if lap_est_hint > 10.0 {
        lap_est_hint
    } else {
        estimate_lap_est(cars)
    };

    for (i, car) in cars.iter_mut().enumerate() {
        if i == focus_idx || car.is_pace_car {
            continue;
        }
        let lap_diff = car.lap - focus_lap;
        // Prefer LapDistPct (same basis as Relative neighbors); EstTime fallback.
        let relative_secs = {
            let car_on_route = car.on_track || car.in_pit || car.on_pit;
            if car.lap_dist_pct >= 0.0 && focus_pct >= 0.0 && car_on_route && focus_on_route {
                wrap_lap_delta(car.lap_dist_pct, focus_pct) * lap_est
            } else if car.est_time > 1.0 && focus_est > 1.0 {
                wrap_est_delta(car.est_time - focus_est, lap_est)
            } else {
                f32::NAN
            }
        };
        let one_lap_close = relative_secs.is_finite()
            && match lap_diff {
                // Focus is one lap ahead; slower car is just ahead on track.
                -1 => relative_secs > 0.0 && relative_secs <= 5.0,
                // Other car is one lap ahead and just behind the focus.
                1 => (-5.0..0.0).contains(&relative_secs),
                _ => false,
            };
        car.lapping = lap_diff.abs() >= 2 || one_lap_close;
        car.lap_ahead = car.lapping && lap_diff > 0;
    }
}

fn is_race_session(session_type: Option<&str>) -> bool {
    session_type
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("race")
}

/// Build relative rows centered on the player (Python `_update_relative`).
///
/// Live order is by track position (`LapDistPct`) so the list shows who is
/// physically ahead/behind on track. Gaps are seconds ≈ Δpct × lap estimate
/// (EstTime when both cars have a valid sample).
pub fn build_relative(
    cars: &[CarRow],
    cfg: &OverlayConfig,
    lap_est_hint: f32,
    sticky: Option<&mut RelativeOrderHysteresis>,
    session_state: i32,
    session_type: Option<&str>,
    app: &serde_json::Value,
    groups: &serde_json::Value,
) -> Vec<TableRow> {
    let n_ahead = cfg.f64_key("relative", "rows_ahead", 3.0).max(0.0) as usize;
    let n_behind = cfg.f64_key("relative", "rows_behind", 3.0).max(0.0) as usize;
    // Ahead/behind are authoritative. Do not re-derive behind from `rows`
    // (that zeroed rows_behind when an old preset had rows ~= ahead).
    let _ = cfg.f64_key("relative", "rows", (n_ahead + n_behind) as f64);
    let center = cfg.bool_key("relative", "center_on_player", true);
    let _ = session_type;

    let Some(player) = cars.iter().find(|c| c.is_player) else {
        if let Some(s) = sticky {
            s.ahead_idxs.clear();
            s.behind_idxs.clear();
        }
        return Vec::new();
    };

    // Pre-green / warmup: order by qualify/grid position when the focus car
    // is not yet on a usable track percentage.
    if use_grid_relative(session_state, cars, player) {
        return build_relative_by_position(
            cars, player, n_ahead, n_behind, center, sticky, app, groups,
        );
    }

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
        let Some(delta) = relative_gap_secs(c, player, lap_est) else {
            continue;
        };
        rels.push((delta, c));
    }

    let prev_ahead = sticky
        .as_ref()
        .map(|s| s.ahead_idxs.clone())
        .unwrap_or_default();
    let prev_behind = sticky
        .as_ref()
        .map(|s| s.behind_idxs.clone())
        .unwrap_or_default();

    let mut ahead: Vec<_> = rels.iter().copied().filter(|(d, _)| *d > 0.0).collect();
    sticky_sort_by_delta(&mut ahead, &prev_ahead, true);
    ahead.truncate(n_ahead);

    let mut behind: Vec<_> = rels.iter().copied().filter(|(d, _)| *d <= 0.0).collect();
    sticky_sort_by_delta(&mut behind, &prev_behind, false);
    behind.truncate(n_behind);

    if let Some(s) = sticky {
        s.ahead_idxs = ahead.iter().map(|(_, c)| c.car_idx).collect();
        s.behind_idxs = behind.iter().map(|(_, c)| c.car_idx).collect();
    }

    let mut rows = Vec::new();
    if center {
        for k in 0..(n_ahead.saturating_sub(ahead.len())) {
            rows.push(empty_row(&format!("rel_top{k}")));
        }
    }
    for (delta, c) in ahead.iter().rev() {
        rows.push(TableRow::from_car(
            c,
            Some(*delta),
            format!("{:.1}", delta.abs()),
            c.inactive,
            app,
        ));
    }
    rows.push(TableRow::from_car(
        player,
        Some(0.0),
        "0.0".into(),
        player.inactive,
        app,
    ));
    for (delta, c) in &behind {
        rows.push(TableRow::from_car(
            c,
            Some(*delta),
            format!("{:.1}", delta.abs()),
            c.inactive,
            app,
        ));
    }
    if center {
        for k in 0..(n_behind.saturating_sub(behind.len())) {
            rows.push(empty_row(&format!("rel_bot{k}")));
        }
    }

    for row in &mut rows {
        row.apply_driver_group(groups);
    }
    rows
}

/// Signed gap in seconds: positive = ahead on track, negative = behind.
/// Prefer LapDistPct (physical neighbors); fall back to EstTime.
fn relative_gap_secs(car: &CarRow, player: &CarRow, lap_est: f32) -> Option<f32> {
    let le = if lap_est > 10.0 { lap_est } else { 90.0 };
    let pct_ok = car.lap_dist_pct >= 0.0
        && player.lap_dist_pct >= 0.0
        && (car.on_track || car.in_pit || car.on_pit);
    if pct_ok {
        return Some(wrap_lap_delta(car.lap_dist_pct, player.lap_dist_pct) * le);
    }
    if car.est_time > 1.0 && player.est_time > 1.0 {
        return Some(wrap_est_delta(car.est_time - player.est_time, le));
    }
    None
}

/// Pre-race: SessionState < 4 (GetInCar / Warmup / Parade) when the focus
/// car is not on a usable lap %, or live timing is mostly missing while grid
/// positions are populated.
fn use_grid_relative(session_state: i32, cars: &[CarRow], player: &CarRow) -> bool {
    // Once the focus car has a track percentage, order by on-track neighbors.
    if player.lap_dist_pct >= 0.0 && (player.on_track || player.in_pit || player.on_pit) {
        return false;
    }
    if session_state > 0 && session_state < 4 {
        return true;
    }
    let ranked = cars
        .iter()
        .filter(|c| !c.is_pace_car && c.position > 0)
        .count();
    if ranked < 2 {
        return false;
    }
    let with_est = cars
        .iter()
        .filter(|c| !c.is_pace_car && c.position > 0 && c.est_time > 1.0)
        .count();
    let with_pct = cars
        .iter()
        .filter(|c| !c.is_pace_car && c.position > 0 && c.lap_dist_pct >= 0.0 && c.on_track)
        .count();
    with_pct < 2 && with_est * 2 < ranked
}

fn build_relative_by_position(
    cars: &[CarRow],
    player: &CarRow,
    n_ahead: usize,
    n_behind: usize,
    center: bool,
    sticky: Option<&mut RelativeOrderHysteresis>,
    app: &serde_json::Value,
    groups: &serde_json::Value,
) -> Vec<TableRow> {
    if let Some(s) = sticky {
        s.ahead_idxs.clear();
        s.behind_idxs.clear();
    }
    let mut ranked: Vec<&CarRow> = cars
        .iter()
        .filter(|c| {
            if c.is_pace_car || c.position <= 0 {
                return false;
            }
            c.car_idx == player.car_idx || relative_include(c, player)
        })
        .collect();
    ranked.sort_by_key(|c| c.position);
    let Some(pidx) = ranked.iter().position(|c| c.car_idx == player.car_idx) else {
        // Player not on grid yet -- still show top of qualify order.
        let mut rows: Vec<TableRow> = ranked
            .iter()
            .take(n_ahead + 1 + n_behind)
            .map(|c| TableRow::from_car(c, None, "--".into(), c.inactive, app))
            .collect();
        for row in &mut rows {
            row.apply_driver_group(groups);
        }
        return rows;
    };

    let ahead_cars: Vec<&CarRow> = ranked[..pidx].iter().rev().take(n_ahead).copied().collect();
    let behind_cars: Vec<&CarRow> = ranked[pidx + 1..].iter().take(n_behind).copied().collect();

    let mut rows = Vec::new();
    if center {
        for k in 0..(n_ahead.saturating_sub(ahead_cars.len())) {
            rows.push(empty_row(&format!("rel_top{k}")));
        }
    }
    for c in ahead_cars.iter().rev() {
        rows.push(TableRow::from_car(c, None, "--".into(), c.inactive, app));
    }
    rows.push(TableRow::from_car(
        player,
        Some(0.0),
        "0.0".into(),
        player.inactive,
        app,
    ));
    for c in &behind_cars {
        rows.push(TableRow::from_car(c, None, "--".into(), c.inactive, app));
    }
    if center {
        for k in 0..(n_behind.saturating_sub(behind_cars.len())) {
            rows.push(empty_row(&format!("rel_bot{k}")));
        }
    }
    for row in &mut rows {
        row.apply_driver_group(groups);
    }
    rows
}

fn relative_include(c: &CarRow, _player: &CarRow) -> bool {
    if c.is_pace_car || c.inactive {
        return false;
    }
    // Racing surface, brief off-track, or pit-entry blend. Pit stalls and
    // garage (NotInWorld / no lap %) stay off the neighbor list.
    if c.on_track || c.approaching_pits {
        return true;
    }
    // OffTrack (grass/kerb) still has a usable lap % — keep as a neighbor.
    c.lap_dist_pct >= 0.0 && !c.on_pit && !c.in_pit
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

/// Standings windowing — same rules while racing or spectating.
///
/// Uses live race positions (already on each `CarRow`) and centers on the
/// table focus car when `center_on_player` is on (camera / seated / leader).
/// Otherwise lists from P1. Pin-podium is race-only.
///
/// Qualifying/practice order by best lap and renumber 1..N. Race sessions keep
/// live race positions so the table matches iRacing Results. When spectating
/// as a pure spectator, the seated player's ghost entry is omitted — but if you
/// are still racing and just watching another car, you stay in the field.
///
/// `standings.rows` (Total rows) is the exact number of slots shown.
pub fn build_standings(
    cars: &[CarRow],
    cfg: &OverlayConfig,
    app: &serde_json::Value,
    groups: &serde_json::Value,
    session_type: Option<&str>,
    camera_car_idx: Option<i32>,
    seated_car_idx: Option<i32>,
) -> Vec<TableRow> {
    let spectating = matches!(
        (camera_car_idx, seated_car_idx),
        (Some(cam), Some(seat)) if cam != seat
    );
    let field: Vec<&CarRow> = cars
        .iter()
        .filter(|c| {
            if c.is_pace_car {
                return false;
            }
            // Pure spectator ghost only — keep your live race car when watching.
            if spectating && Some(c.car_idx) == seated_car_idx && c.is_spectator_ghost() {
                return false;
            }
            true
        })
        .collect();

    // Race: always live race order (match iRacing Results). Qual/practice: best lap.
    // Do NOT use the sparse-position heuristic during race — retired/garage cars
    // leave holes in CarIdxPosition and that was re-sorting the field by best lap.
    let timing_order = !is_race_session(session_type);
    let ordered = standings_field_order(&field, timing_order);

    // Qualifying: always list from P1 (timing board). Race/practice: window
    // around the focus car when center_on_player is on.
    let is_qual = session_type
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("qual");
    let center = !is_qual && cfg.bool_key("standings", "center_on_player", true);
    let pin_podium =
        is_race_session(session_type) && cfg.bool_key("standings", "pin_podium", false);
    let rows_ahead = cfg.f64_key("standings", "rows_ahead", 4.0).max(0.0) as usize;
    let _rows_behind = cfg.f64_key("standings", "rows_behind", 5.0).max(0.0) as usize;
    let target = cfg
        .f64_key("standings", "rows", (rows_ahead + _rows_behind) as f64)
        .max(0.0) as usize;

    let leader_f2 = ordered
        .iter()
        .find(|c| c.position == 1)
        .map(|c| c.f2_time)
        .or_else(|| ordered.first().map(|c| c.f2_time))
        .unwrap_or(0.0);

    let build_at = |c: &CarRow, ord: usize| -> TableRow {
        let display_pos = if timing_order || c.position <= 0 {
            (ord + 1) as i32
        } else {
            c.position
        };
        let gap_text = if display_pos <= 1 {
            "--".into()
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
        let inactive =
            c.inactive || (!c.on_track && !c.in_pit && !c.on_pit && c.lap_dist_pct < 0.0);
        let mut row = TableRow::from_car(c, None, gap_text, inactive, app);
        row.position = display_pos;
        row
    };

    let idxs: Vec<usize> = (0..ordered.len()).collect();
    let mut out: Vec<TableRow> = match (center, ordered.iter().position(|c| c.is_player)) {
        (true, Some(pidx)) if target > 0 => {
            let player = pidx;
            let above = rows_ahead.min(target.saturating_sub(1));
            let below = target.saturating_sub(1 + above);

            if pin_podium {
                let mut podium = Vec::new();
                for slot in 0..3.min(target) {
                    if slot < ordered.len() {
                        podium.push(build_at(ordered[slot], slot));
                    } else {
                        podium.push(empty_row(&format!("podium{slot}")));
                    }
                }
                let podium_set: std::collections::HashSet<usize> =
                    (0..ordered.len().min(3)).collect();
                let on_podium = podium_set.contains(&player);
                let mut slots = target.saturating_sub(podium.len());
                if on_podium {
                    slots = slots.max(below.max(1).min(target.saturating_sub(podium.len())));
                } else {
                    slots = slots.max(1);
                }
                let picked =
                    pick_context_indices(&idxs, pidx, player, &podium_set, slots, above, below);
                let context: Vec<TableRow> =
                    picked.iter().map(|&i| build_at(ordered[i], i)).collect();
                podium.extend(context);
                pad_standings_to(&mut podium, target);
                podium
            } else {
                let take = target.min(ordered.len().max(1));
                let mut start = pidx.saturating_sub(above);
                if start + take > ordered.len() {
                    start = ordered.len().saturating_sub(take);
                }
                let mut rows: Vec<TableRow> = ordered[start..start + take]
                    .iter()
                    .enumerate()
                    .map(|(k, c)| build_at(c, start + k))
                    .collect();
                pad_standings_to(&mut rows, target);
                rows
            }
        }
        _ if target == 0 => ordered
            .iter()
            .enumerate()
            .map(|(i, c)| build_at(c, i))
            .collect(),
        _ => {
            let mut rows: Vec<TableRow> = ordered
                .iter()
                .take(target.max(1))
                .enumerate()
                .map(|(i, c)| build_at(c, i))
                .collect();
            pad_standings_to(&mut rows, target.max(1));
            rows
        }
    };

    for row in &mut out {
        row.apply_driver_group(groups);
    }
    out
}

/// When `by_best_lap`, sort by best lap (fastest first) then car number.
fn standings_field_order<'a>(cars: &[&'a CarRow], by_best_lap: bool) -> Vec<&'a CarRow> {
    let mut field: Vec<&CarRow> = cars.to_vec();
    if by_best_lap {
        field.sort_by(|a, b| {
            match (
                parse_lap_clock(&a.best_lap).filter(|t| *t > 0.0),
                parse_lap_clock(&b.best_lap).filter(|t| *t > 0.0),
            ) {
                (Some(ta), Some(tb)) => ta
                    .partial_cmp(&tb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| {
                        car_number_sort_key(&a.car_number).cmp(&car_number_sort_key(&b.car_number))
                    }),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => car_number_sort_key(&a.car_number)
                    .cmp(&car_number_sort_key(&b.car_number))
                    .then_with(|| a.car_idx.cmp(&b.car_idx)),
            }
        });
        return field;
    }

    let mut ranked: Vec<&CarRow> = field.iter().copied().filter(|c| c.position > 0).collect();
    ranked.sort_by_key(|c| c.position);

    let mut unranked: Vec<&CarRow> = field.iter().copied().filter(|c| c.position <= 0).collect();
    unranked.sort_by(|a, b| {
        car_number_sort_key(&a.car_number)
            .cmp(&car_number_sort_key(&b.car_number))
            .then_with(|| a.car_idx.cmp(&b.car_idx))
    });
    ranked.extend(unranked);
    ranked
}

fn car_number_sort_key(num: &str) -> (i32, String) {
    let digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
    let n = digits.parse::<i32>().unwrap_or(i32::MAX);
    (n, num.to_ascii_lowercase())
}

fn pad_standings_to(rows: &mut Vec<TableRow>, target: usize) {
    if target == 0 {
        return;
    }
    let mut k = 0usize;
    while rows.len() < target {
        rows.push(empty_row(&format!("std_pad{k}")));
        k += 1;
    }
    if rows.len() > target {
        rows.truncate(target);
    }
}

/// Align dash `frame.position` (+ car/relative rows) with practice/qual best-lap
/// order used by standings. Raw IRSDK PlayerCarPosition often lags or counts
/// garage cars differently, which made dash show e.g. P13 while tables showed 10.
fn sync_practice_overlay_position(frame: &mut TelemetryFrame) {
    // Positive check only — missing session_type must not rewrite race order.
    let st = frame
        .session_type
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase();
    let practice_like = st.contains("practice") || st.contains("qual") || st == "open";
    if !practice_like {
        return;
    }
    let order_idxs: Vec<i32> = {
        let refs: Vec<&CarRow> = frame.cars.iter().filter(|c| !c.is_pace_car).collect();
        if refs.is_empty() {
            return;
        }
        standings_field_order(&refs, true)
            .iter()
            .map(|c| c.car_idx)
            .collect()
    };
    let mut rank = std::collections::HashMap::with_capacity(order_idxs.len());
    for (i, idx) in order_idxs.into_iter().enumerate() {
        rank.insert(idx, (i + 1) as i32);
    }
    for c in &mut frame.cars {
        if let Some(&p) = rank.get(&c.car_idx) {
            c.position = p;
        }
    }
    for row in frame
        .relative_cars
        .iter_mut()
        .chain(frame.standings_cars.iter_mut())
    {
        if row.empty {
            continue;
        }
        if let Ok(idx) = row.key.parse::<i32>() {
            if let Some(&p) = rank.get(&idx) {
                row.position = p;
            }
        }
    }
    if let Some(pos) = frame
        .cars
        .iter()
        .find(|c| c.is_player)
        .map(|c| c.position)
        .filter(|&p| p > 0)
        .or_else(|| {
            frame
                .standings_cars
                .iter()
                .chain(frame.relative_cars.iter())
                .find(|r| r.is_player && !r.empty && r.position > 0)
                .map(|r| r.position)
        })
    {
        frame.position = pos;
    }
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
        // Focus is already in P1–P3 — fill remaining slots with the next cars
        // after the podium (P4+). Searching ±offset from the focus only hits
        // other podium rows first and used to abort with an empty context.
        let mut chosen = Vec::new();
        for slot in 0..total {
            if chosen.len() >= limit {
                break;
            }
            let idx = ranked[slot];
            if podium_idxs.contains(&idx) {
                continue;
            }
            chosen.push(idx);
        }
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
    chosen.sort_by_key(|i| ranked.iter().position(|x| x == i).unwrap_or(*i));
    chosen
}

/// Fill relative_cars / standings_cars and enrich radar from cars + cfg.
pub fn finalize_frame(
    frame: &mut TelemetryFrame,
    cfg: &OverlayConfig,
    rel_sticky: &mut RelativeOrderHysteresis,
) {
    let app = crate::cloud::load_app_settings_cache().unwrap_or_else(|| serde_json::json!({}));
    let groups = cfg
        .cfg
        .get("driver_groups")
        .cloned()
        .unwrap_or_else(|| serde_json::json!([]));

    if let Some(radio) = frame.radio.as_mut() {
        radio.is_pro = crate::cloud::is_pro_driver(&radio.name, &app);
        if !radio.is_pro {
            if let Some(g) = crate::driver_groups::driver_group_for_name(&radio.name, &groups) {
                radio.group_icon = g.icon;
                radio.group_color = g.color;
            }
        }
    }
    apply_irating_projection(frame, cfg, rel_sticky);
    resolve_delta_mode(frame, cfg);
    apply_flag_config(frame, cfg);

    // Map dots: tint relative to presentation focus (live you, else camera).
    let map_focus = super::presentation_focus_car_idx(&frame.cars, frame.camera_car_idx);
    apply_lap_tints(
        &mut frame.cars,
        frame.lap_est_time,
        frame.session_type.as_deref(),
        map_focus,
    );

    // Rebuild fuel first so relative undercut/cover tags can use the pit window.
    let inp = crate::telemetry::FuelInputs {
        level: frame.fuel_l,
        fuel_pct: frame.fuel_pct,
        fuel_max: frame.fuel_max_l,
        lap: frame.lap,
        last_lap_s: frame.last_lap_s,
        lap_est: frame.lap_est_time,
        laps_remain: frame.session_laps_remain,
        time_remain: frame.session_time_remain,
        fuel_use_per_hour: frame.fuel_use_per_hour,
        laps_total: frame.laps_total,
        fc_use: frame.fuel_use_history.clone(),
    };
    frame.fuel = crate::telemetry::build_fuel_snapshot(&inp, cfg);

    // Timing tables: live seated racer keeps focus; pure spectators center on
    // camera -> seated -> leader. Keep `frame.cars.is_player` unchanged so
    // map/fuel/radar/dashboard still use the user's actual car.
    let seated_car_idx = frame.cars.iter().find(|c| c.is_player).map(|c| c.car_idx);
    let mut focused_cars = frame.cars.clone();
    apply_table_focus(&mut focused_cars, frame.camera_car_idx);
    let table_focus = focused_cars.iter().find(|c| c.is_player).map(|c| c.car_idx);
    apply_lap_tints(
        &mut focused_cars,
        frame.lap_est_time,
        frame.session_type.as_deref(),
        table_focus,
    );

    let mut rel = build_relative(
        &focused_cars,
        cfg,
        frame.lap_est_time,
        Some(rel_sticky),
        frame.session_state,
        frame.session_type.as_deref(),
        &app,
        &groups,
    );
    if super::strategy_hints::strategy_window_active(&frame.fuel, frame.fuel_pct, cfg) {
        super::strategy_hints::apply_strategy_hints(&mut rel, cfg);
    }
    let mut std = build_standings(
        &focused_cars,
        cfg,
        &app,
        &groups,
        frame.session_type.as_deref(),
        frame.camera_car_idx,
        seated_car_idx,
    );
    let fl_idx = session_best_car_idx(&focused_cars);
    mark_session_best(&mut rel, fl_idx);
    mark_session_best(&mut std, fl_idx);
    frame.relative_slots = build_table_slots(frame, cfg, "relative", &rel);
    frame.standings_slots = build_table_slots(frame, cfg, "standings", &std);
    frame.relative_cars = rel;
    frame.standings_cars = std;
    // Practice/qual: dash POS must match table best-lap ranks (not raw
    // PlayerCarPosition, which often disagrees by several spots).
    sync_practice_overlay_position(frame);

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

    // Pit engineer advice + mirror fuel add / window into pit_* fields.
    let advice = super::pit_advice::compute_pit_advice(frame, cfg);
    if frame.pit_fuel_to_add.is_none() {
        frame.pit_fuel_to_add = frame.fuel.add;
    }
    if frame.pit_fuel_add_l.is_none() {
        frame.pit_fuel_add_l = frame.fuel.add;
    }
    if frame.pit_laps_to_go.is_none() {
        if let Some((a, _)) = frame.fuel.window {
            let remain = (a - frame.lap).max(0);
            frame.pit_laps_to_go = Some(remain);
        }
    }
    frame.pit_advice = Some(advice);
}

/// Pick `frame.delta` from raw SDK/demo values using `delta_bar.mode`.
fn resolve_delta_mode(frame: &mut TelemetryFrame, cfg: &OverlayConfig) {
    let mode = cfg.str_key("delta_bar", "mode", "session_best");
    frame.delta = match mode.as_str() {
        "best_lap" => frame.delta_best_lap.or(frame.delta_session_best),
        "optimal" => frame
            .delta_optimal
            .or(frame.delta_best_lap)
            .or(frame.delta_session_best),
        "last_lap" => match (frame.cur_lap_s, frame.last_lap_s) {
            (Some(cur), Some(last)) if cur > 0.0 && last > 0.0 => Some(cur - last),
            _ => None,
        },
        "leader_last" => {
            let cur = frame.cur_lap_s.filter(|v| *v > 0.0);
            let leader_last = frame
                .cars
                .iter()
                .find(|c| c.position == 1)
                .and_then(|c| parse_lap_clock(&c.last_lap));
            match (cur, leader_last) {
                (Some(c), Some(l)) => Some(c - l),
                _ => None,
            }
        }
        // session_best (default)
        _ => frame
            .delta_session_best
            .or(frame.delta_best_lap)
            .or(frame.delta_optimal)
            .or(frame.delta),
    };
}

fn parse_lap_clock(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() || s == "--" || s == "—" {
        return None;
    }
    // "1:23.456" or "83.456"
    if let Some((m, rest)) = s.split_once(':') {
        let mins: f64 = m.parse().ok()?;
        let secs: f64 = rest.parse().ok()?;
        return Some(mins * 60.0 + secs);
    }
    s.parse().ok()
}

/// CarIdx with the fastest valid best-lap in the field (pace car excluded).
/// Ties keep the lowest car_idx so only one session-best badge is marked.
fn session_best_car_idx(cars: &[CarRow]) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    for c in cars {
        if c.is_pace_car || c.inactive {
            continue;
        }
        let Some(t) = parse_lap_clock(&c.best_lap).filter(|t| *t > 5.0) else {
            continue;
        };
        match best {
            Some((bt, bi)) if (t - bt).abs() <= 1e-4 => {
                if c.car_idx < bi {
                    best = Some((t, c.car_idx));
                }
            }
            Some((bt, _)) if t >= bt - 1e-4 => {}
            _ => best = Some((t, c.car_idx)),
        }
    }
    best.map(|(_, idx)| idx)
}

fn mark_session_best(rows: &mut [TableRow], fl_idx: Option<i32>) {
    for row in rows.iter_mut() {
        row.session_best = false;
    }
    let Some(idx) = fl_idx else {
        return;
    };
    let key = idx.to_string();
    if let Some(row) = rows.iter_mut().find(|r| !r.empty && r.key == key) {
        row.session_best = true;
    }
}

/// Honor flags widget toggles after IRSDK/demo fill the frame.
fn apply_flag_config(frame: &mut TelemetryFrame, cfg: &OverlayConfig) {
    let thresh = cfg
        .f64_key("flags", "incident_warn_pct", 0.75)
        .clamp(0.0, 1.0);
    if cfg.bool_key("flags", "show_incident_warning", true) && frame.incidents_limit > 0 {
        let pct = frame.incidents as f64 / frame.incidents_limit as f64;
        if pct >= thresh && frame.flag.is_none() {
            frame.incident_warn = true;
            if frame.secondary.is_none() {
                frame.secondary = Some(format!(
                    "Incidents {}/{}",
                    frame.incidents, frame.incidents_limit
                ));
            }
        } else if !frame.incident_warn {
            // keep IRSDK-set warn if any, else clear stale incident secondary
        }
    } else if !cfg.bool_key("flags", "show_incident_warning", true) {
        frame.incident_warn = false;
        if frame
            .secondary
            .as_deref()
            .map(|s| s.starts_with("Incidents "))
            .unwrap_or(false)
        {
            frame.secondary = None;
        }
    }

    if frame.flag.as_deref() == Some("blue") {
        if !cfg.bool_key("flags", "show_blue_detail", true) {
            frame.flag_context = Some("Faster car approaching -- let them pass".into());
        } else {
            // Prefer "#N +Xs" from nearest ahead relative row when available.
            let mut best: Option<(f32, String)> = None;
            for row in &frame.relative_cars {
                if row.empty || row.is_player {
                    continue;
                }
                let Some(g) = row.gap_secs.filter(|g| *g > 0.0) else {
                    continue;
                };
                if best.as_ref().map(|(d, _)| g < *d).unwrap_or(true) {
                    best = Some((g, row.car_number.clone()));
                }
            }
            if let Some((g, num)) = best {
                frame.flag_context = Some(format!("Car #{num} +{g:.1}s"));
            } else if frame.flag_context.is_none() {
                frame.flag_context = Some("Faster car approaching -- let them pass".into());
            }
        }
    }

    if frame.flag.as_deref() == Some("checkered")
        && !cfg.bool_key("flags", "show_finish_position", true)
    {
        frame.flag_context = Some("Session complete".into());
    } else if frame.flag.as_deref() == Some("checkered")
        && cfg.bool_key("flags", "show_finish_position", true)
        && frame.position > 0
    {
        let ctx = frame.flag_context.as_deref().unwrap_or("");
        if !ctx.contains("P") {
            frame.flag_context = Some(format!("Session complete -- P{}", frame.position));
        }
    }

    if cfg.bool_key("flags", "show_pit_limiter", true) && frame.pit_limiter {
        if frame.secondary.is_none() {
            frame.secondary = Some("Pit limiter active".into());
        }
    } else if !cfg.bool_key("flags", "show_pit_limiter", true)
        && frame
            .secondary
            .as_deref()
            .map(|s| s.contains("Pit limiter"))
            .unwrap_or(false)
    {
        frame.secondary = None;
    }
}

fn needs_irating_projection(cfg: &OverlayConfig) -> bool {
    if cfg.bool_key("dash", "show_irating_projection", false) && cfg.dash_uses_irating() {
        return true;
    }
    for section in ["relative", "standings"] {
        if cfg.bool_key(section, "show_irating_projection", false)
            && cfg.has_column(section, "irating")
        {
            return true;
        }
    }
    false
}

fn apply_irating_projection(
    frame: &mut TelemetryFrame,
    cfg: &OverlayConfig,
    sticky: &mut RelativeOrderHysteresis,
) {
    // Clear prior deltas unless we recompute / restore.
    for c in &mut frame.cars {
        c.irating_delta = None;
    }
    frame.irating_delta = None;

    if !needs_irating_projection(cfg) {
        sticky.irating_deltas.clear();
        sticky.irating_player_delta = None;
        return;
    }
    // Racing / checkered / cooldown (4–6). Keep the projection up after the race.
    if !(4..=6).contains(&frame.session_state) {
        sticky.irating_deltas.clear();
        sticky.irating_player_delta = None;
        return;
    }

    let deltas = crate::irating::project_deltas_by_class(&frame.cars);
    if !deltas.is_empty() {
        sticky.irating_deltas = deltas;
        sticky.irating_player_delta = frame
            .cars
            .iter()
            .find(|c| c.is_player)
            .and_then(|p| sticky.irating_deltas.get(&p.car_idx).copied());
    } else if sticky.irating_deltas.is_empty() {
        return;
    }

    for c in &mut frame.cars {
        if let Some(d) = sticky.irating_deltas.get(&c.car_idx) {
            c.irating_delta = Some(*d);
        }
    }
    if let Some(p) = frame.cars.iter().find(|c| c.is_player) {
        frame.irating_delta = p
            .irating_delta
            .or(sticky.irating_player_delta);
        if frame.irating <= 0 {
            frame.irating = p.irating;
        }
    } else {
        frame.irating_delta = sticky.irating_player_delta;
    }
}

fn slot_defaults(section: &str) -> [(&'static str, &'static str, &'static str); 2] {
    if section == "standings" {
        [
            ("order_pill", "title", "count"),
            ("track_temp", "session_time", "air_temp"),
        ]
    } else {
        [
            ("sof", "none", "position"),
            ("race_time", "lap", "incidents"),
        ]
    }
}

/// Build header/footer slot keys + values from Settings maps.
pub fn build_table_slots(
    frame: &TelemetryFrame,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
) -> TableSlots {
    let defs = slot_defaults(section);
    let (hl, hc, hr) = defs[0];
    let (fl, fc, fr) = defs[1];
    let item = |group: &str, pos: &str, default: &str| -> TableSlotItem {
        let key = cfg.nested_str(section, group, pos, default);
        if key == "none" || key.is_empty() {
            return TableSlotItem::default();
        }
        TableSlotItem {
            value: format_slot_value(&key, frame, cfg, section, rows),
            key,
        }
    };
    TableSlots {
        header_left: item("header", "left", hl),
        header_center: item("header", "center", hc),
        header_right: item("header", "right", hr),
        footer_left: item("footer", "left", fl),
        footer_center: item("footer", "center", fc),
        footer_right: item("footer", "right", fr),
    }
}

pub fn slot_label(key: &str) -> &'static str {
    match key {
        "sof" => "SOF",
        "class_sof" => "CSOF",
        "position" => "POS",
        "class_position" => "CPOS",
        "session_time" => "TIME",
        "race_time" => "RACE",
        "lap" => "LAP",
        "incidents" => "INC",
        "track_temp" => "TRK",
        "air_temp" => "AIR",
        "best_lap" => "BEST",
        "session_best" | "my_session_best" => "SBEST",
        "local_time" => "CLK",
        "sim_time" => "SIM",
        "cpu" => "CPU",
        "mem" => "MEM",
        "gpu" => "GPU",
        "laps_remain" => "LEFT",
        "incident_limit" => "INC",
        "fast_repairs" => "FR",
        "weather" => "WX",
        "track_wetness" => "WET",
        "session_type" => "SESS",
        "race_split" => "SPLIT",
        _ => "",
    }
}

fn fmt_clock(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "--:--".into();
    }
    let secs = secs as i64;
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if h > 0 {
        format!("{h:02}:{m:02}:{s:02}")
    } else {
        format!("{m:02}:{s:02}")
    }
}

fn format_slot_value(
    key: &str,
    frame: &TelemetryFrame,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
) -> String {
    // Prefer the table focus car (camera while spectating) over the seated player.
    let player = slot_focus_car(frame, rows);
    let total = frame
        .cars
        .iter()
        .filter(|c| c.position > 0 && !c.is_pace_car)
        .count();
    match key {
        "sof" => {
            let irs: Vec<i32> = frame
                .cars
                .iter()
                .filter(|c| !c.is_pace_car && c.irating > 0)
                .map(|c| c.irating)
                .collect();
            if irs.is_empty() {
                "--".into()
            } else {
                fmt_irating(irs.iter().sum::<i32>() / irs.len() as i32)
            }
        }
        "class_sof" => {
            let cid = player.map(|p| p.class_id).unwrap_or(0);
            let irs: Vec<i32> = frame
                .cars
                .iter()
                .filter(|c| !c.is_pace_car && c.irating > 0 && (cid == 0 || c.class_id == cid))
                .map(|c| c.irating)
                .collect();
            if irs.is_empty() {
                "--".into()
            } else {
                fmt_irating(irs.iter().sum::<i32>() / irs.len() as i32)
            }
        }
        "position" => {
            if let Some(p) = player {
                if p.position > 0 && total > 0 {
                    format!("{}/{}", p.position, total)
                } else {
                    "--".into()
                }
            } else {
                "--".into()
            }
        }
        "class_position" => {
            if let Some(p) = player {
                let class_total = frame
                    .cars
                    .iter()
                    .filter(|c| !c.is_pace_car && c.class_id == p.class_id && c.class_position > 0)
                    .count();
                if p.class_position > 0 && class_total > 0 {
                    format!("{}/{}", p.class_position, class_total)
                } else {
                    "--".into()
                }
            } else {
                "--".into()
            }
        }
        "session_time" => {
            if let Some(rem) = frame.session_time_remain {
                if rem >= 0.0 {
                    return fmt_clock(rem as f64);
                }
            }
            "--".into()
        }
        "race_time" => {
            let el = if frame.session_time >= 0.0 {
                Some(frame.session_time)
            } else {
                None
            };
            match el {
                Some(el) => fmt_clock(el),
                None => "--".into(),
            }
        }
        "lap" => {
            let lap = player
                .map(|p| p.lap)
                .filter(|l| *l > 0)
                .unwrap_or(frame.lap);
            if let Some(total) = crate::telemetry::finite_laps_total(frame.laps_total) {
                format!("{}/{}", lap, total)
            } else if lap > 0 {
                format!("{lap}")
            } else {
                "--".into()
            }
        }
        "incidents" => format!("{}x", frame.incidents),
        "track_name" => frame.track_name.clone().unwrap_or_else(|| "--".into()),
        "track_temp" => {
            if let Some(t) = frame.track_temp {
                format!("{:.0}{}", cfg.conv_temp(t), cfg.temp_unit())
            } else {
                "--".into()
            }
        }
        "air_temp" => {
            if let Some(t) = frame.air_temp {
                format!("{:.0}{}", cfg.conv_temp(t), cfg.temp_unit())
            } else {
                "--".into()
            }
        }
        "best_lap" => frame
            .best_lap_s
            .map(|s| format::fmt_laptime(s, "--"))
            .unwrap_or_else(|| "--".into()),
        "my_session_best" => player
            .map(|p| {
                if p.best_lap.is_empty() {
                    "--".into()
                } else {
                    p.best_lap.clone()
                }
            })
            .unwrap_or_else(|| "--".into()),
        "session_best" => {
            let best = frame
                .cars
                .iter()
                .filter(|c| !c.best_lap.is_empty() && c.best_lap != "--")
                .map(|c| c.best_lap.as_str())
                .min();
            best.unwrap_or("--").to_string()
        }
        "local_time" => {
            use chrono::{Local, Timelike};
            let now = Local::now();
            let h24 = now.hour();
            let h12 = {
                let h = h24 % 12;
                if h == 0 {
                    12
                } else {
                    h
                }
            };
            let ampm = if h24 < 12 { "AM" } else { "PM" };
            format!("{h12}:{:02} {ampm}", now.minute())
        }
        "sim_time" => frame
            .session_time_of_day
            .map(format::fmt_tod)
            .unwrap_or_else(|| "--".into()),
        "incident_limit" => {
            if frame.incidents_limit > 0 {
                format!("{}/{}x", frame.incidents, frame.incidents_limit)
            } else {
                format!("{}x", frame.incidents)
            }
        }
        "fast_repairs" => match (frame.pit_repairs_used, frame.pit_repairs) {
            (Some(used), Some(avail)) => {
                let total = used + avail;
                if total > 0 {
                    format!("{used}/{total}")
                } else {
                    "--".into()
                }
            }
            (_, Some(avail)) if avail > 0 => format!("{avail}"),
            (Some(used), _) if used > 0 => format!("{used}"),
            _ => "--".into(),
        },
        "session_type" => frame
            .session_type
            .clone()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "--".into()),
        "race_split" => frame
            .race_split
            .filter(|n| *n > 0)
            .map(
                |n| match frame.race_split_total.filter(|total| *total > 0) {
                    Some(total) => format!("{n}/{total}"),
                    None => n.to_string(),
                },
            )
            .unwrap_or_else(|| "--".into()),
        "cpu" => frame.cpu.clone().unwrap_or_else(|| "--".into()),
        "mem" => frame.mem.clone().unwrap_or_else(|| "--".into()),
        "gpu" => frame.gpu.clone().unwrap_or_else(|| "--".into()),
        "laps_remain" => frame
            .session_laps_remain
            .filter(|v| v.is_finite() && *v >= 0.0 && *v < 32_000.0)
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "--".into()),
        "weather" => {
            let mut parts = Vec::new();
            if let Some(s) = &frame.skies {
                parts.push(s.clone());
            }
            if let Some(h) = frame.humidity {
                parts.push(format!("{h:.0}%"));
            }
            if parts.is_empty() {
                "--".into()
            } else {
                parts.join(" ")
            }
        }
        "track_wetness" => frame
            .track_wetness
            .map(|w| format!("{w:.0}%"))
            .unwrap_or_else(|| "--".into()),
        "title" => cfg.str_key(section, "title", "Standings"),
        "order_pill" => "ORDER".into(),
        "count" => {
            let shown = rows.iter().filter(|r| !r.empty).count();
            format!("{shown}/{total}")
        }
        _ => "--".into(),
    }
}

/// Car that header/footer POS / class / best-lap slots should describe.
fn slot_focus_car<'a>(frame: &'a TelemetryFrame, rows: &[TableRow]) -> Option<&'a CarRow> {
    if let Some(row) = rows.iter().find(|r| r.is_player && !r.empty) {
        if let Ok(idx) = row.key.parse::<i32>() {
            if let Some(c) = frame.cars.iter().find(|c| c.car_idx == idx) {
                return Some(c);
            }
        }
    }
    if let Some(cam) = frame.camera_car_idx {
        if let Some(c) = frame.cars.iter().find(|c| c.car_idx == cam) {
            return Some(c);
        }
    }
    frame.cars.iter().find(|c| c.is_player)
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
    alongside.sort_by(|a, b| {
        a.0.abs()
            .partial_cmp(&b.0.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

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
    let closeness = |delta: Option<f32>| delta.map(|d| (1.0 - d.abs() / range).clamp(0.0, 1.0));

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
                // Distinct track positions: higher pct = ahead of player (i=3 @ 0.50).
                lap_dist_pct: 0.35 + i as f32 * 0.05,
                is_player: i == 3,
                on_track: true,
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let rows = build_relative(
            &cars,
            &cfg,
            90.0,
            None,
            4,
            Some("Race"),
            &serde_json::json!({}),
            &serde_json::json!([]),
        );
        let player_i = rows.iter().position(|r| r.is_player).unwrap();
        // With 3 ahead / 3 behind and centering, player is in the middle slot.
        assert_eq!(player_i, 3);
        assert_eq!(rows.len(), 7);
        let live: Vec<_> = rows
            .iter()
            .filter(|r| !r.empty)
            .map(|r| r.key.as_str())
            .collect();
        assert_eq!(live, vec!["6", "5", "4", "3", "2", "1", "0"]);
    }

    #[test]
    fn standings_near_front_keeps_cars_behind_focus() {
        // Spectating P5 with 4 ahead / 5 behind must still include P6+.
        let mut cars = Vec::new();
        for i in 0..12 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 4, // P5
                on_track: true,
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert!(live.iter().any(|r| r.is_player));
        let max_pos = live.iter().map(|r| r.position).max().unwrap_or(0);
        assert!(
            max_pos > 5,
            "expected cars behind P5 in the window, got max P{max_pos} from {} rows",
            live.len()
        );
    }

    #[test]
    fn standings_pin_podium_fills_below_when_focus_on_leader() {
        let mut cars = Vec::new();
        for i in 0..12 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 0, // P1 — on the pinned podium
                on_track: true,
                ..Default::default()
            });
        }
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["pin_podium"] = serde_json::json!(true);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert!(
            live.len() > 3,
            "expected P4+ under podium, got {} rows",
            live.len()
        );
        assert_eq!(live[0].position, 1);
        assert_eq!(live[1].position, 2);
        assert_eq!(live[2].position, 3);
        assert!(
            live.iter().any(|r| r.position == 4),
            "P4 must appear below the podium separator"
        );
    }

    #[test]
    fn standings_keeps_lap_tints() {
        let mut cars = Vec::new();
        for i in 0..8 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 4,
                on_track: true,
                lapping: i == 2 || i == 3,
                lap_ahead: i == 3,
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        let d2 = rows.iter().find(|r| r.name == "D2").unwrap();
        let d3 = rows.iter().find(|r| r.name == "D3").unwrap();
        assert!(d2.lapping && !d2.lap_ahead, "lapped traffic tint kept");
        assert!(d3.lapping && d3.lap_ahead, "lapper tint kept");
    }

    #[test]
    fn standings_follows_focus_car() {
        let mut cars = Vec::new();
        for i in 0..12 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 7, // midfield focus (camera or seated)
                on_track: true,
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert!(
            live.iter().any(|r| r.is_player && r.position == 8),
            "standings must window around the focus car, got {:?}",
            live.iter().map(|r| r.position).collect::<Vec<_>>()
        );
        assert_ne!(
            live.first().map(|r| r.position),
            Some(1),
            "midfield focus should not stay locked at P1"
        );
    }

    #[test]
    fn standings_qualifying_lists_from_p1_not_focus() {
        let mut cars = Vec::new();
        for i in 0..8 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 5, // P6 — would center in race mode
                on_track: true,
                ..Default::default()
            });
        }
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(true);
        cfg.cfg["standings"]["pin_podium"] = serde_json::json!(true);
        cfg.cfg["standings"]["rows"] = serde_json::json!(5.0);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Qualifying"),
            None,
            None,
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert_eq!(live.first().map(|r| r.position), Some(1));
        assert!(
            live.iter().all(|r| r.position <= 5),
            "qualifying should take from the top, got {:?}",
            live.iter().map(|r| r.position).collect::<Vec<_>>()
        );
        assert!(
            !live.iter().any(|r| r.is_player),
            "qualifying must not center the window on the racer"
        );
        assert_eq!(rows.len(), 5, "must fill configured row count");
    }

    #[test]
    fn standings_pads_to_configured_row_count() {
        let cars = vec![
            CarRow {
                car_idx: 0,
                position: 1,
                name: "A".into(),
                is_player: true,
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                position: 2,
                name: "B".into(),
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 3,
                name: "C".into(),
                on_track: true,
                ..Default::default()
            },
        ];
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(true);
        cfg.cfg["standings"]["rows"] = serde_json::json!(12.0);
        cfg.cfg["standings"]["rows_ahead"] = serde_json::json!(4.0);
        cfg.cfg["standings"]["rows_behind"] = serde_json::json!(8.0);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        assert_eq!(
            rows.len(),
            12,
            "Total rows must be filled exactly, got {}",
            rows.len()
        );
        assert_eq!(rows.iter().filter(|r| !r.empty).count(), 3);
        assert!(rows.iter().any(|r| r.empty));
    }

    #[test]
    fn standings_fills_with_unranked_cars_to_total_rows() {
        // 9 cars with positions + 7 without — Total rows 12 should list 3
        // unranked cars after the last ranked row (not leave blank panel space).
        let mut cars = Vec::new();
        for i in 0..9 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("R{i}"),
                car_number: format!("{}", i + 1),
                is_player: i == 8,
                on_track: true,
                ..Default::default()
            });
        }
        for (k, num) in [
            (9, "20"),
            (10, "6"),
            (11, "14"),
            (12, "1"),
            (13, "9"),
            (14, "18"),
            (15, "22"),
        ] {
            cars.push(CarRow {
                car_idx: k,
                position: 0,
                name: format!("U{k}"),
                car_number: num.into(),
                on_track: false,
                inactive: true,
                ..Default::default()
            });
        }
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(false);
        cfg.cfg["standings"]["rows"] = serde_json::json!(12.0);
        // Race session: keep live positions, append unranked by car number.
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        assert_eq!(rows.len(), 12);
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert_eq!(live.len(), 12);
        assert_eq!(live[8].name, "R8");
        // Unranked follow by car number: #1, #6, #9, ...
        assert_eq!(live[9].car_number, "1");
        assert_eq!(live[10].car_number, "6");
        assert_eq!(live[11].car_number, "9");
    }

    #[test]
    fn standings_qual_orders_by_best_lap_no_position_holes() {
        // Live positions jump 1..7 then 16 — standings must re-rank by best lap
        // so the list is sequential and fastest is P1.
        let cars = vec![
            CarRow {
                car_idx: 0,
                position: 1,
                name: "Slow".into(),
                car_number: "8".into(),
                best_lap: "0:40.298".into(),
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                position: 2,
                name: "Fast".into(),
                car_number: "7".into(),
                best_lap: "0:39.956".into(),
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 7,
                name: "Mid".into(),
                car_number: "12".into(),
                best_lap: "0:40.327".into(),
                is_player: true,
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 3,
                position: 16,
                name: "NoTime".into(),
                car_number: "6".into(),
                best_lap: String::new(),
                inactive: true,
                ..Default::default()
            },
        ];
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["rows"] = serde_json::json!(12.0);
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(true);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Qualifying"),
            None,
            None,
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert_eq!(live[0].name, "Fast");
        assert_eq!(live[0].position, 1);
        assert_eq!(live[1].name, "Slow");
        assert_eq!(live[2].name, "Mid");
        assert_eq!(live[3].name, "NoTime");
        assert_eq!(live[3].position, 4, "no-time car must not keep live P16");
        assert_eq!(rows.len(), 12);
    }

    #[test]
    fn standings_hides_spectator_seated_car() {
        let cars = vec![
            CarRow {
                car_idx: 1,
                position: 1,
                name: "Leader".into(),
                best_lap: "0:40.000".into(),
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 5,
                name: "CameraTarget".into(),
                best_lap: "0:40.200".into(),
                on_track: true,
                is_player: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 99,
                position: 17,
                name: "SpectatorGhost".into(),
                car_number: "12".into(),
                inactive: true,
                lap_dist_pct: -1.0,
                ..Default::default()
            },
        ];
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["rows"] = serde_json::json!(12.0);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Qualifying"),
            Some(2),
            Some(99),
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert!(
            live.iter().all(|r| r.name != "SpectatorGhost"),
            "spectating must not list the seated player's ghost"
        );
        assert_eq!(live[0].name, "Leader");
        assert_eq!(live.len(), 2);
    }

    #[test]
    fn standings_keeps_live_seated_car_while_watching() {
        // Racing but camera on someone else — stay listed with your race position.
        let cars = vec![
            CarRow {
                car_idx: 1,
                position: 1,
                name: "Leader".into(),
                best_lap: "0:40.000".into(),
                on_track: true,
                lap_dist_pct: 0.1,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 3,
                name: "Watched".into(),
                best_lap: "0:40.200".into(),
                on_track: true,
                lap_dist_pct: 0.2,
                ..Default::default()
            },
            CarRow {
                car_idx: 5,
                position: 4,
                name: "Me".into(),
                best_lap: "0:40.300".into(),
                on_track: true,
                is_player: true,
                lap_dist_pct: 0.25,
                ..Default::default()
            },
        ];
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["rows"] = serde_json::json!(12.0);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            Some(2),
            Some(5),
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert!(
            live.iter().any(|r| r.name == "Me" && r.position == 4),
            "live racer must stay in standings while watching another car"
        );
        assert_eq!(live.len(), 3);
    }

    #[test]
    fn table_focus_stays_on_live_seated_player_while_watching() {
        let mut cars = vec![
            CarRow {
                car_idx: 1,
                position: 1,
                lap_dist_pct: 0.1,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 3,
                name: "Watched".into(),
                lap_dist_pct: 0.2,
                ..Default::default()
            },
            CarRow {
                car_idx: 5,
                position: 4,
                name: "Me".into(),
                is_player: true,
                lap_dist_pct: 0.25,
                ..Default::default()
            },
        ];
        apply_table_focus(&mut cars, Some(2));
        assert!(cars.iter().find(|c| c.car_idx == 5).unwrap().is_player);
        assert!(!cars.iter().find(|c| c.car_idx == 2).unwrap().is_player);
    }

    #[test]
    fn table_focus_follows_camera_for_spectator_ghost() {
        let mut cars = vec![
            CarRow {
                car_idx: 2,
                position: 3,
                name: "Watched".into(),
                lap_dist_pct: 0.2,
                ..Default::default()
            },
            CarRow {
                car_idx: 99,
                position: 0,
                name: "Ghost".into(),
                is_player: true,
                inactive: true,
                lap_dist_pct: -1.0,
                ..Default::default()
            },
        ];
        apply_table_focus(&mut cars, Some(2));
        assert!(cars.iter().find(|c| c.car_idx == 2).unwrap().is_player);
        assert!(!cars.iter().find(|c| c.car_idx == 99).unwrap().is_player);
    }

    #[test]
    fn standings_race_keeps_live_order_even_with_position_gaps() {
        // Retired/garage cars can leave holes in CarIdxPosition. Race standings
        // must still follow live race order (match Results), not best lap.
        let cars = vec![
            CarRow {
                car_idx: 0,
                position: 1,
                name: "RaceLeader".into(),
                best_lap: "0:40.336".into(),
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                position: 2,
                name: "P2".into(),
                best_lap: "0:39.956".into(), // faster best lap than leader
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 5,
                name: "Focus".into(),
                best_lap: "0:40.110".into(),
                is_player: true,
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 3,
                position: 14,
                name: "Back".into(),
                best_lap: "0:40.378".into(),
                on_track: true,
                ..Default::default()
            },
        ];
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["rows"] = serde_json::json!(12.0);
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(false);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert_eq!(live[0].name, "RaceLeader");
        assert_eq!(live[0].position, 1);
        assert_eq!(live[1].name, "P2");
        assert_eq!(live[1].position, 2);
        assert_eq!(live[2].position, 5);
        assert_eq!(live[3].position, 14);
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
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        assert!(rows.iter().any(|r| r.is_player));
        assert!(!rows.is_empty());
    }

    #[test]
    fn pre_race_relative_hides_garage_keeps_on_track() {
        let mut cars = Vec::new();
        for i in 0..6 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 2,
                // Nobody has EstTime yet; half are still in garage.
                inactive: i >= 3,
                on_track: i < 3,
                lap_dist_pct: if i < 3 { 0.10 + i as f32 * 0.05 } else { -1.0 },
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let rows = build_relative(
            &cars,
            &cfg,
            90.0,
            None,
            2,
            Some("Race"),
            &serde_json::json!({}),
            &serde_json::json!([]),
        ); // Warmup
        let keys: Vec<_> = rows
            .iter()
            .filter(|r| !r.empty)
            .map(|r| r.key.as_str())
            .collect();
        assert!(keys.contains(&"2"), "player present");
        assert!(
            keys.contains(&"0") || keys.contains(&"1"),
            "cars near player on track"
        );
        assert!(
            !keys.iter().any(|k| *k == "3" || *k == "4" || *k == "5"),
            "garage cars must be hidden: {keys:?}"
        );
    }

    #[test]
    fn relative_excludes_pit_stall_and_garage_keeps_circuit() {
        let cars = vec![
            CarRow {
                car_idx: 0,
                position: 1,
                name: "AheadPitStall".into(),
                car_number: "1".into(),
                est_time: 12.0,
                lap_dist_pct: 0.62,
                on_track: false,
                on_pit: true,
                in_pit: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                position: 2,
                name: "OnTrack".into(),
                car_number: "2".into(),
                est_time: 14.0,
                lap_dist_pct: 0.50,
                on_track: true,
                is_player: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 3,
                name: "BehindOffTrack".into(),
                car_number: "3".into(),
                est_time: 16.0,
                lap_dist_pct: 0.40,
                on_track: false,
                ..Default::default()
            },
            CarRow {
                car_idx: 3,
                position: 4,
                name: "Garage".into(),
                car_number: "4".into(),
                est_time: 18.0,
                inactive: true,
                on_track: false,
                lap_dist_pct: -1.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 4,
                position: 5,
                name: "EnteringPits".into(),
                car_number: "5".into(),
                est_time: 11.0,
                lap_dist_pct: 0.70,
                on_track: false,
                in_pit: true,
                approaching_pits: true,
                ..Default::default()
            },
        ];
        let cfg = OverlayConfig::default();
        let rows = build_relative(
            &cars,
            &cfg,
            90.0,
            None,
            4,
            Some("Race"),
            &serde_json::json!({}),
            &serde_json::json!([]),
        );
        let keys: Vec<_> = rows
            .iter()
            .filter(|r| !r.empty)
            .map(|r| r.key.as_str())
            .collect();
        assert!(keys.contains(&"1"), "player present");
        assert!(!keys.contains(&"0"), "pit stall excluded: {keys:?}");
        assert!(keys.contains(&"2"), "off-track neighbor kept: {keys:?}");
        assert!(!keys.contains(&"3"), "garage excluded: {keys:?}");
        assert!(keys.contains(&"4"), "pit-entry neighbor kept: {keys:?}");
    }

    #[test]
    fn practice_standings_centers_on_player() {
        let mut cars = Vec::new();
        for i in 0..20 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                best_lap: format!("1:{:06.3}", 40.0 + i as f64),
                is_player: i == 14, // P15 mid-pack
                on_track: true,
                ..Default::default()
            });
        }
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["rows"] = serde_json::json!(9.0);
        cfg.cfg["standings"]["rows_ahead"] = serde_json::json!(4.0);
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(true);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Practice"),
            None,
            None,
        );
        assert!(
            rows.iter().any(|r| r.is_player && !r.empty),
            "practice standings must keep the player in the window"
        );
    }

    #[test]
    fn practice_relative_hides_garage_cars() {
        let mut cars = Vec::new();
        for i in 0..6 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                est_time: 10.0 + i as f32 * 2.0,
                lap_dist_pct: if i < 3 { 0.10 + i as f32 * 0.05 } else { -1.0 },
                is_player: i == 2,
                inactive: i >= 3,
                on_track: i < 3,
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let rows = build_relative(
            &cars,
            &cfg,
            90.0,
            None,
            4,
            Some("Practice"),
            &serde_json::json!({}),
            &serde_json::json!([]),
        );
        let keys: Vec<_> = rows
            .iter()
            .filter(|r| !r.empty)
            .map(|r| r.key.as_str())
            .collect();
        assert!(keys.contains(&"2"), "player present");
        assert!(
            !keys.iter().any(|k| *k == "3" || *k == "4" || *k == "5"),
            "garage cars must be hidden in practice: {keys:?}"
        );
    }

    #[test]
    fn standings_includes_inactive_grid_cars() {
        let cars = vec![
            CarRow {
                car_idx: 0,
                position: 1,
                name: "Leader".into(),
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                position: 2,
                name: "Garage".into(),
                is_player: true,
                inactive: true,
                lap_dist_pct: -1.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                position: 3,
                name: "AlsoGarage".into(),
                inactive: true,
                lap_dist_pct: -1.0,
                ..Default::default()
            },
        ];
        let cfg = OverlayConfig::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert_eq!(live.len(), 3);
        assert!(live.iter().any(|r| r.inactive && r.name == "AlsoGarage"));
    }

    #[test]
    fn lap_tints_require_close_approach_for_one_lap() {
        let mut cars = vec![
            CarRow {
                car_idx: 0,
                is_player: true,
                lap: 10,
                est_time: 20.0,
                ..Default::default()
            },
            // Focus is lapping this car: red when it is just ahead.
            CarRow {
                car_idx: 1,
                lap: 9,
                est_time: 24.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                lap: 9,
                est_time: 30.0,
                ..Default::default()
            },
            // This car is lapping focus: blue when it is just behind.
            CarRow {
                car_idx: 3,
                lap: 11,
                est_time: 16.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 4,
                lap: 11,
                est_time: 10.0,
                ..Default::default()
            },
            // More than one completed lap is always tinted.
            CarRow {
                car_idx: 5,
                lap: 8,
                est_time: 60.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 6,
                lap: 12,
                est_time: 60.0,
                ..Default::default()
            },
        ];
        apply_lap_tints(&mut cars, 90.0, Some("Race"), None);

        assert!(cars[1].lapping && !cars[1].lap_ahead);
        assert!(!cars[2].lapping);
        assert!(cars[3].lapping && cars[3].lap_ahead);
        assert!(!cars[4].lapping);
        assert!(cars[5].lapping && !cars[5].lap_ahead);
        assert!(cars[6].lapping && cars[6].lap_ahead);
    }

    #[test]
    fn lap_tints_use_lap_dist_pct_like_relative() {
        // Relative neighbors are LapDistPct-based; one-lap tint must use the
        // same gap so cars in the Relative list actually get red/blue.
        let mut cars = vec![
            CarRow {
                car_idx: 0,
                is_player: true,
                lap: 10,
                lap_dist_pct: 0.50,
                on_track: true,
                est_time: 0.0, // EstTime missing — pct must still work
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                lap: 9,
                // ~3s ahead on a 90s lap
                lap_dist_pct: 0.50 + 3.0 / 90.0,
                on_track: true,
                est_time: 0.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                lap: 9,
                // Far ahead — no tint
                lap_dist_pct: 0.50 + 20.0 / 90.0,
                on_track: true,
                ..Default::default()
            },
        ];
        apply_lap_tints(&mut cars, 90.0, Some("Race"), None);
        assert!(cars[1].lapping && !cars[1].lap_ahead);
        assert!(!cars[2].lapping);
    }

    #[test]
    fn lap_tints_disabled_outside_race() {
        let mut cars = vec![
            CarRow {
                car_idx: 0,
                is_player: true,
                lap: 10,
                est_time: 20.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                lap: 8,
                est_time: 24.0,
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                lap: 12,
                est_time: 16.0,
                ..Default::default()
            },
        ];
        apply_lap_tints(&mut cars, 90.0, Some("Practice"), None);
        assert!(cars.iter().all(|c| !c.lapping && !c.lap_ahead));
        apply_lap_tints(&mut cars, 90.0, Some("Qualifying"), None);
        assert!(cars.iter().all(|c| !c.lapping && !c.lap_ahead));
    }

    #[test]
    fn format_slot_uses_camera_focus_when_no_seated_player() {
        let frame = TelemetryFrame {
            camera_car_idx: Some(2),
            cars: vec![
                CarRow {
                    car_idx: 1,
                    position: 1,
                    is_player: false,
                    ..Default::default()
                },
                CarRow {
                    car_idx: 2,
                    position: 5,
                    is_player: false,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        let cfg = OverlayConfig::default();
        let rows = vec![TableRow {
            key: "2".into(),
            is_player: true,
            position: 5,
            ..Default::default()
        }];
        assert_eq!(
            format_slot_value("position", &frame, &cfg, "relative", &rows),
            "5/2"
        );
    }

    #[test]
    fn format_slot_live_metrics() {
        let mut frame = TelemetryFrame {
            session_time_of_day: Some(14.0 * 3600.0 + 30.0 * 60.0),
            incidents: 3,
            incidents_limit: 17,
            pit_repairs_used: Some(1),
            pit_repairs: Some(2),
            session_type: Some("Race".into()),
            race_split: Some(2),
            race_split_total: Some(5),
            ..Default::default()
        };
        let cfg = OverlayConfig::default();
        let rows: Vec<TableRow> = Vec::new();
        assert_eq!(
            format_slot_value("sim_time", &frame, &cfg, "relative", &rows),
            "14:30"
        );
        assert_eq!(
            format_slot_value("incident_limit", &frame, &cfg, "relative", &rows),
            "3/17x"
        );
        assert_eq!(
            format_slot_value("fast_repairs", &frame, &cfg, "relative", &rows),
            "1/3"
        );
        assert_eq!(
            format_slot_value("session_type", &frame, &cfg, "relative", &rows),
            "Race"
        );
        assert_eq!(
            format_slot_value("race_split", &frame, &cfg, "relative", &rows),
            "2/5"
        );
        frame.incidents_limit = 0;
        assert_eq!(
            format_slot_value("incident_limit", &frame, &cfg, "relative", &rows),
            "3x"
        );
    }

    #[test]
    fn practice_dash_position_matches_best_lap_tables() {
        // SDK live position (13) vs best-lap ordinal among the painted field (10).
        let mut frame = TelemetryFrame {
            session_type: Some("Practice".into()),
            position: 13,
            cars: (0..12)
                .map(|i| CarRow {
                    car_idx: i,
                    // Sparse / wrong live ranks (player "P13").
                    position: if i == 9 { 13 } else { i + 1 },
                    best_lap: if i < 10 {
                        format!("1:{:06.3}", 40.0 + i as f64)
                    } else {
                        String::new()
                    },
                    car_number: format!("{}", i + 1),
                    name: format!("D{i}"),
                    is_player: i == 9,
                    on_track: true,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["rows"] = serde_json::json!(12.0);
        let mut sticky = RelativeOrderHysteresis::default();
        finalize_frame(&mut frame, &cfg, &mut sticky);
        assert_eq!(
            frame.position, 10,
            "dash POS should follow practice best-lap order, got {}",
            frame.position
        );
        let player_car = frame.cars.iter().find(|c| c.is_player).unwrap();
        assert_eq!(player_car.position, 10);
        let std_player = frame
            .standings_cars
            .iter()
            .find(|r| r.is_player && !r.empty)
            .expect("player should appear in standings window");
        assert_eq!(std_player.position, frame.position);
    }
}
