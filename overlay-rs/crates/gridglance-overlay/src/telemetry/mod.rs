//! Telemetry frame shared by widgets + demo feed.

use serde::{Deserialize, Serialize};

mod irsdk;
mod tables;

pub use irsdk::IrsdkReader;
pub use tables::{finalize_frame, RadarState, TableRow, TableSlots};

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
    pub license: String,
    pub class_color: String,
    pub on_pit: bool,
    pub in_pit: bool,
    pub on_track: bool,
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelemetryFrame {
    pub connected: bool,
    pub session_time: f64,
    pub flag: Option<String>,
    pub flag_context: Option<String>,
    pub incident_warn: bool,
    pub secondary: Option<String>,
    pub delta: Option<f64>,
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
    pub chan_latency: Option<f32>,
    pub ers_pct: Option<f32>,
    pub ers_mode: Option<String>,
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
    pub track_name: Option<String>,
    pub radar: RadarState,
    /// Legacy mirrors of `radar.left` / `radar.right`.
    pub radar_left: bool,
    pub radar_right: bool,
    pub sector_times: Vec<Option<f64>>,
    pub lap_log: Vec<LapLogRow>,
    pub tire_temps: [f32; 4],
    pub tire_pressures: [f32; 4],
    pub pit_laps_to_go: Option<i32>,
    pub pit_fuel_to_add: Option<f32>,
    pub radio_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LapLogRow {
    pub lap: i32,
    pub time: String,
    pub delta: String,
}

pub mod demo {
    use super::*;
    use std::time::Instant;

    pub struct DemoFeed {
        start: Instant,
    }

    impl DemoFeed {
        pub fn new() -> Self {
            Self {
                start: Instant::now(),
            }
        }

        pub fn tick(&self) -> TelemetryFrame {
            let t = self.start.elapsed().as_secs_f64();
            let lap_est = 90.0_f32;
            let player_i = 3i32;
            let mut cars = Vec::new();
            for i in 0..12 {
                let pct = ((t * 0.03 + i as f64 * 0.08) % 1.0) as f32;
                let est = (pct * lap_est + (t as f32 * 0.15)) % lap_est;
                let f2 = i as f32 * 0.85;
                let licenses = ["A 4.12", "B 3.80", "A 2.99", "A 3.50", "C 2.10", "B 4.00"];
                cars.push(CarRow {
                    car_idx: i,
                    position: i + 1,
                    class_position: i + 1,
                    car_number: format!("{}", 10 + i),
                    name: format!("Driver {}", i + 1),
                    gap: if i == 0 {
                        "—".into()
                    } else {
                        format!("+{:.1}", f2)
                    },
                    last_lap: "1:28.442".into(),
                    best_lap: "1:27.901".into(),
                    irating: 1800 + i * 50,
                    irating_delta: if i == player_i { Some(-28) } else { None },
                    license: licenses[i as usize % licenses.len()].into(),
                    class_color: "#2f6bd8".into(),
                    on_pit: i == 5,
                    in_pit: i == 5,
                    on_track: i != 5,
                    is_player: i == player_i,
                    is_speaking: i == 2 && ((t as i32) % 4 == 0),
                    is_pace_car: false,
                    lapping: i == 8,
                    lap_ahead: false,
                    inactive: false,
                    lap_dist_pct: pct,
                    est_time: est,
                    f2_time: f2,
                    lap: 12 + (i % 3),
                });
            }

            // Animate a car sliding past on the left, then right.
            let phase = (t * 0.35) % 6.0;
            let mut radar = RadarState::default();
            if phase < 2.0 {
                radar.left = true;
                radar.left2 = phase > 1.2;
                radar.left_pos = ((phase / 2.0) * 2.0 - 1.0) as f32;
            } else if phase < 4.0 {
                radar.right = true;
                radar.right2 = phase > 3.2;
                radar.right_pos = (1.0 - ((phase - 2.0) / 2.0) * 2.0) as f32;
            }
            // Front/rear closeness pulses.
            let pulse = ((t * 0.8).sin() as f32 * 0.5 + 0.5).clamp(0.0, 1.0);
            if (t as i32 % 5) < 2 {
                radar.ahead = Some(pulse * 0.85);
            }
            if (t as i32 % 7) < 2 {
                radar.behind = Some((1.0 - pulse) * 0.7);
            }

            let rpm = 5600.0 + (t * 0.6).sin() as f32 * 400.0;
            let speed_mps = 147.0 / 2.236_936_3;
            let fuel_l = 15.5 / 0.264_172_05;
            TelemetryFrame {
                connected: true,
                session_time: t,
                flag: Some("white".into()),
                flag_context: Some("Lap 2 of 50 — finish this lap".into()),
                incident_warn: false,
                secondary: None,
                delta: Some(-0.12),
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
                fuel_pct: 0.55,
                laps_fuel: 37.5,
                position: 4,
                car_number: "48".into(),
                lap: 2,
                laps_total: 50,
                incidents: 11,
                last_lap_s: Some(88.442),
                best_lap_s: Some(87.901),
                cur_lap_s: Some(42.1),
                irating: 2500,
                irating_delta: Some(-28),
                tire_wear_l: 0.90,
                tire_wear_r: 0.86,
                track_temp: Some(32.0 + (t * 0.05).sin() as f32),
                air_temp: Some(24.0),
                skies: Some("Partly Cloudy".into()),
                humidity: Some(48.0),
                fog: Some(0.0),
                track_wetness: Some(5.0),
                rain_intensity: Some(0.0),
                wind_dir: Some(210.0),
                wind_vel: Some(3.2),
                cpu: Some("34%".into()),
                mem: Some("11.2 GB".into()),
                gpu: Some("41%".into()),
                fps: Some(144),
                chan_quality: Some(92.0),
                chan_latency: Some(28.0),
                ers_pct: Some(62.0),
                ers_mode: Some("Balanced".into()),
                player_lap_dist_pct: cars[player_i as usize].lap_dist_pct,
                lap_est_time: lap_est,
                track_id: Some(1),
                track_name: Some("Demo Circuit".into()),
                radar: radar.clone(),
                radar_left: radar.left,
                radar_right: radar.right,
                sector_times: vec![Some(28.1), Some(31.4), None],
                lap_log: vec![
                    LapLogRow {
                        lap: 4,
                        time: "1:28.112".into(),
                        delta: "-0.12".into(),
                    },
                    LapLogRow {
                        lap: 3,
                        time: "1:28.401".into(),
                        delta: "+0.17".into(),
                    },
                    LapLogRow {
                        lap: 2,
                        time: "1:28.230".into(),
                        delta: "+0.00".into(),
                    },
                ],
                tire_temps: [78.0, 81.0, 79.0, 82.0],
                tire_pressures: [27.1, 27.0, 26.9, 27.2],
                pit_laps_to_go: Some(6),
                pit_fuel_to_add: Some(18.5),
                radio_name: if (t as i32) % 4 == 0 {
                    Some("Driver 3".into())
                } else {
                    None
                },
                cars,
                ..Default::default()
            }
        }
    }
}
