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
    /// Player-only projected SR change (hundredths); `None` on other rows.
    #[serde(default)]
    pub sr_delta: Option<i32>,
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
    /// ISO2 country code for the `country` column (club-region flag).
    #[serde(default)]
    pub country_code: Option<String>,
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
            sr_delta: None,
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
            country_code: c.country_code.clone(),
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
    /// Last projected SR delta (hundredths), held through cooldown.
    sr_player_delta: Option<i32>,
    /// Live Standings order (race progress, pit freeze; passes update immediately).
    pub standings: StandingsOrderHysteresis,
}

/// Committed Standings order so passes update before iRacing Position, without
/// Relative-style pit-lane reshuffles. On-track order follows live progress
/// immediately (no swap margin).
#[derive(Debug, Clone, Default)]
pub struct StandingsOrderHysteresis {
    /// car_idx order, index 0 = display P1.
    committed: Vec<i32>,
    /// Focus/player lap % for S/F soft re-sync.
    focus_prev_pct: Option<f32>,
}

/// Seconds of gap required to overturn the previous Relative order.
const REL_ORDER_MARGIN_S: f32 = 0.25;
/// Race-progress epsilon so equal floats do not thrash; passes flip immediately.
const STANDINGS_PROGRESS_EPS: f64 = 1e-9;

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

/// Max on-track gap (seconds) for a one-lap red/blue tint. Wide enough to cover
/// cars that appear in Relative, tight enough to avoid opposite-side SF flashes.
const ONE_LAP_TINT_SECS: f32 = 10.0;

/// Apply lapped-traffic tint relative to a focus car.
///
/// Only applied in race sessions — practice/qualifying ignore lap-down
/// blue/red row coloring. iRacing Relative convention: red = car lapping
/// you (`lap_ahead`), blue = traffic you're lapping.
///
/// For a one-lap difference, tint only when the faster car is within
/// [`ONE_LAP_TINT_SECS`] of catching the slower car. A difference of two or
/// more laps is always tinted. Garage / not-in-world cars are never tinted.
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
    let focus_lap = cars[focus_idx].lap.max(0);
    if focus_lap <= 0 {
        return;
    }
    let focus_est = cars[focus_idx].est_time;
    let focus_pct = cars[focus_idx].lap_dist_pct;
    let lap_est = if lap_est_hint > 10.0 {
        lap_est_hint
    } else {
        estimate_lap_est(cars)
    };

    for (i, car) in cars.iter_mut().enumerate() {
        if i == focus_idx || car.is_pace_car {
            continue;
        }
        // Don't paint garage / spectator ghosts as lapped traffic.
        if car.inactive {
            continue;
        }
        let car_lap = car.lap.max(0);
        if car_lap <= 0 {
            continue;
        }
        let lap_diff = car_lap - focus_lap;
        // Prefer LapDistPct (same basis as Relative); EstTime fallback.
        // Valid pct is enough — off-track / grass still have a lap % and must
        // tint when they're the car you're catching or being caught by.
        let relative_secs = if car.lap_dist_pct >= 0.0 && focus_pct >= 0.0 {
            wrap_lap_delta(car.lap_dist_pct, focus_pct) * lap_est
        } else if car.est_time > 0.0 && focus_est > 0.0 {
            wrap_est_delta(car.est_time - focus_est, lap_est)
        } else {
            f32::NAN
        };
        // Two+ laps always tint (even with no gap sample). One-lap needs proximity.
        if lap_diff.abs() >= 2 {
            car.lapping = true;
            car.lap_ahead = lap_diff > 0;
            continue;
        }
        let one_lap_close = relative_secs.is_finite()
            && match lap_diff {
                // Focus is one lap ahead; slower car is just ahead on track.
                -1 => relative_secs > 0.0 && relative_secs <= ONE_LAP_TINT_SECS,
                // Other car is one lap ahead and just behind the focus.
                1 => (-ONE_LAP_TINT_SECS..0.0).contains(&relative_secs),
                _ => false,
            };
        car.lapping = one_lap_close;
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
    // Ahead/behind are authoritative. `rows` is the on-screen total including you
    // (ahead + self + behind); do not re-derive behind from it here.
    let _ = cfg.f64_key("relative", "rows", (n_ahead + n_behind + 1) as f64);
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
/// Pin-podium keeps P1–P3 at the top whenever centering (race or practice;
/// qualifying already lists from P1).
///
/// `standings.rows` (Total rows) is the max slots shown. With `grow` on, the
/// table only lists real cars (up to that max) so the panel can hug the field.
pub fn build_standings(
    cars: &[CarRow],
    cfg: &OverlayConfig,
    app: &serde_json::Value,
    groups: &serde_json::Value,
    session_type: Option<&str>,
    camera_car_idx: Option<i32>,
    seated_car_idx: Option<i32>,
    standings_sticky: Option<&mut StandingsOrderHysteresis>,
) -> Vec<TableRow> {
    let field = standings_field_cars(cars, camera_car_idx, seated_car_idx);

    // Prefer positions already written by [`apply_shared_standing_positions`].
    // When called standalone (tests), still derive list-index P# from order.
    let timing_order = !is_race_session(session_type);
    let list_display_pos = timing_order || standings_sticky.is_some();
    let ordered = compute_standings_order(&field, session_type, standings_sticky);

    // Qualifying: always list from P1 (timing board). Race/practice: window
    // around the focus car when center_on_player is on.
    let is_qual = session_type
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("qual");
    let center = !is_qual && cfg.bool_key("standings", "center_on_player", true);
    // Qual already lists from P1; elsewhere honor the setting whenever we center.
    let pin_podium = center && cfg.bool_key("standings", "pin_podium", false);
    let rows_ahead = cfg.f64_key("standings", "rows_ahead", 4.0).max(0.0) as usize;
    let _rows_behind = cfg.f64_key("standings", "rows_behind", 5.0).max(0.0) as usize;
    let target = cfg
        .f64_key("standings", "rows", (rows_ahead + _rows_behind) as f64)
        .max(0.0) as usize;
    let grow = cfg.bool_key("standings", "grow", true);

    let leader_f2 = ordered.first().map(|c| c.f2_time).unwrap_or(0.0);

    let build_at = |c: &CarRow, ord: usize| -> TableRow {
        // When ordering from sticky/best-lap, list index is the display P#
        // (SDK CarIdxPosition can lag). After finalize_frame writeback,
        // c.position already matches that index.
        let display_pos = if list_display_pos || c.position <= 0 {
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
                if !grow {
                    pad_standings_to(&mut podium, target);
                }
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
                if !grow {
                    pad_standings_to(&mut rows, target);
                }
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
            if !grow {
                pad_standings_to(&mut rows, target.max(1));
            }
            rows
        }
    };

    for row in &mut out {
        row.apply_driver_group(groups);
    }
    out
}

/// Field used for shared standings ranks (same filter as [`build_standings`]).
fn standings_field_cars(
    cars: &[CarRow],
    camera_car_idx: Option<i32>,
    seated_car_idx: Option<i32>,
) -> Vec<&CarRow> {
    let spectating = matches!(
        (camera_car_idx, seated_car_idx),
        (Some(cam), Some(seat)) if cam != seat
    );
    cars.iter()
        .filter(|c| {
            if c.is_pace_car {
                return false;
            }
            if spectating && Some(c.car_idx) == seated_car_idx && c.is_spectator_ghost() {
                return false;
            }
            true
        })
        .collect()
}

/// Single standings order for the whole overlay (race sticky / practice best-lap).
fn compute_standings_order<'a>(
    field: &[&'a CarRow],
    session_type: Option<&str>,
    sticky: Option<&mut StandingsOrderHysteresis>,
) -> Vec<&'a CarRow> {
    let timing_order = !is_race_session(session_type);
    let focus_pct = field
        .iter()
        .find(|c| c.is_player)
        .map(|c| c.lap_dist_pct)
        .or_else(|| field.first().map(|c| c.lap_dist_pct));
    if timing_order {
        standings_field_order(field, true)
    } else if let Some(sticky) = sticky {
        standings_live_order(field, sticky, focus_pct)
    } else {
        standings_field_order(field, false)
    }
}

