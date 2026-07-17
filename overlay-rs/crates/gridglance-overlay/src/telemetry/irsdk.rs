//! Live iRacing shared-memory reader (Windows). Non-Windows uses a stub.

#[cfg(windows)]
mod win {
    use crate::telemetry::{
        any_requested, decode_pit_flags, CarRow, RadarState, RadioSpeaker, TelemetryFrame,
        TireCorner,
    };
    use iracing_telem::{flags, Client, DataUpdateResult, Session, Value};
    use std::collections::HashMap;

    // SessionFlags bits (match overlay/app.py)
    const FLAG_CHECKERED: i32 = 0x0000_0001;
    const FLAG_WHITE: i32 = 0x0000_0002;
    const FLAG_GREEN: i32 = 0x0000_0004;
    const FLAG_YELLOW: i32 = 0x0000_0008;
    const FLAG_RED: i32 = 0x0000_0010;
    const FLAG_BLUE: i32 = 0x0000_0020;
    const FLAG_DEBRIS: i32 = 0x0000_0040;
    const FLAG_CROSSED: i32 = 0x0000_0080;
    const FLAG_YELLOW_WAVING: i32 = 0x0000_0100;
    const FLAG_GREEN_HELD: i32 = 0x0000_0400;
    const FLAG_ONE_LAP_GREEN: i32 = 0x0000_0200;
    const FLAG_TEN_TO_GO: i32 = 0x0000_0800;
    const FLAG_FIVE_TO_GO: i32 = 0x0000_1000;
    const FLAG_CAUTION: i32 = 0x0000_4000;
    const FLAG_CAUTION_WAVING: i32 = 0x0000_8000;
    const FLAG_BLACK: i32 = 0x0001_0000;
    const FLAG_DQ: i32 = 0x0002_0000;
    const FLAG_FURLED: i32 = 0x0008_0000;
    const FLAG_REPAIR: i32 = 0x0010_0000;
    const FLAG_START_READY: i32 = 0x2000_0000;
    const FLAG_START_SET: i32 = 0x4000_0000;
    const FLAG_START_GO: i32 = i32::MIN; // 0x8000_0000

    // TrackSurface
    const TRK_NOT_IN_WORLD: i32 = -1;
    const TRK_OFF_TRACK: i32 = 0;
    const TRK_IN_PIT_STALL: i32 = 1;
    const TRK_APPROACHING_PITS: i32 = 2;
    const TRK_ON_TRACK: i32 = 3;

    #[derive(Clone, Default)]
    struct DriverInfo {
        car_idx: i32,
        name: String,
        car_number: String,
        car_path: String,
        irating: i32,
        license: String,
        class_color: String,
        class_id: i32,
        is_pace_car: bool,
    }

    #[derive(Clone, Default)]
    struct ResultPosEntry {
        position: i32,
        class_position: i32,
        laps_complete: i32,
    }

    #[derive(Default)]
    struct SessionCache {
        update: i32,
        player_idx: i32,
        car_number: String,
        irating: i32,
        track_id: Option<i32>,
        track_name: Option<String>,
        league_id: Option<i32>,
        car_path: Option<String>,
        redline: f32,
        laps_total: i32,
        incidents_limit: i32,
        /// SessionType strings indexed by SessionNum.
        session_types: Vec<String>,
        /// WeekendInfo split number when present.
        race_split: Option<i32>,
        drivers: HashMap<i32, DriverInfo>,
        /// From SessionInfo ResultsPositions (race session when present).
        results: HashMap<i32, ResultPosEntry>,
    }

    pub struct IrsdkReader {
        client: Client,
        session: Option<Session>,
        cache: SessionCache,
    }

    impl IrsdkReader {
        pub fn new() -> Self {
            Self {
                client: Client::new(),
                session: None,
                cache: SessionCache {
                    redline: 8000.0,
                    update: -1,
                    ..Default::default()
                },
            }
        }

        pub fn tick(&mut self) -> TelemetryFrame {
            // Safety: iracing-telem maps iRacing shared memory; all Session
            // methods are unsafe by crate design.
            unsafe { self.tick_inner() }
        }

        unsafe fn tick_inner(&mut self) -> TelemetryFrame {
            if self.session.as_ref().map(|s| s.expired()).unwrap_or(true) {
                self.session = self.client.session();
                self.cache.update = -1;
            }
            let Some(session) = self.session.as_mut() else {
                return disconnected();
            };
            if !session.connected() {
                self.session = None;
                return disconnected();
            }

            match session.get_new_data() {
                DataUpdateResult::Updated | DataUpdateResult::NoUpdate => {}
                DataUpdateResult::SessionExpired => {
                    self.session = None;
                    return disconnected();
                }
                _ => {}
            }

            let info_upd = session.session_info_update();
            if info_upd != self.cache.update {
                self.cache.update = info_upd;
                refresh_cache(&mut self.cache, &session.session_info());
            }

            build_frame(session, &self.cache)
        }
    }

    fn disconnected() -> TelemetryFrame {
        TelemetryFrame {
            connected: false,
            redline: 8000.0,
            ..Default::default()
        }
    }

