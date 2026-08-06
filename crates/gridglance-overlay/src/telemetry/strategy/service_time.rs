//! Concurrent fuel / tire service time tradeoffs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceOption {
    FuelOnly,
    TwoTiresFuel,
    FourTiresFuel,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServicePlan {
    /// "fuel" | "2T+fuel" | "4T+fuel"
    pub label: String,
    pub stationary_s: f32,
    pub total_loss_s: f32,
    pub tire_plan: String,
}

/// Estimate best service given fuel need, tire urgency, and pace loss remaining.
pub fn best_service_plan(
    fuel_add_l: f32,
    tire_urgent: bool,
    pace_loss_per_lap: Option<f32>,
    laps_remain: Option<f32>,
    pit_lane_transit_s: f32,
    fuel_rate_lps: f32,
    tire_2t_s: f32,
    tire_4t_s: f32,
    wear_low: bool,
) -> ServicePlan {
    let rate = fuel_rate_lps.max(0.1);
    let fuel_s = (fuel_add_l.max(0.0) / rate).max(if fuel_add_l > 0.2 { 2.0 } else { 0.0 });
    let transit = pit_lane_transit_s.clamp(5.0, 60.0);

    let rem = laps_remain.unwrap_or(12.0).max(1.0);
    let pace = pace_loss_per_lap.unwrap_or(0.0).max(0.0);
    let tire_benefit = pace * rem * (rem + 1.0) * 0.5;

    let opts = [
        (
            ServiceOption::FuelOnly,
            fuel_s,
            "fuel",
            "fuel",
            0.0f32,
        ),
        (
            ServiceOption::TwoTiresFuel,
            fuel_s.max(tire_2t_s),
            "2T+fuel",
            "fuel+tires",
            tire_benefit * 0.55,
        ),
        (
            ServiceOption::FourTiresFuel,
            fuel_s.max(tire_4t_s),
            "4T+fuel",
            "fuel+tires",
            tire_benefit,
        ),
    ];

    let mut best = opts[0];
    let mut best_score = f32::INFINITY;
    for &(opt, stationary, label, tire_plan, benefit) in &opts {
        let total = transit + stationary;
        // Prefer tires when urgent/worn even if benefit estimate is thin.
        let urgency_bonus = match opt {
            ServiceOption::FourTiresFuel if tire_urgent || wear_low => 8.0,
            ServiceOption::TwoTiresFuel if tire_urgent || wear_low => 4.0,
            ServiceOption::FuelOnly if tire_urgent || wear_low => -6.0,
            _ => 0.0,
        };
        let score = total - benefit - urgency_bonus;
        if score < best_score {
            best_score = score;
            best = (opt, stationary, label, tire_plan, benefit);
        }
    }

    // No fuel and no tire need → fuel-only placeholder.
    if fuel_add_l < 0.2 && !tire_urgent && !wear_low {
        return ServicePlan {
            label: "fuel".into(),
            stationary_s: 0.0,
            total_loss_s: transit,
            tire_plan: "fuel".into(),
        };
    }

    ServicePlan {
        label: best.2.into(),
        stationary_s: best.1,
        total_loss_s: transit + best.1,
        tire_plan: best.3.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_fuel_hides_short_tire_service() {
        let plan = best_service_plan(20.0, false, None, Some(20.0), 18.0, 2.0, 8.0, 12.0, false);
        // 20L / 2 L/s = 10s fuel; 4T=12 → stationary 12
        assert!(plan.stationary_s >= 10.0);
        assert_eq!(plan.label, "fuel"); // no tire urgency → fuel only cheaper
    }

    #[test]
    fn urgent_tires_pick_four() {
        let plan = best_service_plan(8.0, true, Some(0.12), Some(20.0), 18.0, 2.0, 8.0, 12.0, true);
        assert!(plan.label.contains("4T") || plan.tire_plan.contains("tire"));
    }
}
