//! Live iRacing shared-memory reader (Windows). Non-Windows uses a stub.

#[cfg(windows)]
mod win {
    use crate::telemetry::format::{
        apply_white_flag_timing, fmt_car_gap, fmt_laptime, format_session_type,
        map_session_flag_label, parse_race_split, parse_race_split_total, parse_session_types,
        update_white_flag_hud, WhiteFlagHud,
    };
    use crate::telemetry::pit_service::any_requested;
    use crate::telemetry::{
        decode_pit_flags, CarRow, RadarState, RadioSpeaker, TelemetryFrame, TireCorner,
    };
    use iracing_telem::{Client, DataUpdateResult, Session, Value};
    use std::collections::HashMap;
    use std::sync::mpsc::{self, Receiver, Sender};

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
        /// DriverInfo LicLevel (1–4 Rookie … 21+ Pro); 0 when unknown.
        lic_level: i32,
        /// DriverInfo LicSubLevel (SR × 100); 0 when unknown.
        lic_sub_level: i32,
        class_color: String,
        class_id: i32,
        is_pace_car: bool,
        /// DriverInfo ClubName from session YAML.
        club_name: String,
        /// DriverInfo FlairID (profile country flag); 0 = unset.
        flair_id: i32,
        /// DriverInfo FlairName when present.
        flair_name: String,
    }

    #[derive(Clone, Default)]
    struct ResultPosEntry {
        position: i32,
        class_position: i32,
        laps_complete: i32,
        /// Session / qualify FastestTime (seconds). 0 = none.
        fastest_time: f32,
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
        race_split_total: Option<i32>,
        subsession_id: i32,
        /// WeekendInfo / telemetry pit road limit (m/s).
        pit_speed_limit_mps: Option<f32>,
        /// WeekendInfo `TrackLength` (meters) for pace-car speed estimate.
        track_length_m: Option<f32>,
        /// WeekendInfo `TrackNumTurns` — SR corner multiplier per lap.
        track_num_turns: i32,
        /// Player `LicString` from DriverInfo.
        license: String,
        /// `DriverInfo:PaceCarIdx` when known.
        pace_car_idx: Option<i32>,
        /// Last pace-car sample: (session_time, lap_dist_pct).
        pace_speed_prev: Option<(f64, f32)>,
        /// Smoothed pace-car speed (CarIdxSpeed is not an official SDK var).
        pace_speed_mps: f32,
        drivers: HashMap<i32, DriverInfo>,
        /// From SessionInfo ResultsPositions (race session when present).
        results: HashMap<i32, ResultPosEntry>,
        /// From QualifyResultsInfo.Results (starting grid / live qual order).
        qualify_grid: HashMap<i32, ResultPosEntry>,
        /// Dash white HUD: show from in-game white until the player takes S/F.
        white_flag_hud: WhiteFlagHud,
    }

    pub struct IrsdkReader {
        client: Client,
        session: Option<Session>,
        cache: SessionCache,
        split_tx: Sender<(i32, Option<(i32, i32)>)>,
        split_rx: Receiver<(i32, Option<(i32, i32)>)>,
        split_pending: Option<i32>,
    }

    impl IrsdkReader {
        pub fn new() -> Self {
            let (split_tx, split_rx) = mpsc::channel();
            Self {
                client: Client::new(),
                session: None,
                cache: SessionCache {
                    redline: 8000.0,
                    update: -1,
                    ..Default::default()
                },
                split_tx,
                split_rx,
                split_pending: None,
            }
        }

        pub fn tick(&mut self) -> TelemetryFrame {
            // Safety: iracing-telem maps iRacing shared memory; all Session
            // methods are unsafe by crate design.
            unsafe { self.tick_inner() }
        }

        unsafe fn tick_inner(&mut self) -> TelemetryFrame {
            while let Ok((sid, result)) = self.split_rx.try_recv() {
                if self.cache.subsession_id == sid {
                    if let Some((split, total)) = result {
                        self.cache.race_split = Some(split);
                        self.cache.race_split_total = Some(total);
                    }
                }
            }
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
            if self.cache.race_split.is_none()
                && self.cache.subsession_id > 0
                && self.split_pending != Some(self.cache.subsession_id)
            {
                let sid = self.cache.subsession_id;
                let tx = self.split_tx.clone();
                self.split_pending = Some(sid);
                std::thread::spawn(move || {
                    let result = crate::iracing_results::split_for_subsession(sid);
                    let _ = tx.send((sid, result));
                });
            }

            build_frame(session, &mut self.cache)
        }
    }

    fn disconnected() -> TelemetryFrame {
        TelemetryFrame {
            connected: false,
            redline: 8000.0,
            ..Default::default()
        }
    }

    unsafe fn build_frame(session: &Session, cache: &mut SessionCache) -> TelemetryFrame {
        let speed = read_f32(session, "Speed");
        let rpm = read_f32(session, "RPM");
        let gear = read_i32(session, "Gear");
        let throttle = read_f32(session, "Throttle");
        let brake = read_f32(session, "Brake");
        let clutch = read_f32(session, "Clutch");
        let steering = read_f32(session, "SteeringWheelAngle");
        let lat_accel = read_f32(session, "LatAccel");
        let long_accel = read_f32(session, "LongAccel");
        let vert_accel = read_f32_opt(session, "VertAccel").unwrap_or(0.0);
        let yaw_rate = read_f32_opt(session, "YawRate")
            .or_else(|| read_f32_opt(session, "Yaw"))
            .unwrap_or(0.0);
        let ffb_pct = read_f32_opt(session, "SteeringWheelPctTorque").map(|v| {
            // SDK unit is %; some builds return 0–1 — normalize to percent display units.
            if v.is_finite() && v.abs() <= 2.0 {
                v.abs() * 100.0
            } else {
                v.abs()
            }
        });
        let pace_mode = read_i32(session, "PaceMode").max(0);
        let pit_speed_limit_mps = read_f32_opt(session, "TrackPitSpeedLimit")
            .filter(|v| v.is_finite() && *v > 0.0)
            .map(normalize_pit_speed_limit)
            .or(cache.pit_speed_limit_mps);
        let fuel_l = read_f32(session, "FuelLevel");
        let fuel_pct = read_f32(session, "FuelLevelPct");
        let lap = read_i32(session, "Lap").max(0);
        let incidents = read_i32(session, "PlayerCarMyIncidentCount").max(0);
        let lap_dist = read_f32(session, "LapDistPct");
        // (0, 0) is open ocean, so a zero pair means "not reporting" rather than
        // a real fix — cheaper than asking the SDK whether the var is populated.
        let player_lat = read_f64_opt(session, "Lat").filter(|v| v.is_finite() && *v != 0.0);
        let player_lon = read_f64_opt(session, "Lon").filter(|v| v.is_finite() && *v != 0.0);
        // Absent vars, not zeroed ones: a parked car reports Some(0.0) here.
        let player_motion = match (
            read_f32_opt(session, "VelocityX"),
            read_f32_opt(session, "VelocityY"),
            read_f32_opt(session, "Yaw"),
        ) {
            (Some(vx), Some(vy), Some(yaw)) => Some(crate::telemetry::PlayerMotion {
                vx,
                vy,
                yaw,
                yaw_north: read_f32(session, "YawNorth"),
            }),
            _ => None,
        };
        if lap_dist >= 0.0 && player_lat.is_none() && player_motion.is_none() {
            warn_position_vars_once(session);
        }
        let last_lap = read_f32(session, "LapLastLapTime");
        let best_lap = read_f32(session, "LapBestLapTime");
        let cur_lap = read_f32(session, "LapCurrentLapTime");
        let session_time = read_f64(session, "SessionTime");
        let session_state = read_i32(session, "SessionState");
        let camera_car_idx = read_i32_opt(session, "CamCarIdx").filter(|idx| *idx >= 0);
        // Garage UI / garage physics — wins over on-track when both are set.
        let in_garage = read_bool(session, "IsInGarage") || read_bool(session, "IsGarageVisible");
        // Player seated in the car with physics (not replay / garage / menus).
        // Do NOT use IsOnTrackCar alone — it stays true while the car is in the
        // world even after you leave to the garage.
        let in_car = read_bool(session, "IsOnTrack") && !in_garage;
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

        let sf = read_i32(session, "SessionFlags");
        let mut flag = map_flag(sf);
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
        let tire_sets = crate::telemetry::TireSets {
            available: read_i32_opt(session, "TireSetsAvailable"),
            used: read_i32_opt(session, "TireSetsUsed"),
            dry_limit: read_i32_opt(session, "PlayerCarDryTireSetLimit"),
            left_available: read_i32_opt(session, "LeftTireSetsAvailable"),
            left_used: read_i32_opt(session, "LeftTireSetsUsed"),
            right_available: read_i32_opt(session, "RightTireSetsAvailable"),
            right_used: read_i32_opt(session, "RightTireSetsUsed"),
            front_available: read_i32_opt(session, "FrontTireSetsAvailable"),
            front_used: read_i32_opt(session, "FrontTireSetsUsed"),
            rear_available: read_i32_opt(session, "RearTireSetsAvailable"),
            rear_used: read_i32_opt(session, "RearTireSetsUsed"),
        };

        let fps = read_f32_opt(session, "FrameRate").map(|v| v.round() as i32);
        let chan_quality = read_f32_opt(session, "ChanQuality")
            .or_else(|| read_f32_opt(session, "ConnectionQuality"));
        let session_time_of_day =
            read_f32_opt(session, "SessionTimeOfDay").filter(|v| v.is_finite());
        let session_num = read_i32(session, "SessionNum").max(0) as usize;
        let session_type = cache
            .session_types
            .get(session_num)
            .cloned()
            .filter(|s| !s.is_empty())
            .or_else(|| {
                cache
                    .session_types
                    .last()
                    .cloned()
                    .filter(|s| !s.is_empty())
            })
            .map(format_session_type);
        let race_split = cache.race_split;
        let race_split_total = cache.race_split_total;

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
            // Prefer opt read — missing var must not become car 0.
            match read_i32_opt(session, "RadioTransmitCarIdx") {
                Some(v) if v >= 0 => Some(v),
                _ => None,
            }
        };
        let mut cars = car_rows(
            session,
            cache,
            radio_idx,
            session_state,
            session_type.as_deref().unwrap_or(""),
        );
        // Prefer live pace-car speed; fall back to lap% × track length.
        let player_lap_dist = read_f32(session, "LapDist");
        update_pace_car_speed(cache, session_time, player_lap_dist, &mut cars);
        let pace_car_speed_mps = {
            let v = cache.pace_speed_mps;
            if v.is_finite() && v > 0.5 {
                Some(v)
            } else {
                None
            }
        };
        // Prefer resolved grid/qual position over raw CarIdxPosition (often 0 pre-green).
        let position = cars
            .iter()
            .find(|c| c.is_player)
            .map(|c| c.position)
            .filter(|&p| p > 0)
            .unwrap_or_else(|| player_position(session, cache.player_idx));
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
            if v.is_finite() && (0.0..32000.0).contains(&v) {
                Some(v)
            } else {
                let v2 = read_f32(session, "SessionLapsRemain");
                if v2.is_finite() && (0.0..32000.0).contains(&v2) {
                    Some(v2)
                } else {
                    None
                }
            }
        };
        let session_time_remain = {
            let v = read_f32(session, "SessionTimeRemain");
            if v.is_finite() && (0.0..48.0 * 3600.0).contains(&v) {
                Some(v)
            } else {
                None
            }
        };

        let laps_total = {
            let t = read_i32(session, "SessionLapsTotal");
            if let Some(n) = crate::telemetry::finite_laps_total(t) {
                n
            } else {
                crate::telemetry::finite_laps_total(cache.laps_total).unwrap_or(0)
            }
        };
        // Lap-limited races: remaining follows the lead lap, not the player.
        let session_laps_remain = if laps_total > 0 && lead_lap > 0 {
            Some((laps_total - lead_lap).max(0) as f32)
        } else {
            session_laps_remain_sdk
        };
        // Dash white: only after in-game FLAG_WHITE, until this car takes S/F
        // (do not synthesize from SessionLapsRemain — timed races sit near ~2 laps).
        let sdk_white = sf & FLAG_WHITE != 0;
        let (white_hud, show_white_hud) =
            update_white_flag_hud(cache.white_flag_hud, sdk_white, lap);
        cache.white_flag_hud = white_hud;
        flag = apply_white_flag_timing(flag, show_white_hud);
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
            camera_car_idx,
            in_car,
            in_garage,
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
            lat_accel,
            long_accel,
            vert_accel,
            yaw_rate,
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
            sr_delta: None,
            license: cache.license.clone(),
            track_num_turns: cache.track_num_turns,
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
            player_lat,
            player_lon,
            player_motion,
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
            tire_sets,
            have_hybrid,
            ers_battery_pct,
            ers_pct: ers_battery_pct,
            ers_boost_active,
            ers_p2p_active,
            fps,
            chan_quality,
            ffb_pct,
            pace_mode,
            pit_speed_limit_mps,
            pace_car_speed_mps,
            session_time_of_day,
            session_type,
            subsession_id: (cache.subsession_id > 0).then_some(cache.subsession_id),
            race_split,
            race_split_total,
            ..Default::default()
        }
    }

    /// Resolve pace-car ground speed into `cache.pace_speed_mps` and the pace-car row.
    /// Prefers live `CarIdxSpeed` when present; otherwise lap% Δ × track length.
    fn update_pace_car_speed(
        cache: &mut SessionCache,
        session_time: f64,
        player_lap_dist_m: f32,
        cars: &mut [CarRow],
    ) {
        let pace_i = cars.iter().position(|c| c.is_pace_car).or_else(|| {
            cache
                .pace_car_idx
                .and_then(|idx| cars.iter().position(|c| c.car_idx == idx))
        });
        let Some(pace_i) = pace_i else {
            cache.pace_speed_prev = None;
            return;
        };
        cars[pace_i].is_pace_car = true;

        // 1) Direct SDK speed on the pace-car index (unofficial / rare, but best).
        let live = cars[pace_i].speed_mps;
        if live.is_finite() && live > 0.5 {
            let a = 0.40_f32;
            cache.pace_speed_mps = if cache.pace_speed_mps > 0.5 {
                cache.pace_speed_mps * (1.0 - a) + live * a
            } else {
                live
            };
            let pct = cars[pace_i].lap_dist_pct;
            if pct.is_finite() && pct >= 0.0 {
                cache.pace_speed_prev = Some((session_time, pct));
            }
            cars[pace_i].speed_mps = cache.pace_speed_mps;
            return;
        }

        // 2) Derive from lap% progress × track length.
        let pct = cars[pace_i].lap_dist_pct;
        if !pct.is_finite() || pct < 0.0 {
            return;
        }
        let mut track_len = cache.track_length_m.unwrap_or(0.0);
        // Fallback: player LapDist / LapDistPct when YAML length is missing.
        if track_len <= 100.0 {
            let player_pct = cars
                .iter()
                .find(|c| c.is_player)
                .map(|c| c.lap_dist_pct)
                .unwrap_or(-1.0);
            if player_lap_dist_m > 50.0
                && player_pct.is_finite()
                && player_pct > 0.05
                && player_pct < 0.95
            {
                let est = player_lap_dist_m / player_pct;
                if est.is_finite() && est > 100.0 && est < 50_000.0 {
                    track_len = est;
                    cache.track_length_m = Some(est);
                }
            }
        }
        if track_len <= 100.0 {
            return;
        }
        if let Some((t0, p0)) = cache.pace_speed_prev {
            let dt = (session_time - t0) as f32;
            if dt > 0.05 && dt < 3.0 {
                let mut dp = pct - p0;
                if dp < -0.5 {
                    dp += 1.0;
                } else if dp > 0.5 {
                    dp -= 1.0;
                }
                // Ignore tiny backwards noise; take forward progress only.
                if dp > 1e-5 {
                    let inst = (dp * track_len / dt).clamp(0.0, 90.0);
                    if inst > 0.5 {
                        let a = 0.35_f32;
                        cache.pace_speed_mps = if cache.pace_speed_mps > 0.5 {
                            cache.pace_speed_mps * (1.0 - a) + inst * a
                        } else {
                            inst
                        };
                    }
                }
            }
        }
        cache.pace_speed_prev = Some((session_time, pct));
        if cache.pace_speed_mps > 0.5 {
            cars[pace_i].speed_mps = cache.pace_speed_mps;
        }
    }

    fn map_car_status_kind(surface: i32, on_pit: bool, car_flags: i32) -> Option<String> {
        if let Some(label) = map_session_flag_label(car_flags) {
            return Some(label);
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
    /// Pace-car / Race Control transmissions are shown (common under caution).
    unsafe fn build_radio_speaker(
        session: &Session,
        cache: &SessionCache,
        radio_idx: i32,
    ) -> Option<RadioSpeaker> {
        if radio_idx < 0 {
            return None;
        }
        let is_pace = cache
            .drivers
            .get(&radio_idx)
            .map(|d| d.is_pace_car)
            .unwrap_or(false);
        let (name, car_number) = if is_pace {
            // Pace car is driven by Race Control — show that under yellow.
            ("Race Control".into(), String::new())
        } else if let Some(d) = cache.drivers.get(&radio_idx) {
            (d.name.clone(), d.car_number.clone())
        } else {
            (format!("Car {radio_idx}"), format!("{radio_idx}"))
        };
        let position = if is_pace {
            0
        } else {
            int_arr(session, "CarIdxPosition")
                .and_then(|a| a.get(radio_idx as usize).copied())
                .filter(|&p| p > 0)
                .unwrap_or(0)
        };
        let country_code = if is_pace {
            None
        } else {
            cache.drivers.get(&radio_idx).and_then(driver_country_code)
        };
        Some(RadioSpeaker {
            position,
            car_number,
            name,
            active: true,
            is_player: !is_pace && radio_idx == cache.player_idx,
            is_pro: false,
            group_icon: String::new(),
            group_color: String::new(),
            country_code,
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
        // Formation start lights (before green).
        if sf & FLAG_START_GO != 0 {
            return Some("start_go".into());
        }
        if sf & FLAG_START_SET != 0 {
            return Some("start_set".into());
        }
        if sf & FLAG_START_READY != 0 {
            return Some("start_ready".into());
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
        _session_laps_remain: Option<f32>,
        session_time_remain: Option<f32>,
        lap: i32,
        laps_total: i32,
        position: i32,
    ) -> Option<String> {
        match flag? {
            "start_go" => Some("Green light — go".into()),
            "start_set" => Some("Start lights set".into()),
            "start_ready" => Some("Get ready — start imminent".into()),
            "yellow" | "caution" => {
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
            "green" => {
                if sf & FLAG_GREEN_HELD != 0 {
                    Some("Green held — stay in formation".into())
                } else {
                    Some("Track clear — racing resumes".into())
                }
            }
            "red" => {
                if let Some(remain) = session_time_remain.filter(|t| *t > 0.0) {
                    Some(format!("Session stopped — {} left", fmt_clock(remain)))
                } else {
                    Some("Session stopped — stand by".into())
                }
            }
            "white" => {
                if laps_total > 0 && lap > 0 {
                    Some(format!("Lap {lap} of {laps_total} — last lap next"))
                } else {
                    Some("White flag — last lap next".into())
                }
            }
            "blue" => Some("Faster car approaching — let them pass".into()),
            "black" => Some("Report to the pits — penalty".into()),
            "meatball" => Some("Mandatory pit — repairs required".into()),
            "furled" => Some("Warning — next infraction is a penalty".into()),
            "dq" => Some("Disqualified — exit the track".into()),
            "debris" => Some("Debris on track — reduce speed".into()),
            "crossed" => {
                if laps_total > 0 && lap > 0 {
                    let rem = (laps_total - lap).max(0);
                    Some(format!("Halfway — {rem} laps to go"))
                } else {
                    Some("Halfway point".into())
                }
            }
            "checkered" => {
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
        // iRacing: Off=0 Clear=1 Left=2 Right=3 Both=4 TwoLeft=5 TwoRight=6.
        // Read as i32 so Bitfield/Float layouts still map; enum TryFrom is stricter.
        match read_i32_opt(session, "CarLeftRight").unwrap_or(0) {
            2 => (true, false, false, false),
            3 => (false, true, false, false),
            4 => (true, true, false, false),
            5 => (true, false, true, false),
            6 => (false, true, false, true),
            _ => (false, false, false, false),
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
            Some(Value::Doubles(a)) => Some(a.iter().map(|v| *v as f32).collect()),
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
        session_type: &str,
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
        let best_laps_telem = float_arr(session, "CarIdxBestLapTime");
        // Open-qual / practice: CarIdxBestLapTime is often sparse (garage cars
        // drop to 0) while QualifyResultsInfo / ResultsPositions keep FastestTime.
        // Merge so standings match the iRacing Results board.
        let best_laps = if is_qualifying_session(session_type) {
            // Prefer QualifyResultsInfo only — Session ResultsPositions may still
            // hold the prior Practice block and would pollute qualify times.
            merge_best_lap_times(
                best_laps_telem.as_deref(),
                &cache.drivers,
                &cache.qualify_grid,
                &HashMap::new(),
            )
        } else {
            merge_best_lap_times(
                best_laps_telem.as_deref(),
                &cache.drivers,
                &HashMap::new(),
                &cache.results,
            )
        };
        let laps = int_arr(session, "CarIdxLap");
        let laps_done = int_arr(session, "CarIdxLapCompleted");
        let speeds = float_arr(session, "CarIdxSpeed");
        let steers = float_arr(session, "CarIdxSteer");
        let gears = int_arr(session, "CarIdxGear");
        let surface = int_arr(session, "CarIdxTrackSurface");
        let session_flags = int_arr(session, "CarIdxSessionFlags");
        let player_idx = cache.player_idx;
        let player_lap = laps
            .as_ref()
            .and_then(|a| a.get(player_idx as usize).copied())
            .unwrap_or(0);
        let lap_completed = read_i32(session, "LapCompleted");
        let use_results = session_state >= 5 && !cache.results.is_empty();
        let resolved = resolve_positions(
            cache,
            session_state,
            session_type,
            player_idx,
            lap_completed,
            positions.as_deref(),
            Some(best_laps.as_slice()),
            use_results,
        );

        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (i, &pct) in pcts.iter().enumerate() {
            let di = cache.drivers.get(&(i as i32));
            let is_pace_early = di.map(|d| d.is_pace_car).unwrap_or(false);
            let live_pos = positions
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(0)
                .max(0);
            let live_cpos = class_pos
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .unwrap_or(live_pos)
                .max(0);
            let (mut pos, mut cpos) = resolved_pos_for(i as i32, live_pos, live_cpos, &resolved);
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
            let last_lap_time_s = last_laps
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .filter(|s| s.is_finite() && *s > 0.0);
            let best_lap_time_s = best_laps
                .get(i)
                .copied()
                .filter(|s| s.is_finite() && *s > 0.0);
            let last_lap = last_lap_time_s
                .map(|s| fmt_laptime(s as f64, ""))
                .unwrap_or_default();
            let best_lap = best_lap_time_s
                .map(|s| fmt_laptime(s as f64, ""))
                .unwrap_or_default();
            let car_lap = laps.as_ref().and_then(|a| a.get(i).copied()).unwrap_or(0);
            let speed_mps = speeds
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .filter(|v| v.is_finite() && *v > 0.0)
                .unwrap_or(0.0);
            let steer_rad = steers
                .as_ref()
                .and_then(|a| a.get(i).copied())
                .filter(|v| v.is_finite())
                .unwrap_or(0.0);
            let gear = gears.as_ref().and_then(|a| a.get(i).copied()).unwrap_or(0);

            // Keep DriverInfo entries with a grid/qual position even when they
            // haven't joined (NOT_IN_WORLD / invalid LapDistPct).
            let has_driver = di.is_some();
            let joined = on_track || in_pit || pit;
            // Always keep the pace car so caution speed can be derived.
            let is_pace_idx = is_pace_early || cache.pace_car_idx == Some(i as i32);
            if !is_pace_idx {
                if (!pct.is_finite() || pct < 0.0)
                    && (!has_driver || (pos <= 0 && !joined && i as i32 != player_idx))
                {
                    continue;
                }
                if pct == 0.0 && i as i32 != player_idx && pos <= 0 && !has_driver && !joined {
                    continue;
                }
                if !has_driver && pos <= 0 && !joined && i as i32 != player_idx {
                    continue;
                }
            }

            let (name, number, ir, lic, class_color, class_id, is_pace, country_code) =
                if let Some(d) = di {
                    (
                        d.name.clone(),
                        d.car_number.clone(),
                        d.irating,
                        d.license.clone(),
                        d.class_color.clone(),
                        d.class_id,
                        d.is_pace_car || is_pace_idx,
                        driver_country_code(d),
                    )
                } else {
                    (
                        format!("Car {i}"),
                        format!("{i}"),
                        0,
                        String::new(),
                        String::new(),
                        0,
                        is_pace_idx,
                        None,
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
            let car_flag = map_session_flag_label(flags);
            let gap = fmt_car_gap(pos, if f2_t.is_finite() { f2_t } else { 0.0 });
            let lap_dist_pct = if pct.is_finite() && pct >= 0.0 {
                pct.rem_euclid(1.0)
            } else {
                -1.0
            };
            // Garage / not joined → greyed in standings & relative.
            let inactive = surf == TRK_NOT_IN_WORLD
                || lap_dist_pct < 0.0
                || (!on_track && !in_pit && !pit && pos > 0);

            out.push(CarRow {
                car_idx: i as i32,
                position: pos,
                class_position: cpos,
                car_number: number,
                name,
                gap,
                last_lap,
                best_lap,
                last_lap_time_s,
                best_lap_time_s,
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
                inactive,
                lap_dist_pct,
                est_time: if est_t.is_finite() { est_t } else { 0.0 },
                f2_time: if f2_t.is_finite() { f2_t } else { 0.0 },
                lap: car_lap.max(0),
                laps_completed,
                speed_mps,
                status_kind,
                car_flag,
                country_code,
                steer_rad,
                gear,
            });
            seen.insert(i as i32);
        }

        // Drivers registered in session YAML but missing from telem arrays —
        // still show on pre-race grid if they have a qualify slot.
        for (&idx, d) in &cache.drivers {
            if seen.contains(&idx) || d.is_pace_car {
                continue;
            }
            let live_pos = positions
                .as_ref()
                .and_then(|a| a.get(idx as usize).copied())
                .unwrap_or(0)
                .max(0);
            let live_cpos = class_pos
                .as_ref()
                .and_then(|a| a.get(idx as usize).copied())
                .unwrap_or(live_pos)
                .max(0);
            let (pos, cpos) = resolved_pos_for(idx, live_pos, live_cpos, &resolved);
            if pos <= 0 && idx != player_idx {
                continue;
            }
            out.push(CarRow {
                car_idx: idx,
                position: pos,
                class_position: cpos,
                car_number: d.car_number.clone(),
                name: d.name.clone(),
                gap: "—".into(),
                irating: d.irating,
                class_id: d.class_id,
                license: d.license.clone(),
                class_color: d.class_color.clone(),
                is_player: idx == player_idx,
                is_speaking: radio_idx == Some(idx),
                inactive: true,
                lap_dist_pct: -1.0,
                status_kind: Some("garage".into()),
                country_code: driver_country_code(d),
                ..Default::default()
            });
        }

        // Ensure transmitter appears for map/table speaking badges even when
        // car_rows filters them out (garage / invalid LapDistPct, etc.).
        if let Some(idx) = radio_idx {
            if idx >= 0 && !out.iter().any(|c| c.car_idx == idx) {
                let di = cache.drivers.get(&idx);
                let (name, number, ir, lic, class_color, class_id, is_pace, country_code) =
                    if let Some(d) = di {
                        (
                            if d.is_pace_car {
                                "Race Control".into()
                            } else {
                                d.name.clone()
                            },
                            if d.is_pace_car {
                                String::new()
                            } else {
                                d.car_number.clone()
                            },
                            d.irating,
                            d.license.clone(),
                            d.class_color.clone(),
                            d.class_id,
                            d.is_pace_car,
                            if d.is_pace_car {
                                None
                            } else {
                                driver_country_code(d)
                            },
                        )
                    } else {
                        (
                            format!("Car {idx}"),
                            format!("{idx}"),
                            0,
                            String::new(),
                            String::new(),
                            0,
                            false,
                            None,
                        )
                    };
                let live_pos = positions
                    .as_ref()
                    .and_then(|a| a.get(idx as usize).copied())
                    .unwrap_or(0)
                    .max(0);
                let (pos, cpos) = resolved_pos_for(idx, live_pos, live_pos, &resolved);
                out.push(CarRow {
                    car_idx: idx,
                    position: if is_pace { 0 } else { pos },
                    class_position: if is_pace { 0 } else { cpos },
                    car_number: number,
                    name,
                    gap: "—".into(),
                    irating: ir,
                    class_id,
                    license: lic,
                    class_color,
                    is_player: !is_pace && idx == player_idx,
                    is_speaking: true,
                    is_pace_car: is_pace,
                    inactive: !is_pace,
                    lap_dist_pct: -1.0,
                    status_kind: Some(if is_pace {
                        "pace".into()
                    } else {
                        "garage".into()
                    }),
                    country_code,
                    ..Default::default()
                });
            }
        }
        out
    }

    /// Resolved overall + class positions (Python `_resolve_positions`).
    struct ResolvedPositions {
        /// Per CarIdx override; missing keys fall back to live telem unless strict.
        by_idx: HashMap<i32, ResultPosEntry>,
        strict: bool,
    }

    fn is_qualifying_session(session_type: &str) -> bool {
        session_type.to_ascii_lowercase().contains("qual")
    }

    fn is_practice_session(session_type: &str) -> bool {
        let s = session_type.to_ascii_lowercase();
        s.contains("practice") || s == "open"
    }

    /// Merge live `CarIdxBestLapTime` with YAML FastestTime so open-qual standings
    /// keep times for cars that left the track / went to garage.
    fn merge_best_lap_times(
        telem: Option<&[f32]>,
        drivers: &HashMap<i32, DriverInfo>,
        qualify: &HashMap<i32, ResultPosEntry>,
        results: &HashMap<i32, ResultPosEntry>,
    ) -> Vec<f32> {
        let mut n = telem.map(|t| t.len()).unwrap_or(0);
        for &idx in drivers.keys().chain(qualify.keys()).chain(results.keys()) {
            n = n.max(idx as usize + 1);
        }
        let mut out = vec![0.0_f32; n];
        if let Some(t) = telem {
            for (i, &v) in t.iter().enumerate() {
                if v.is_finite() && v > 0.0 {
                    out[i] = v;
                }
            }
        }
        let absorb = |out: &mut [f32], map: &HashMap<i32, ResultPosEntry>| {
            for (&idx, e) in map {
                if !(e.fastest_time.is_finite() && e.fastest_time > 0.0) {
                    continue;
                }
                let i = idx as usize;
                if i >= out.len() {
                    continue;
                }
                if out[i] <= 0.0 || e.fastest_time < out[i] {
                    out[i] = e.fastest_time;
                }
            }
        };
        absorb(&mut out, qualify);
        absorb(&mut out, results);
        out
    }

    /// Open-qual / practice: rank by best lap (not sparse CarIdxPosition).
    fn use_best_lap_positions(session_type: &str) -> bool {
        is_qualifying_session(session_type) || is_practice_session(session_type)
    }

    fn prefer_grid_positions(
        live: Option<&[i32]>,
        _player_idx: i32,
        grid: &HashMap<i32, ResultPosEntry>,
        session_state: i32,
        lap_completed: i32,
    ) -> bool {
        if grid.values().all(|e| e.position <= 0) {
            return false;
        }
        // Warmup / parade / not racing yet.
        if session_state < 4 {
            return true;
        }
        if lap_completed < 0 {
            return true;
        }
        let Some(live) = live else {
            return true;
        };
        let valid_live = live.iter().filter(|&&p| p > 0).count();
        if valid_live == 0 {
            return true;
        }
        // Live race ranks are published for the field — use them. Do NOT fall
        // back to the qualify grid just because the local player car has no
        // position (normal while spectating / in garage during a race).
        let grid_count = grid.values().filter(|e| e.position > 0).count();
        if grid_count >= 2 && valid_live < (2).max(grid_count / 2) {
            return true;
        }
        false
    }

    /// Numeric car-number key for qualifying no-time tiebreaks (`"16"` → 16).
    fn car_number_sort_key(num: &str) -> (i32, String) {
        let digits: String = num.chars().filter(|c| c.is_ascii_digit()).collect();
        let n = digits.parse::<i32>().unwrap_or(i32::MAX);
        (n, num.to_ascii_lowercase())
    }

    /// Qualifying order: fastest best-lap first; drivers with no time follow,
    /// sorted by car number (not race/live position).
    fn positions_from_best_lap(
        best: Option<&[f32]>,
        drivers: &HashMap<i32, DriverInfo>,
    ) -> HashMap<i32, ResultPosEntry> {
        struct Cand {
            idx: i32,
            class_id: i32,
            time: Option<f32>,
            car_key: (i32, String),
        }
        let mut cands: Vec<Cand> = Vec::new();
        for (&idx, d) in drivers {
            if d.is_pace_car {
                continue;
            }
            let time = best
                .and_then(|b| b.get(idx as usize))
                .copied()
                .filter(|t| t.is_finite() && *t > 0.0);
            cands.push(Cand {
                idx,
                class_id: d.class_id,
                time,
                car_key: car_number_sort_key(&d.car_number),
            });
        }
        cands.sort_by(|a, b| {
            match (a.time, b.time) {
                (Some(ta), Some(tb)) => ta
                    .partial_cmp(&tb)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.car_key.cmp(&b.car_key)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.car_key.cmp(&b.car_key),
            }
            .then_with(|| a.idx.cmp(&b.idx))
        });
        let mut out = HashMap::new();
        let mut class_rank: HashMap<i32, i32> = HashMap::new();
        for (rank, c) in cands.into_iter().enumerate() {
            let p = (rank + 1) as i32;
            let next = class_rank.entry(c.class_id).or_insert(0);
            *next += 1;
            out.insert(
                c.idx,
                ResultPosEntry {
                    position: p,
                    class_position: *next,
                    laps_complete: 0,
                    fastest_time: c.time.unwrap_or(0.0),
                },
            );
        }
        out
    }

    fn resolve_positions(
        cache: &SessionCache,
        session_state: i32,
        session_type: &str,
        player_idx: i32,
        lap_completed: i32,
        live: Option<&[i32]>,
        best_laps: Option<&[f32]>,
        use_results: bool,
    ) -> ResolvedPositions {
        if use_results {
            return ResolvedPositions {
                by_idx: cache.results.clone(),
                strict: false,
            };
        }
        if use_best_lap_positions(session_type) {
            // Open-qual / practice: rank by best lap (telem merged with YAML
            // FastestTime upstream). CarIdxPosition is sparse and not by time.
            let blap = positions_from_best_lap(best_laps, &cache.drivers);
            if !blap.is_empty() {
                return ResolvedPositions {
                    by_idx: blap,
                    strict: true,
                };
            }
            if is_qualifying_session(session_type)
                && cache.qualify_grid.values().any(|e| e.position > 0)
            {
                return ResolvedPositions {
                    by_idx: cache.qualify_grid.clone(),
                    strict: false,
                };
            }
            return ResolvedPositions {
                by_idx: HashMap::new(),
                strict: false,
            };
        }
        if prefer_grid_positions(
            live,
            player_idx,
            &cache.qualify_grid,
            session_state,
            lap_completed,
        ) {
            return ResolvedPositions {
                by_idx: cache.qualify_grid.clone(),
                strict: true,
            };
        }
        ResolvedPositions {
            by_idx: HashMap::new(),
            strict: false,
        }
    }

    fn resolved_pos_for(
        idx: i32,
        live_pos: i32,
        live_cpos: i32,
        resolved: &ResolvedPositions,
    ) -> (i32, i32) {
        if let Some(e) = resolved.by_idx.get(&idx) {
            let pos = if e.position > 0 {
                e.position
            } else if !resolved.strict {
                live_pos
            } else {
                0
            };
            let cpos = if e.class_position > 0 {
                e.class_position
            } else if pos > 0 {
                pos
            } else if !resolved.strict {
                live_cpos
            } else {
                0
            };
            return (pos, cpos);
        }
        if resolved.strict {
            (0, 0)
        } else {
            (live_pos, live_cpos)
        }
    }

    fn parse_qualify_results(yaml: &str) -> HashMap<i32, ResultPosEntry> {
        let mut out = HashMap::new();
        let mut in_qualify = false;
        let mut in_results = false;
        let mut cur_idx: Option<i32> = None;
        let mut cur = ResultPosEntry::default();
        // Track whether Position/ClassPosition appeared — both are 0-based in
        // QualifyResultsInfo (pole = 0), unlike SessionInfo ResultsPositions.
        let mut saw_position = false;
        let mut saw_class_position = false;

        let flush = |map: &mut HashMap<i32, ResultPosEntry>,
                     idx: &mut Option<i32>,
                     e: &mut ResultPosEntry,
                     saw_pos: &mut bool,
                     saw_cpos: &mut bool| {
            if let Some(i) = idx.take() {
                if *saw_pos || *saw_cpos {
                    // Convert 0-based qualify ranks to 1-based overlay positions.
                    let pos = if *saw_pos { e.position + 1 } else { 0 };
                    let cpos = if *saw_cpos {
                        e.class_position + 1
                    } else if pos > 0 {
                        pos
                    } else {
                        0
                    };
                    if pos > 0 || cpos > 0 {
                        map.insert(
                            i,
                            ResultPosEntry {
                                position: pos,
                                class_position: cpos,
                                laps_complete: e.laps_complete,
                                fastest_time: e.fastest_time,
                            },
                        );
                    }
                }
            }
            *e = ResultPosEntry::default();
            *saw_pos = false;
            *saw_cpos = false;
        };

        for line in yaml.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("QualifyResultsInfo:") {
                in_qualify = true;
                in_results = false;
                continue;
            }
            if in_qualify {
                let indent = line.len() - line.trim_start().len();
                if !trimmed.is_empty()
                    && indent == 0
                    && trimmed.contains(':')
                    && !trimmed.starts_with('-')
                    && !trimmed.starts_with("QualifyResultsInfo:")
                {
                    flush(
                        &mut out,
                        &mut cur_idx,
                        &mut cur,
                        &mut saw_position,
                        &mut saw_class_position,
                    );
                    break;
                }
            }
            if !in_qualify {
                continue;
            }
            if trimmed.starts_with("Results:") {
                in_results = true;
                continue;
            }
            if !in_results {
                continue;
            }
            if trimmed.starts_with("- ") || trimmed == "-" {
                flush(
                    &mut out,
                    &mut cur_idx,
                    &mut cur,
                    &mut saw_position,
                    &mut saw_class_position,
                );
                if let Some(rest) = trimmed.strip_prefix("- ") {
                    if let Some((k, v)) = rest.split_once(':') {
                        let key = k.trim();
                        apply_result_field(&mut cur_idx, &mut cur, key, v.trim());
                        if key == "Position" {
                            saw_position = true;
                        } else if key == "ClassPosition" {
                            saw_class_position = true;
                        }
                    }
                }
                continue;
            }
            if let Some((k, v)) = trimmed.split_once(':') {
                let key = k.trim();
                apply_result_field(&mut cur_idx, &mut cur, key, v.trim());
                if key == "Position" {
                    saw_position = true;
                } else if key == "ClassPosition" {
                    saw_class_position = true;
                }
            }
        }
        flush(
            &mut out,
            &mut cur_idx,
            &mut cur,
            &mut saw_position,
            &mut saw_class_position,
        );
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
        if let Some(v) = yaml_i32(yaml, "TrackNumTurns") {
            if v > 0 && v < 500 {
                cache.track_num_turns = v;
            }
        }
        if let Some(v) = yaml_i32(yaml, "LeagueID") {
            cache.league_id = if v > 0 { Some(v) } else { None };
        }
        if let Some(n) =
            yaml_str(yaml, "TrackDisplayShortName").or_else(|| yaml_str(yaml, "TrackDisplayName"))
        {
            cache.track_name = Some(n.trim_matches('"').to_string());
        }
        if let Some(v) = yaml_f32(yaml, "TrackPitSpeedLimit") {
            if v.is_finite() && v > 0.0 {
                // WeekendInfo documents this as kph.
                cache.pit_speed_limit_mps = Some(v / 3.6);
            }
        }
        if let Some(raw) =
            yaml_str(yaml, "TrackLength").or_else(|| yaml_str(yaml, "TrackLengthOfficial"))
        {
            if let Some(m) = parse_track_length_m(&raw) {
                cache.track_length_m = Some(m);
            }
        }
        if let Some(v) = yaml_f32(yaml, "DriverCarRedLine") {
            if v > 100.0 {
                cache.redline = v;
            }
        }
        if let Some(v) = yaml_i32(yaml, "SessionLaps") {
            if let Some(n) = crate::telemetry::finite_laps_total(v) {
                cache.laps_total = n;
            }
        }
        if let Some(v) = yaml_i32(yaml, "IncidentLimit") {
            if v > 0 && v < 10_000 {
                cache.incidents_limit = v;
            }
        }
        cache.session_types = parse_session_types(yaml);
        cache.race_split = parse_race_split(yaml);
        cache.race_split_total = parse_race_split_total(yaml);
        cache.subsession_id = yaml_i32(yaml, "SubSessionID").unwrap_or(0);
        cache.drivers = parse_drivers(yaml);
        cache.results = parse_results_positions(yaml);
        cache.qualify_grid = parse_qualify_results(yaml);
        if let Some(idx) = yaml_i32(yaml, "PaceCarIdx") {
            if idx >= 0 {
                cache.pace_car_idx = Some(idx);
                if let Some(d) = cache.drivers.get_mut(&idx) {
                    d.is_pace_car = true;
                }
            }
        } else {
            cache.pace_car_idx = cache
                .drivers
                .values()
                .find(|d| d.is_pace_car)
                .map(|d| d.car_idx);
        }
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
            if !d.license.is_empty() {
                cache.license = d.license.clone();
            }
        }
    }

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
        if !current.is_empty() && (block_is_race || best.is_empty()) {
            best = current;
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
            "FastestTime" | "Time" => {
                if let Ok(t) = val.parse::<f32>() {
                    if t.is_finite() && t > 0.0 {
                        cur.fastest_time = t;
                    }
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
            } else if let Some(v) = kv(t, "LicLevel") {
                d.lic_level = v.parse().unwrap_or(0);
            } else if let Some(v) = kv(t, "LicSubLevel") {
                d.lic_sub_level = v.parse().unwrap_or(0);
            } else if let Some(v) = kv(t, "CarClassColor") {
                // iRacing packs RGB in the low 24 bits (sometimes with high bits set).
                if let Ok(n) = v.parse::<u32>() {
                    d.class_color = format!("#{:06x}", n & 0x00FF_FFFF);
                } else {
                    d.class_color = unquote(v);
                }
            } else if let Some(v) = kv(t, "CarIsPaceCar") {
                d.is_pace_car = v == "1" || v.eq_ignore_ascii_case("true");
            } else if let Some(v) = kv(t, "ClubName") {
                d.club_name = unquote(v);
            } else if let Some(v) = kv(t, "FlairID") {
                d.flair_id = v.parse().unwrap_or(0);
            } else if let Some(v) = kv(t, "FlairName") {
                d.flair_name = unquote(v);
            }
        }
        flush(&mut cur, &mut out);
        out
    }

    fn driver_country_code(d: &DriverInfo) -> Option<String> {
        crate::country_flags::resolve_country_code(d.flair_id, &d.flair_name, &d.club_name)
            .map(|c| c.to_string())
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
                let rest = rest.trim().trim_matches('"');
                // Accept `80`, `80.0`, `80.0 km/h` — take the leading float token.
                let token = rest
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .find(|s| !s.is_empty())?;
                if let Ok(v) = token.parse::<f32>() {
                    return Some(v);
                }
            }
        }
        None
    }

    /// WeekendInfo `TrackLength` is usually `"0.75 mi"` / `"4.02 km"`, not meters.
    fn parse_track_length_m(raw: &str) -> Option<f32> {
        let t = raw.trim().trim_matches('"');
        let mut parts = t.split_whitespace();
        let n: f32 = parts.next()?.parse().ok()?;
        if !n.is_finite() || n <= 0.0 {
            return None;
        }
        let unit = parts.next().unwrap_or("").to_ascii_lowercase();
        let meters = if unit.starts_with("mi") {
            n * 1609.344
        } else if unit.starts_with("km") {
            n * 1000.0
        } else if unit.starts_with('m') && !unit.starts_with("mi") {
            n
        } else if n < 50.0 {
            // Bare small number → miles (iRacing WeekendInfo convention).
            n * 1609.344
        } else {
            n
        };
        (meters > 100.0).then_some(meters)
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

    /// Telemetry `TrackPitSpeedLimit` is m/s; some dumps / YAML use kph.
    fn normalize_pit_speed_limit(v: f32) -> f32 {
        if v > 40.0 {
            v / 3.6
        } else {
            v
        }
    }

    unsafe fn read_f64(session: &Session, name: &str) -> f64 {
        read_f64_opt(session, name).unwrap_or(0.0)
    }

    /// Whether the sim reports world position decides if map calibration is even
    /// possible, and an absent variable looks identical to a zeroed one from the
    /// frame alone — so say which of them exist, once, when the car is on track.
    unsafe fn warn_position_vars_once(session: &Session) {
        use std::sync::atomic::{AtomicBool, Ordering};
        static WARNED: AtomicBool = AtomicBool::new(false);
        if WARNED.swap(true, Ordering::Relaxed) {
            return;
        }
        let mut parts = Vec::new();
        for name in [
            "Lat",
            "Lon",
            "Alt",
            "VelocityX",
            "VelocityY",
            "Yaw",
            "YawNorth",
        ] {
            match session.find_var(name) {
                Some(var) => parts.push(format!("{name}={:?}", session.var_value(&var))),
                None => parts.push(format!("{name}=absent")),
            }
        }
        eprintln!("[gridglance] position telemetry: {}", parts.join(" "));
    }

    unsafe fn read_f64_opt(session: &Session, name: &str) -> Option<f64> {
        let var = session.find_var(name)?;
        match session.var_value(&var) {
            Value::Double(v) => Some(v),
            Value::Float(v) => Some(v as f64),
            Value::Int(v) => Some(v as f64),
            _ => session.value::<f64>(&var).ok(),
        }
    }

    unsafe fn read_i32(session: &Session, name: &str) -> i32 {
        read_i32_opt(session, name).unwrap_or(0)
    }

    unsafe fn read_i32_opt(session: &Session, name: &str) -> Option<i32> {
        let var = session.find_var(name)?;
        match session.var_value(&var) {
            Value::Int(v) | Value::Bitfield(v) => Some(v),
            Value::Ints(a) | Value::Bitfields(a) => a.first().copied(),
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

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn qualify_results_zero_based_pole_and_field() {
            let yaml = r#"
QualifyResultsInfo:
  Results:
  - CarIdx: 10
    Position: 0
    ClassPosition: 0
    FastestTime: 22.100
  - CarIdx: 7
    Position: 1
    ClassPosition: 1
    FastestTime: 22.200
  - CarIdx: 3
    Position: 2
    ClassPosition: 0
    FastestTime: 22.300
SessionInfo:
  Sessions:
"#;
            let grid = parse_qualify_results(yaml);
            assert_eq!(
                grid.get(&10).map(|e| e.position),
                Some(1),
                "pole is Position:0 → P1"
            );
            assert_eq!(grid.get(&10).map(|e| e.class_position), Some(1));
            assert_eq!(grid.get(&10).map(|e| e.fastest_time), Some(22.100));
            assert_eq!(grid.get(&7).map(|e| e.position), Some(2));
            assert_eq!(grid.get(&7).map(|e| e.fastest_time), Some(22.200));
            assert_eq!(grid.get(&3).map(|e| e.position), Some(3));
            assert_eq!(grid.get(&3).map(|e| e.fastest_time), Some(22.300));
            // Multiclass: car 3 is class pole (ClassPosition: 0 → 1) while overall P3.
            assert_eq!(grid.get(&3).map(|e| e.class_position), Some(1));
        }

        #[test]
        fn merge_best_laps_fills_sparse_telem_from_qualify_yaml() {
            let mut drivers = HashMap::new();
            for i in 1..=4 {
                drivers.insert(
                    i,
                    DriverInfo {
                        car_idx: i,
                        car_number: format!("{i}"),
                        ..Default::default()
                    },
                );
            }
            // Only pole still has a live CarIdxBestLapTime; others cleared in garage.
            let telem = [0.0_f32, 91.186, 0.0, 0.0, 0.0];
            let mut qualify = HashMap::new();
            qualify.insert(
                1,
                ResultPosEntry {
                    position: 1,
                    class_position: 1,
                    fastest_time: 91.186,
                    ..Default::default()
                },
            );
            qualify.insert(
                2,
                ResultPosEntry {
                    position: 2,
                    class_position: 2,
                    fastest_time: 92.137,
                    ..Default::default()
                },
            );
            qualify.insert(
                3,
                ResultPosEntry {
                    position: 3,
                    class_position: 3,
                    fastest_time: 92.148,
                    ..Default::default()
                },
            );
            qualify.insert(
                4,
                ResultPosEntry {
                    position: 4,
                    class_position: 4,
                    fastest_time: 92.653,
                    ..Default::default()
                },
            );
            let merged = merge_best_lap_times(Some(&telem), &drivers, &qualify, &HashMap::new());
            assert!((merged[1] - 91.186).abs() < 0.001);
            assert!((merged[2] - 92.137).abs() < 0.001);
            assert!((merged[3] - 92.148).abs() < 0.001);
            assert!((merged[4] - 92.653).abs() < 0.001);
            let ranked = positions_from_best_lap(Some(&merged), &drivers);
            assert_eq!(ranked.get(&1).map(|e| e.position), Some(1));
            assert_eq!(ranked.get(&2).map(|e| e.position), Some(2));
            assert_eq!(ranked.get(&3).map(|e| e.position), Some(3));
            assert_eq!(ranked.get(&4).map(|e| e.position), Some(4));
        }

        #[test]
        fn positions_from_best_lap_ranks_overall_and_class() {
            let mut drivers = HashMap::new();
            drivers.insert(
                1,
                DriverInfo {
                    car_idx: 1,
                    class_id: 100,
                    car_number: "10".into(),
                    ..Default::default()
                },
            );
            drivers.insert(
                2,
                DriverInfo {
                    car_idx: 2,
                    class_id: 200,
                    car_number: "20".into(),
                    ..Default::default()
                },
            );
            drivers.insert(
                3,
                DriverInfo {
                    car_idx: 3,
                    class_id: 100,
                    car_number: "3".into(),
                    ..Default::default()
                },
            );
            // idx 0 unused; 1=22.2, 2=22.0 (other class), 3=22.1
            let best = [0.0_f32, 22.2, 22.0, 22.1];
            let ranked = positions_from_best_lap(Some(&best), &drivers);
            assert_eq!(ranked.get(&2).map(|e| e.position), Some(1));
            assert_eq!(ranked.get(&3).map(|e| e.position), Some(2));
            assert_eq!(ranked.get(&1).map(|e| e.position), Some(3));
            // Class 100: car 3 then car 1
            assert_eq!(ranked.get(&3).map(|e| e.class_position), Some(1));
            assert_eq!(ranked.get(&1).map(|e| e.class_position), Some(2));
            // Class 200: only car 2
            assert_eq!(ranked.get(&2).map(|e| e.class_position), Some(1));
        }

        #[test]
        fn positions_from_best_lap_no_time_by_car_number() {
            let mut drivers = HashMap::new();
            drivers.insert(
                1,
                DriverInfo {
                    car_idx: 1,
                    car_number: "16".into(),
                    ..Default::default()
                },
            );
            drivers.insert(
                2,
                DriverInfo {
                    car_idx: 2,
                    car_number: "3".into(),
                    ..Default::default()
                },
            );
            drivers.insert(
                3,
                DriverInfo {
                    car_idx: 3,
                    car_number: "15".into(),
                    ..Default::default()
                },
            );
            // Only #3 has a time — untimed cars follow by car number (#15 then #16).
            let best = [0.0_f32, 0.0, 40.0, 0.0];
            let ranked = positions_from_best_lap(Some(&best), &drivers);
            assert_eq!(ranked.get(&2).map(|e| e.position), Some(1));
            assert_eq!(ranked.get(&3).map(|e| e.position), Some(2));
            assert_eq!(ranked.get(&1).map(|e| e.position), Some(3));

            // No times yet: pure car-number order (#3, #15, #16).
            let ranked_all = positions_from_best_lap(None, &drivers);
            assert_eq!(ranked_all.get(&2).map(|e| e.position), Some(1));
            assert_eq!(ranked_all.get(&3).map(|e| e.position), Some(2));
            assert_eq!(ranked_all.get(&1).map(|e| e.position), Some(3));
        }

        #[test]
        fn prefer_grid_before_green() {
            let mut grid = HashMap::new();
            grid.insert(
                1,
                ResultPosEntry {
                    position: 1,
                    class_position: 1,
                    laps_complete: 0,
                    ..Default::default()
                },
            );
            assert!(prefer_grid_positions(Some(&[0, 0]), 1, &grid, 2, -1));
            assert!(!prefer_grid_positions(Some(&[0, 3]), 1, &grid, 4, 2));
        }

        #[test]
        fn prefer_live_race_positions_while_spectating() {
            // Player car has no live position (spectating / garage), but the
            // field already has race ranks — must NOT lock to qualify grid.
            let mut grid = HashMap::new();
            for i in 1..=8 {
                grid.insert(
                    i,
                    ResultPosEntry {
                        position: i,
                        class_position: i,
                        laps_complete: 0,
                        ..Default::default()
                    },
                );
            }
            // idx0 unused; player at idx1 pos 0; others have live race positions.
            let live = [0, 0, 1, 2, 3, 4, 5, 6, 7];
            assert!(
                !prefer_grid_positions(Some(&live), 1, &grid, 4, 12),
                "spectating during a race must use live CarIdxPosition"
            );
        }

        #[test]
        fn parse_drivers_reads_flair_and_club() {
            let yaml = r#"
DriverInfo:
  DriverCarIdx: 0
  Drivers:
  - CarIdx: 0
    UserName: Alice
    ClubName: Florida
    FlairID: 223
    FlairName: United States
  - CarIdx: 1
    UserName: Bob
    ClubName: Brazil
    FlairID: 31
    FlairName: Brazil
  - CarIdx: 2
    UserName: Cara
    ClubName: International
    FlairID: 1
    FlairName: Unaffiliated
"#;
            let drivers = parse_drivers(yaml);
            assert_eq!(drivers.get(&0).map(|d| d.flair_id), Some(223));
            assert_eq!(
                drivers.get(&0).and_then(driver_country_code).as_deref(),
                Some("us")
            );
            assert_eq!(
                drivers.get(&1).and_then(driver_country_code).as_deref(),
                Some("br")
            );
            // Unaffiliated flair, International club → no country.
            assert_eq!(drivers.get(&2).and_then(driver_country_code), None);
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
