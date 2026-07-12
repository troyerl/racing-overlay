//! Live iRacing shared-memory reader (Windows). Non-Windows uses a stub.

use super::TelemetryFrame;

#[cfg(windows)]
mod win {
    use crate::telemetry::{CarRow, TelemetryFrame};
    use iracing_telem::{flags, Client, DataUpdateResult, Session, Value};

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
    const FLAG_CAUTION: i32 = 0x0000_4000;
    const FLAG_CAUTION_WAVING: i32 = 0x0000_8000;
    const FLAG_BLACK: i32 = 0x0001_0000;
    const FLAG_DQ: i32 = 0x0002_0000;
    const FLAG_FURLED: i32 = 0x0008_0000;
    const FLAG_REPAIR: i32 = 0x0010_0000;

    #[derive(Default)]
    struct SessionCache {
        update: i32,
        player_idx: i32,
        car_number: String,
        irating: i32,
        track_id: Option<i32>,
        track_name: Option<String>,
        redline: f32,
        laps_total: i32,
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
        let lf = read_f32(session, "LFwearM");
        let rf = read_f32(session, "RFwearM");
        let lr = read_f32(session, "LRwearM");
        let rr = read_f32(session, "RRwearM");
        let air_temp = read_f32_opt(session, "AirTemp");
        let track_temp = read_f32_opt(session, "TrackTempCrew")
            .or_else(|| read_f32_opt(session, "TrackTemp"));

        let position = player_position(session, cache.player_idx);
        let sf = read_i32(session, "SessionFlags");
        let flag = map_flag(sf);

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
        let (radar_left, radar_right) = car_left_right(session);

        let cars = car_rows(session, cache.player_idx);
        let fuel_use = read_f32(session, "FuelUsePerHour");
        let laps_fuel = if fuel_use > 0.05 && last_lap > 10.0 {
            (fuel_l / (fuel_use / 3600.0 * last_lap)).max(0.0)
        } else {
            0.0
        };

        let laps_total = {
            let t = read_i32(session, "SessionLapsTotal");
            if t > 0 && t < 100_000 {
                t
            } else {
                cache.laps_total
            }
        };

        TelemetryFrame {
            connected: true,
            session_time,
            flag,
            flag_context: None,
            incident_warn: false,
            secondary: None,
            delta: None,
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
            position,
            car_number: cache.car_number.clone(),
            lap,
            laps_total,
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
            player_lap_dist_pct: lap_dist,
            track_id: cache.track_id,
            track_name: cache.track_name.clone(),
            radar_left,
            radar_right,
            cars,
            ..Default::default()
        }
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
        if sf
            & (FLAG_YELLOW
                | FLAG_YELLOW_WAVING
                | FLAG_CAUTION
                | FLAG_CAUTION_WAVING)
            != 0
        {
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

    unsafe fn car_left_right(session: &Session) -> (bool, bool) {
        let Some(var) = session.find_var("CarLeftRight") else {
            return (false, false);
        };
        match session.value::<flags::CarLeftRight>(&var) {
            Ok(v) => {
                use flags::CarLeftRight::*;
                match v {
                    CarLeft | TwoCarsLeft => (true, false),
                    CarRight | TwoCarsRight => (false, true),
                    CarLeftRight => (true, true),
                    _ => (false, false),
                }
            }
            Err(_) => (false, false),
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

    unsafe fn car_rows(session: &Session, player_idx: i32) -> Vec<CarRow> {
        let Some(Value::Floats(pcts)) = session
            .find_var("CarIdxLapDistPct")
            .map(|v| session.var_value(&v))
        else {
            return Vec::new();
        };
        let on_pit = session
            .find_var("CarIdxOnPitRoad")
            .map(|v| session.var_value(&v));
        let positions = session
            .find_var("CarIdxPosition")
            .map(|v| session.var_value(&v));

        let mut out = Vec::new();
        for (i, &pct) in pcts.iter().enumerate() {
            if !pct.is_finite() || pct < 0.0 {
                continue;
            }
            // Skip empty slots (often -1 pct or zero with no car)
            if pct == 0.0 && i as i32 != player_idx {
                // still include if they have a position
                let pos = match &positions {
                    Some(Value::Ints(a)) => a.get(i).copied().unwrap_or(0),
                    _ => 0,
                };
                if pos <= 0 {
                    continue;
                }
            }
            let pos = match &positions {
                Some(Value::Ints(a)) => a.get(i).copied().unwrap_or(0),
                _ => 0,
            };
            let pit = match &on_pit {
                Some(Value::Bools(a)) => a.get(i).copied().unwrap_or(false),
                _ => false,
            };
            out.push(CarRow {
                car_idx: i as i32,
                position: pos.max(0),
                class_position: pos.max(0),
                car_number: format!("{}", i),
                name: format!("Car {i}"),
                gap: String::new(),
                last_lap: String::new(),
                best_lap: String::new(),
                irating: 0,
                license: String::new(),
                on_pit: pit,
                is_player: i as i32 == player_idx,
                is_speaking: false,
                lap_dist_pct: pct.fract().abs(),
            });
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
        if let Some(n) = yaml_str(yaml, "TrackDisplayShortName")
            .or_else(|| yaml_str(yaml, "TrackDisplayName"))
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

    pub fn tick(&mut self) -> TelemetryFrame {
        TelemetryFrame {
            connected: false,
            redline: 8000.0,
            ..Default::default()
        }
    }
}
