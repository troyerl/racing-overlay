//! Fuel calculator snapshot (Python `pit_strategy.build_fuel_snapshot`).
//!
//! Green-flag EMA burn tracking, timed-race leader lap projection, and light
//! economy (low-throttle) usage estimates.

use crate::config::OverlayConfig;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FuelScenario {
    pub usage: Option<f32>,
    pub laps: Option<f32>,
    pub pits: Option<f32>,
    pub refuel: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FuelStrip {
    pub total: i32,
    pub window: Option<(i32, i32)>,
    pub now: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FuelCalcState {
    pub level: Option<f32>,
    pub cap: Option<f32>,
    pub add: Option<f32>,
    pub window: Option<(i32, i32)>,
    pub window_open: bool,
    pub avg: FuelScenario,
    pub max: FuelScenario,
    pub min: FuelScenario,
    pub time_empty: Option<f32>,
    pub time_margin: Option<f32>,
    pub laps_empty: Option<f32>,
    pub laps_margin: Option<f32>,
    pub strip: FuelStrip,
    pub live_burn: Option<f32>,
    pub fuel_pct: Option<f32>,
    pub pit_hint: Option<String>,
    pub alert: bool,
    pub lap: Option<i32>,
    pub laps_remaining: Option<f32>,
    /// EMA of green-flag burn (L/lap).
    #[serde(default)]
    pub ema_usage: Option<f32>,
    /// Estimated burn under fuel-save / low-throttle (L/lap).
    #[serde(default)]
    pub economy_usage: Option<f32>,
    /// Extra laps available at economy vs race-pace EMA.
    #[serde(default)]
    pub economy_extra_laps: Option<f32>,
}

/// Context for filtering a completed lap before recording burn.
#[derive(Debug, Clone, Copy, Default)]
pub struct FuelLapContext {
    pub caution: bool,
    pub on_pit: bool,
    pub in_pit: bool,
    pub approaching_pits: bool,
    pub throttle: f32,
}

/// Raw telemetry bits needed to build a fuel snapshot.
#[derive(Debug, Clone, Default)]
pub struct FuelInputs {
    pub level: f32,
    pub fuel_pct: f32,
    pub fuel_max: f32,
    pub lap: i32,
    pub last_lap_s: Option<f64>,
    pub lap_est: f32,
    pub laps_remain: Option<f32>,
    pub time_remain: Option<f32>,
    pub fuel_use_per_hour: f32,
    pub laps_total: i32,
    /// Per-lap burn (litres), newest first — Python `fc_use`.
    pub fc_use: Vec<f32>,
    /// Optional precomputed EMA from the burn tracker.
    pub ema_usage: Option<f32>,
    /// Optional economy burn from the burn tracker.
    pub economy_usage: Option<f32>,
    /// Leader lap for timed-race projection (0 = unknown).
    pub leader_lap: i32,
    /// Leader track position 0..1.
    pub leader_lap_dist_pct: f32,
    /// Leader last/est lap time (seconds).
    pub leader_lap_s: Option<f32>,
    /// When true, scale usage by caution_fuel_multiplier (slower burn under yellow).
    pub caution: bool,
}

/// Tracks fuel burned each completed green-flag lap.
#[derive(Debug, Clone, Default)]
pub struct FuelBurnTracker {
    prev_lap: Option<i32>,
    lap_start_fuel: f32,
    /// Throttle sum / samples for the lap in progress.
    throttle_sum: f64,
    throttle_n: u32,
    /// True if any sample this lap was caution / pit-contaminated.
    lap_contaminated: bool,
    /// True if we left pit road this lap (out-lap).
    left_pits_this_lap: bool,
    was_on_pit: bool,
    pub uses: Vec<f32>,
    /// Parallel avg throttle (0..1) for each entry in `uses`.
    pub throttle_avgs: Vec<f32>,
    pub ema: Option<f32>,
    pub economy_ema: Option<f32>,
}

/// Typical full-tank stint length used as a pre-drive seed when iRacing's
/// `FuelUsePerHour` is still idling (garage / grid) and would imply hundreds of laps.
const SEED_STINT_LAPS: f32 = 22.0;

impl FuelBurnTracker {
    /// Extrapolate burn for the lap in progress (L/lap) before a green lap completes.
    /// Returns `None` when too early, contaminated, or not on a clean green lap.
    pub fn provisional_usage(&self, fuel_now: f32, lap_dist_pct: f32) -> Option<f32> {
        if self.prev_lap.is_none() || self.lap_contaminated || self.left_pits_this_lap {
            return None;
        }
        if !fuel_now.is_finite() || !self.lap_start_fuel.is_finite() {
            return None;
        }
        let used = self.lap_start_fuel - fuel_now;
        if used <= 0.08 {
            return None;
        }
        let pct = lap_dist_pct;
        if !(0.18..=0.995).contains(&pct) {
            return None;
        }
        let proj = used / pct;
        if !proj.is_finite() || proj < 0.25 || proj > 12.0 {
            return None;
        }
        if is_burn_outlier(proj, self.ema) {
            return None;
        }
        Some(proj)
    }

    /// Sample continuous state every tick (throttle + pit/caution contamination).
    pub fn tick(&mut self, ctx: &FuelLapContext) {
        if ctx.caution || ctx.on_pit || ctx.in_pit || ctx.approaching_pits {
            self.lap_contaminated = true;
        }
        if self.was_on_pit && !ctx.on_pit && !ctx.in_pit {
            self.left_pits_this_lap = true;
            self.lap_contaminated = true;
        }
        self.was_on_pit = ctx.on_pit || ctx.in_pit;
        if ctx.throttle.is_finite() && ctx.throttle >= 0.0 {
            self.throttle_sum += ctx.throttle.clamp(0.0, 1.0) as f64;
            self.throttle_n = self.throttle_n.saturating_add(1);
        }
    }

    /// Call every tick, then `observe` for lap transitions.
    pub fn observe(
        &mut self,
        lap: i32,
        fuel: f32,
        cap: f32,
        history_n: usize,
        ema_alpha: f32,
    ) {
        if lap <= 0 || !fuel.is_finite() {
            return;
        }
        let cap = if cap > 0.0 { cap } else { 1e9 };
        let alpha = ema_alpha.clamp(0.05, 0.95);

        match self.prev_lap {
            None => {
                self.prev_lap = Some(lap);
                self.lap_start_fuel = fuel;
                self.reset_lap_accum();
            }
            Some(prev) if lap > prev => {
                // Finalize the lap that just completed using accumulators from
                // prior ticks — do not fold the new lap's context in.
                let used = self.lap_start_fuel - fuel;
                let thr_avg = if self.throttle_n > 0 {
                    (self.throttle_sum / self.throttle_n as f64) as f32
                } else {
                    1.0
                };
                let eligible = !self.lap_contaminated
                    && !self.left_pits_this_lap
                    && used > 0.0
                    && used < cap
                    && !is_burn_outlier(used, self.ema);

                if eligible {
                    self.uses.insert(0, used);
                    self.throttle_avgs.insert(0, thr_avg);
                    let n = history_n.max(1);
                    if self.uses.len() > n {
                        self.uses.truncate(n);
                        self.throttle_avgs.truncate(n);
                    }
                    self.ema = Some(match self.ema {
                        Some(prev_e) => alpha * used + (1.0 - alpha) * prev_e,
                        None => used,
                    });
                    // Low-throttle laps feed the economy EMA.
                    if thr_avg < 0.78 {
                        self.economy_ema = Some(match self.economy_ema {
                            Some(prev_e) => alpha * used + (1.0 - alpha) * prev_e,
                            None => used,
                        });
                    }
                }
                self.prev_lap = Some(lap);
                self.lap_start_fuel = fuel;
                self.reset_lap_accum();
            }
            Some(prev) if lap < prev => {
                self.uses.clear();
                self.throttle_avgs.clear();
                self.ema = None;
                self.economy_ema = None;
                self.prev_lap = Some(lap);
                self.lap_start_fuel = fuel;
                self.reset_lap_accum();
            }
            _ => {}
        }
    }

    fn reset_lap_accum(&mut self) {
        self.throttle_sum = 0.0;
        self.throttle_n = 0;
        self.lap_contaminated = false;
        self.left_pits_this_lap = false;
    }
}

fn is_burn_outlier(used: f32, ema: Option<f32>) -> bool {
    match ema {
        Some(e) if e > 0.05 => used > e * 1.5 || used < e * 0.35,
        _ => false,
    }
}

const MAX_SESSION_SEC: f32 = 48.0 * 3600.0;

fn sane_session_seconds(secs: Option<f32>) -> Option<f32> {
    secs.filter(|t| *t >= 0.0 && *t <= MAX_SESSION_SEC)
}

fn fuel_capacity(level: f32, fuel_max: f32, fuel_pct: f32) -> Option<f32> {
    if fuel_max > 0.0 {
        return Some(fuel_max);
    }
    if level > 0.0 && fuel_pct > 0.01 {
        return Some(level / fuel_pct);
    }
    None
}

/// Pre-drive L/lap estimate: race-pace `FuelUsePerHour` when plausible, else tank/stint seed.
fn seed_usage_l_per_lap(cap: Option<f32>, lap_avg: Option<f32>, fuph: f32) -> Option<f32> {
    if let Some(lap_s) = lap_avg.filter(|s| *s > 10.0) {
        if fuph >= 12.0 {
            let u = fuph * (lap_s / 3600.0);
            if fuph_usage_plausible(u, fuph, cap) {
                return Some(u);
            }
        }
    }
    cap.filter(|c| *c > 5.0).map(|c| (c / SEED_STINT_LAPS).clamp(0.5, 10.0))
}

fn fuph_usage_plausible(u: f32, fuph: f32, cap: Option<f32>) -> bool {
    if !(u.is_finite() && fuph.is_finite()) {
        return false;
    }
    // Idle / grid FuelUsePerHour is a few L/h and yields absurd lap counts.
    if fuph < 12.0 || u < 0.35 || u > 12.0 {
        return false;
    }
    if let Some(c) = cap.filter(|c| *c > 5.0) {
        let laps = c / u;
        if !(4.0..=70.0).contains(&laps) {
            return false;
        }
    }
    true
}

fn fuel_lap_secs(est_lap: f32, last_lap: Option<f64>) -> Option<f32> {
    if let Some(s) = last_lap {
        if s > 10.0 {
            let s = s as f32;
            if est_lap > 0.0 {
                if (s - est_lap).abs() / est_lap <= 0.20 {
                    return Some(s);
                }
                return Some(est_lap);
            }
            return Some(s);
        }
    }
    if est_lap > 0.0 {
        Some(est_lap)
    } else {
        None
    }
}

/// Project remaining race laps for a timed race from the leader's pace and
/// track position when the clock hits zero (plus the checkered finish lap).
pub fn project_timed_race_laps_remain(
    time_remain: Option<f32>,
    leader_lap: i32,
    leader_pct: f32,
    leader_lap_s: Option<f32>,
    player_lap: i32,
    player_pace: Option<f32>,
) -> Option<f32> {
    let t = sane_session_seconds(time_remain)?;
    let pace = leader_lap_s
        .filter(|s| *s > 10.0)
        .or(player_pace.filter(|s| *s > 10.0))?;
    if pace <= 0.0 || leader_lap <= 0 {
        return None;
    }
    let pct = leader_pct.clamp(0.0, 0.999);
    // Laps the leader still completes before the clock expires, including the
    // fractional lap they are on, then +1 for the checkered finish lap.
    let laps_to_zero = t / pace + (1.0 - pct);
    let finish_lap = leader_lap as f32 + laps_to_zero.ceil();
    let player = player_lap.max(0) as f32;
    Some((finish_lap - player).max(0.0))
}

fn race_remaining(
    laps_remain: Option<f32>,
    time_remain: Option<f32>,
    lap_avg: Option<f32>,
    inp: &FuelInputs,
) -> (Option<f32>, Option<f32>) {
    let mut laps = laps_remain.filter(|l| *l >= 0.0 && *l <= 32000.0);
    let mut t = sane_session_seconds(time_remain);

    // Timed race: prefer leader-based projection over player-only time/pace.
    let timed = inp.laps_total <= 0;
    if timed {
        if let Some(proj) = project_timed_race_laps_remain(
            time_remain,
            inp.leader_lap,
            inp.leader_lap_dist_pct,
            inp.leader_lap_s,
            inp.lap,
            lap_avg,
        ) {
            laps = Some(proj);
        }
    }

    if laps.is_none() {
        if let (Some(tt), Some(avg)) = (t, lap_avg) {
            if avg > 0.0 {
                laps = Some(tt / avg);
            }
        }
    }
    if t.is_none() {
        if let (Some(ll), Some(avg)) = (laps, lap_avg) {
            if avg > 0.0 {
                t = Some(ll * avg);
            }
        }
    }
    (laps, t)
}

fn scenario(u: Option<f32>, fuel: f32, laps_rem: Option<f32>, cap: Option<f32>) -> FuelScenario {
    let Some(u) = u.filter(|x| *x > 0.0) else {
        return FuelScenario {
            usage: u,
            ..Default::default()
        };
    };
    if fuel <= 0.0 {
        return FuelScenario {
            usage: Some(u),
            ..Default::default()
        };
    }
    let laps_on_fuel = fuel / u;
    let mut refuel = None;
    let mut pits = None;
    if let Some(rem) = laps_rem {
        let r = (rem * u - fuel).max(0.0);
        refuel = Some(r);
        if let Some(c) = cap.filter(|c| *c > 0.0) {
            pits = Some(r / c);
        }
    }
    FuelScenario {
        usage: Some(u),
        laps: Some(laps_on_fuel),
        pits,
        refuel,
    }
}

/// Port of Python `build_fuel_snapshot` (EMA preferred; history / FuelUsePerHour fallback).
pub fn build_fuel_snapshot(inp: &FuelInputs, cfg: &OverlayConfig) -> FuelCalcState {
    let level = if inp.level.is_finite() && inp.level >= 0.0 {
        Some(inp.level)
    } else {
        None
    };
    let cap = fuel_capacity(inp.level, inp.fuel_max, inp.fuel_pct);
    let lap = if inp.lap > 0 { Some(inp.lap) } else { None };
    let lap_avg = fuel_lap_secs(inp.lap_est, inp.last_lap_s);
    let (laps_rem, time_rem) = race_remaining(inp.laps_remain, inp.time_remain, lap_avg, inp);

    let caution_mul = if inp.caution {
        cfg.f64_key("pit_advisor", "caution_fuel_multiplier", 0.55) as f32
    } else {
        1.0
    }
    .clamp(0.2, 1.0);

    let (u_avg, u_max, u_min) = if let Some(ema) = inp.ema_usage.filter(|u| *u > 0.0) {
        let ema = ema * caution_mul;
        let (max, min) = if !inp.fc_use.is_empty() {
            let max = inp.fc_use.iter().copied().fold(f32::NEG_INFINITY, f32::max) * caution_mul;
            let min = inp.fc_use.iter().copied().fold(f32::INFINITY, f32::min) * caution_mul;
            (max.max(ema), min.min(ema))
        } else {
            (ema * 1.08, ema * 0.92)
        };
        (Some(ema), Some(max), Some(min))
    } else if !inp.fc_use.is_empty() {
        let sum: f32 = inp.fc_use.iter().sum();
        let avg = (sum / inp.fc_use.len() as f32) * caution_mul;
        let max = inp.fc_use.iter().copied().fold(f32::NEG_INFINITY, f32::max) * caution_mul;
        let min = inp.fc_use.iter().copied().fold(f32::INFINITY, f32::min) * caution_mul;
        (Some(avg), Some(max), Some(min))
    } else {
        // No completed green burns yet: prefer a race-plausible FuelUsePerHour
        // estimate, else a typical full-tank stint seed (avoids 800+ lap garage idle).
        let est = seed_usage_l_per_lap(cap, lap_avg, inp.fuel_use_per_hour)
            .map(|u| u * caution_mul);
        (est, est.map(|u| u * 1.08), est.map(|u| u * 0.92))
    };

    let ema_usage = inp.ema_usage.or(u_avg);
    let economy_usage = inp
        .economy_usage
        .filter(|e| u_avg.map(|a| *e < a * 0.98).unwrap_or(true));
    let economy_extra_laps = match (economy_usage, ema_usage, level) {
        (Some(eco), Some(race), Some(fuel)) if eco > 0.0 && race > 0.0 && fuel > 0.0 => {
            Some((fuel / eco) - (fuel / race))
        }
        _ => None,
    };

    let live_burn = if cfg.bool_key("fuel_calc", "show_live_burn", false) {
        inp.fc_use.first().copied().or(u_avg)
    } else {
        None
    };

    let fuel_pct = if cfg.bool_key("fuel_calc", "show_tank_pct", false) {
        if inp.fuel_pct > 0.0 {
            Some(inp.fuel_pct * 100.0)
        } else if let (Some(c), Some(l)) = (cap, level) {
            if c > 0.0 {
                Some(100.0 * l / c)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let fuel = level.unwrap_or(0.0);
    let avg = scenario(u_avg, fuel, laps_rem, cap);
    let max = scenario(u_max, fuel, laps_rem, cap);
    let min = scenario(u_min, fuel, laps_rem, cap);

    let laps_empty = avg.laps;
    let time_empty = match (laps_empty, lap_avg) {
        (Some(le), Some(la)) => Some(le * la),
        _ => None,
    };
    let laps_margin = match (laps_empty, laps_rem) {
        (Some(le), Some(lr)) => Some(le - lr),
        _ => None,
    };
    let time_margin = match (time_empty, time_rem) {
        (Some(te), Some(tr)) => Some(te - tr),
        _ => None,
    };
    let add = avg.refuel;

    let mut window = None;
    let mut win_open = false;
    let mut strip = FuelStrip::default();
    if let (Some(lap_n), Some(max_laps), Some(min_laps), Some(add_v)) =
        (lap, max.laps, min.laps, add)
    {
        if add_v > 0.0 {
            let a = lap_n + max_laps as i32;
            let b = lap_n + min_laps as i32;
            window = Some((a, b));
            win_open = lap_n >= a - 1;
            if let Some(lr) = laps_rem.filter(|x| *x > 0.0) {
                let total = (1).max((lr.round() as i32).min(40));
                let wa = (max_laps as i32).clamp(0, total - 1);
                let wb = (min_laps as i32).clamp(0, total - 1);
                let now_idx = if inp.laps_total > 0 {
                    let elapsed = (lap_n - 1).max(0);
                    ((elapsed as f32 / inp.laps_total as f32 * total as f32).round() as i32)
                        .clamp(0, total - 1)
                } else {
                    (total - lr as i32).clamp(0, total - 1)
                };
                strip = FuelStrip {
                    total,
                    window: Some((wa.min(wb), wa.max(wb))),
                    now: Some(now_idx),
                };
            }
        }
    }

    let pit_hint = if cfg.bool_key("fuel_calc", "show_pit_compare", false) {
        u_avg.map(|u| {
            let loss = cfg.f64_key("fuel_calc", "pit_loss_seconds", 25.0);
            format!("Pit now ~{loss:.0}s vs +2 laps ~{:.1}L", 2.0 * u)
        })
    } else {
        None
    };

    let mut alert = false;
    if cfg.bool_key("fuel_calc", "show_low_fuel_alert", true) {
        let lt = cfg.f64_key("fuel_calc", "low_fuel_laps_threshold", 2.0) as f32;
        let tt = cfg.f64_key("fuel_calc", "low_fuel_time_threshold", 120.0) as f32;
        if laps_margin.map(|m| m < lt).unwrap_or(false) {
            alert = true;
        }
        if time_margin.map(|m| m < tt).unwrap_or(false) {
            alert = true;
        }
    }

    FuelCalcState {
        level,
        cap,
        add,
        window,
        window_open: win_open,
        avg,
        max,
        min,
        time_empty,
        time_margin,
        laps_empty,
        laps_margin,
        strip,
        live_burn,
        fuel_pct,
        pit_hint,
        alert,
        lap,
        laps_remaining: laps_rem,
        ema_usage,
        economy_usage,
        economy_extra_laps,
    }
}

/// Demo / placeholder fuel payload for QA (unused — finalize_frame rebuilds from fields).
#[allow(dead_code)]
pub fn demo_fuel(t: f64, fuel_l: f32, fuel_pct: f32, lap: i32) -> FuelCalcState {
    let cfg = OverlayConfig::default();
    let inp = FuelInputs {
        level: fuel_l,
        fuel_pct,
        fuel_max: fuel_l / fuel_pct.max(0.05),
        lap,
        last_lap_s: Some(88.4),
        lap_est: 90.0,
        laps_remain: Some(37.0 - (t * 0.01) as f32),
        time_remain: Some(3300.0 - t as f32 * 0.5),
        fuel_use_per_hour: 48.0,
        laps_total: 50,
        fc_use: Vec::new(),
        ema_usage: None,
        economy_usage: None,
        leader_lap: lap,
        leader_lap_dist_pct: 0.5,
        leader_lap_s: Some(90.0),
        caution: false,
    };
    build_fuel_snapshot(&inp, &cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_produces_scenarios() {
        let cfg = OverlayConfig::default();
        let snap = build_fuel_snapshot(
            &FuelInputs {
                level: 20.0,
                fuel_pct: 0.25,
                fuel_max: 80.0,
                lap: 10,
                last_lap_s: Some(90.0),
                lap_est: 90.0,
                laps_remain: Some(30.0),
                time_remain: Some(2700.0),
                fuel_use_per_hour: 40.0,
                laps_total: 40,
                fc_use: Vec::new(),
                ..Default::default()
            },
            &cfg,
        );
        assert!(snap.avg.usage.unwrap() > 0.0);
        assert!(snap.avg.laps.unwrap() > 0.0);
        assert!(snap.add.unwrap() > 0.0);
        assert!(snap.strip.total > 0);
    }

    #[test]
    fn ema_ignores_yellow_and_pit_burns() {
        let mut tr = FuelBurnTracker::default();
        let green = FuelLapContext {
            throttle: 0.9,
            ..Default::default()
        };
        let yellow = FuelLapContext {
            caution: true,
            throttle: 0.4,
            ..Default::default()
        };
        let pit = FuelLapContext {
            on_pit: true,
            throttle: 0.2,
            ..Default::default()
        };

        // Lap 1 start
        tr.tick(&green);
        tr.observe(1, 60.0, 80.0, 5, 0.35);
        tr.tick(&green);
        // Complete lap 1 green: burn 2.0
        tr.observe(2, 58.0, 80.0, 5, 0.35);
        assert_eq!(tr.uses.len(), 1);
        assert!((tr.uses[0] - 2.0).abs() < 1e-3);
        assert!(tr.ema.is_some());

        // Contaminated yellow lap — should not record
        tr.tick(&yellow);
        tr.observe(3, 56.5, 80.0, 5, 0.35);
        assert_eq!(tr.uses.len(), 1);

        // Pit lap — should not record
        tr.tick(&pit);
        tr.observe(4, 55.0, 80.0, 5, 0.35);
        assert_eq!(tr.uses.len(), 1);

        // First green after pits is an out-lap — still filtered
        tr.tick(&green);
        tr.observe(5, 53.0, 80.0, 5, 0.35);
        assert_eq!(tr.uses.len(), 1);

        // Clean green lap after out-lap — recorded
        tr.tick(&green);
        tr.observe(6, 51.0, 80.0, 5, 0.35);
        assert_eq!(tr.uses.len(), 2);
    }

    #[test]
    fn timed_race_projector_uses_leader_fraction() {
        // Leader on lap 20 at 50% with 90s pace, 180s remain → 2.5 laps to zero
        // ceil → 3, finish lap = 23, player lap 20 → 3 remain.
        let rem = project_timed_race_laps_remain(
            Some(180.0),
            20,
            0.5,
            Some(90.0),
            20,
            Some(90.0),
        );
        assert!(rem.is_some());
        let rem = rem.unwrap();
        assert!((rem - 3.0).abs() < 0.01, "got {rem}");
    }

    #[test]
    fn snapshot_uses_ema_over_mean() {
        let cfg = OverlayConfig::default();
        let snap = build_fuel_snapshot(
            &FuelInputs {
                level: 20.0,
                fuel_pct: 0.25,
                fuel_max: 80.0,
                lap: 10,
                last_lap_s: Some(90.0),
                lap_est: 90.0,
                laps_remain: Some(20.0),
                time_remain: None,
                fuel_use_per_hour: 0.0,
                laps_total: 40,
                fc_use: vec![3.0, 2.0, 2.0],
                ema_usage: Some(2.2),
                ..Default::default()
            },
            &cfg,
        );
        assert!((snap.avg.usage.unwrap() - 2.2).abs() < 1e-3);
        assert!((snap.ema_usage.unwrap() - 2.2).abs() < 1e-3);
    }

    #[test]
    fn idle_fuel_use_per_hour_uses_tank_seed_not_hundreds_of_laps() {
        let cfg = OverlayConfig::default();
        // Garage idle ~4 L/h used to imply ~800 laps on an 80L tank.
        let snap = build_fuel_snapshot(
            &FuelInputs {
                level: 80.0,
                fuel_pct: 1.0,
                fuel_max: 80.0,
                lap: 0,
                last_lap_s: None,
                lap_est: 90.0,
                laps_remain: Some(30.0),
                time_remain: Some(2700.0),
                fuel_use_per_hour: 4.0,
                laps_total: 40,
                fc_use: Vec::new(),
                ..Default::default()
            },
            &cfg,
        );
        let laps = snap.laps_empty.expect("seeded laps");
        assert!(
            (18.0..=28.0).contains(&laps),
            "expected ~{SEED_STINT_LAPS} lap seed, got {laps}"
        );
        assert!(laps < 100.0, "idle FUPH must not produce absurd lap counts");
    }

    #[test]
    fn race_pace_fuel_use_per_hour_still_used_pre_drive() {
        let cfg = OverlayConfig::default();
        // 72 L/h @ 90s → 1.8 L/lap → 80/1.8 ≈ 44.4 laps.
        let snap = build_fuel_snapshot(
            &FuelInputs {
                level: 80.0,
                fuel_pct: 1.0,
                fuel_max: 80.0,
                lap: 0,
                last_lap_s: None,
                lap_est: 90.0,
                laps_remain: Some(30.0),
                time_remain: None,
                fuel_use_per_hour: 72.0,
                laps_total: 40,
                fc_use: Vec::new(),
                ..Default::default()
            },
            &cfg,
        );
        let usage = snap.avg.usage.expect("usage");
        assert!((usage - 1.8).abs() < 0.05, "got {usage}");
    }

    #[test]
    fn provisional_usage_extrapolates_mid_lap() {
        let mut tr = FuelBurnTracker::default();
        let green = FuelLapContext {
            throttle: 0.9,
            ..Default::default()
        };
        tr.tick(&green);
        tr.observe(1, 60.0, 80.0, 5, 0.35);
        // Halfway, burned 1.2L → project 2.4 L/lap.
        let proj = tr.provisional_usage(58.8, 0.5).expect("provisional");
        assert!((proj - 2.4).abs() < 0.05, "got {proj}");
        assert!(tr.provisional_usage(59.9, 0.05).is_none());
    }
}
