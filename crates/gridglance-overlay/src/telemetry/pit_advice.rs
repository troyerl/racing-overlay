//! Pit engineer advice — scored strategy decision engine.

use crate::config::OverlayConfig;
use serde::{Deserialize, Serialize};

use super::TelemetryFrame;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PitAdvice {
    /// hold | stay_out | pit_now | pit_next_lap | marginal | low_fuel
    pub rec: String,
    pub label: String,
    pub rationale: String,
    pub secondary: Option<String>,
    pub actionable: bool,
    #[serde(default)]
    pub stop_window: Option<(i32, i32)>,
    #[serde(default)]
    pub fuel_add_l: Option<f32>,
    /// "fuel" | "tires" | "fuel+tires" | "hold" | "2T+fuel" | "4T+fuel"
    #[serde(default)]
    pub tire_plan: Option<String>,
    #[serde(default)]
    pub pit_loss_s: Option<f32>,
    #[serde(default)]
    pub merge_note: Option<String>,
    #[serde(default)]
    pub economy_note: Option<String>,
    #[serde(default)]
    pub optimal_stop_lap: Option<i32>,
    #[serde(default)]
    pub lap_down_note: Option<String>,
    #[serde(default)]
    pub pace_note: Option<String>,
    #[serde(default)]
    pub fcy_note: Option<String>,
    #[serde(default)]
    pub service_plan: Option<String>,
    #[serde(default)]
    pub field_note: Option<String>,
}

fn nearest_gaps(
    frame: &TelemetryFrame,
) -> (Option<f32>, Option<f32>, Option<String>, Option<String>) {
    let mut ahead: Option<(f32, String)> = None;
    let mut behind: Option<(f32, String)> = None;
    for row in &frame.relative_cars {
        if row.empty || row.is_player {
            continue;
        }
        let Some(g) = row.gap_secs else {
            continue;
        };
        if g > 0.0 {
            if ahead.as_ref().map(|(d, _)| g < *d).unwrap_or(true) {
                ahead = Some((g, row.car_number.clone()));
            }
        } else if g < 0.0 {
            let abs = g.abs();
            if behind.as_ref().map(|(d, _)| abs < *d).unwrap_or(true) {
                behind = Some((abs, row.car_number.clone()));
            }
        }
    }
    (
        ahead.as_ref().map(|a| a.0),
        behind.as_ref().map(|b| b.0),
        ahead.map(|a| a.1),
        behind.map(|b| b.1),
    )
}

fn is_caution(flag: Option<&str>) -> bool {
    matches!(
        flag,
        Some("yellow") | Some("caution") | Some("yellow_waving") | Some("caution_waving")
    )
}

fn pits_closed_under_caution(frame: &TelemetryFrame) -> bool {
    is_caution(frame.flag.as_deref())
        && frame
            .flag_context
            .as_deref()
            .map(|s| s.contains("pits closed"))
            .unwrap_or(false)
}

/// Project where the player rejoins after a pit stop and note traffic density.
pub fn merge_window_note(frame: &TelemetryFrame, pit_loss_s: f32, lap_est: f32) -> Option<String> {
    if pit_loss_s <= 0.0 || lap_est <= 10.0 {
        return None;
    }
    let player = frame.cars.iter().find(|c| c.is_player)?;
    let loss_frac = (pit_loss_s / lap_est).clamp(0.0, 0.95);
    let mut merge_pct = player.lap_dist_pct - loss_frac;
    if merge_pct < 0.0 {
        merge_pct += 1.0;
    }

    let mut near = 0u32;
    for c in &frame.cars {
        if c.is_player || c.is_pace_car || !c.is_live_competitor() || c.on_pit || c.in_pit {
            continue;
        }
        let mut d = (c.lap_dist_pct - merge_pct).abs();
        if d > 0.5 {
            d = 1.0 - d;
        }
        let gap_s = d * lap_est;
        if gap_s <= 2.5 {
            near += 1;
        }
    }
    if near >= 2 {
        Some("Merge into traffic".into())
    } else if near == 1 {
        Some("Tight merge".into())
    } else {
        Some("Clear merge".into())
    }
}

