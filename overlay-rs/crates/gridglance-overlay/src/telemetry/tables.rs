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
) -> Vec<TableRow> {
    let n_ahead = cfg.f64_key("relative", "rows_ahead", 3.0) as usize;
    let n_behind = cfg.f64_key("relative", "rows_behind", 3.0) as usize;
    let center = cfg.bool_key("relative", "center_on_player", true);

    let Some(player) = cars.iter().find(|c| c.is_player) else {
        return Vec::new();
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

    rows
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

/// Standings windowing (Python `standings_row_list`).
pub fn build_standings(cars: &[CarRow], cfg: &OverlayConfig) -> Vec<TableRow> {
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

    out
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
    apply_irating_projection(frame, cfg);

    let rel = build_relative(&frame.cars, cfg, frame.lap_est_time);
    let std = build_standings(&frame.cars, cfg);
    frame.relative_slots = build_table_slots(frame, cfg, "relative", &rel);
    frame.standings_slots = build_table_slots(frame, cfg, "standings", &std);
    frame.relative_cars = rel;
    frame.standings_cars = std;

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

    // Rebuild fuel snapshot from live frame fields + CFG.
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
    };
    frame.fuel = crate::telemetry::build_fuel_snapshot(&inp, cfg);
}

fn needs_irating_projection(cfg: &OverlayConfig) -> bool {
    if cfg.bool_key("dash", "show_irating_projection", false) && cfg.dash_uses_irating() {
        return true;
    }
    for section in ["relative", "standings"] {
        if cfg.bool_key(section, "show_irating_projection", false) && cfg.has_column(section, "irating")
        {
            return true;
        }
    }
    false
}

fn apply_irating_projection(frame: &mut TelemetryFrame, cfg: &OverlayConfig) {
    // Clear prior deltas unless we recompute.
    for c in &mut frame.cars {
        c.irating_delta = None;
    }
    frame.irating_delta = None;

    if !needs_irating_projection(cfg) {
        return;
    }
    // Race / checkered only (SessionState 4, 5); demo uses 4.
    if frame.session_state != 4 && frame.session_state != 5 {
        return;
    }

    let deltas = crate::irating::project_deltas_by_class(&frame.cars);
    if deltas.is_empty() {
        return;
    }
    for c in &mut frame.cars {
        if let Some(d) = deltas.get(&c.car_idx) {
            c.irating_delta = Some(*d);
        }
    }
    if let Some(p) = frame.cars.iter().find(|c| c.is_player) {
        frame.irating_delta = p.irating_delta;
        if frame.irating <= 0 {
            frame.irating = p.irating;
        }
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

/// Build header/footer strings from Settings `header` / `footer` maps.
pub fn build_table_slots(
    frame: &TelemetryFrame,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
) -> TableSlots {
    let defs = slot_defaults(section);
    let (hl, hc, hr) = defs[0];
    let (fl, fc, fr) = defs[1];
    let fmt = |group: &str, pos: &str, default: &str| -> String {
        let key = cfg.nested_str(section, group, pos, default);
        if key == "none" || key.is_empty() {
            return String::new();
        }
        format_slot_display(&key, frame, cfg, section, rows)
    };
    TableSlots {
        header_left: fmt("header", "left", hl),
        header_center: fmt("header", "center", hc),
        header_right: fmt("header", "right", hr),
        footer_left: fmt("footer", "left", fl),
        footer_center: fmt("footer", "center", fc),
        footer_right: fmt("footer", "right", fr),
    }
}

fn slot_label(key: &str) -> &'static str {
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
        _ => "",
    }
}

fn with_label(key: &str, value: &str) -> String {
    let lead = slot_label(key);
    if lead.is_empty() {
        value.to_string()
    } else {
        format!("{lead} {value}")
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

fn fmt_laptime(secs: f64) -> String {
    if !secs.is_finite() || secs <= 0.0 {
        return "—".into();
    }
    let m = (secs / 60.0).floor() as i32;
    let s = secs - (m as f64) * 60.0;
    format!("{m}:{s:06.3}")
}

fn format_slot_value(
    key: &str,
    frame: &TelemetryFrame,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
) -> String {
    let player = frame.cars.iter().find(|c| c.is_player);
    let total = frame.cars.iter().filter(|c| c.position > 0 && !c.is_pace_car).count();
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
                .filter(|c| !c.is_pace_car && c.irating > 0 && c.class_id == cid)
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
                    "—".into()
                }
            } else {
                "—".into()
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
                    "—".into()
                }
            } else {
                "—".into()
            }
        }
        "session_time" => {
            if let Some(rem) = frame.session_time_remain {
                if rem >= 0.0 {
                    return fmt_clock(rem as f64);
                }
            }
            "—".into()
        }
        "race_time" => {
            let el = if frame.session_time >= 0.0 {
                Some(frame.session_time)
            } else {
                None
            };
            match el {
                Some(el) => fmt_clock(el),
                None => "—".into(),
            }
        }
        "lap" => {
            if frame.laps_total > 0 {
                format!("{}/{}", frame.lap, frame.laps_total)
            } else if frame.lap > 0 {
                format!("{}", frame.lap)
            } else {
                "—".into()
            }
        }
        "incidents" => format!("{}x", frame.incidents),
        "track_name" => frame
            .track_name
            .clone()
            .unwrap_or_else(|| "—".into()),
        "track_temp" => {
            if let Some(t) = frame.track_temp {
                format!("{:.0}{}", cfg.conv_temp(t), cfg.temp_unit())
            } else {
                "—".into()
            }
        }
        "air_temp" => {
            if let Some(t) = frame.air_temp {
                format!("{:.0}{}", cfg.conv_temp(t), cfg.temp_unit())
            } else {
                "—".into()
            }
        }
        "best_lap" => frame
            .best_lap_s
            .map(fmt_laptime)
            .unwrap_or_else(|| "—".into()),
        "my_session_best" => player
            .map(|p| {
                if p.best_lap.is_empty() {
                    "—".into()
                } else {
                    p.best_lap.clone()
                }
            })
            .unwrap_or_else(|| "—".into()),
        "session_best" => {
            let best = frame
                .cars
                .iter()
                .filter(|c| !c.best_lap.is_empty() && c.best_lap != "—")
                .map(|c| c.best_lap.as_str())
                .min();
            best.unwrap_or("—").to_string()
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
        "cpu" => frame.cpu.clone().unwrap_or_else(|| "—".into()),
        "mem" => frame.mem.clone().unwrap_or_else(|| "—".into()),
        "gpu" => frame.gpu.clone().unwrap_or_else(|| "—".into()),
        "laps_remain" => frame
            .session_laps_remain
            .map(|v| format!("{v:.0}"))
            .unwrap_or_else(|| "—".into()),
        "weather" => {
            let mut parts = Vec::new();
            if let Some(s) = &frame.skies {
                parts.push(s.clone());
            }
            if let Some(h) = frame.humidity {
                parts.push(format!("{h:.0}%"));
            }
            if parts.is_empty() {
                "—".into()
            } else {
                parts.join(" ")
            }
        }
        "track_wetness" => frame
            .track_wetness
            .map(|w| format!("{w:.0}%"))
            .unwrap_or_else(|| "—".into()),
        "title" => cfg.str_key(section, "title", "Standings"),
        "order_pill" => "ORDER".into(),
        "count" => {
            let shown = rows.iter().filter(|r| !r.empty).count();
            format!("{shown}/{total}")
        }
        _ => "—".into(),
    }
}

fn format_slot_display(
    key: &str,
    frame: &TelemetryFrame,
    cfg: &OverlayConfig,
    section: &str,
    rows: &[TableRow],
) -> String {
    if key == "title" || key == "order_pill" || key == "count" || key == "track_name" {
        return format_slot_value(key, frame, cfg, section, rows);
    }
    let val = format_slot_value(key, frame, cfg, section, rows);
    with_label(key, &val)
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
        let rows = build_relative(&cars, &cfg, 90.0);
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
        let rows = build_standings(&cars, &cfg);
        assert!(rows.iter().any(|r| r.is_player));
        assert!(!rows.is_empty());
    }
}