    unsafe fn build_frame(session: &Session, cache: &SessionCache) -> TelemetryFrame {
        let speed = read_f32(session, "Speed");
        let rpm = read_f32(session, "RPM");
        let gear = read_i32(session, "Gear");
        let throttle = read_f32(session, "Throttle");
        let brake = read_f32(session, "Brake");
        let clutch = read_f32(session, "Clutch");
        let steering = read_f32(session, "SteeringWheelAngle");
        let fuel_l = read_f32(session, "FuelLevel");
        let fuel_pct = read_f32(session, "FuelLevelPct");
        let lap = read_i32(session, "Lap").max(0);
        let incidents = read_i32(session, "PlayerCarMyIncidentCount").max(0);
        let lap_dist = read_f32(session, "LapDistPct");
        let last_lap = read_f32(session, "LapLastLapTime");
        let best_lap = read_f32(session, "LapBestLapTime");
        let cur_lap = read_f32(session, "LapCurrentLapTime");
        let session_time = read_f64(session, "SessionTime");
        let session_state = read_i32(session, "SessionState");
        let lf = read_f32(session, "LFwearM");
        let rf = read_f32(session, "RFwearM");
        let lr = read_f32(session, "LRwearM");
        let rr = read_f32(session, "RRwearM");
        let air_temp = read_f32_opt(session, "AirTemp");
        let track_temp =
            read_f32_opt(session, "TrackTempCrew").or_else(|| read_f32_opt(session, "TrackTemp"));
        let wind_dir = read_f32_opt(session, "WindDir");
        let wind_vel = read_f32_opt(session, "WindVel");
        let track_wetness =
            read_f32_opt(session, "TrackWetness").map(|v| if v <= 1.0 { v * 100.0 } else { v });
        let rain_intensity = read_f32_opt(session, "Precipitation")
            .or_else(|| read_f32_opt(session, "RainIntensity"))
            .map(|v| if v <= 1.0 { v * 100.0 } else { v });
        let lap_est = {
            let v = read_f32(session, "LapEstTime");
            if v > 10.0 {
                v
            } else {
                90.0
            }
        };

        let position = player_position(session, cache.player_idx);
        let sf = read_i32(session, "SessionFlags");
        let flag = map_flag(sf);
        // Incident warn is computed in finalize_frame from count/limit + settings.
        let incident_warn = false;
        let incidents_limit = cache.incidents_limit;
        let engine_warn = read_i32(session, "EngineWarnings");
        let pit_limiter = (engine_warn & 0x10) != 0;

        let (delta_session_best, delta_best_lap, delta_optimal) = read_deltas(session);
        let delta = delta_session_best;

        let skies = weather_skies(session);
        let humidity =
            read_f32_opt(session, "RelativeHumidity").map(|h| if h <= 1.0 { h * 100.0 } else { h });
        let fog = read_f32_opt(session, "FogLevel");

        let tire_corners = read_tire_corners(session);
        let tire_temps = [
            tire_corners[0].temp.unwrap_or(0.0),
            tire_corners[1].temp.unwrap_or(0.0),
            tire_corners[2].temp.unwrap_or(0.0),
            tire_corners[3].temp.unwrap_or(0.0),
        ];
        let tire_pressures = [
            tire_corners[0].pressure.unwrap_or(0.0),
            tire_corners[1].pressure.unwrap_or(0.0),
            tire_corners[2].pressure.unwrap_or(0.0),
            tire_corners[3].pressure.unwrap_or(0.0),
        ];
        let pit_flags = read_i32(session, "PitSvFlags");
        let pit_services = decode_pit_flags(pit_flags);
        let pit_active = any_requested(pit_flags) || read_bool(session, "PlayerCarInPitStall");
        let pit_fuel_add_l = {
            let v = read_f32(session, "PitSvFuel");
            if v > 0.05 {
                Some(v)
            } else {
                None
            }
        };
        let pit_compound = read_i32_opt(session, "PitSvTireCompound");
        let pit_repairs = read_i32_opt(session, "FastRepairAvailable");
        let pit_repairs_used = read_i32_opt(session, "FastRepairUsed");

        let fps = read_f32_opt(session, "FrameRate").map(|v| v.round() as i32);
        let chan_quality = read_f32_opt(session, "ChanQuality").or_else(|| {
            read_f32_opt(session, "ConnectionQuality")
        });
        let session_time_of_day = read_f32_opt(session, "SessionTimeOfDay").filter(|v| v.is_finite());
        let session_num = read_i32(session, "SessionNum").max(0) as usize;
        let session_type = cache
            .session_types
            .get(session_num)
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| cache.session_types.last().cloned().filter(|s| !s.is_empty()))
            .map(format_session_type);
        let race_split = cache.race_split;

        let ers_battery_pct =
            read_f32_opt(session, "EnergyERSBatteryPct").map(
                |v| {
                    if v <= 1.0 {
                        v * 100.0
                    } else {
                        v
                    }
                },
            );
        let have_hybrid =
            ers_battery_pct.is_some() || read_f32_opt(session, "PowerMGU_K").is_some();
        let ers_boost_active = read_f32(session, "PowerMGU_K") > 50.0;
        let ers_p2p_active =
            read_bool(session, "dcPushToPass") || read_f32(session, "PushToPass") > 0.5;

        let redline = {
            let rl = read_f32(session, "DriverCarRedLine");
            if rl > 100.0 {
                rl
            } else if cache.redline > 100.0 {
                cache.redline
            } else {
                8000.0
            }
        };

        let abs_active = read_bool(session, "BrakeABSactive");
        let (left, right, left2, right2) = car_left_right(session);
        let radio_idx = {
            let v = read_i32(session, "RadioTransmitCarIdx");
            if v >= 0 {
                Some(v)
            } else {
                None
            }
        };
        let cars = car_rows(session, cache, radio_idx, session_state);
        let lead_lap = cars
            .iter()
            .filter(|c| !c.is_pace_car && c.lap > 0)
            .map(|c| c.lap)
            .max()
            .unwrap_or(lap)
            .max(0);
        // Direct from IRSDK + driver cache (not filtered cars) — Python parity.
        let radio = radio_idx.and_then(|idx| build_radio_speaker(session, cache, idx));
        let radar = RadarState {
            left,
            right,
            left2,
            right2,
            ..Default::default()
        };
        let fuel_use = read_f32(session, "FuelUsePerHour");
        let laps_fuel = if fuel_use > 0.05 && last_lap > 10.0 {
            (fuel_l / (fuel_use / 3600.0 * last_lap)).max(0.0)
        } else {
            0.0
        };
        let fuel_max = {
            let v = read_f32(session, "DriverCarFuelMaxLtr");
            if v > 1.0 {
                v
            } else {
                0.0
            }
        };
        let session_laps_remain_sdk = {
            let v = read_f32(session, "SessionLapsRemainEx");
            if v.is_finite() && v >= 0.0 && v < 32000.0 {
                Some(v)
            } else {
                let v2 = read_f32(session, "SessionLapsRemain");
                if v2.is_finite() && v2 >= 0.0 && v2 < 32000.0 {
                    Some(v2)
                } else {
                    None
                }
            }
        };
        let session_time_remain = {
            let v = read_f32(session, "SessionTimeRemain");
            if v.is_finite() && v >= 0.0 && v < 48.0 * 3600.0 {
                Some(v)
            } else {
                None
            }
        };