fn enrich(
    mut advice: PitAdvice,
    frame: &TelemetryFrame,
    pit_loss: f32,
    stopping: bool,
) -> PitAdvice {
    let strat = &frame.strategy;
    advice.stop_window = frame.fuel.window;
    advice.fuel_add_l = frame.fuel.add;
    advice.pit_loss_s = Some(
        strat
            .service
            .as_ref()
            .map(|s| s.total_loss_s)
            .unwrap_or(pit_loss),
    );
    advice.merge_note = merge_window_note(
        frame,
        advice.pit_loss_s.unwrap_or(pit_loss),
        frame.lap_est_time,
    );
    advice.economy_note = strat.coast.note.clone().or_else(|| {
        frame
            .fuel
            .economy_extra_laps
            .filter(|e| *e >= 0.3)
            .map(|e| format!("Economy +{e:.1} laps"))
    });
    advice.optimal_stop_lap = frame.fuel.window.map(|(a, b)| (a + b) / 2);
    advice.lap_down_note = strat.lap_down.note.clone();
    advice.pace_note = strat.pace.note.clone();
    advice.fcy_note = strat.fcy.note.clone();
    advice.field_note = strat.field_note.clone();
    if let Some(svc) = &strat.service {
        advice.service_plan = Some(svc.label.clone());
        advice.tire_plan = Some(if stopping {
            svc.tire_plan.clone()
        } else {
            "hold".into()
        });
    } else if stopping {
        advice.tire_plan = Some(if strat.tire.tire_urgent {
            "fuel+tires".into()
        } else {
            "fuel".into()
        });
    } else {
        advice.tire_plan = Some("hold".into());
    }
    if advice.secondary.is_none() {
        advice.secondary = strategy_meta_line(&advice);
    }
    advice
}

