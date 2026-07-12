//! Telemetry frame shared by widgets + demo feed.

use serde::{Deserialize, Serialize};

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
    pub license: String,
    pub on_pit: bool,
    pub is_player: bool,
    pub is_speaking: bool,
    pub lap_dist_pct: f32,
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
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub steering: f32,
    pub fuel_l: f32,
    pub fuel_pct: f32,
    pub laps_fuel: f32,
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
    pub player_lap_dist_pct: f32,
    pub track_id: Option<i32>,
    pub track_name: Option<String>,
    /// Relative gaps ahead/behind for radar.
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
            let mut cars = Vec::new();
            for i in 0..12 {
                let pct = ((t * 0.03 + i as f64 * 0.08) % 1.0) as f32;
                cars.push(CarRow {
                    car_idx: i,
                    position: i + 1,
                    class_position: i + 1,
                    car_number: format!("{}", 10 + i),
                    name: format!("Driver {}", i + 1),
                    gap: if i == 0 {
                        "—".into()
                    } else {
                        format!("+{:.1}", i as f64 * 0.42)
                    },
                    last_lap: "1:28.442".into(),
                    best_lap: "1:27.901".into(),
                    irating: 1800 + i * 50,
                    license: "A".into(),
                    on_pit: i == 5,
                    is_player: i == 3,
                    is_speaking: i == 2 && ((t as i32) % 4 == 0),
                    lap_dist_pct: pct,
                });
            }
            TelemetryFrame {
                connected: true,
                session_time: t,
                flag: if ((t as i32) / 20) % 5 == 0 {
                    Some("yellow".into())
                } else {
                    Some("green".into())
                },
                flag_context: None,
                incident_warn: false,
                secondary: None,
                delta: Some(((t * 0.7).sin()) * 0.35),
                speed_mps: 55.0 + (t.sin() as f32) * 8.0,
                rpm: 6200.0 + (t * 2.0).sin() as f32 * 400.0,
                gear: 4,
                throttle: (0.5 + 0.5 * (t * 1.3).sin()) as f32,
                brake: (0.2 + 0.2 * (t * 0.9).cos()).max(0.0) as f32 * 0.3,
                clutch: 0.0,
                steering: (t * 0.8).sin() as f32 * 0.4,
                fuel_l: 42.0 - (t as f32 * 0.02),
                fuel_pct: 0.55,
                laps_fuel: 8.4,
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
                player_lap_dist_pct: cars[3].lap_dist_pct,
                track_id: Some(1),
                track_name: Some("Demo Circuit".into()),
                radar_left: (t as i32 % 7) < 2,
                radar_right: (t as i32 % 11) < 2,
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
                radio_name: if ((t as i32) % 4 == 0) {
                    Some("Driver 3".into())
                } else {
                    None
                },
                cars,
            }
        }
    }
}

/// Placeholder IRSDK reader — returns disconnected until Windows shared-memory
/// binding is wired. Demo mode uses [`demo::DemoFeed`] instead.
pub struct IrsdkReader;

impl IrsdkReader {
    pub fn new() -> Self {
        Self
    }

    pub fn tick(&mut self) -> TelemetryFrame {
        TelemetryFrame {
            connected: false,
            ..Default::default()
        }
    }
}