        let laps_total = {
            let t = read_i32(session, "SessionLapsTotal");
            if t > 0 && t < 100_000 {
                t
            } else {
                cache.laps_total
            }
        };
        // Lap-limited races: remaining follows the lead lap, not the player.
        let session_laps_remain = if laps_total > 0 && lead_lap > 0 {
            Some((laps_total - lead_lap).max(0) as f32)
        } else {
            session_laps_remain_sdk
        };
        let pits_open = {
            // PitsOpen may be absent; treat unknown as None.
            match session.find_var("PitsOpen").map(|v| session.var_value(&v)) {
                Some(Value::Bool(b)) => Some(b),
                Some(Value::Int(v)) => Some(v != 0),
                _ => None,
            }
        };
        let flag_context = flag_context_for(
            flag.as_deref(),
            sf,
            pits_open,
            session_laps_remain,
            session_time_remain,
            lap,
            laps_total,
            position,
        );

        TelemetryFrame {
            connected: true,
            in_garage: read_bool(session, "IsInGarage") || read_bool(session, "IsGarageVisible"),
            session_time,
            session_state,
            flag,
            flag_context,
            incident_warn,
            secondary: None,
            delta,
            delta_session_best,
            delta_best_lap,
            delta_optimal,
            incidents_limit,
            pit_limiter,
            speed_mps: speed,
            rpm,
            redline,
            gear,
            throttle,
            brake,
            clutch,
            steering,
            abs_active,
            fuel_l,
            fuel_pct,
            laps_fuel,
            fuel_max_l: fuel_max,
            fuel_use_per_hour: fuel_use,
            session_laps_remain,
            session_time_remain,
            position,
            car_number: cache.car_number.clone(),
            lap,
            laps_total,
            lead_lap,
            incidents,
            last_lap_s: positive_opt(last_lap as f64),
            best_lap_s: positive_opt(best_lap as f64),
            cur_lap_s: positive_opt(cur_lap as f64),
            irating: cache.irating,
            irating_delta: None,
            tire_wear_l: ((lf + lr) * 0.5).clamp(0.0, 1.0),
            tire_wear_r: ((rf + rr) * 0.5).clamp(0.0, 1.0),
            track_temp,
            air_temp,
            skies,
            humidity,
            fog,
            track_wetness,
            rain_intensity,
            wind_dir,
            wind_vel,
            player_lap_dist_pct: lap_dist,
            lap_est_time: lap_est,
            track_id: cache.track_id,
            track_name: cache.track_name.clone(),
            league_id: cache.league_id,
            car_path: cache.car_path.clone(),
            radar: radar.clone(),
            radar_left: left,
            radar_right: right,
            radio_name: radio.as_ref().map(|r| r.name.clone()),
            radio,
            cars,
            tire_corners,
            tire_temps,
            tire_pressures,
            pit_services,
            pit_active,
            pit_fuel_add_l,
            pit_fuel_to_add: pit_fuel_add_l,
            pit_compound,
            pit_repairs,
            pit_repairs_used,
            have_hybrid,
            ers_battery_pct,
            ers_pct: ers_battery_pct,
            ers_boost_active,
            ers_p2p_active,
            fps,
            chan_quality,
            session_time_of_day,
            session_type,
            race_split,
            ..Default::default()
        }
    }

    fn format_session_type(raw: String) -> String {
        let st = raw.to_ascii_lowercase();
        if st.contains("qualif") || st == "qual" {
            "Qualifying".into()
        } else if st.contains("race") {
            "Race".into()
        } else if st.contains("practice") || st == "open" {
            "Practice".into()
        } else {
            raw.replace('_', " ")
        }
    }

    fn map_car_flag(car_flags: i32) -> Option<String> {
        if car_flags & FLAG_REPAIR != 0 {
            return Some("meatball".into());
        }
        if car_flags & FLAG_BLACK != 0 {
            return Some("black".into());
        }
        if car_flags & FLAG_DQ != 0 {
            return Some("dq".into());
        }
        if car_flags & FLAG_FURLED != 0 {
            return Some("furled".into());
        }
        if car_flags & FLAG_BLUE != 0 {
            return Some("blue".into());
        }
        None
    }

    fn fmt_car_laptime(secs: f32) -> String {
        if !secs.is_finite() || secs <= 0.0 {
            return String::new();
        }
        let m = (secs as f64 / 60.0).floor() as i32;
        let s = secs as f64 - (m as f64) * 60.0;
        format!("{m}:{s:06.3}")
    }

    fn fmt_car_gap(position: i32, f2: f32) -> String {
        if position == 1 {
            return "LEADER".into();
        }
        if !f2.is_finite() || f2 <= 0.0 {
            return String::new();
        }
        format!("+{f2:.1}")
    }

    fn map_car_status_kind(surface: i32, on_pit: bool, car_flags: i32) -> Option<String> {
        if car_flags & FLAG_REPAIR != 0 {
            return Some("meatball".into());
        }
        if car_flags & FLAG_BLACK != 0 {
            return Some("black".into());
        }
        if car_flags & FLAG_DQ != 0 {
            return Some("dq".into());
        }
        if car_flags & FLAG_FURLED != 0 {
            return Some("furled".into());
        }
        if on_pit || surface == TRK_IN_PIT_STALL || surface == TRK_APPROACHING_PITS {
            return Some("pit".into());
        }
        if surface == TRK_OFF_TRACK {
            return Some("off".into());
        }
        if surface == TRK_NOT_IN_WORLD {
            return Some("garage".into());
        }
        None
    }

    /// Build radio tower row from transmit index + session info (Python
    /// `_update_radio_tower`). Does not require the car to be in `car_rows`.
    unsafe fn build_radio_speaker(
        session: &Session,
        cache: &SessionCache,
        radio_idx: i32,
    ) -> Option<RadioSpeaker> {
        if radio_idx < 0 {
            return None;
        }
        if cache
            .drivers
            .get(&radio_idx)
            .map(|d| d.is_pace_car)
            .unwrap_or(false)
        {
            return None;
        }
        let (name, car_number) = if let Some(d) = cache.drivers.get(&radio_idx) {
            (d.name.clone(), d.car_number.clone())
        } else {
            (
                format!("Car {radio_idx}"),
                format!("{radio_idx}"),
            )
        };
        let position = int_arr(session, "CarIdxPosition")
            .and_then(|a| a.get(radio_idx as usize).copied())
            .filter(|&p| p > 0)
            .unwrap_or(0);
        Some(RadioSpeaker {
            position,
            car_number,
            name,
            active: true,
            is_player: radio_idx == cache.player_idx,
            is_pro: false,
            group_icon: String::new(),
            group_color: String::new(),
        })
    }

    fn positive_opt(v: f64) -> Option<f64> {
        if v.is_finite() && v > 0.0 {
            Some(v)
        } else {
            None
        }
    }

    fn map_flag(sf: i32) -> Option<String> {
        if sf & FLAG_CHECKERED != 0 {
            return Some("checkered".into());
        }
        if sf & FLAG_WHITE != 0 {
            return Some("white".into());
        }
        if sf & (FLAG_RED) != 0 {
            return Some("red".into());
        }
        if sf & (FLAG_DQ) != 0 {
            return Some("dq".into());
        }
        if sf & (FLAG_BLACK) != 0 {
            return Some("black".into());
        }
        if sf & (FLAG_REPAIR) != 0 {
            return Some("meatball".into());
        }
        if sf & (FLAG_FURLED) != 0 {
            return Some("furled".into());
        }
        if sf & (FLAG_YELLOW | FLAG_YELLOW_WAVING | FLAG_CAUTION | FLAG_CAUTION_WAVING) != 0 {
            return Some("yellow".into());
        }
        if sf & FLAG_BLUE != 0 {
            return Some("blue".into());
        }
        if sf & FLAG_DEBRIS != 0 {
            return Some("debris".into());
        }
        if sf & FLAG_CROSSED != 0 {
            return Some("crossed".into());
        }
        if sf & (FLAG_GREEN | FLAG_GREEN_HELD) != 0 {
            return Some("green".into());
        }
        None
    }

    fn fmt_clock(secs: f32) -> String {
        let s = secs.max(0.0) as i32;
        let h = s / 3600;
        let m = (s % 3600) / 60;
        let sec = s % 60;
        if h > 0 {
            format!("{h}:{m:02}:{sec:02}")
        } else {
            format!("{m}:{sec:02}")
        }
    }

    /// Rich flag context (Python `AdvancedSimHUD._flag_context`).
    fn flag_context_for(
        flag: Option<&str>,
        sf: i32,
        pits_open: Option<bool>,
        session_laps_remain: Option<f32>,
        session_time_remain: Option<f32>,
        lap: i32,
        laps_total: i32,
        position: i32,
    ) -> Option<String> {
        if flag.is_none() {
            if sf & FLAG_START_GO != 0 {
                return Some("Green light — go".into());
            }
            if sf & FLAG_START_SET != 0 {
                return Some("Start lights set".into());
            }
            if sf & FLAG_START_READY != 0 {
                return Some("Get ready — start imminent".into());
            }
            return None;
        }
        match flag {
            Some("yellow") | Some("caution") => {
                let base = if sf & FLAG_ONE_LAP_GREEN != 0 {
                    "1 lap to green".to_string()
                } else if sf & FLAG_TEN_TO_GO != 0 {
                    "10 to go".to_string()
                } else if sf & FLAG_FIVE_TO_GO != 0 {
                    "5 to go".to_string()
                } else if sf & FLAG_CAUTION_WAVING != 0 {
                    match pits_open {
                        Some(true) => "Caution waving — pits open".into(),
                        _ => "Caution waving — pits closed".into(),
                    }
                } else if sf & FLAG_CAUTION != 0 {
                    "Full course caution — hold position".into()
                } else if sf & FLAG_YELLOW_WAVING != 0 {
                    "Local yellow — slow in sector".into()
                } else if sf & FLAG_YELLOW != 0 {
                    "Local yellow — slow down".into()
                } else {
                    "Slow down — no passing".into()
                };
                Some(base)
            }
            Some("green") => {
                if sf & FLAG_GREEN_HELD != 0 {
                    Some("Green held — stay in formation".into())
                } else {
                    Some("Track clear — racing resumes".into())
                }
            }
            Some("red") => {
                if let Some(remain) = session_time_remain.filter(|t| *t > 0.0) {
                    Some(format!("Session stopped — {} left", fmt_clock(remain)))
                } else {
                    Some("Session stopped — stand by".into())
                }
            }
            Some("white") => {
                if session_laps_remain.map(|v| v.round() as i32) == Some(1) {
                    Some("1 lap remaining".into())
                } else if laps_total > 0 && lap > 0 {
                    Some(format!("Lap {lap} of {laps_total} — finish this lap"))
                } else {
                    Some("Final lap — finish the race".into())
                }
            }
            Some("blue") => Some("Faster car approaching — let them pass".into()),
            Some("black") => Some("Report to the pits — penalty".into()),
            Some("meatball") => Some("Mandatory pit — repairs required".into()),
            Some("furled") => Some("Warning — next infraction is a penalty".into()),
            Some("dq") => Some("Disqualified — exit the track".into()),
            Some("debris") => Some("Debris on track — reduce speed".into()),
            Some("crossed") => {
                if laps_total > 0 && lap > 0 {
                    let rem = (laps_total - lap).max(0);
                    Some(format!("Halfway — {rem} laps to go"))
                } else {
                    Some("Halfway point".into())
                }
            }
            Some("checkered") => {
                if position > 0 {
                    Some(format!("Session complete — P{position}"))
                } else {
                    Some("Session complete".into())
                }
            }
            _ => None,
        }
    }

    unsafe fn read_deltas(session: &Session) -> (Option<f64>, Option<f64>, Option<f64>) {
        let named = |val_name: &str, ok_name: &str| -> Option<f64> {
            if !read_bool(session, ok_name) {
                return None;
            }
            let v = read_f32(session, val_name) as f64;
            if v.is_finite() && v.abs() < 600.0 {
                Some(v)
            } else {
                None
            }
        };
        let session_best = named("LapDeltaToSessionBestLap", "LapDeltaToSessionBestLap_OK");
        let best = named("LapDeltaToBestLap", "LapDeltaToBestLap_OK");
        let optimal = named("LapDeltaToOptimalLap", "LapDeltaToOptimalLap_OK").or_else(|| {
            named(
                "LapDeltaToSessionOptimalLap",
                "LapDeltaToSessionOptimalLap_OK",
            )
        });
        (session_best, best, optimal)
    }

    unsafe fn weather_skies(session: &Session) -> Option<String> {
        let v = read_i32_opt(session, "Skies")?;
        Some(
            match v {
                0 => "Clear",
                1 => "Partly Cloudy",
                2 => "Mostly Cloudy",
                3 => "Overcast",
                _ => "Cloudy",
            }
            .into(),
        )
    }

    unsafe fn read_tire_corners(session: &Session) -> [TireCorner; 4] {
        let wear = |a: &str, b: &str, c: &str| -> Option<f32> {
            let vals = [
                read_f32(session, a),
                read_f32(session, b),
                read_f32(session, c),
            ];
            let good: Vec<f32> = vals.into_iter().filter(|v| *v > 0.0 && *v <= 1.0).collect();
            if good.is_empty() {
                None
            } else {
                Some(good.iter().sum::<f32>() / good.len() as f32)
            }
        };
        let temp = |a: &str, b: &str, c: &str| -> Option<f32> {
            let vals = [
                read_f32(session, a),
                read_f32(session, b),
                read_f32(session, c),
            ];
            let good: Vec<f32> = vals
                .into_iter()
                .filter(|v| *v > 10.0 && *v < 200.0)
                .collect();
            if good.is_empty() {
                None
            } else {
                Some(good.iter().sum::<f32>() / good.len() as f32)
            }
        };
        let press = |name: &str| -> Option<f32> {
            let v = read_f32(session, name);
            if v > 50.0 && v < 400.0 {
                Some(v)
            } else {
                None
            }
        };
        [
            TireCorner {
                wear: wear("LFwearL", "LFwearM", "LFwearR"),
                temp: temp("LFtempCL", "LFtempCM", "LFtempCR"),
                pressure: press("LFpressure"),
            },
            TireCorner {
                wear: wear("RFwearL", "RFwearM", "RFwearR"),
                temp: temp("RFtempCL", "RFtempCM", "RFtempCR"),
                pressure: press("RFpressure"),
            },
            TireCorner {
                wear: wear("LRwearL", "LRwearM", "LRwearR"),
                temp: temp("LRtempCL", "LRtempCM", "LRtempCR"),
                pressure: press("LRpressure"),
            },
            TireCorner {
                wear: wear("RRwearL", "RRwearM", "RRwearR"),
                temp: temp("RRtempCL", "RRtempCM", "RRtempCR"),
                pressure: press("RRpressure"),
            },
        ]
    }

    unsafe fn car_left_right(session: &Session) -> (bool, bool, bool, bool) {
        let Some(var) = session.find_var("CarLeftRight") else {
            return (false, false, false, false);
        };
        match session.value::<flags::CarLeftRight>(&var) {
            Ok(v) => {
                use flags::CarLeftRight::*;
                match v {
                    CarLeft => (true, false, false, false),
                    CarRight => (false, true, false, false),
                    CarLeftRight => (true, true, false, false),
                    TwoCarsLeft => (true, false, true, false),
                    TwoCarsRight => (false, true, false, true),
                    _ => (false, false, false, false),
                }
            }
            Err(_) => (false, false, false, false),
        }
    }

    unsafe fn player_position(session: &Session, player_idx: i32) -> i32 {
        if let Some(p) = read_i32_opt(session, "PlayerCarPosition") {
            if p > 0 {
                return p;
            }
        }
        if player_idx >= 0 {
            if let Some(Value::Ints(arr)) = session
                .find_var("CarIdxPosition")
                .map(|v| session.var_value(&v))
            {
                if let Some(&p) = arr.get(player_idx as usize) {
                    return p.max(0);
                }
            }
        }
        0
    }

    unsafe fn float_arr(session: &Session, name: &str) -> Option<Vec<f32>> {
        match session.find_var(name).map(|v| session.var_value(&v)) {
            Some(Value::Floats(a)) => Some(a.to_vec()),
            _ => None,
        }
    }

    unsafe fn int_arr(session: &Session, name: &str) -> Option<Vec<i32>> {
        match session.find_var(name).map(|v| session.var_value(&v)) {
            Some(Value::Ints(a)) => Some(a.to_vec()),
            _ => None,
        }
    }

    unsafe fn bool_arr(session: &Session, name: &str) -> Option<Vec<bool>> {
        match session.find_var(name).map(|v| session.var_value(&v)) {
            Some(Value::Bools(a)) => Some(a.to_vec()),
            _ => None,
        }
    }

    unsafe fn car_rows(
        session: &Session,
        cache: &SessionCache,
        radio_idx: Option<i32>,
        session_state: i32,
    ) -> Vec<CarRow> {
        let Some(pcts) = float_arr(session, "CarIdxLapDistPct") else {
            return Vec::new();
        };
        let on_pit = bool_arr(session, "CarIdxOnPitRoad");
        let positions = int_arr(session, "CarIdxPosition");
        let class_pos = int_arr(session, "CarIdxClassPosition");
        let est = float_arr(session, "CarIdxEstTime");
        let f2 = float_arr(session, "CarIdxF2Time");
        let last_laps = float_arr(session, "CarIdxLastLapTime");
        let best_laps = float_arr(session, "CarIdxBestLapTime");
        let laps = int_arr(session, "CarIdxLap");
        let laps_done = int_arr(session, "CarIdxLapCompleted");
        let speeds = float_arr(session, "CarIdxSpeed");
        let surface = int_arr(session, "CarIdxTrackSurface");
        let session_flags = int_arr(session, "CarIdxSessionFlags");
        let player_idx = cache.player_idx;
        let player_lap = laps
            .as_ref()
            .and_then(|a| a.get(player_idx as usize).copied())
            .unwrap_or(0);
        let use_results = session_state >= 5 && !cache.results.is_empty();

        let mut out = Vec::new();
        for (i, &pct) in pcts.iter().enumerate() {
            let di = cache.drivers.get(&(i as i32));
            if di.map(|d| d.is_pace_car).unwrap_or(false) {
                // Still include pace car marked so builders can skip.
            }
            let mut pos = positions
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(0)
                .max(0);
            let mut cpos = class_pos
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(pos)
                .max(0);
            let mut laps_completed = laps_done
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(0)
                .max(0);
            if use_results {
                if let Some(r) = cache.results.get(&(i as i32)) {
                    if r.position > 0 {
                        pos = r.position;
                    }
                    if r.class_position > 0 {
                        cpos = r.class_position;
                    } else if r.position > 0 {
                        cpos = r.position;
                    }
                    if r.laps_complete > laps_completed {
                        laps_completed = r.laps_complete;
                    }
                }
            }
            let pit = on_pit
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(false);
            let surf = surface
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(-1);
            let on_track = surf == TRK_ON_TRACK;
            let in_pit = surf == TRK_IN_PIT_STALL || surf == TRK_APPROACHING_PITS;
            let approaching_pits = surf == TRK_APPROACHING_PITS;
            let est_t = est.as_ref().and_then(|a| a.get(i).copied()).unwrap_or(0.0);
            let f2_t = f2.as_ref().and_then(|a| a.get(i).copied()).unwrap_or(0.0);
            let last_lap = last_laps
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .map(fmt_car_laptime)
                .unwrap_or_default();
            let best_lap = best_laps
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .map(fmt_car_laptime)
                .unwrap_or_default();
            let car_lap = laps.as_ref().and_then(|a| a.get(i).copied()).unwrap_or(0);
            let speed_mps = speeds
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(0.0);

            // Skip empty world slots.
            let has_driver = di.is_some();
            if !pct.is_finite() || pct < 0.0 {
                if !has_driver || pos <= 0 {
                    continue;
                }
            }
            if pct == 0.0 && i as i32 != player_idx && pos <= 0 && !has_driver {
                continue;
            }
            if !has_driver && pos <= 0 && !on_track && !in_pit && i as i32 != player_idx {
                continue;
            }

            let (name, number, ir, lic, class_color, class_id, is_pace) = if let Some(d) = di {
                (
                    d.name.clone(),
                    d.car_number.clone(),
                    d.irating,
                    d.license.clone(),
                    d.class_color.clone(),
                    d.class_id,
                    d.is_pace_car,
                )
            } else {
                (
                    format!("Car {i}"),
                    format!("{i}"),
                    0,
                    String::new(),
                    String::new(),
                    0,
                    false,
                )
            };

            let lap_delta = if player_lap > 0 && car_lap > 0 {
                car_lap - player_lap
            } else {
                0
            };
            let lapping = lap_delta != 0;
            let lap_ahead = lap_delta > 0;
            let flags = session_flags
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(0);
            let status_kind = map_car_status_kind(surf, pit, flags);
            let car_flag = map_car_flag(flags);
            let gap = fmt_car_gap(pos, if f2_t.is_finite() { f2_t } else { 0.0 });

            out.push(CarRow {
                car_idx: i as i32,
                position: pos,
                class_position: cpos,
                car_number: number,
                name,
                gap,
                last_lap,
                best_lap,
                irating: ir,
                irating_delta: None,
                class_id,
                license: lic,
                class_color,
                on_pit: pit,
                in_pit,
                on_track,
                approaching_pits,
                is_player: i as i32 == player_idx,
                is_speaking: radio_idx == Some(i as i32),
                is_pace_car: is_pace,
                lapping,
                lap_ahead,
                inactive: !on_track && !in_pit && pos > 0,
                lap_dist_pct: if pct.is_finite() && pct >= 0.0 {
                    pct.fract().abs()
                } else {
                    -1.0
                },
                est_time: if est_t.is_finite() { est_t } else { 0.0 },
                f2_time: if f2_t.is_finite() { f2_t } else { 0.0 },
                lap: car_lap.max(0),
                laps_completed,
                speed_mps,
                status_kind,
                car_flag,
            });
        }
        // Ensure transmitter appears for map/table speaking badges even when
        // car_rows filters them out (garage / invalid LapDistPct, etc.).
        if let Some(idx) = radio_idx {
            if idx >= 0 && !out.iter().any(|c| c.car_idx == idx) {
                let di = cache.drivers.get(&idx);
                if !di.map(|d| d.is_pace_car).unwrap_or(false) {
                    let (name, number, ir, lic, class_color, class_id) = if let Some(d) = di {
                        (
                            d.name.clone(),
                            d.car_number.clone(),
                            d.irating,
                            d.license.clone(),
                            d.class_color.clone(),
                            d.class_id,
                        )
                    } else {
                        (
                            format!("Car {idx}"),
                            format!("{idx}"),
                            0,
                            String::new(),
                            String::new(),
                            0,
                        )
                    };
                    let pos = positions
                        .as_ref()
                        .and_then(|a| a.get(idx as usize).copied())
                        .unwrap_or(0)
                        .max(0);
                    let cpos = class_pos
                        .as_ref()
                        .and_then(|a| a.get(idx as usize).copied())
                        .unwrap_or(pos)
                        .max(0);
                    let pct = pcts
                        .get(idx as usize)
                        .copied()
                        .unwrap_or(-1.0);
                    out.push(CarRow {
                        car_idx: idx,
                        position: pos,
                        class_position: cpos,
                        car_number: number,
                        name,
                        gap: String::new(),
                        last_lap: String::new(),
                        best_lap: String::new(),
                        irating: ir,
                        irating_delta: None,
                        class_id,
                        license: lic,
                        class_color,
                        on_pit: false,
                        in_pit: false,
                        on_track: false,
                        approaching_pits: false,
                        is_player: idx == player_idx,
                        is_speaking: true,
                        is_pace_car: false,
                        lapping: false,
                        lap_ahead: false,
                        inactive: pos > 0,
                        lap_dist_pct: if pct.is_finite() && pct >= 0.0 {
                            pct.fract().abs()
                        } else {
                            -1.0
                        },
                        est_time: 0.0,
                        f2_time: 0.0,
                        lap: 0,
                        laps_completed: 0,
                        speed_mps: 0.0,
                        status_kind: None,
                        car_flag: None,
                    });
                }
            }
        }
        out
    }

    fn refresh_cache(cache: &mut SessionCache, yaml: &str) {
        if let Some(v) = yaml_i32(yaml, "PlayerCarIdx") {
            cache.player_idx = v;
        } else if let Some(v) = yaml_i32(yaml, "DriverCarIdx") {
            cache.player_idx = v;
        }
        if let Some(n) = yaml_str(yaml, "CarNumber") {
            cache.car_number = n.trim_matches('"').to_string();
        }
        if let Some(v) = yaml_i32(yaml, "IRating") {
            cache.irating = v;
        }
        if let Some(v) = yaml_i32(yaml, "TrackID") {
            cache.track_id = Some(v);
        }
        if let Some(v) = yaml_i32(yaml, "LeagueID") {
            cache.league_id = if v > 0 { Some(v) } else { None };
        }
        if let Some(n) =
            yaml_str(yaml, "TrackDisplayShortName").or_else(|| yaml_str(yaml, "TrackDisplayName"))
        {
            cache.track_name = Some(n.trim_matches('"').to_string());
        }
        if let Some(v) = yaml_f32(yaml, "DriverCarRedLine") {
            if v > 100.0 {
                cache.redline = v;
            }
        }
        if let Some(v) = yaml_i32(yaml, "SessionLaps") {
            if v > 0 && v < 100_000 {
                cache.laps_total = v;
            }
        }
        if let Some(v) = yaml_i32(yaml, "IncidentLimit") {
            if v > 0 && v < 10_000 {
                cache.incidents_limit = v;
            }
        }
        cache.session_types = parse_session_types(yaml);
        cache.race_split = parse_race_split(yaml);
        cache.drivers = parse_drivers(yaml);
        cache.results = parse_results_positions(yaml);
        if let Some(d) = cache.drivers.get(&cache.player_idx) {
            if !d.car_number.is_empty() {
                cache.car_number = d.car_number.clone();
            }
            if d.irating > 0 {
                cache.irating = d.irating;
            }
            if !d.car_path.is_empty() {
                cache.car_path = Some(d.car_path.clone());
            }
        }
    }

    /// Parse SessionInfo ResultsPositions entries (prefer Race session block).
    fn parse_session_types(yaml: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut in_sessions = false;
        for line in yaml.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Sessions:") {
                in_sessions = true;
                continue;
            }
            if in_sessions {
                let indent = line.len() - line.trim_start().len();
                if !trimmed.is_empty() && indent == 0 && !trimmed.starts_with('-') {
                    break;
                }
                if trimmed.starts_with("SessionType:") {
                    let v = trimmed
                        .trim_start_matches("SessionType:")
                        .trim()
                        .trim_matches('"')
                        .to_string();
                    out.push(v);
                }
            }
        }
        out
    }

    fn parse_race_split(yaml: &str) -> Option<i32> {
        const KEYS: &[&str] = &[
            "RaceSplit",
            "SplitNum",
            "SplitNumber",
            "SessionSplit",
            "SessionSplitNum",
            "EventSplit",
        ];
        for key in KEYS {
            if let Some(v) = yaml_i32(yaml, key) {
                if v > 0 {
                    return Some(v);
                }
            }
        }
        // Any other *Split* key under WeekendInfo-style YAML.
        for line in yaml.lines() {
            let trimmed = line.trim();
            let Some((key, rest)) = trimmed.split_once(':') else {
                continue;
            };
            if !key.to_ascii_lowercase().contains("split") {
                continue;
            }
            if let Ok(v) = rest.trim().trim_matches('"').parse::<i32>() {
                if v > 0 {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Parse SessionInfo ResultsPositions entries (prefer Race session block).
    fn parse_results_positions(yaml: &str) -> HashMap<i32, ResultPosEntry> {
        let mut best: HashMap<i32, ResultPosEntry> = HashMap::new();
        let mut current: HashMap<i32, ResultPosEntry> = HashMap::new();
        let mut in_results = false;
        let mut race_session = false;
        let mut block_is_race = false;
        let mut cur_idx: Option<i32> = None;
        let mut cur = ResultPosEntry::default();

        let flush_cur = |map: &mut HashMap<i32, ResultPosEntry>,
                         idx: &mut Option<i32>,
                         e: &mut ResultPosEntry| {
            if let Some(i) = idx.take() {
                if e.position > 0 || e.class_position > 0 || e.laps_complete > 0 {
                    map.insert(i, e.clone());
                }
            }
            *e = ResultPosEntry::default();
        };

        for line in yaml.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("SessionType:") {
                let v = trimmed
                    .trim_start_matches("SessionType:")
                    .trim()
                    .trim_matches('"');
                race_session = v.to_ascii_lowercase().contains("race");
            }
            if trimmed.starts_with("ResultsPositions:") {
                flush_cur(&mut current, &mut cur_idx, &mut cur);
                if !current.is_empty() {
                    if block_is_race || best.is_empty() {
                        best = std::mem::take(&mut current);
                    } else {
                        current.clear();
                    }
                }
                in_results = true;
                block_is_race = race_session;
                continue;
            }
            // End of indented results list.
            if in_results {
                let indent = line.len() - line.trim_start().len();
                if !trimmed.is_empty() && indent == 0 && !trimmed.starts_with('-') {
                    flush_cur(&mut current, &mut cur_idx, &mut cur);
                    if !current.is_empty() {
                        if block_is_race || best.is_empty() {
                            best = std::mem::take(&mut current);
                        } else {
                            current.clear();
                        }
                    }
                    in_results = false;
                }
            }
            if !in_results {
                continue;
            }
            if trimmed.starts_with("- ") || trimmed == "-" {
                flush_cur(&mut current, &mut cur_idx, &mut cur);
                if let Some(rest) = trimmed.strip_prefix("- ") {
                    if let Some((k, v)) = rest.split_once(':') {
                        apply_result_field(&mut cur_idx, &mut cur, k.trim(), v.trim());
                    }
                }
                continue;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                apply_result_field(&mut cur_idx, &mut cur, k.trim(), v.trim());
            }
        }
        flush_cur(&mut current, &mut cur_idx, &mut cur);
        if !current.is_empty() {
            if block_is_race || best.is_empty() {
                best = current;
            }
        }
        best
    }

    fn apply_result_field(
        cur_idx: &mut Option<i32>,
        cur: &mut ResultPosEntry,
        key: &str,
        raw: &str,
    ) {
        let val = raw.trim_matches('"').trim();
        let n = val.parse::<i32>().ok();
        match key {
            "CarIdx" => {
                if let Some(i) = n {
                    *cur_idx = Some(i);
                }
            }
            "Position" => {
                if let Some(i) = n {
                    cur.position = i.max(0);
                }
            }
            "ClassPosition" => {
                if let Some(i) = n {
                    cur.class_position = i.max(0);
                }
            }
            "LapsComplete" | "LapsDriven" => {
                if let Some(i) = n {
                    cur.laps_complete = i.max(0);
                }
            }
            _ => {}
        }
    }

    /// Parse `DriverInfo: Drivers:` list entries from session YAML.
    fn parse_drivers(yaml: &str) -> HashMap<i32, DriverInfo> {
        let mut out = HashMap::new();
        let mut in_drivers = false;
        let mut cur: Option<DriverInfo> = None;

        let flush = |cur: &mut Option<DriverInfo>, out: &mut HashMap<i32, DriverInfo>| {
            if let Some(d) = cur.take() {
                if d.car_idx >= 0 {
                    out.insert(d.car_idx, d);
                }
            }
        };

        for line in yaml.lines() {
            let raw = line;
            let t = raw.trim();
            if t.starts_with("Drivers:") {
                in_drivers = true;
                continue;
            }
            if in_drivers {
                // Leaving Drivers section when a top-level key appears (no indent).
                if !raw.is_empty()
                    && !raw.starts_with(' ')
                    && !raw.starts_with('\t')
                    && t.contains(':')
                    && !t.starts_with('-')
                {
                    flush(&mut cur, &mut out);
                    in_drivers = false;
                    continue;
                }
            }
            if !in_drivers {
                continue;
            }
            if t.starts_with("- CarIdx:") || t.starts_with("-CarIdx:") {
                flush(&mut cur, &mut out);
                let mut d = DriverInfo::default();
                if let Some(rest) = t.split_once(':').map(|(_, r)| r.trim()) {
                    d.car_idx = rest.parse().unwrap_or(-1);
                }
                cur = Some(d);
                continue;
            }
            if t.starts_with("- ") && t.contains("CarIdx:") {
                flush(&mut cur, &mut out);
                let mut d = DriverInfo::default();
                if let Some(rest) = t.split("CarIdx:").nth(1) {
                    d.car_idx = rest.trim().parse().unwrap_or(-1);
                }
                cur = Some(d);
                continue;
            }
            let Some(d) = cur.as_mut() else { continue };
            if let Some(v) = kv(t, "CarIdx") {
                d.car_idx = v.parse().unwrap_or(d.car_idx);
            } else if let Some(v) = kv(t, "UserName") {
                d.name = unquote(v);
            } else if let Some(v) = kv(t, "AbbrevName") {
                if d.name.is_empty() {
                    d.name = unquote(v);
                }
            } else if let Some(v) = kv(t, "CarNumber") {
                d.car_number = unquote(v);
            } else if let Some(v) = kv(t, "CarPath") {
                d.car_path = unquote(v);
            } else if let Some(v) = kv(t, "IRating") {
                d.irating = v.parse().unwrap_or(0);
            } else if let Some(v) = kv(t, "CarClassID") {
                d.class_id = v.parse().unwrap_or(0);
            } else if let Some(v) = kv(t, "LicString") {
                d.license = unquote(v);
            } else if let Some(v) = kv(t, "CarClassColor") {
                // iRacing packs RGB in the low 24 bits (sometimes with high bits set).
                if let Ok(n) = v.parse::<u32>() {
                    d.class_color = format!("#{:06x}", n & 0x00FF_FFFF);
                } else {
                    d.class_color = unquote(v);
                }
            } else if let Some(v) = kv(t, "CarIsPaceCar") {
                d.is_pace_car = v == "1" || v.eq_ignore_ascii_case("true");
            }
        }
        flush(&mut cur, &mut out);
        out
    }

    fn kv<'a>(line: &'a str, key: &str) -> Option<&'a str> {
        let needle = format!("{key}:");
        let t = line.trim();
        t.strip_prefix(&needle).map(|r| r.trim())
    }

    fn unquote(s: &str) -> String {
        s.trim().trim_matches('"').to_string()
    }

    fn yaml_i32(yaml: &str, key: &str) -> Option<i32> {
        let needle = format!("{key}:");
        for line in yaml.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix(&needle) {
                let rest = rest.trim();
                if let Ok(v) = rest.parse::<i32>() {
                    return Some(v);
                }
            }
        }
        None
    }

    fn yaml_f32(yaml: &str, key: &str) -> Option<f32> {
        let needle = format!("{key}:");
        for line in yaml.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix(&needle) {
                if let Ok(v) = rest.trim().parse::<f32>() {
                    return Some(v);
                }
            }
        }
        None
    }

    fn yaml_str(yaml: &str, key: &str) -> Option<String> {
        let needle = format!("{key}:");
        for line in yaml.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix(&needle) {
                return Some(rest.trim().to_string());
            }
        }
        None
    }

    unsafe fn read_f32(session: &Session, name: &str) -> f32 {
        read_f32_opt(session, name).unwrap_or(0.0)
    }

    unsafe fn read_f32_opt(session: &Session, name: &str) -> Option<f32> {
        let var = session.find_var(name)?;
        match session.var_value(&var) {
            Value::Float(v) => Some(v),
            Value::Double(v) => Some(v as f32),
            Value::Int(v) => Some(v as f32),
            _ => session.value::<f32>(&var).ok(),
        }
    }

    unsafe fn read_f64(session: &Session, name: &str) -> f64 {
        let Some(var) = session.find_var(name) else {
            return 0.0;
        };
        match session.var_value(&var) {
            Value::Double(v) => v,
            Value::Float(v) => v as f64,
            Value::Int(v) => v as f64,
            _ => session.value::<f64>(&var).unwrap_or(0.0),
        }
    }

    unsafe fn read_i32(session: &Session, name: &str) -> i32 {
        read_i32_opt(session, name).unwrap_or(0)
    }

    unsafe fn read_i32_opt(session: &Session, name: &str) -> Option<i32> {
        let var = session.find_var(name)?;
        match session.var_value(&var) {
            Value::Int(v) | Value::Bitfield(v) => Some(v),
            Value::Float(v) => Some(v as i32),
            _ => session.value::<i32>(&var).ok(),
        }
    }

    unsafe fn read_bool(session: &Session, name: &str) -> bool {
        let Some(var) = session.find_var(name) else {
            return false;
        };
        match session.var_value(&var) {
            Value::Bool(v) => v,
            Value::Int(v) => v != 0,
            _ => session.value::<bool>(&var).unwrap_or(false),
        }
    }
}

#[cfg(windows)]
pub use win::IrsdkReader;

#[cfg(not(windows))]
pub struct IrsdkReader;

#[cfg(not(windows))]
impl IrsdkReader {
    pub fn new() -> Self {
        Self
    }

    pub fn tick(&mut self) -> super::TelemetryFrame {
        super::TelemetryFrame {
            connected: false,
            redline: 8000.0,
            ..Default::default()
        }
    }
}