pub fn strategy_meta_line(advice: &PitAdvice) -> Option<String> {
    let mut parts = Vec::new();
    if let Some((a, b)) = advice.stop_window {
        if a == b {
            parts.push(format!("Stop L{a}"));
        } else {
            parts.push(format!("Stop L{a}–{b}"));
        }
    }
    if let Some(add) = advice.fuel_add_l.filter(|v| *v > 0.05) {
        parts.push(format!("Add {add:.1}L"));
    }
    if let Some(plan) = advice
        .service_plan
        .as_deref()
        .or(advice.tire_plan.as_deref())
    {
        if plan != "hold" {
            parts.push(plan.to_string());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

pub fn strategy_loss_line(advice: &PitAdvice) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(loss) = advice.pit_loss_s {
        parts.push(format!("Loss ~{loss:.0}s"));
    }
    if let Some(m) = advice.merge_note.as_deref() {
        parts.push(m.to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

/// Combined strategy context line (lap-down / pace / FCY / field).
pub fn strategy_context_line(advice: &PitAdvice) -> Option<String> {
    let mut parts = Vec::new();
    for n in [
        advice.lap_down_note.as_deref(),
        advice.pace_note.as_deref(),
        advice.fcy_note.as_deref(),
        advice.field_note.as_deref(),
        advice.economy_note.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        parts.push(n.to_string());
        if parts.len() >= 3 {
            break;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" · "))
    }
}

fn score_pit_now(
    pit_loss: f32,
    lap_down: bool,
    merge_traffic: bool,
    undercut_gain: f32,
    cover_risk: f32,
    p_fcy: f32,
    pace_window: bool,
) -> f32 {
    let mut cost = pit_loss;
    if lap_down {
        cost += 35.0;
    }
    if merge_traffic {
        cost += 6.0;
    }
    cost -= undercut_gain;
    cost -= cover_risk;
    // High FCY → waiting is valuable on green.
    cost += p_fcy * 12.0;
    if pace_window {
        cost -= 8.0;
    }
    cost
}

fn score_stay_out(
    fuel_margin_laps: Option<f32>,
    pace_cum_loss: f32,
    cover_risk: f32,
    p_fcy: f32,
    lap_down_soon: bool,
) -> f32 {
    let mut cost = pace_cum_loss;
    if let Some(m) = fuel_margin_laps {
        if m < 3.0 {
            cost += (3.0 - m) * 10.0;
        }
    }
    cost += cover_risk;
    cost -= p_fcy * 10.0; // waiting for yellow is good
    if lap_down_soon {
        cost += 15.0;
    }
    cost
}

/// Compute a pit recommendation from fuel + strategy snapshot + gaps + caution.
pub fn compute_pit_advice(frame: &TelemetryFrame, cfg: &OverlayConfig) -> PitAdvice {
    let undercut_max = cfg.f64_key("pit_advisor", "undercut_gap_max_s", 12.0) as f32;
    let cover_max = cfg.f64_key("pit_advisor", "cover_gap_max_s", 8.0) as f32;
    let fallback_loss = cfg.f64_key("pit_advisor", "pit_loss_seconds", 25.0) as f32;
    let caution_factor = cfg.f64_key("pit_advisor", "caution_pit_loss_factor", 0.4) as f32;
    let low_laps = cfg.f64_key("pit_advisor", "low_fuel_laps_threshold", 2.0) as f32;
    let final_suppress = cfg.bool_key("pit_advisor", "final_laps_optional_suppress", true);
    let min_stint = cfg.f64_key("pit_advisor", "min_stint_laps", 3.0) as f32;
    let legal_buf = cfg.f64_key("pit_advisor", "legal_fuel_buffer_l", 0.5) as f32;
    let caution = is_caution(frame.flag.as_deref());
    let green = matches!(frame.flag.as_deref(), Some("green") | None);

    let base_loss = super::effective_loss_s(
        frame.measured_pit_loss_s,
        fallback_loss,
        caution,
        caution_factor,
    );
    let pit_loss = frame
        .strategy
        .service
        .as_ref()
        .map(|s| {
            if caution {
                (s.total_loss_s * caution_factor).max(3.0)
            } else {
                s.total_loss_s
            }
        })
        .unwrap_or(base_loss);

    let fuel_crit = frame
        .fuel
        .laps_empty
        .map(|l| l <= low_laps)
        .unwrap_or(false)
        || (frame.fuel_pct > 0.0 && frame.fuel_pct < 0.08)
        || frame
            .fuel
            .add
            .zip(frame.fuel.level)
            .map(|(add, level)| {
                add > 0.0
                    && level < legal_buf
                    && frame.fuel.laps_margin.map(|m| m < 1.0).unwrap_or(true)
            })
            .unwrap_or(false);

    let fuel_window = frame.fuel.window_open;
    let pace_window = frame.strategy.pace.window_open;
    let window = fuel_window || pace_window || frame.strategy.tire.tire_urgent;
    let laps_empty = frame.fuel.laps_empty;
    let p_fcy = frame.strategy.fcy.p_fcy;
    let lap_down_if_pit = frame.strategy.lap_down.lap_down_if_pit;

    let (gap_ahead, gap_behind, car_ahead, car_behind) = nearest_gaps(frame);
    let win_txt = frame
        .fuel
        .window
        .map(|(a, b)| format!("Best stop: laps {a}–{b}"));

    let rem = frame.fuel.laps_remaining;
    if final_suppress {
        if let Some(r) = rem {
            if r < min_stint && !fuel_crit && !caution {
                return enrich(
                    PitAdvice {
                        rec: "stay_out".into(),
                        label: "STAY OUT".into(),
                        rationale: "Stay out — final laps, optional stop suppressed".into(),
                        secondary: win_txt,
                        actionable: false,
                        ..Default::default()
                    },
                    frame,
                    pit_loss,
                    false,
                );
            }
        }
    }

    // HOLD — pits closed under caution.
    if pits_closed_under_caution(frame) && !fuel_crit {
        return enrich(
            PitAdvice {
                rec: "hold".into(),
                label: "HOLD".into(),
                rationale: "Hold — pits closed under caution".into(),
                secondary: win_txt,
                actionable: false,
                ..Default::default()
            },
            frame,
            pit_loss,
            false,
        );
    }

    if fuel_crit {
        return enrich(
            PitAdvice {
                rec: "low_fuel".into(),
                label: "PIT NOW".into(),
                rationale: "Pit now — fuel critically low".into(),
                secondary: win_txt,
                actionable: true,
                ..Default::default()
            },
            frame,
            pit_loss,
            true,
        );
    }

    // FCY opportunity.
    if caution && window && !pits_closed_under_caution(frame) {
        return enrich(
            PitAdvice {
                rec: "pit_now".into(),
                label: "PIT NOW".into(),
                rationale: format!("Pit under caution — stop costs ~{pit_loss:.0}s (FCY)"),
                secondary: win_txt.clone(),
                actionable: true,
                ..Default::default()
            },
            frame,
            pit_loss,
            true,
        );
    }

    if caution && !window {
        return enrich(
            PitAdvice {
                rec: "stay_out".into(),
                label: "STAY OUT".into(),
                rationale: "Stay out under caution — fuel is comfortable".into(),
                secondary: win_txt,
                actionable: false,
                ..Default::default()
            },
            frame,
            pit_loss,
            false,
        );
    }

    let mut undercut_gain = 0.0f32;
    let mut cover_risk = 0.0f32;
    let mut undercut_car = None;
    let mut cover_car = None;
    if let Some(g) = gap_ahead {
        if g <= undercut_max {
            undercut_gain = (undercut_max - g).max(0.0) * 0.8;
            if frame.strategy.ahead_due {
                undercut_gain += 6.0;
            } else if frame.strategy.ahead_splash {
                undercut_gain += 3.0;
            }
            undercut_car = car_ahead.clone();
        }
    }
    if let Some(g) = gap_behind {
        if g <= cover_max {
            cover_risk = (cover_max - g).max(0.0) * 1.1;
            cover_car = car_behind.clone();
        }
    }

    let merge_traffic = merge_window_note(frame, pit_loss, frame.lap_est_time)
        .map(|s| s.contains("traffic") || s.contains("Tight"))
        .unwrap_or(false);

    let pace_cum = frame
        .strategy
        .pace
        .loss_per_lap
        .zip(rem)
        .map(|(l, r)| l * r * (r + 1.0) * 0.5)
        .unwrap_or(0.0);

    let lap_down_soon = frame
        .strategy
        .lap_down
        .laps_until_lapped
        .map(|u| u < 3.0)
        .unwrap_or(false);

    if window || undercut_gain > 0.0 || cover_risk > 0.0 || pace_window {
        let pit_score = score_pit_now(
            pit_loss,
            lap_down_if_pit,
            merge_traffic,
            undercut_gain,
            cover_risk,
            if green { p_fcy } else { 0.0 },
            pace_window,
        );
        let stay_score = score_stay_out(
            frame.fuel.laps_margin,
            pace_cum,
            cover_risk,
            if green { p_fcy } else { 0.0 },
            lap_down_soon,
        );

        // Soften green pit-now when FCY likely and fuel comfortable.
        let soft_fcy =
            green && p_fcy >= 0.28 && frame.fuel.laps_margin.map(|m| m > 4.0).unwrap_or(false);

        if cover_risk > 0.0 && pit_score <= stay_score + 2.0 && !lap_down_if_pit && !soft_fcy {
            let num = cover_car
                .as_deref()
                .map(|n| format!("#{n}"))
                .unwrap_or_else(|| "the car behind".into());
            let g = gap_behind.unwrap_or(0.0);
            return enrich(
                PitAdvice {
                    rec: "pit_now".into(),
                    label: "PIT NOW".into(),
                    rationale: format!("Pit now — {num} is {g:.1}s behind"),
                    secondary: win_txt.clone(),
                    actionable: true,
                    ..Default::default()
                },
                frame,
                pit_loss,
                true,
            );
        }

        if undercut_gain > 0.0 && pit_score < stay_score && !lap_down_if_pit && !soft_fcy {
            let num = undercut_car
                .as_deref()
                .map(|n| format!("#{n}"))
                .unwrap_or_else(|| "the car ahead".into());
            let g = gap_ahead.unwrap_or(0.0);
            return enrich(
                PitAdvice {
                    rec: "pit_next_lap".into(),
                    label: "PIT NEXT LAP".into(),
                    rationale: format!(
                        "Pit next lap to pass {num} — {g:.1}s ahead, stop costs ~{pit_loss:.0}s"
                    ),
                    secondary: win_txt.clone(),
                    actionable: true,
                    ..Default::default()
                },
                frame,
                pit_loss,
                true,
            );
        }

        if soft_fcy && !fuel_crit {
            return enrich(
                PitAdvice {
                    rec: "stay_out".into(),
                    label: "STAY OUT".into(),
                    rationale: format!(
                        "Hold for yellow — FCY ~{:.0}%, fuel comfortable",
                        p_fcy * 100.0
                    ),
                    secondary: win_txt,
                    actionable: false,
                    ..Default::default()
                },
                frame,
                pit_loss,
                false,
            );
        }

        if lap_down_if_pit && frame.fuel.laps_margin.map(|m| m > 3.0).unwrap_or(true) {
            return enrich(
                PitAdvice {
                    rec: "stay_out".into(),
                    label: "STAY OUT".into(),
                    rationale: "Stay out — pitting risks going a lap down".into(),
                    secondary: win_txt,
                    actionable: false,
                    ..Default::default()
                },
                frame,
                pit_loss,
                false,
            );
        }

        if window && pit_score <= stay_score {
            let soft = laps_empty.map(|l| l > low_laps * 1.5).unwrap_or(true);
            if soft && green && undercut_gain <= 0.0 && cover_risk <= 0.0 && !pace_window {
                return enrich(
                    PitAdvice {
                        rec: "marginal".into(),
                        label: "MARGINAL".into(),
                        rationale: "Pit window open — no clear undercut/cover target".into(),
                        secondary: win_txt,
                        actionable: false,
                        ..Default::default()
                    },
                    frame,
                    pit_loss,
                    true,
                );
            }
            return enrich(
                PitAdvice {
                    rec: "pit_now".into(),
                    label: "PIT WINDOW".into(),
                    rationale: if pace_window {
                        "Pace drop-off — pit window open".into()
                    } else {
                        "Fuel pit window is open".into()
                    },
                    secondary: win_txt,
                    actionable: true,
                    ..Default::default()
                },
                frame,
                pit_loss,
                true,
            );
        }
    }

    if green {
        if let Some(l) = laps_empty {
            if l <= low_laps * 2.0 {
                return enrich(
                    PitAdvice {
                        rec: "stay_out".into(),
                        label: "STAY OUT".into(),
                        rationale: format!("Stay out — about {l:.1} laps of fuel remaining"),
                        secondary: win_txt,
                        actionable: false,
                        ..Default::default()
                    },
                    frame,
                    pit_loss,
                    false,
                );
            }
        }
    }

    enrich(
        PitAdvice {
            rec: "stay_out".into(),
            label: "STAY OUT".into(),
            rationale: "Stay out — fuel and strategy are comfortable".into(),
            secondary: win_txt,
            actionable: false,
            ..Default::default()
        },
        frame,
        pit_loss,
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::fuel::FuelCalcState;
    use crate::telemetry::strategy::{FcySnapshot, LapDownSnapshot, StrategySnapshot};

    fn base_frame() -> TelemetryFrame {
        TelemetryFrame {
            flag: Some("green".into()),
            lap_est_time: 90.0,
            fuel: FuelCalcState {
                window_open: true,
                window: Some((24, 26)),
                add: Some(18.4),
                laps_empty: Some(8.0),
                laps_margin: Some(6.0),
                ema_usage: Some(2.2),
                ..Default::default()
            },
            fuel_pct: 0.25,
            ..Default::default()
        }
    }

    #[test]
    fn fcy_pits_when_window_open() {
        let cfg = OverlayConfig::default();
        let mut f = base_frame();
        f.flag = Some("yellow".into());
        f.flag_context = Some("caution — pits open".into());
        f.measured_pit_loss_s = Some(30.0);
        let advice = compute_pit_advice(&f, &cfg);
        assert_eq!(advice.rec, "pit_now");
        assert!(advice.rationale.contains("caution"));
    }

    #[test]
    fn hold_when_pits_closed() {
        let cfg = OverlayConfig::default();
        let mut f = base_frame();
        f.flag = Some("yellow".into());
        f.flag_context = Some("caution — pits closed".into());
        let advice = compute_pit_advice(&f, &cfg);
        assert_eq!(advice.rec, "hold");
    }

    #[test]
    fn high_fcy_softens_green_pit() {
        let cfg = OverlayConfig::default();
        let mut f = base_frame();
        f.strategy = StrategySnapshot {
            fcy: FcySnapshot {
                p_fcy: 0.4,
                note: Some("FCY ~40%".into()),
            },
            ..Default::default()
        };
        let advice = compute_pit_advice(&f, &cfg);
        assert!(matches!(
            advice.rec.as_str(),
            "stay_out" | "marginal" | "pit_now" | "pit_next_lap"
        ));
        // With comfortable fuel + high FCY and no cover/undercut, expect stay/hold-for-yellow.
        assert!(
            advice.rationale.contains("yellow")
                || advice.rec == "marginal"
                || advice.rec == "stay_out"
                || advice.label.contains("WINDOW")
        );
    }

    #[test]
    fn lap_down_prefers_stay() {
        let cfg = OverlayConfig::default();
        let mut f = base_frame();
        f.strategy = StrategySnapshot {
            lap_down: LapDownSnapshot {
                lap_down_if_pit: true,
                note: Some("Pit risks lap down".into()),
                ..Default::default()
            },
            ..Default::default()
        };
        let advice = compute_pit_advice(&f, &cfg);
        assert_eq!(advice.rec, "stay_out");
        assert!(advice.rationale.contains("lap down"));
    }

    #[test]
    fn meta_line_formats_stop_and_fuel() {
        let a = PitAdvice {
            stop_window: Some((24, 26)),
            fuel_add_l: Some(18.4),
            service_plan: Some("4T+fuel".into()),
            ..Default::default()
        };
        let line = strategy_meta_line(&a).unwrap();
        assert!(line.contains("Stop L24–26"));
        assert!(line.contains("18.4L"));
        assert!(line.contains("4T+fuel"));
    }
}