/// Write overall (+ class) display positions from a shared order onto every car.
///
/// Index 0 → P1. Class positions are renumbered within each `class_id` in that
/// same order so overall and class never disagree across widgets.
fn apply_shared_standing_positions(cars: &mut [CarRow], order: &[i32]) {
    if order.is_empty() {
        return;
    }
    let overall: HashMap<i32, i32> = order
        .iter()
        .enumerate()
        .map(|(i, &idx)| (idx, (i + 1) as i32))
        .collect();
    for c in cars.iter_mut() {
        if let Some(&p) = overall.get(&c.car_idx) {
            c.position = p;
        }
    }

    let by_idx: HashMap<i32, usize> = cars
        .iter()
        .enumerate()
        .map(|(i, c)| (c.car_idx, i))
        .collect();
    let mut next_class: HashMap<i32, i32> = HashMap::new();
    for &idx in order {
        let Some(&i) = by_idx.get(&idx) else {
            continue;
        };
        let cid = cars[i].class_id;
        let n = next_class.entry(cid).or_insert(1);
        cars[i].class_position = *n;
        *n += 1;
    }
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

/// On-track race progress for Standings (excludes pit lane — that is Relative).
fn standings_racing_progress(car: &CarRow) -> Option<f64> {
    if car.is_pace_car || car.on_pit || car.in_pit || !car.on_track {
        return None;
    }
    if car.lap_dist_pct < 0.0 {
        return None;
    }
    let laps = car.laps_completed.max(0) as f64;
    let pct = (car.lap_dist_pct as f64).clamp(0.0, 1.0);
    Some(laps + pct)
}

/// Live Standings order: race progress with pit freeze (passes flip immediately).
fn standings_live_order<'a>(
    field: &[&'a CarRow],
    sticky: &mut StandingsOrderHysteresis,
    focus_pct: Option<f32>,
) -> Vec<&'a CarRow> {
    let by_idx: HashMap<i32, &CarRow> = field.iter().map(|c| (c.car_idx, *c)).collect();
    let official = standings_field_order(field, false);

    let sf_wrap = match (sticky.focus_prev_pct, focus_pct) {
        (Some(prev), Some(now)) if prev >= 0.0 && now >= 0.0 => (prev - now) > 0.5,
        _ => false,
    };
    if let Some(pct) = focus_pct.filter(|p| *p >= 0.0) {
        sticky.focus_prev_pct = Some(pct);
    }
    if sf_wrap || sticky.committed.is_empty() {
        sticky.committed = official.iter().map(|c| c.car_idx).collect();
        // S/F soft re-sync: keep official this frame. First seed still bubbles
        // below so a clear mid-lap pass is not delayed a tick.
        if sf_wrap {
            return official;
        }
    } else {
        // Drop cars that left; append newcomers in official order.
        sticky.committed.retain(|idx| by_idx.contains_key(idx));
        for c in &official {
            if !sticky.committed.contains(&c.car_idx) {
                let pos = c.position;
                let insert_at = sticky
                    .committed
                    .iter()
                    .position(|idx| {
                        by_idx
                            .get(idx)
                            .map(|o| o.position <= 0 || (pos > 0 && o.position > pos))
                            .unwrap_or(false)
                    })
                    .unwrap_or(sticky.committed.len());
                sticky.committed.insert(insert_at, c.car_idx);
            }
        }
    }

    // Adjacent swaps between racing cars as soon as progress order flips.
    // Pit / garage cars stay frozen (no progress) so pit-lane % cannot reshuffle.
    let n = sticky.committed.len();
    if n >= 2 {
        let mut order = sticky.committed.clone();
        let mut swapped = true;
        while swapped {
            swapped = false;
            for i in 0..n.saturating_sub(1) {
                let a_idx = order[i];
                let b_idx = order[i + 1];
                let (Some(a), Some(b)) = (by_idx.get(&a_idx), by_idx.get(&b_idx)) else {
                    continue;
                };
                let (Some(pa), Some(pb)) =
                    (standings_racing_progress(a), standings_racing_progress(b))
                else {
                    continue;
                };
                if pb > pa + STANDINGS_PROGRESS_EPS {
                    order.swap(i, i + 1);
                    swapped = true;
                }
            }
        }
        sticky.committed = order;
    }

    sticky
        .committed
        .iter()
        .filter_map(|idx| by_idx.get(idx).copied())
        .collect()
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

/// Dash P# from shared car ranks (after [`apply_shared_standing_positions`]).
fn sync_dash_position_with_standings(
    frame: &mut TelemetryFrame,
    sticky: &StandingsOrderHysteresis,
    table_focus_car_idx: Option<i32>,
) {
    let target =
        table_focus_car_idx.or_else(|| frame.cars.iter().find(|c| c.is_player).map(|c| c.car_idx));
    let Some(car_idx) = target else {
        return;
    };

    if let Some(c) = frame.cars.iter().find(|c| c.car_idx == car_idx) {
        if c.position > 0 {
            frame.position = c.position;
            return;
        }
    }
    if is_race_session(frame.session_type.as_deref()) {
        if let Some(ord) = sticky.committed.iter().position(|&i| i == car_idx) {
            frame.position = (ord + 1) as i32;
            return;
        }
    }
    if let Some(p) = frame
        .standings_cars
        .iter()
        .find(|r| !r.empty && r.key.parse::<i32>().ok() == Some(car_idx))
    {
        if p.position > 0 {
            frame.position = p.position;
        }
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
        for &idx in ranked {
            if chosen.len() >= limit {
                break;
            }
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

/// Leader lap / track % / pace for timed-race fuel projection.
fn leader_pace_bits(frame: &TelemetryFrame) -> (i32, f32, Option<f32>) {
    let leader = frame
        .cars
        .iter()
        .filter(|c| c.is_live_competitor())
        .max_by(|a, b| {
            a.lap.cmp(&b.lap).then_with(|| {
                a.lap_dist_pct
                    .partial_cmp(&b.lap_dist_pct)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        })
        .or_else(|| {
            frame
                .cars
                .iter()
                .find(|c| c.is_live_competitor() && c.position == 1)
        });
    let Some(l) = leader else {
        return (
            frame.lead_lap.max(frame.lap),
            frame.player_lap_dist_pct.clamp(0.0, 0.999),
            frame.last_lap_s.map(|s| s as f32).filter(|s| *s > 10.0),
        );
    };
    let pace = if l.is_player {
        frame
            .last_lap_s
            .map(|s| s as f32)
            .filter(|s| *s > 10.0)
            .or_else(|| (frame.lap_est_time > 10.0).then_some(frame.lap_est_time))
    } else if l.est_time > 10.0 {
        // EstTime is not lap duration; prefer session lap estimate.
        (frame.lap_est_time > 10.0).then_some(frame.lap_est_time)
    } else {
        (frame.lap_est_time > 10.0).then_some(frame.lap_est_time)
    };
    (l.lap.max(0), l.lap_dist_pct.clamp(0.0, 0.999), pace)
}

/// Fill relative_cars / standings_cars and enrich radar from cars + cfg.
pub fn finalize_frame(
    frame: &mut TelemetryFrame,
    cfg: &OverlayConfig,
    rel_sticky: &mut RelativeOrderHysteresis,
) {
    let needs = cfg.telem_needs();
    let need_tables = needs.relative || needs.standings || needs.pit_advisor;
    let app = if need_tables || needs.radio {
        crate::cloud::load_app_settings_cache().unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    let groups = if need_tables || needs.radio {
        cfg.driver_groups_value()
    } else {
        serde_json::json!([])
    };

    if needs.radio {
        if let Some(radio) = frame.radio.as_mut() {
            radio.is_pro = crate::cloud::is_pro_driver(&radio.name, &app);
            if !radio.is_pro {
                if let Some(g) = crate::driver_groups::driver_group_for_name(&radio.name, &groups) {
                    radio.group_icon = g.icon;
                    radio.group_color = g.color;
                }
            }
        }
    }
    apply_irating_projection(frame, cfg, rel_sticky);
    apply_sr_projection(frame, cfg, rel_sticky);
    if needs.delta {
        resolve_delta_mode(frame, cfg);
    }
    if needs.flags {
        apply_flag_config(frame, cfg);
    }

    if needs.map {
        let map_focus = super::presentation_focus_car_idx(&frame.cars, frame.camera_car_idx);
        apply_lap_tints(
            &mut frame.cars,
            frame.lap_est_time,
            frame.session_type.as_deref(),
            map_focus,
        );
    }

    if needs.fuel {
        let (leader_lap, leader_pct, leader_lap_s) = leader_pace_bits(frame);
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
            ema_usage: frame.fuel_ema_l,
            economy_usage: frame.fuel_economy_l,
            leader_lap,
            leader_lap_dist_pct: leader_pct,
            leader_lap_s,
            caution: matches!(
                frame.flag.as_deref(),
                Some("yellow") | Some("caution") | Some("yellow_waving") | Some("caution_waving")
            ),
        };
        frame.fuel = crate::telemetry::build_fuel_snapshot(&inp, cfg);

        if needs.pit_advisor {
            let fuel_rate = cfg.f64_key("pit_advisor", "fuel_fill_rate_lps", 2.2) as f32;
            let t2 = cfg.f64_key("pit_advisor", "tire_change_2t_s", 8.0) as f32;
            let t4 = cfg.f64_key("pit_advisor", "tire_change_4t_s", 12.0) as f32;
            let fallback_loss = cfg.f64_key("pit_advisor", "pit_loss_seconds", 25.0) as f32;
            let transit = frame
                .measured_pit_loss_s
                .map(|m| (m - 10.0).clamp(8.0, 40.0))
                .unwrap_or(18.0_f32.min(fallback_loss * 0.7));
            let wear_low = frame
                .tire_corners
                .iter()
                .filter_map(|c| c.wear)
                .any(|w| w > 0.0 && w < 0.45);
            let add = frame.fuel.add.unwrap_or(0.0);
            frame.strategy.service = Some(crate::telemetry::best_service_plan(
                add,
                frame.strategy.tire.tire_urgent,
                frame.strategy.pace.loss_per_lap,
                frame.fuel.laps_remaining,
                transit,
                fuel_rate,
                t2,
                t4,
                wear_low,
            ));
            if let Some(eco) = frame.strategy.coast.economy_usage {
                frame.fuel.economy_usage = Some(eco);
                if let (Some(race), Some(fuel)) = (
                    frame.fuel.ema_usage.or(frame.fuel.avg.usage),
                    frame.fuel.level,
                ) {
                    if eco > 0.0 && race > 0.0 {
                        frame.fuel.economy_extra_laps = Some((fuel / eco) - (fuel / race));
                    }
                }
            }
        }
    }

    let dash_on = needs.dash;
    let need_order = needs.standings_order();
    if need_order || needs.relative || needs.standings || needs.pit_advisor {
        let seated_car_idx = frame.cars.iter().find(|c| c.is_player).map(|c| c.car_idx);
        let mut focused_cars = frame.cars.clone();
        apply_table_focus(&mut focused_cars, frame.camera_car_idx);
        let table_focus = focused_cars.iter().find(|c| c.is_player).map(|c| c.car_idx);
        if needs.relative || needs.standings {
            apply_lap_tints(
                &mut focused_cars,
                frame.lap_est_time,
                frame.session_type.as_deref(),
                table_focus,
            );
        }

        if need_order {
            let field = standings_field_cars(&focused_cars, frame.camera_car_idx, seated_car_idx);
            let order: Vec<i32> = compute_standings_order(
                &field,
                frame.session_type.as_deref(),
                Some(&mut rel_sticky.standings),
            )
            .iter()
            .map(|c| c.car_idx)
            .collect();
            apply_shared_standing_positions(&mut focused_cars, &order);
            apply_shared_standing_positions(&mut frame.cars, &order);
        }
        if dash_on {
            sync_dash_position_with_standings(frame, &rel_sticky.standings, table_focus);
        }
        // Stamp even when radio_tower is hidden so other widgets (and tests)
        // see the same live order as standings.
        if let Some(radio) = frame.radio.as_mut() {
            if let Some(c) = frame.cars.iter().find(|c| {
                (!radio.car_number.is_empty() && c.car_number == radio.car_number)
                    || (!radio.name.is_empty() && c.name == radio.name)
            }) {
                if c.position > 0 {
                    radio.position = c.position;
                }
            }
        }

        let mut rel = if needs.relative || needs.pit_advisor {
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
            if needs.relative
                && needs.fuel
                && cfg.widget_shown("fuel_calc")
                && super::strategy_hints::strategy_window_active(&frame.fuel, frame.fuel_pct, cfg)
            {
                super::strategy_hints::apply_strategy_hints(&mut rel, cfg);
            }
            rel
        } else {
            Vec::new()
        };
        let mut std = if needs.standings {
            build_standings(
                &focused_cars,
                cfg,
                &app,
                &groups,
                frame.session_type.as_deref(),
                frame.camera_car_idx,
                seated_car_idx,
                None,
            )
        } else {
            Vec::new()
        };
        if needs.relative || needs.standings {
            let fl_idx = session_best_car_idx(&focused_cars);
            mark_session_best(&mut rel, fl_idx);
            mark_session_best(&mut std, fl_idx);
            stamp_player_sr_delta(&mut rel, seated_car_idx, frame.sr_delta);
            stamp_player_sr_delta(&mut std, seated_car_idx, frame.sr_delta);
        }
        if needs.relative {
            frame.relative_slots = build_table_slots(frame, cfg, "relative", &rel);
            frame.relative_cars = rel;
        } else if needs.pit_advisor {
            frame.relative_cars = rel;
        }
        if needs.standings {
            frame.standings_slots = build_table_slots(frame, cfg, "standings", &std);
            frame.standings_cars = std;
        }
    }

    if needs.radar {
        let enriched = build_radar(
            &frame.cars,
            cfg,
            frame.radar.left,
            frame.radar.right,
            frame.radar.left2,
            frame.radar.right2,
            frame.player_lap_dist_pct,
        );
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

    if needs.fuel {
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
    }
    if needs.pit_advisor {
        frame.pit_advice = Some(super::pit_advice::compute_pit_advice(frame, cfg));
    }
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
/// Includes garage / off-track drivers — session best is about the time, not
/// who is currently on circuit. Ties keep the lowest car_idx so only one
/// session-best badge is marked.
fn session_best_car_idx(cars: &[CarRow]) -> Option<i32> {
    let mut best: Option<(f64, i32)> = None;
    for c in cars {
        if c.is_pace_car {
            continue;
        }
        let t = c
            .best_lap_time_s
            .map(|s| s as f64)
            .or_else(|| parse_lap_clock(&c.best_lap))
            .filter(|t| *t > 5.0);
        let Some(t) = t else {
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
        if cfg.widget_shown(section)
            && cfg.bool_key(section, "show_irating_projection", false)
            && cfg.has_column(section, "irating")
        {
            return true;
        }
    }
    false
}

fn needs_sr_projection(cfg: &OverlayConfig) -> bool {
    if cfg.bool_key("dash", "show_sr_projection", false) && cfg.dash_uses_license() {
        return true;
    }
    for section in ["relative", "standings"] {
        if cfg.widget_shown(section)
            && cfg.bool_key(section, "show_sr_projection", false)
            && cfg.has_column(section, "license")
        {
            return true;
        }
    }
    false
}

fn stamp_player_sr_delta(rows: &mut [TableRow], seated_car_idx: Option<i32>, delta: Option<i32>) {
    let Some(idx) = seated_car_idx else {
        return;
    };
    let key = idx.to_string();
    for row in rows {
        if row.key == key {
            row.sr_delta = delta;
        }
    }
}

fn apply_sr_projection(
    frame: &mut TelemetryFrame,
    cfg: &OverlayConfig,
    sticky: &mut RelativeOrderHysteresis,
) {
    frame.sr_delta = None;
    if !needs_sr_projection(cfg) {
        sticky.sr_player_delta = None;
        return;
    }
    // Racing / checkered / cooldown — keep the last projection after the flag.
    if !(4..=6).contains(&frame.session_state) {
        sticky.sr_player_delta = None;
        return;
    }

    let turns = frame.track_num_turns;
    if turns <= 0 {
        frame.sr_delta = sticky.sr_player_delta;
        return;
    }

    let player = frame.cars.iter().find(|c| c.is_player);
    let lic = player
        .map(|p| p.license.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(frame.license.as_str());
    let Some((class_idx, sr)) = crate::safety_rating::parse_license_sr(lic) else {
        frame.sr_delta = sticky.sr_player_delta;
        return;
    };
    if sr <= 0.0 {
        frame.sr_delta = sticky.sr_player_delta;
        return;
    }
    let laps = player
        .map(|p| {
            let completed = p.laps_completed.max(0) as f64;
            let pct = if frame.session_state >= 5 {
                0.0
            } else {
                frame.player_lap_dist_pct.clamp(0.0, 0.999) as f64
            };
            completed + pct
        })
        .unwrap_or(0.0);
    let corners = laps * turns as f64;
    let session = frame.session_type.as_deref().unwrap_or("Race");
    if let Some(proj) = crate::safety_rating::project_delta(
        class_idx,
        sr,
        corners,
        frame.incidents.max(0) as f64,
        session,
    ) {
        sticky.sr_player_delta = Some(proj.delta_hundredths);
        frame.sr_delta = Some(proj.delta_hundredths);
    } else {
        frame.sr_delta = sticky.sr_player_delta;
    }

    if frame.license.is_empty() {
        if let Some(p) = frame.cars.iter().find(|c| c.is_player) {
            frame.license = p.license.clone();
        }
    }
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
        frame.irating_delta = p.irating_delta.or(sticky.irating_player_delta);
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
            let (lap, pct) = player
                .map(|p| (p.lap, p.lap_dist_pct))
                .filter(|(l, _)| *l > 0)
                .unwrap_or((frame.lap, frame.player_lap_dist_pct));
            if let Some(total) = frame.display_laps_total_for(lap, pct) {
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
            // Full field (on/off track) — same rule as the purple session-best badge.
            match session_best_car_idx(&frame.cars)
                .and_then(|idx| frame.cars.iter().find(|c| c.car_idx == idx))
            {
                Some(c) if !c.best_lap.is_empty() => c.best_lap.clone(),
                _ => "--".into(),
            }
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
    // Side markers follow the spotter. Only match door-overlap for fore/aft
    // slide — a car 1% ahead is in front, not "on the right".
    let side_band = zone.max(span);
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
        if d.abs() <= side_band {
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
        if let Some((d, c)) = alongside.first() {
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
    fn radar_front_glow_and_alongside_within_range() {
        let mut cfg = OverlayConfig::default();
        cfg.cfg["radar"]["show_front"] = serde_json::json!(true);
        cfg.cfg["radar"]["show_rear"] = serde_json::json!(true);
        cfg.cfg["radar"]["range_pct"] = serde_json::json!(0.03);
        cfg.cfg["radar"]["alongside_zone_pct"] = serde_json::json!(0.004);
        let cars = vec![
            CarRow {
                car_idx: 0,
                is_player: true,
                on_track: true,
                lap_dist_pct: 0.50,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                on_track: true,
                lap_dist_pct: 0.518,
                car_number: "19".into(),
                ..Default::default()
            },
            CarRow {
                car_idx: 2,
                on_track: true,
                lap_dist_pct: 0.485,
                car_number: "5".into(),
                ..Default::default()
            },
        ];
        let r = build_radar(&cars, &cfg, true, false, false, false, 0.50);
        assert!(r.ahead.is_some() && r.ahead.unwrap() > 0.0);
        assert!(r.behind.is_some() && r.behind.unwrap() > 0.0);
        assert!(
            r.left && !r.right,
            "spotter left must not invent a right car"
        );
        assert_eq!(
            r.left_pos, 0.0,
            "0.015 behind is in front/rear, not a door-overlap side match"
        );
    }

    #[test]
    fn radar_does_not_invent_sides_for_cars_ahead() {
        let mut cfg = OverlayConfig::default();
        cfg.cfg["radar"]["show_front"] = serde_json::json!(true);
        cfg.cfg["radar"]["alongside_zone_pct"] = serde_json::json!(0.004);
        cfg.cfg["radar"]["range_pct"] = serde_json::json!(0.03);
        let cars = vec![
            CarRow {
                car_idx: 0,
                is_player: true,
                on_track: true,
                lap_dist_pct: 0.50,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                on_track: true,
                lap_dist_pct: 0.518,
                car_number: "19".into(),
                ..Default::default()
            },
        ];
        let r = build_radar(&cars, &cfg, false, false, false, false, 0.50);
        assert!(!r.left && !r.right, "lap overlap must not guess a side");
        assert!(r.ahead.is_some(), "car 1.8% ahead is front, not a side");
    }

    #[test]
    fn radar_spotter_left_does_not_light_right() {
        let mut cfg = OverlayConfig::default();
        cfg.cfg["radar"]["alongside_zone_pct"] = serde_json::json!(0.004);
        let cars = vec![
            CarRow {
                car_idx: 0,
                is_player: true,
                on_track: true,
                lap_dist_pct: 0.50,
                ..Default::default()
            },
            CarRow {
                car_idx: 1,
                on_track: true,
                lap_dist_pct: 0.502,
                car_number: "19".into(),
                ..Default::default()
            },
        ];
        let r = build_radar(&cars, &cfg, true, false, false, false, 0.50);
        assert!(r.left && !r.right);
        assert!(r.left_pos > 0.0);
    }

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
    fn standings_keeps_official_order_while_player_in_pits() {
        // In pits, LapDistPct can look like Relative track order. Standings must
        // freeze the committed slot (P3 here), not reshuffle by pit-lane %.
        let mut cars = Vec::new();
        for i in 0..6 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 2, // official P3
                on_track: i != 2,
                on_pit: i == 2,
                in_pit: i == 2,
                laps_completed: 10,
                // Pit lane % would put the player "ahead" of P1/P2 on progress.
                lap_dist_pct: if i == 2 { 0.95 } else { 0.10 + i as f32 * 0.05 },
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let mut sticky = StandingsOrderHysteresis::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
            Some(&mut sticky),
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        let player = live.iter().find(|r| r.is_player).expect("player row");
        assert_eq!(player.position, 3);
        let positions: Vec<i32> = live.iter().map(|r| r.position).collect();
        let mut sorted = positions.clone();
        sorted.sort();
        assert_eq!(positions, sorted, "standings rows must stay official order");
    }

    #[test]
    fn standings_live_pass_updates_before_official_position() {
        // Official still P3; on-track progress has already passed P2.
        let mut cars = Vec::new();
        for i in 0..4 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 2,
                on_track: true,
                laps_completed: 5,
                lap_dist_pct: match i {
                    0 => 0.80,
                    1 => 0.40, // official P2, now behind player
                    2 => 0.55, // player
                    _ => 0.20,
                },
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let mut sticky = StandingsOrderHysteresis::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
            Some(&mut sticky),
        );
        let player = rows.iter().find(|r| r.is_player).expect("player");
        assert_eq!(player.position, 2, "live pass should show P2");
        let p2 = rows.iter().find(|r| r.car_number == "1").expect("old P2");
        assert_eq!(p2.position, 3);
    }

    #[test]
    fn standings_tiny_progress_lead_swaps_immediately() {
        let mut cars = Vec::new();
        for i in 0..3 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 1,
                on_track: true,
                laps_completed: 2,
                // Player only 0.5% ahead of P1 — must still take P1 immediately.
                lap_dist_pct: match i {
                    0 => 0.500,
                    1 => 0.505,
                    _ => 0.100,
                },
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let mut sticky = StandingsOrderHysteresis::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
            Some(&mut sticky),
        );
        let player = rows.iter().find(|r| r.is_player).expect("player");
        assert_eq!(
            player.position, 1,
            "on-track pass must update P# immediately"
        );
        let old_leader = rows.iter().find(|r| r.car_number == "0").expect("old P1");
        assert_eq!(old_leader.position, 2);
    }

    #[test]
    fn standings_qual_ignores_live_progress_dampener() {
        let mut cars = Vec::new();
        for i in 0..3 {
            cars.push(CarRow {
                car_idx: i,
                position: 3 - i, // reverse official
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 0,
                on_track: true,
                best_lap: match i {
                    0 => "1:20.00".into(),
                    1 => "1:19.00".into(), // fastest
                    _ => "1:21.00".into(),
                },
                laps_completed: 10,
                lap_dist_pct: 0.99, // would dominate race live order
                ..Default::default()
            });
        }
        let cfg = OverlayConfig::default();
        let mut sticky = StandingsOrderHysteresis::default();
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Qualifying"),
            None,
            None,
            Some(&mut sticky),
        );
        let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
        assert_eq!(live[0].name, "D1", "qual must stay best-lap order");
        assert_eq!(live[0].position, 1);
        assert!(
            sticky.committed.is_empty(),
            "qual must not seed race sticky"
        );
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
    fn standings_pin_podium_keeps_top3_when_focus_midfield() {
        let mut cars = Vec::new();
        for i in 0..16 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 10, // P11
                on_track: true,
                best_lap: format!("1:{:05.2}", 30.0 + i as f32 * 0.1),
                ..Default::default()
            });
        }
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["pin_podium"] = serde_json::json!(true);
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(true);
        cfg.cfg["standings"]["rows"] = serde_json::json!(9.0);
        cfg.cfg["standings"]["rows_ahead"] = serde_json::json!(4.0);
        cfg.cfg["standings"]["rows_behind"] = serde_json::json!(5.0);
        for session in ["Race", "Practice"] {
            let rows = build_standings(
                &cars,
                &cfg,
                &serde_json::json!({}),
                &serde_json::json!([]),
                Some(session),
                None,
                None,
                None,
            );
            let live: Vec<_> = rows.iter().filter(|r| !r.empty).collect();
            assert_eq!(
                live.iter().map(|r| r.position).take(3).collect::<Vec<_>>(),
                vec![1, 2, 3],
                "{session}: pinned podium must stay P1–P3"
            );
            assert!(
                live.iter().any(|r| r.is_player),
                "{session}: focus car must remain in the window"
            );
        }
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
        cfg.cfg["standings"]["grow"] = serde_json::json!(false);
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
    fn standings_grow_hugs_field_up_to_total_rows() {
        let mut cars = Vec::new();
        for i in 0..13 {
            cars.push(CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                is_player: i == 6,
                on_track: true,
                ..Default::default()
            });
        }
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(false);
        cfg.cfg["standings"]["grow"] = serde_json::json!(true);
        cfg.cfg["standings"]["rows"] = serde_json::json!(20.0);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
            None,
        );
        assert_eq!(rows.len(), 13);
        assert!(rows.iter().all(|r| !r.empty));

        cfg.cfg["standings"]["rows"] = serde_json::json!(10.0);
        let capped = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Race"),
            None,
            None,
            None,
        );
        assert_eq!(capped.len(), 10);
        assert!(capped.iter().all(|r| !r.empty));
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
        cfg.cfg["standings"]["grow"] = serde_json::json!(false);
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(true);
        let rows = build_standings(
            &cars,
            &cfg,
            &serde_json::json!({}),
            &serde_json::json!([]),
            Some("Qualifying"),
            None,
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
            None,
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
            None,
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
                lap_dist_pct: 0.50,
                on_track: true,
                ..Default::default()
            },
            // ~4s ahead, one lap down → blue
            CarRow {
                car_idx: 1,
                lap: 9,
                lap_dist_pct: 0.50 + 4.0 / 90.0,
                on_track: true,
                ..Default::default()
            },
            // ~15s ahead, one lap down → no tint
            CarRow {
                car_idx: 2,
                lap: 9,
                lap_dist_pct: 0.50 + 15.0 / 90.0,
                on_track: true,
                ..Default::default()
            },
            // ~4s behind, one lap up → red
            CarRow {
                car_idx: 3,
                lap: 11,
                lap_dist_pct: 0.50 - 4.0 / 90.0,
                on_track: true,
                ..Default::default()
            },
            // ~15s behind, one lap up → no tint
            CarRow {
                car_idx: 4,
                lap: 11,
                lap_dist_pct: 0.50 - 15.0 / 90.0,
                on_track: true,
                ..Default::default()
            },
            // Two+ laps always tinted.
            CarRow {
                car_idx: 5,
                lap: 8,
                lap_dist_pct: 0.20,
                on_track: true,
                ..Default::default()
            },
            CarRow {
                car_idx: 6,
                lap: 12,
                lap_dist_pct: 0.80,
                on_track: true,
                ..Default::default()
            },
        ];
        apply_lap_tints(&mut cars, 90.0, Some("Race"), None);

        assert!(cars[1].lapping && !cars[1].lap_ahead, "lapped traffic blue");
        assert!(!cars[2].lapping, "one-lap far ahead stays untinted");
        assert!(cars[3].lapping && cars[3].lap_ahead, "lapper red");
        assert!(!cars[4].lapping, "one-lap far behind stays untinted");
        assert!(cars[5].lapping && !cars[5].lap_ahead);
        assert!(cars[6].lapping && cars[6].lap_ahead);
    }

    #[test]
    fn lap_tints_skip_inactive_and_work_off_track() {
        let mut cars = vec![
            CarRow {
                car_idx: 0,
                is_player: true,
                lap: 10,
                lap_dist_pct: 0.50,
                on_track: true,
                ..Default::default()
            },
            // Garage ghost — must not paint blue just because lap is 0/low.
            CarRow {
                car_idx: 1,
                lap: 5,
                lap_dist_pct: -1.0,
                inactive: true,
                ..Default::default()
            },
            // Off-track but still in the race, one lap down and close → blue.
            CarRow {
                car_idx: 2,
                lap: 9,
                lap_dist_pct: 0.50 + 3.0 / 90.0,
                on_track: false,
                inactive: false,
                ..Default::default()
            },
        ];
        apply_lap_tints(&mut cars, 90.0, Some("Race"), None);
        assert!(!cars[1].lapping, "inactive cars stay untinted");
        assert!(
            cars[2].lapping && !cars[2].lap_ahead,
            "off-track still tints"
        );
    }

    #[test]
    fn session_best_includes_garage_and_off_track() {
        // On-track car is slower; garage driver holds the real session best.
        let mut frame = TelemetryFrame {
            cars: vec![
                CarRow {
                    car_idx: 0,
                    position: 1,
                    name: "OnTrack".into(),
                    best_lap: "1:30.500".into(),
                    best_lap_time_s: Some(90.5),
                    is_player: true,
                    on_track: true,
                    ..Default::default()
                },
                CarRow {
                    car_idx: 1,
                    position: 2,
                    name: "GarageFast".into(),
                    best_lap: "1:29.100".into(),
                    best_lap_time_s: Some(89.1),
                    on_track: false,
                    inactive: true,
                    lap_dist_pct: -1.0,
                    ..Default::default()
                },
            ],
            session_type: Some("Practice".into()),
            ..Default::default()
        };
        let cfg = OverlayConfig::default();
        let mut sticky = RelativeOrderHysteresis::default();
        finalize_frame(&mut frame, &cfg, &mut sticky);
        assert_eq!(session_best_car_idx(&frame.cars), Some(1));
        assert!(
            frame
                .standings_cars
                .iter()
                .any(|r| r.key == "1" && r.session_best),
            "garage driver must keep the purple session-best mark"
        );
        assert!(!frame
            .standings_cars
            .iter()
            .any(|r| r.key == "0" && r.session_best));
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

    #[test]
    fn race_dash_position_matches_live_standings_pass() {
        // Official still P5; on-track progress already passed P4 — Standings
        // would show P4. Dash must follow that, not raw PlayerCarPosition (5),
        // even when the visible standings window is only P1–P2.
        let mut frame = TelemetryFrame {
            session_type: Some("Race".into()),
            position: 5,
            cars: (0..6)
                .map(|i| CarRow {
                    car_idx: i,
                    position: i + 1,
                    name: format!("D{i}"),
                    car_number: format!("{i}"),
                    is_player: i == 4,
                    on_track: true,
                    laps_completed: 5,
                    lap_dist_pct: match i {
                        0 => 0.90,
                        1 => 0.80,
                        2 => 0.70,
                        3 => 0.40, // official P4, now behind player
                        4 => 0.55, // player (official P5)
                        _ => 0.20,
                    },
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["center_on_player"] = serde_json::json!(false);
        cfg.cfg["standings"]["rows"] = serde_json::json!(2.0);
        cfg.cfg["standings"]["grow"] = serde_json::json!(false);
        let mut sticky = RelativeOrderHysteresis::default();
        finalize_frame(&mut frame, &cfg, &mut sticky);
        assert_eq!(
            frame.position, 4,
            "dash POS should follow live standings pass, got {}",
            frame.position
        );
        assert!(
            !frame.standings_cars.iter().any(|r| r.is_player && !r.empty),
            "test setup: player must be outside the visible standings window"
        );
        let player_car = frame.cars.iter().find(|c| c.is_player).unwrap();
        assert_eq!(player_car.position, frame.position);
        if let Some(rel) = frame.relative_cars.iter().find(|r| r.is_player && !r.empty) {
            assert_eq!(rel.position, frame.position);
        }
    }

    #[test]
    fn shared_positions_match_across_widgets() {
        let mut frame = TelemetryFrame {
            session_type: Some("Race".into()),
            position: 3,
            radio: Some(crate::telemetry::RadioSpeaker {
                position: 99,
                car_number: "2".into(),
                name: "D2".into(),
                active: true,
                ..Default::default()
            }),
            cars: (0..4)
                .map(|i| CarRow {
                    car_idx: i,
                    position: i + 1,
                    class_position: i + 1,
                    class_id: 1,
                    name: format!("D{i}"),
                    car_number: format!("{i}"),
                    is_player: i == 2,
                    on_track: true,
                    laps_completed: 3,
                    // Progress order: 1, 0, 2, 3 → display P1=car1, P2=car0, P3=car2, P4=car3
                    lap_dist_pct: match i {
                        0 => 0.50,
                        1 => 0.80,
                        2 => 0.40,
                        _ => 0.20,
                    },
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut cfg = OverlayConfig::default();
        cfg.cfg["standings"]["rows"] = serde_json::json!(8.0);
        cfg.cfg["standings"]["grow"] = serde_json::json!(true);
        let mut sticky = RelativeOrderHysteresis::default();
        finalize_frame(&mut frame, &cfg, &mut sticky);

        let by_idx: std::collections::HashMap<i32, i32> =
            frame.cars.iter().map(|c| (c.car_idx, c.position)).collect();
        assert_eq!(by_idx.get(&1), Some(&1));
        assert_eq!(by_idx.get(&0), Some(&2));
        assert_eq!(by_idx.get(&2), Some(&3));
        assert_eq!(frame.position, 3);

        for row in frame
            .standings_cars
            .iter()
            .chain(frame.relative_cars.iter())
            .filter(|r| !r.empty)
        {
            let idx: i32 = row.key.parse().unwrap();
            assert_eq!(
                row.position, by_idx[&idx],
                "table row for car {idx} out of sync"
            );
        }
        assert_eq!(frame.radio.as_ref().unwrap().position, by_idx[&2]);
    }

    fn sample_cars() -> Vec<CarRow> {
        (0..6)
            .map(|i| CarRow {
                car_idx: i,
                position: i + 1,
                name: format!("D{i}"),
                car_number: format!("{i}"),
                is_player: i == 2,
                on_track: true,
                lap: 5,
                lap_dist_pct: 0.5 - i as f32 * 0.05,
                last_lap_time_s: Some(90.0 + i as f32),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn enabled_widgets_keep_display_data_after_gating() {
        let cfg = OverlayConfig::default();
        let needs = cfg.telem_needs();
        assert!(needs.relative && needs.standings && needs.dash && needs.fuel && needs.radar);
        assert!(needs.air_temp && needs.track_temp);

        let mut frame = TelemetryFrame {
            session_type: Some("Race".into()),
            air_temp: Some(24.0),
            track_temp: Some(32.0),
            wind_dir: Some(1.0),
            wind_vel: Some(3.0),
            fuel_l: 40.0,
            fuel_pct: 0.5,
            fuel_max_l: 80.0,
            lap: 5,
            last_lap_s: Some(90.0),
            lap_est_time: 90.0,
            cars: sample_cars(),
            radar: RadarState {
                left: true,
                ..Default::default()
            },
            ..Default::default()
        };
        if !needs.air_temp {
            frame.air_temp = None;
        }
        if !needs.track_temp {
            frame.track_temp = None;
        }
        let mut sticky = RelativeOrderHysteresis::default();
        finalize_frame(&mut frame, &cfg, &mut sticky);

        assert_eq!(
            frame.air_temp,
            Some(24.0),
            "dash/standings still show air temp"
        );
        assert_eq!(
            frame.track_temp,
            Some(32.0),
            "dash/standings still show track temp"
        );
        assert!(
            frame.relative_cars.iter().any(|r| r.is_player && !r.empty),
            "relative must still list the player"
        );
        assert!(
            frame.standings_cars.iter().any(|r| r.is_player && !r.empty),
            "standings must still list the player"
        );
        assert!(
            frame.fuel.level.is_some(),
            "fuel calc widget must still get a snapshot"
        );
        assert!(frame.position > 0, "dash POS must still resolve");
        assert!(
            frame.standings_slots.footer_right.key == "air_temp"
                && !frame.standings_slots.footer_right.value.is_empty(),
            "standings footer air temp must still format"
        );
    }

    #[test]
    fn hiding_fuel_calc_keeps_relative_rows() {
        let mut cfg = OverlayConfig::default();
        cfg.cfg["fuel_calc"]["show"] = serde_json::json!(false);
        cfg.cfg["pit_advisor"]["show"] = serde_json::json!(false);
        let needs = cfg.telem_needs();
        assert!(!needs.fuel);
        assert!(needs.relative);

        let mut frame = TelemetryFrame {
            session_type: Some("Race".into()),
            cars: sample_cars(),
            lap_est_time: 90.0,
            ..Default::default()
        };
        let mut sticky = RelativeOrderHysteresis::default();
        finalize_frame(&mut frame, &cfg, &mut sticky);
        assert!(
            frame.relative_cars.iter().any(|r| r.is_player && !r.empty),
            "relative must still populate when fuel calc is off"
        );
        assert!(frame.fuel.level.is_none());
    }
}
