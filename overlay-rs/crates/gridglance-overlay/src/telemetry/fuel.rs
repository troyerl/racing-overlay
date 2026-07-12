//! Fuel calculator snapshot (Python `pit_strategy.build_fuel_snapshot`).

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

fn race_remaining(
    laps_remain: Option<f32>,
    time_remain: Option<f32>,
    lap_avg: Option<f32>,
) -> (Option<f32>, Option<f32>) {
    let mut laps = laps_remain.filter(|l| *l >= 0.0 && *l <= 32000.0);
    let mut t = sane_session_seconds(time_remain);
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

/// Port of Python `build_fuel_snapshot` (no lap-history; FuelUsePerHour fallback).
pub fn build_fuel_snapshot(inp: &FuelInputs, cfg: &OverlayConfig) -> FuelCalcState {
    let level = if inp.level.is_finite() && inp.level >= 0.0 {
        Some(inp.level)
    } else {
        None
    };
    let cap = fuel_capacity(inp.level, inp.fuel_max, inp.fuel_pct);
    let lap = if inp.lap > 0 { Some(inp.lap) } else { None };
    let lap_avg = fuel_lap_secs(inp.lap_est, inp.last_lap_s);
    let (laps_rem, time_rem) = race_remaining(inp.laps_remain, inp.time_remain, lap_avg);

    let est = if inp.fuel_use_per_hour > 0.05 {
        lap_avg.map(|s| inp.fuel_use_per_hour * (s / 3600.0))
    } else {
        None
    };
    let u_avg = est;
    let u_max = est.map(|u| u * 1.08);
    let u_min = est.map(|u| u * 0.92);

    let live_burn = if cfg.bool_key("fuel_calc", "show_live_burn", false) {
        u_avg
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
            },
            &cfg,
        );
        assert!(snap.avg.usage.unwrap() > 0.0);
        assert!(snap.avg.laps.unwrap() > 0.0);
        assert!(snap.add.unwrap() > 0.0);
        assert!(snap.strip.total > 0);
    }
}
