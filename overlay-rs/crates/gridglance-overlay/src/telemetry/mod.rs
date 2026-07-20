//! Telemetry frame shared by widgets + demo feed.

use serde::{Deserialize, Serialize};

mod format;
mod fuel;
mod irsdk;
mod lap_compare;
mod lap_log;
mod pit_advice;
mod pit_service;
mod pit_track;
mod sector_timer;
mod strategy_hints;
mod tables;

pub use fuel::{build_fuel_snapshot, FuelBurnTracker, FuelCalcState, FuelInputs, FuelScenario};
pub use irsdk::IrsdkReader;
pub use lap_compare::{CompareMarker, LapCompareState, LapCompareView, MarkerKind};
pub use lap_log::{signed_delta_1, LapExtras, LapLogAccum};
pub use pit_advice::PitAdvice;
pub use pit_service::{decode_flags as decode_pit_flags, PitService};
pub use pit_track::PitStopTracker;
pub use sector_timer::{SectorCell, SectorSnapshot, SectorTimer};
pub use tables::{
    finalize_frame, slot_label, RadarState, RelativeOrderHysteresis, TableRow, TableSlotItem,
    TableSlots,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CarRow {
    pub car_idx: i32,
    pub position: i32,
    pub class_position: i32,
    pub car_number: String,
    pub name: String,
    pub gap: String,
    pub last_lap: String,
    pub best_lap: String,
    pub irating: i32,
    pub irating_delta: Option<i32>,
    /// CarClassID from DriverInfo (0 when unknown).
    pub class_id: i32,
    pub license: String,
    pub class_color: String,
    pub on_pit: bool,
    pub in_pit: bool,
    pub on_track: bool,
    /// CarIdxTrackSurface == ApproachingPits (entry blend / commit).
    #[serde(default)]
    pub approaching_pits: bool,
    pub is_player: bool,
    pub is_speaking: bool,
    pub is_pace_car: bool,
    pub lapping: bool,
    pub lap_ahead: bool,
    pub inactive: bool,
    pub lap_dist_pct: f32,
    /// CarIdxEstTime (seconds into estimated lap).
    pub est_time: f32,
    /// CarIdxF2Time (gap-to-leader style clock).
    pub f2_time: f32,
    pub lap: i32,
    /// CarIdxLapCompleted (or Results LapsComplete when available).
    #[serde(default)]
    pub laps_completed: i32,
    /// CarIdxSpeed (m/s); 0 when unknown.
    #[serde(default)]
    pub speed_mps: f32,
    /// Map status badge: pit / off / garage / black / meatball / dq / furled.
    pub status_kind: Option<String>,
    /// Per-car session flag label for the table `car_flag` column (blue/meatball/…).
    #[serde(default)]
    pub car_flag: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryFrame {
    pub connected: bool,
    /// Car currently shown by the iRacing camera. Relative/Standings center on
    /// this car while spectating, without changing player telemetry semantics.
    #[serde(default)]
    pub camera_car_idx: Option<i32>,
    /// Player is seated in their car (`IsOnTrackCar`). False while spectating,
    /// in menus, or in the garage — drives the On track / In garage profile.
    #[serde(default)]
    pub in_car: bool,
    /// Player in garage or garage UI visible (`IsInGarage` / `IsGarageVisible`).
    #[serde(default)]
    pub in_garage: bool,
    pub session_time: f64,
    /// iRacing SessionState (4=racing, 5=checkered).
    pub session_state: i32,
    pub flag: Option<String>,
    pub flag_context: Option<String>,
    pub incident_warn: bool,
    pub secondary: Option<String>,
    /// Resolved delta for the configured `delta_bar.mode`.
    pub delta: Option<f64>,
    /// Raw SDK deltas (filled by IRSDK / demo); `finalize_frame` picks `delta`.
    #[serde(default)]
    pub delta_session_best: Option<f64>,
    #[serde(default)]
    pub delta_best_lap: Option<f64>,
    #[serde(default)]
    pub delta_optimal: Option<f64>,
    /// Weekend IncidentLimit (0 = unknown / unlimited).
    #[serde(default)]
    pub incidents_limit: i32,
    /// Engine pit-limiter bit active.
    #[serde(default)]
    pub pit_limiter: bool,
    pub speed_mps: f32,
    pub rpm: f32,
    pub redline: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steering: f32,
    pub abs_active: bool,
    pub fuel_l: f32,
    pub fuel_pct: f32,
    pub laps_fuel: f32,
    pub position: i32,
    pub car_number: String,
    pub lap: i32,
    pub laps_total: i32,
    /// Highest competitor lap in the field (lead lap).
    #[serde(default)]
    pub lead_lap: i32,
    pub incidents: i32,
    pub last_lap_s: Option<f64>,
    pub best_lap_s: Option<f64>,
    pub cur_lap_s: Option<f64>,
    pub irating: i32,
    pub irating_delta: Option<i32>,
    pub tire_wear_l: f32,
    pub tire_wear_r: f32,
    pub track_temp: Option<f32>,
    pub air_temp: Option<f32>,
    pub skies: Option<String>,
    pub humidity: Option<f32>,
    pub fog: Option<f32>,
    pub track_wetness: Option<f32>,
    pub rain_intensity: Option<f32>,
    pub wind_dir: Option<f32>,
    pub wind_vel: Option<f32>,
    pub cpu: Option<String>,
    pub mem: Option<String>,
    pub gpu: Option<String>,
    pub fps: Option<i32>,
    pub chan_quality: Option<f32>,
    pub ers_pct: Option<f32>,
    pub ers_mode: Option<String>,
    pub have_hybrid: bool,
    pub ers_battery_pct: Option<f32>,
    pub ers_boost_active: bool,
    pub ers_p2p_active: bool,
    pub cars: Vec<CarRow>,
    /// Pre-built relative / standings views (Python `set_data` rows).
    pub relative_cars: Vec<TableRow>,
    pub relative_slots: TableSlots,
    pub standings_cars: Vec<TableRow>,
    pub standings_slots: TableSlots,
    pub player_lap_dist_pct: f32,
    /// Estimated lap duration for EstTime wrap (LapEstTime).
    pub lap_est_time: f32,
    pub track_id: Option<i32>,
    /// iRacing LeagueID when in a league session.
    #[serde(default)]
    pub league_id: Option<i32>,
    /// Player car path for preset auto-switch.
    #[serde(default)]
    pub car_path: Option<String>,
    pub track_name: Option<String>,
    pub radar: RadarState,
    /// Legacy mirrors of `radar.left` / `radar.right`.
    pub radar_left: bool,
    pub radar_right: bool,
    pub sector_times: Vec<Option<f64>>,
    /// Live sector timing snapshot for the sector_timing widget.
    #[serde(default)]
    pub sectors_ui: SectorSnapshot,
    /// Lap-compare delta / spark / turn losses.
    #[serde(default)]
    pub lap_compare: LapCompareView,
    pub lap_log: Vec<LapLogRow>,
    pub tire_temps: [f32; 4],
    pub tire_pressures: [f32; 4],
    /// Per-corner wear / temp / pressure for tire_panel (lf, rf, lr, rr).
    #[serde(default)]
    pub tire_corners: [TireCorner; 4],
    pub pit_laps_to_go: Option<i32>,
    pub pit_fuel_to_add: Option<f32>,
    /// Requested pit services (from PitSvFlags).
    #[serde(default)]
    pub pit_services: Vec<PitService>,
    /// True when any service is requested / board should show.
    #[serde(default)]
    pub pit_active: bool,
    #[serde(default)]
    pub pit_compound: Option<i32>,
    /// Fuel to add (litres); preferred by pit_board over `pit_fuel_to_add`.
    #[serde(default)]
    pub pit_fuel_add_l: Option<f32>,
    #[serde(default)]
    pub pit_repairs: Option<i32>,
    pub radio_name: Option<String>,
    /// Active radio transmitter row (Python radio_tower `rows[0]`).
    pub radio: Option<RadioSpeaker>,
    /// Fuel calculator snapshot (Python fuel_calc `set_data`).
    pub fuel: FuelCalcState,
    /// Per-lap burn history (litres, newest first).
    #[serde(default)]
    pub fuel_use_history: Vec<f32>,
    /// DriverCarFuelMaxLtr when known.
    pub fuel_max_l: f32,
    pub fuel_use_per_hour: f32,
    pub session_laps_remain: Option<f32>,
    pub session_time_remain: Option<f32>,
    /// Pit engineer recommendation (filled in finalize_frame).
    #[serde(default)]
    pub pit_advice: Option<PitAdvice>,
    /// Sim clock seconds since midnight (`SessionTimeOfDay`).
    #[serde(default)]
    pub session_time_of_day: Option<f32>,
    /// Active session type label (Practice / Qualifying / Race).
    #[serde(default)]
    pub session_type: Option<String>,
    /// Registration split number when known (1-based).
    #[serde(default)]
    pub race_split: Option<i32>,
    /// Number of registration splits for this event.
    #[serde(default)]
    pub race_split_total: Option<i32>,
    /// Fast repairs already used this session.
    #[serde(default)]
    pub pit_repairs_used: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TireCorner {
    pub wear: Option<f32>,
    pub temp: Option<f32>,
    pub pressure: Option<f32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RadioSpeaker {
    pub position: i32,
    pub car_number: String,
    pub name: String,
    pub active: bool,
    pub is_player: bool,
    pub is_pro: bool,
    pub group_icon: String,
    pub group_color: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LapLogRow {
    pub lap: i32,
    pub time: String,
    /// Display delta (e.g. `"+0.2"`); kept for IPC / legacy consumers.
    pub delta: String,
    /// Signed seconds vs baseline; preferred for color (neg = faster).
    #[serde(default)]
    pub delta_s: Option<f32>,
    #[serde(default)]
    pub temp: String,
    #[serde(default)]
    pub fuel: Option<String>,
    #[serde(default)]
    pub tires: Option<String>,
    #[serde(default)]
    pub incidents: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
}

impl LapLogRow {
    pub fn from_parts(
        lap: i32,
        time: String,
        delta_s: Option<f32>,
        temp: String,
        fuel: Option<String>,
        tires: Option<String>,
        incidents: Option<String>,
        tag: Option<String>,
    ) -> Self {
        let delta = delta_s
            .map(lap_log::signed_delta_1)
            .unwrap_or_else(|| "—".into());
        Self {
            lap,
            time,
            delta,
            delta_s,
            temp,
            fuel,
            tires,
            incidents,
            tag,
        }
    }

    /// Numeric delta: prefer `delta_s`, else parse `delta` string.
    pub fn delta_seconds(&self) -> Option<f32> {
        self.delta_s
            .or_else(|| lap_log::parse_delta_str(&self.delta))
    }
}

pub mod demo {
    use super::*;

    /// Deterministic standings order for demo churn (car_idx at each rank, P1 first).
    /// Reorders about every 2s so relative/standings row animation can be tested offline.
    fn demo_field_order(t: f64, field_size: usize, player_idx: usize) -> Vec<usize> {
        let n = field_size.max(1);
        let phase = (t / 2.0).floor() as i64;
        let mut order: Vec<usize> = (0..n).collect();
        order.rotate_left((phase.rem_euclid(n as i64)) as usize);
        if n >= 2 {
            let a = ((phase as usize).wrapping_mul(3)) % (n - 1);
            order.swap(a, a + 1);
            let b = ((phase as usize).wrapping_mul(5).wrapping_add(1)) % (n - 1);
            if b != a {
                order.swap(b, b + 1);
            }
        }
        // Keep the player near midfield, nudging ±1 so centered tables still animate.
        let mid = n / 2;
        let nudge = (phase.rem_euclid(3) - 1) as isize;
        let player_slot = (mid as isize + nudge).clamp(0, (n as isize) - 1) as usize;
        if let Some(cur) = order.iter().position(|&c| c == player_idx) {
            order.swap(cur, player_slot);
        }
        order
    }

    /// 1-based race position for a demo car at time `t`.
    fn demo_position(car_idx: i32, t: f64, field_size: i32, player_idx: i32) -> i32 {
        let n = field_size.max(1) as usize;
        let order = demo_field_order(t, n, player_idx.max(0) as usize);
        let idx = (car_idx.max(0) as usize).min(n.saturating_sub(1));
        order.iter().position(|&c| c == idx).unwrap_or(idx) as i32 + 1
    }

    fn demo_flag(t: f64) -> (Option<String>, Option<String>, bool) {
        // (flag, context, incident_warn). Dense cycle ~48s matching Python demo.
        let cyc = t % 48.0;
        if cyc < 3.0 {
            (
                Some("yellow".into()),
                Some("Caution — pace car out".into()),
                false,
            )
        } else if cyc < 5.0 {
            (Some("yellow".into()), Some("1 lap to green".into()), false)
        } else if cyc < 8.0 {
            (
                Some("green".into()),
                Some("Track clear — race on".into()),
                false,
            )
        } else if cyc < 11.0 {
            (
                Some("red".into()),
                Some("Session stopped — stand by".into()),
                false,
            )
        } else if cyc < 14.0 {
            (Some("blue".into()), Some("Car #19 +1.2s".into()), false)
        } else if cyc < 17.0 {
            (
                Some("debris".into()),
                Some("Debris on track — reduce speed".into()),
                false,
            )
        } else if cyc < 20.0 {
            (
                Some("white".into()),
                Some("Lap 49 of 50 — finish this lap".into()),
                false,
            )
        } else if cyc < 23.0 {
            (
                Some("black".into()),
                Some("Report to the pits — penalty".into()),
                false,
            )
        } else if cyc < 26.0 {
            (
                Some("meatball".into()),
                Some("Mandatory pit — repairs required".into()),
                false,
            )
        } else if cyc < 29.0 {
            (
                Some("furled".into()),
                Some("Warning — next infraction is a penalty".into()),
                false,
            )
        } else if cyc < 32.0 {
            (
                Some("dq".into()),
                Some("Disqualified — exit the track".into()),
                false,
            )
        } else if cyc < 35.0 {
            (
                Some("crossed".into()),
                Some("Halfway — 25 laps to go".into()),
                false,
            )
        } else if cyc < 38.0 {
            (
                Some("checkered".into()),
                Some("Session complete — P4".into()),
                false,
            )
        } else if cyc < 41.0 {
            (
                Some("yellow".into()),
                Some("Caution — pace car out".into()),
                false,
            )
        } else if cyc < 44.0 {
            (None, None, true)
        } else {
            (
                Some("green".into()),
                Some("Track clear — race on".into()),
                false,
            )
        }
    }

    pub struct DemoFeed;

    impl DemoFeed {
        pub fn new() -> Self {
            Self
        }

        /// Tick using the shared host monotonic clock (keeps map prediction aligned).
        pub fn tick_at(&self, t: f64) -> TelemetryFrame {
            let lap_est = 90.0_f32;
            let player_i = 3i32;
            let field_size = 12i32;
            // Field order drives Relative est_time (list slides). Map pct stays
            // continuous by car_idx so dots don't teleport every phase.
            let order = demo_field_order(t, field_size as usize, player_i as usize);
            let player_rank = order
                .iter()
                .position(|&c| c == player_i as usize)
                .unwrap_or(0);
            const PLAYER_EST: f32 = 40.0;
            let mut cars = Vec::new();
            for i in 0..field_size {
                let rank = order
                    .iter()
                    .position(|&c| c == i as usize)
                    .unwrap_or(i as usize);
                // Map: smooth continuous spacing (pre–Relative-churn formula).
                let pct = ((t * 0.03 + i as f64 * 0.08).rem_euclid(1.0)) as f32;
                // Relative: pack by race rank so list reorders when field churns.
                // Positive delta vs player ⇒ ahead (build_relative).
                let est = PLAYER_EST + (player_rank as f32 - rank as f32) * 1.5;
                let pos = demo_position(i, t, field_size, player_i);
                let f2 = (pos - 1) as f32 * 0.85;
                let licenses = ["A 4.12", "B 3.80", "A 2.99", "A 3.50", "C 2.10", "B 4.00"];
                let last_s = 88.0 + (i as f64) * 0.11;
                // Car 8 visits the demo pit once per lap: OnPitRoad only on the
                // lane span (not entry/exit blends), matching Python demo.
                let visit_pit = i == 8 && ((t * 0.03 + i as f64 * 0.08) as i32) % 3 == 0;
                let on_pit_lane = visit_pit && crate::track_path::pct_in_demo_pit_lane(pct);
                cars.push(CarRow {
                    car_idx: i,
                    position: pos,
                    class_position: pos,
                    car_number: format!("{}", 10 + i),
                    name: format!("Driver {}", i + 1),
                    gap: if pos == 1 {
                        "LEADER".into()
                    } else {
                        format!("+{f2:.1}")
                    },
                    last_lap: format!("1:{:05.2}", last_s - 60.0),
                    best_lap: format!("1:{:05.2}", last_s - 60.2),
                    irating: 1800 + i * 80,
                    irating_delta: None,
                    class_id: 0,
                    license: licenses[i as usize % licenses.len()].into(),
                    class_color: "#3aa0ff".into(),
                    on_pit: on_pit_lane,
                    in_pit: false,
                    on_track: true,
                    approaching_pits: false,
                    is_player: i == player_i,
                    is_speaking: (t as i32 / 3) % 12 == i,
                    is_pace_car: false,
                    lapping: false,
                    lap_ahead: false,
                    inactive: false,
                    lap_dist_pct: pct,
                    est_time: est,
                    f2_time: f2,
                    lap: 12 - (i / 4),
                    laps_completed: (12 - (i / 4)).max(0),
                    speed_mps: 55.0 + (i as f32) * 1.5 + 3.0 * (t as f32 * 0.4).sin(),
                    status_kind: if on_pit_lane {
                        Some("pit".into())
                    } else {
                        None
                    },
                    car_flag: if i == 2 {
                        Some("blue".into())
                    } else if i == 5 {
                        Some("meatball".into())
                    } else {
                        None
                    },
                });
            }
            let player_pos = cars
                .iter()
                .find(|c| c.is_player)
                .map(|c| c.position)
                .unwrap_or(4);
            if let Some(p) = cars.iter_mut().find(|c| c.is_player) {
                p.car_number = "48".into();
                p.name = "L Troyer".into();
            }

            let player_pct = cars
                .iter()
                .find(|c| c.is_player)
                .map(|c| c.lap_dist_pct)
                .unwrap_or(0.0);
            let mut radar = RadarState {
                left: (t * 0.4).sin() > 0.55,
                right: (t * 0.37).cos() > 0.55,
                left2: false,
                right2: false,
                left_pos: 0.35,
                right_pos: 0.62,
                left_label: "#19".into(),
                right_label: "#5".into(),
                ahead: Some(1.4),
                behind: Some(0.9),
                clear_secs: None,
            };
            if radar.left && radar.right {
                radar.left2 = (t as i32 % 9) < 3;
                radar.right2 = (t as i32 % 11) < 3;
            }

            let speed_mps = 55.0 + 8.0 * (t * 0.7).sin() as f32;
            let rpm = 5200.0 + 800.0 * (t * 1.3).sin() as f32;
            let last_lap_s = Some(88.11);
            let best_lap_s = Some(87.90);
            let cur_lap_s = Some((t % lap_est as f64).max(1.0));
            let fuel_l = (18.0 - t as f32 * 0.12).max(1.5);
            let tire_temps = [85.0f32, 87.0, 82.0, 84.0];
            let tire_pressures = [179.0f32, 181.0, 175.0, 177.0];
            let tire_wear = [0.90f32, 0.86, 0.88, 0.84];
            let tire_corners = [
                TireCorner {
                    wear: Some(tire_wear[0]),
                    temp: Some(tire_temps[0]),
                    pressure: Some(tire_pressures[0]),
                },
                TireCorner {
                    wear: Some(tire_wear[1]),
                    temp: Some(tire_temps[1]),
                    pressure: Some(tire_pressures[1]),
                },
                TireCorner {
                    wear: Some(tire_wear[2]),
                    temp: Some(tire_temps[2]),
                    pressure: Some(tire_pressures[2]),
                },
                TireCorner {
                    wear: Some(tire_wear[3]),
                    temp: Some(tire_temps[3]),
                    pressure: Some(tire_pressures[3]),
                },
            ];
            let ers_pct = 55.0 + 25.0 * (t * 0.25).sin() as f32;
            let pit_flags = pit_service::LF_TIRE
                | pit_service::FUEL_FILL
                | pit_service::TEAROFF
                | pit_service::RF_TIRE;
            let mut pit_services = decode_pit_flags(pit_flags);
            if let Some(svc) = pit_services.iter_mut().find(|s| s.key == "rf_tire") {
                svc.checked = false;
            }

            let (flag, flag_context, incident_warn) = demo_flag(t);
            let secondary = if incident_warn {
                Some("Incidents 13/17".into())
            } else if (t as i32 % 14) < 2 {
                Some("Pit limiter active".into())
            } else {
                None
            };
            let d_sess = Some((t * 0.6).sin() * 0.45);
            let d_best = Some((t * 0.5).sin() * 0.35);
            let d_opt = Some((t * 0.5 + 0.5).sin() * 0.28);

            let radio_idx = ((t / 3.0) as i32).rem_euclid(12);
            let radio = cars
                .iter()
                .find(|c| c.car_idx == radio_idx)
                .map(|c| RadioSpeaker {
                    position: c.position,
                    car_number: c.car_number.clone(),
                    name: c.name.clone(),
                    active: true,
                    is_player: c.is_player,
                    is_pro: c.irating >= 4000,
                    group_icon: String::new(),
                    group_color: String::new(),
                });

            let lead_lap = cars
                .iter()
                .filter(|c| !c.is_pace_car && c.lap > 0)
                .map(|c| c.lap)
                .max()
                .unwrap_or(12);
            let laps_total = 50;
            let session_laps_remain = Some((laps_total - lead_lap).max(0) as f32);

            TelemetryFrame {
                connected: true,
                in_car: true,
                session_time: t,
                session_state: 4,
                flag,
                flag_context,
                incident_warn,
                secondary,
                delta: d_sess,
                delta_session_best: d_sess,
                delta_best_lap: d_best,
                delta_optimal: d_opt,
                incidents_limit: 17,
                pit_limiter: (t as i32 % 14) < 2,
                speed_mps,
                rpm,
                redline: 8000.0,
                gear: 6,
                throttle: 0.72 + 0.18 * (t * 1.1).sin() as f32,
                brake: ((t * 0.7).sin() as f32).max(0.0) * 0.15,
                clutch: 0.0,
                steering: 0.1,
                abs_active: false,
                fuel_l,
                fuel_pct: (fuel_l / 60.0).clamp(0.02, 1.0),
                laps_fuel: (fuel_l / 0.48).max(0.0),
                position: player_pos,
                car_number: "48".into(),
                lap: 12,
                laps_total,
                lead_lap,
                incidents: 13,
                last_lap_s,
                best_lap_s,
                cur_lap_s,
                irating: 2500,
                irating_delta: None,
                tire_wear_l: 0.90,
                tire_wear_r: 0.86,
                track_temp: Some(32.0 + (t * 0.05).sin() as f32),
                air_temp: Some(24.0),
                skies: Some("Partly Cloudy".into()),
                humidity: Some(48.0),
                fog: Some(0.0),
                track_wetness: Some(5.0 + (t as f32 * 0.02).min(25.0)),
                rain_intensity: Some((5.0 + 10.0 * (t * 0.02).sin() as f32).max(0.0)),
                wind_dir: Some(((t * 0.05) % std::f64::consts::TAU) as f32),
                wind_vel: Some(3.2),
                cpu: Some("34%".into()),
                mem: Some("42%".into()),
                gpu: Some("41%".into()),
                fps: Some(144),
                chan_quality: Some(92.0),
                ers_pct: Some(ers_pct),
                ers_mode: Some("Balanced".into()),
                have_hybrid: true,
                ers_battery_pct: Some(ers_pct),
                ers_boost_active: (t as i32 % 5) < 2,
                ers_p2p_active: (t as i32 % 7) < 2,
                player_lap_dist_pct: player_pct,
                lap_est_time: lap_est,
                track_id: Some(1),
                track_name: Some("Demo Speedpark".into()),
                session_time_of_day: Some(14.0 * 3600.0 + (t as f32 * 2.0) % 3600.0),
                session_type: Some("Race".into()),
                race_split: Some(2),
                race_split_total: Some(5),
                pit_repairs_used: Some(0),
                radar: radar.clone(),
                radar_left: radar.left,
                radar_right: radar.right,
                sector_times: vec![Some(28.1), Some(31.4), None],
                lap_log: vec![],
                tire_temps,
                tire_pressures,
                tire_corners,
                pit_services,
                pit_active: (t as i32 % 12) < 3,
                pit_fuel_add_l: Some(18.5),
                pit_compound: Some(1),
                pit_repairs: Some(2),
                pit_laps_to_go: Some(6),
                pit_fuel_to_add: Some(18.5),
                radio_name: radio.as_ref().map(|r| r.name.clone()),
                radio,
                cars,
                fuel_max_l: 60.0,
                fuel_use_per_hour: 48.0,
                session_laps_remain,
                session_time_remain: Some(3300.0 - t as f32 * 0.5),
                ..Default::default()
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{demo_field_order, demo_position, DemoFeed};
        use crate::config::OverlayConfig;
        use crate::telemetry::tables::build_relative;

        #[test]
        fn demo_positions_are_unique_and_churn() {
            let field = 12i32;
            let player = 3i32;
            let phase0: Vec<i32> = (0..field)
                .map(|i| demo_position(i, 0.0, field, player))
                .collect();
            let phase1: Vec<i32> = (0..field)
                .map(|i| demo_position(i, 2.1, field, player))
                .collect();

            let mut sorted0 = phase0.clone();
            sorted0.sort_unstable();
            assert_eq!(sorted0, (1..=field).collect::<Vec<_>>());

            let mut sorted1 = phase1.clone();
            sorted1.sort_unstable();
            assert_eq!(sorted1, (1..=field).collect::<Vec<_>>());

            assert_ne!(phase0, phase1, "positions should change across phases");

            let order = demo_field_order(0.0, field as usize, player as usize);
            let player_slot = order.iter().position(|&c| c == player as usize).unwrap();
            let mid = (field as usize) / 2;
            assert!((player_slot as i32 - mid as i32).abs() <= 1);
        }

        #[test]
        fn demo_relative_key_order_churns_across_phases() {
            let feed = DemoFeed::new();
            let cfg = OverlayConfig::default();
            let f0 = feed.tick_at(0.0);
            let f1 = feed.tick_at(2.1);
            let keys = |cars: &[super::super::CarRow]| -> Vec<String> {
                build_relative(cars, &cfg, 90.0, None, 4)
                    .into_iter()
                    .filter(|r| !r.empty)
                    .map(|r| r.key)
                    .collect()
            };
            let k0 = keys(&f0.cars);
            let k1 = keys(&f1.cars);
            assert!(
                k0.iter().any(|k| k == "3"),
                "player key present in relative at t=0"
            );
            assert_ne!(
                k0, k1,
                "Relative key order should change when demo field order churns"
            );
        }

        #[test]
        fn demo_lap_dist_pct_moves_continuously() {
            let feed = DemoFeed::new();
            let a = feed.tick_at(1.0);
            let b = feed.tick_at(1.05);
            let car = 5usize;
            let pct_a = a.cars[car].lap_dist_pct;
            let pct_b = b.cars[car].lap_dist_pct;
            let delta = (pct_b - pct_a).abs();
            // Continuous formula advances ~0.03 * 0.05 = 0.0015 per 50ms.
            // Rank jumps used to be ~0.02 — reject anything near that scale.
            assert!(
                delta > 0.0 && delta < 0.01,
                "lap_dist_pct should nudge continuously, got delta={delta} ({pct_a} -> {pct_b})"
            );
        }
    }
}
