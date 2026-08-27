//! Predictive strategy engines for pit advice.

mod fcy_model;
mod lap_down;
mod lift_coast;
mod pace_model;
mod service_time;
mod tire_energy;

pub use fcy_model::{FcyModel, FcySnapshot};
pub use lap_down::{project_lap_down, LapDownSnapshot};
pub use lift_coast::{LiftCoastSnapshot, LiftCoastTracker};
pub use pace_model::{PaceModel, PaceSnapshot};
pub use service_time::{best_service_plan, ServicePlan};
pub use tire_energy::{TireEnergySnapshot, TireEnergyTracker};

use serde::{Deserialize, Serialize};

/// Aggregated strategy signals filled each tick for the decision engine / UI.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategySnapshot {
    pub lap_down: LapDownSnapshot,
    pub pace: PaceSnapshot,
    pub tire: TireEnergySnapshot,
    pub coast: LiftCoastSnapshot,
    pub fcy: FcySnapshot,
    pub service: Option<ServicePlan>,
    /// Field context line (opponent due / splash).
    pub field_note: Option<String>,
    #[serde(default)]
    pub ahead_due: bool,
    #[serde(default)]
    pub ahead_splash: bool,
}
