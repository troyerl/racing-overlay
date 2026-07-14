//! Pit engineer advice (Python `pit_strategy.evaluate_pit_strategy` fuel/gap/caution slice).

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
}

fn nearest_gaps(frame: &TelemetryFrame) -> (Option<f32>, Option<f32>, Option<String>, Option<String>) {
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

/// Compute a pit recommendation from fuel + relative gaps + caution.
pub fn compute_pit_advice(frame: &TelemetryFrame, cfg: &OverlayConfig) -> PitAdvice {
    let undercut_max = cfg.f64_key("pit_advisor", "undercut_gap_max_s", 12.0) as f32;
    let cover_max = cfg.f64_key("pit_advisor", "cover_gap_max_s", 8.0) as f32;
    let pit_loss = cfg.f64_key("pit_advisor", "pit_loss_seconds", 25.0);
    let low_laps = cfg.f64_key("pit_advisor", "low_fuel_laps_threshold", 2.0) as f32;
    let caution = is_caution(frame.flag.as_deref());
    let green = matches!(frame.flag.as_deref(), Some("green") | None);

    let fuel_crit = frame
        .fuel
        .laps_empty
        .map(|l| l <= low_laps)
        .unwrap_or(false)
        || (frame.fuel_pct > 0.0 && frame.fuel_pct < 0.08);
    let window = frame.fuel.window_open;
    let laps_empty = frame.fuel.laps_empty;

    let (gap_ahead, gap_behind, car_ahead, car_behind) = nearest_gaps(frame);
    let win_txt = frame.fuel.window.map(|(a, b)| format!("Best stop: laps {a}–{b}"));

    // HOLD — pits closed under caution (Python PRIORITY).
    if pits_closed_under_caution(frame) && !fuel_crit {
        return PitAdvice {
            rec: "hold".into(),
            label: "HOLD".into(),
            rationale: "Hold — pits closed under caution".into(),
            secondary: win_txt,
            actionable: false,
        };
    }

    if fuel_crit {
        return PitAdvice {
            rec: "low_fuel".into(),
            label: "PIT NOW".into(),
            rationale: "Pit now — fuel critically low".into(),
            secondary: win_txt,
            actionable: true,
        };
    }

    // Under yellow without a hard window: stay out.
    if caution && !window {
        return PitAdvice {
            rec: "stay_out".into(),
            label: "STAY OUT".into(),
            rationale: "Stay out under caution — fuel is comfortable".into(),
            secondary: win_txt,
            actionable: false,
        };
    }

    if window {
        if let Some(g) = gap_behind {
            if g <= cover_max {
                let num = car_behind
                    .as_deref()
                    .map(|n| format!("#{n}"))
                    .unwrap_or_else(|| "the car behind".into());
                return PitAdvice {
                    rec: "pit_now".into(),
                    label: "PIT NOW".into(),
                    rationale: format!("Pit now — {num} is {g:.1}s behind"),
                    secondary: win_txt.clone(),
                    actionable: true,
                };
            }
        }
        if let Some(g) = gap_ahead {
            if g <= undercut_max {
                let num = car_ahead
                    .as_deref()
                    .map(|n| format!("#{n}"))
                    .unwrap_or_else(|| "the car ahead".into());
                return PitAdvice {
                    rec: "pit_next_lap".into(),
                    label: "PIT NEXT LAP".into(),
                    rationale: format!(
                        "Pit next lap to pass {num} — {g:.1}s ahead, stop costs ~{pit_loss:.0}s"
                    ),
                    secondary: win_txt.clone(),
                    actionable: true,
                };
            }
        }

        // MARGINAL — window open but no clear cover/undercut target.
        let soft = laps_empty.map(|l| l > low_laps * 1.5).unwrap_or(true);
        if soft && green {
            return PitAdvice {
                rec: "marginal".into(),
                label: "MARGINAL".into(),
                rationale: "Pit window open — no clear undercut/cover target".into(),
                secondary: win_txt,
                actionable: false,
            };
        }

        return PitAdvice {
            rec: "pit_now".into(),
            label: "PIT WINDOW".into(),
            rationale: "Fuel pit window is open".into(),
            secondary: win_txt,
            actionable: true,
        };
    }

    // Green resume: stay quiet unless fuel is tightening.
    if green {
        if let Some(l) = laps_empty {
            if l <= low_laps * 2.0 {
                return PitAdvice {
                    rec: "stay_out".into(),
                    label: "STAY OUT".into(),
                    rationale: format!("Stay out — about {l:.1} laps of fuel remaining"),
                    secondary: win_txt,
                    actionable: false,
                };
            }
        }
    }

    PitAdvice {
        rec: "stay_out".into(),
        label: "STAY OUT".into(),
        rationale: "Stay out — fuel and strategy are comfortable".into(),
        secondary: win_txt,
        actionable: false,
    }
}
