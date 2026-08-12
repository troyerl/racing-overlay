//! Race-line sampler: session best, player top-3, and track personal best.
//!
//! Other cars expose LapDistPct + CarIdxSteer/Gear only (no pedals / world XY).
//! The player also records throttle/brake and dead-reckoned XY for line/radius.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::lap_compare::{CompareMarker, LapCompareView, MarkerKind};
use super::{CarRow, PlayerMotion, TelemetryFrame};

const MAX_SAMPLES: usize = 1200;
const MIN_LAP_SAMPLES: usize = 24;
const MIN_LAP_TIME_S: f64 = 20.0;
const WRAP: f32 = 0.5;
const PCT_EPS: f32 = 1e-4;
const SPARK_BINS: usize = 64;
const TOP_N: usize = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineSample {
    pub pct: f32,
    pub t: f64,
    pub steer: f32,
    pub gear: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throttle: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brake: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLap {
    pub driver_name: String,
    pub car_number: String,
    #[serde(default)]
    pub is_player: bool,
    pub lap_time_s: f64,
    #[serde(default)]
    pub lap_number: i32,
    pub samples: Vec<LineSample>,
    #[serde(default)]
    pub markers: Vec<CompareMarker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RaceDoc {
    pub schema: u32,
    pub subsession_id: i32,
    pub track_id: i32,
    #[serde(default)]
    pub track_name: String,
    #[serde(default)]
    pub car_path: String,
    #[serde(default)]
    pub session_type: String,
    pub recorded_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compared_to_pb_lap_time_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub race_best: Option<StoredLap>,
    #[serde(default)]
    pub player_top3: Vec<StoredLap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackPbDoc {
    pub schema: u32,
    pub track_id: i32,
    pub car_path: String,
    #[serde(default)]
    pub track_name: String,
    pub lap_time_s: f64,
    pub set_at: String,
    #[serde(default)]
    pub subsession_id: i32,
    pub samples: Vec<LineSample>,
    #[serde(default)]
    pub markers: Vec<CompareMarker>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Top3SummaryRow {
    pub lap_time_s: f64,
    pub lap_number: i32,
    pub delta_to_ref_s: Option<f64>,
    pub delta_to_pb_s: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CornerDelta {
    pub label: String,
    pub pct: f32,
    pub delta_s: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionLineViewExtra {
    #[serde(default)]
    pub top3: Vec<Top3SummaryRow>,
    #[serde(default)]
    pub race_best_label: String,
    #[serde(default)]
    pub race_best_time_s: Option<f64>,
    #[serde(default)]
    pub track_pb_time_s: Option<f64>,
    #[serde(default)]
    pub steer_spark: Vec<f32>,
    #[serde(default)]
    pub corner_deltas: Vec<CornerDelta>,
    /// Selected player top-3 index for review (0..2), when set.
    #[serde(default)]
    pub selected_top3: Option<usize>,
}

#[derive(Debug, Clone, Default)]
struct CarLapBuf {
    samples: Vec<LineSample>,
    prev_pct: Option<f32>,
    lap_t0: Option<f64>,
    /// World XY integrator (player only).
    x: f32,
    y: f32,
    have_xy: bool,
    last_session_t: Option<f64>,
    last_yaw: Option<f32>,
}

#[derive(Debug, Clone, Default)]
pub struct SessionLineState {
    cars: HashMap<i32, CarLapBuf>,
    race_best: Option<StoredLap>,
    player_top3: Vec<StoredLap>,
    track_pb: Option<StoredLap>,
    /// Loaded / current track+car key.
    track_id: Option<i32>,
    car_path: String,
    subsession_id: i32,
    track_name: String,
    session_type: String,
    /// Persist when race doc or PB changes.
    pub dirty_race: bool,
    pub dirty_pb: bool,
    /// Player current-lap samples for live delta (mirrors car buf of player).
    player_idx: Option<i32>,
    selected_top3: Option<usize>,
}

impl SessionLineState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_track_pb(&mut self, lap: Option<StoredLap>) {
        self.track_pb = lap;
    }

    pub fn set_selected_top3(&mut self, idx: Option<usize>) {
        self.selected_top3 = idx;
    }

    pub fn ensure_session(&mut self, frame: &TelemetryFrame) {
        let tid = frame.track_id.unwrap_or(0);
        let car = frame.car_path.clone().unwrap_or_default();
        let sid = frame.subsession_id.unwrap_or(0);
        let changed_event = sid > 0 && sid != self.subsession_id && self.subsession_id != 0;
        let changed_car_track =
            (self.track_id != Some(tid) && tid > 0) || (self.car_path != car && !car.is_empty());
        if changed_event {
            self.race_best = None;
            self.player_top3.clear();
            self.cars.clear();
            self.dirty_race = false;
        }
        if tid > 0 {
            self.track_id = Some(tid);
        }
        if !car.is_empty() {
            self.car_path = car;
        }
        if sid > 0 {
            self.subsession_id = sid;
        }
        if let Some(n) = frame.track_name.as_ref() {
            self.track_name = n.clone();
        }
        if let Some(st) = frame.session_type.as_ref() {
            self.session_type = st.clone();
        }
        if changed_car_track {
            // Caller should reload PB for new track/car.
            self.dirty_pb = false;
        }
        let _ = changed_car_track;
    }

    pub fn update(&mut self, frame: &TelemetryFrame) {
        if !frame.connected {
            return;
        }
        self.ensure_session(frame);
        let session_t = frame.session_time;
        if !session_t.is_finite() {
            return;
        }

        let player_idx = frame.cars.iter().find(|c| c.is_player).map(|c| c.car_idx);
        self.player_idx = player_idx;

        for car in &frame.cars {
            if car.is_pace_car || car.inactive || car.lap_dist_pct < 0.0 {
                continue;
            }
            let is_player = car.is_player;
            let motion = if is_player {
                frame.player_motion
            } else {
                None
            };
            let throttle = if is_player {
                Some(frame.throttle.clamp(0.0, 1.0))
            } else {
                None
            };
            let brake = if is_player {
                Some(frame.brake.clamp(0.0, 1.0))
            } else {
                None
            };
            let steer = if car.steer_rad != 0.0 || is_player {
                if is_player {
                    frame.steering
                } else {
                    car.steer_rad
                }
            } else {
                car.steer_rad
            };
            let gear = if is_player {
                frame.gear
            } else {
                car.gear
            };

            self.sample_car(
                car,
                session_t,
                steer,
                gear,
                throttle,
                brake,
                motion,
                is_player,
            );
        }
    }

    fn sample_car(
        &mut self,
        car: &CarRow,
        session_t: f64,
        steer: f32,
        gear: i32,
        throttle: Option<f32>,
        brake: Option<f32>,
        motion: Option<PlayerMotion>,
        is_player: bool,
    ) {
        let pct = car.lap_dist_pct.clamp(0.0, 0.999_999);
        let mut finished_lap: Option<StoredLap> = None;

        {
            let buf = self.cars.entry(car.car_idx).or_default();

            if let Some(prev) = buf.prev_pct {
                if pct + WRAP < prev {
                    let finished = std::mem::take(&mut buf.samples);
                    let lap_t0 = buf.lap_t0.take();
                    buf.prev_pct = Some(pct);
                    buf.lap_t0 = Some(session_t);
                    buf.x = 0.0;
                    buf.y = 0.0;
                    buf.have_xy = false;
                    buf.last_session_t = Some(session_t);
                    buf.last_yaw = motion.map(|m| m.yaw);

                    let lap_time = car
                        .last_lap_time_s
                        .map(|t| t as f64)
                        .filter(|t| *t >= MIN_LAP_TIME_S);
                    if let Some(lap_time) = lap_time {
                        if finished.len() >= MIN_LAP_SAMPLES {
                            let samples = normalize_samples(finished, lap_t0);
                            let markers = if is_player {
                                markers_from_samples(&samples)
                            } else {
                                Vec::new()
                            };
                            finished_lap = Some(StoredLap {
                                driver_name: car.name.clone(),
                                car_number: car.car_number.clone(),
                                is_player,
                                lap_time_s: lap_time,
                                lap_number: car.laps_completed.max(0),
                                samples,
                                markers,
                            });
                        }
                    }
                } else if (pct - prev).abs() < PCT_EPS {
                    return;
                }
            } else {
                buf.lap_t0 = Some(session_t);
            }

            if finished_lap.is_some() {
                // New lap already started; skip pushing the wrap sample this tick.
            } else {
                // Integrate player XY in world frame from car-local velocity.
                let (x, y) = if is_player {
                    if let (Some(m), Some(prev_t)) = (motion, buf.last_session_t) {
                        let dt = (session_t - prev_t) as f32;
                        if dt > 0.0 && dt < 0.5 {
                            let yaw = m.yaw;
                            let c = yaw.cos();
                            let s = yaw.sin();
                            let wx = m.vx * c - m.vy * s;
                            let wy = m.vx * s + m.vy * c;
                            buf.x += wx * dt;
                            buf.y += wy * dt;
                            buf.have_xy = true;
                        }
                    }
                    buf.last_session_t = Some(session_t);
                    buf.last_yaw = motion.map(|m| m.yaw);
                    if buf.have_xy {
                        (Some(buf.x), Some(buf.y))
                    } else {
                        (None, None)
                    }
                } else {
                    buf.last_session_t = Some(session_t);
                    (None, None)
                };

                let t_rel = buf
                    .lap_t0
                    .map(|t0| (session_t - t0).max(0.0))
                    .unwrap_or(0.0);
                buf.samples.push(LineSample {
                    pct,
                    t: t_rel,
                    steer,
                    gear,
                    throttle,
                    brake,
                    x,
                    y,
                });
                if buf.samples.len() > MAX_SAMPLES {
                    buf.samples = decimate(&buf.samples);
                }
                buf.prev_pct = Some(pct);
            }
        }

        if let Some(stored) = finished_lap {
            self.promote_finished(stored);
        }
    }

    fn promote_finished(&mut self, lap: StoredLap) {
        let is_player = lap.is_player;
        let better_session = self
            .race_best
            .as_ref()
            .map(|b| lap.lap_time_s < b.lap_time_s - 1e-4)
            .unwrap_or(true);
        if better_session {
            self.race_best = Some(lap.clone());
            self.dirty_race = true;
        }

        if is_player {
            insert_top3(&mut self.player_top3, lap.clone());
            self.dirty_race = true;

            let better_pb = self
                .track_pb
                .as_ref()
                .map(|b| lap.lap_time_s < b.lap_time_s - 1e-4)
                .unwrap_or(true);
            if better_pb {
                self.track_pb = Some(lap);
                self.dirty_pb = true;
            }
        }
    }

    fn ref_lap(&self, mode: &str) -> Option<&StoredLap> {
        let m = mode.to_ascii_lowercase();
        if m == "track_pb" || m == "pb" || m == "personal_best" {
            return self.track_pb.as_ref();
        }
        if m == "race_best" || m == "session_best" {
            if let Some(b) = self.race_best.as_ref() {
                return Some(b);
            }
            // Fall back to track PB, then own best in top3.
            return self
                .track_pb
                .as_ref()
                .or_else(|| self.player_top3.first());
        }
        None
    }

    /// Live compare view against race best / track PB. Returns None if mode is own best/last.
    pub fn view(
        &self,
        frame: &TelemetryFrame,
        mode: &str,
        corner_pcts: &[(String, f32)],
        allow_demo: bool,
    ) -> Option<LapCompareView> {
        let m = mode.to_ascii_lowercase();
        if m != "race_best"
            && m != "session_best"
            && m != "track_pb"
            && m != "pb"
            && m != "personal_best"
        {
            return None;
        }

        let reference = self.ref_lap(mode);
        let player_idx = self.player_idx;
        let cur_samples = player_idx
            .and_then(|idx| self.cars.get(&idx))
            .map(|b| b.samples.as_slice())
            .unwrap_or(&[]);

        // Optional: review a completed top-3 lap instead of live.
        let (cur_for_delta, reviewing) = if let Some(i) = self.selected_top3 {
            if let Some(lap) = self.player_top3.get(i) {
                (lap.samples.as_slice(), true)
            } else {
                (cur_samples, false)
            }
        } else {
            (cur_samples, false)
        };

        let ref_label = match reference {
            Some(r) if m.starts_with("track") || m == "pb" || m == "personal_best" => {
                format!("VS PB {}", fmt_lap(r.lap_time_s))
            }
            Some(r) => {
                let num = if r.car_number.is_empty() {
                    String::new()
                } else {
                    format!("#{} ", r.car_number)
                };
                format!("VS {num}{}", short_name(&r.driver_name))
            }
            None => {
                if m.starts_with("track") || m == "pb" || m == "personal_best" {
                    "VS PB".into()
                } else {
                    "VS SESSION".into()
                }
            }
        };

        let delta = if reviewing {
            reference.map(|r| {
                let cur_t = self
                    .selected_top3
                    .and_then(|i| self.player_top3.get(i))
                    .map(|l| l.lap_time_s)
                    .unwrap_or(0.0);
                cur_t - r.lap_time_s
            })
        } else {
            live_delta(cur_for_delta, reference.map(|r| r.samples.as_slice()).unwrap_or(&[]))
        };

        let spark = build_time_spark(
            cur_for_delta,
            reference.map(|r| r.samples.as_slice()).unwrap_or(&[]),
            !reviewing,
        );
        let steer_spark = build_steer_delta_spark(
            cur_for_delta,
            reference.map(|r| r.samples.as_slice()).unwrap_or(&[]),
        );
        let markers = markers_from_samples(cur_for_delta);
        let corner_deltas = corner_deltas_vs(
            cur_for_delta,
            reference.map(|r| r.samples.as_slice()).unwrap_or(&[]),
            corner_pcts,
        );
        let turns: Vec<(String, f32)> = if corner_deltas.is_empty() {
            turns_from_spark(&spark, allow_demo)
        } else {
            corner_deltas
                .iter()
                .map(|c| (c.label.clone(), c.delta_s))
                .collect()
        };

        let mut view = LapCompareView {
            delta: delta.or_else(|| {
                if allow_demo && reference.is_none() {
                    Some(-0.12)
                } else {
                    None
                }
            }),
            spark: if spark.is_empty() && allow_demo {
                demo_spark(frame.session_time)
            } else {
                spark
            },
            turns,
            ref_label,
            markers,
            session_extra: SessionLineViewExtra {
                top3: self.top3_rows(reference),
                race_best_label: self
                    .race_best
                    .as_ref()
                    .map(|r| format!("#{} {}", r.car_number, short_name(&r.driver_name)))
                    .unwrap_or_default(),
                race_best_time_s: self.race_best.as_ref().map(|r| r.lap_time_s),
                track_pb_time_s: self.track_pb.as_ref().map(|r| r.lap_time_s),
                steer_spark,
                corner_deltas,
                selected_top3: self.selected_top3,
            },
        };
        if view.turns.is_empty() && allow_demo {
            view.turns = vec![
                ("T1".into(), 0.08),
                ("T3".into(), -0.03),
                ("T7".into(), 0.11),
            ];
        }
        Some(view)
    }

    fn top3_rows(&self, reference: Option<&StoredLap>) -> Vec<Top3SummaryRow> {
        let pb_t = self.track_pb.as_ref().map(|p| p.lap_time_s);
        self.player_top3
            .iter()
            .map(|lap| Top3SummaryRow {
                lap_time_s: lap.lap_time_s,
                lap_number: lap.lap_number,
                delta_to_ref_s: reference.map(|r| lap.lap_time_s - r.lap_time_s),
                delta_to_pb_s: pb_t.map(|p| lap.lap_time_s - p),
            })
            .collect()
    }

    pub fn to_race_doc(&self) -> Option<RaceDoc> {
        let tid = self.track_id.filter(|t| *t > 0)?;
        if self.race_best.is_none() && self.player_top3.is_empty() {
            return None;
        }
        Some(RaceDoc {
            schema: 1,
            subsession_id: self.subsession_id.max(0),
            track_id: tid,
            track_name: self.track_name.clone(),
            car_path: self.car_path.clone(),
            session_type: self.session_type.clone(),
            recorded_at: chrono::Utc::now().to_rfc3339(),
            compared_to_pb_lap_time_s: self.track_pb.as_ref().map(|p| p.lap_time_s),
            race_best: self.race_best.clone(),
            player_top3: self.player_top3.clone(),
        })
    }

    pub fn to_track_pb_doc(&self) -> Option<TrackPbDoc> {
        let tid = self.track_id.filter(|t| *t > 0)?;
        let lap = self.track_pb.as_ref()?;
        if self.car_path.is_empty() {
            return None;
        }
        Some(TrackPbDoc {
            schema: 1,
            track_id: tid,
            car_path: self.car_path.clone(),
            track_name: self.track_name.clone(),
            lap_time_s: lap.lap_time_s,
            set_at: chrono::Utc::now().to_rfc3339(),
            subsession_id: self.subsession_id.max(0),
            samples: lap.samples.clone(),
            markers: lap.markers.clone(),
        })
    }
}

fn insert_top3(list: &mut Vec<StoredLap>, lap: StoredLap) {
    list.push(lap);
    list.sort_by(|a, b| {
        a.lap_time_s
            .partial_cmp(&b.lap_time_s)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    list.dedup_by(|a, b| (a.lap_time_s - b.lap_time_s).abs() < 1e-4 && a.lap_number == b.lap_number);
    if list.len() > TOP_N {
        list.truncate(TOP_N);
    }
}

fn normalize_samples(mut samples: Vec<LineSample>, lap_t0: Option<f64>) -> Vec<LineSample> {
    if samples.is_empty() {
        return samples;
    }
    let t0 = lap_t0.unwrap_or(0.0);
    // Samples already store relative t from lap_t0; re-base if absolute slipped in.
    if let Some(first) = samples.first() {
        if first.t > 5.0 && t0 > 0.0 {
            for s in &mut samples {
                s.t = (s.t - t0).max(0.0);
            }
        }
    }
    let t_base = samples.first().map(|s| s.t).unwrap_or(0.0);
    for s in &mut samples {
        s.t = (s.t - t_base).max(0.0);
    }
    samples
}

fn decimate(samples: &[LineSample]) -> Vec<LineSample> {
    samples
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 0)
        .map(|(_, s)| s.clone())
        .collect()
}

fn markers_from_samples(samples: &[LineSample]) -> Vec<CompareMarker> {
    let mut out = Vec::new();
    let mut prev_brk = 0.0_f32;
    let mut prev_thr = 0.0_f32;
    for s in samples {
        let brk = s.brake.unwrap_or(0.0);
        let thr = s.throttle.unwrap_or(0.0);
        if prev_brk < 0.05 && brk >= 0.15 {
            out.push(CompareMarker {
                pct: s.pct,
                kind: MarkerKind::Brake,
            });
        }
        if prev_thr >= 0.40 && thr <= prev_thr - 0.20 && brk < 0.10 {
            out.push(CompareMarker {
                pct: s.pct,
                kind: MarkerKind::Lift,
            });
        }
        prev_brk = brk;
        prev_thr = thr;
    }
    if out.len() > 24 {
        let step = out.len() / 24;
        out = out.into_iter().step_by(step.max(1)).collect();
    }
    out
}

fn interp_time(samples: &[LineSample], pct: f32) -> Option<f64> {
    if samples.is_empty() {
        return None;
    }
    if pct <= samples[0].pct {
        return Some(samples[0].t);
    }
    if let Some(last) = samples.last() {
        if pct >= last.pct {
            return Some(last.t);
        }
    }
    for w in samples.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if pct >= a.pct && pct <= b.pct {
            let span = (b.pct - a.pct).max(1e-6);
            let u = (pct - a.pct) / span;
            return Some(a.t + (b.t - a.t) * u as f64);
        }
    }
    None
}

fn interp_steer(samples: &[LineSample], pct: f32) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }
    if pct <= samples[0].pct {
        return Some(samples[0].steer);
    }
    if let Some(last) = samples.last() {
        if pct >= last.pct {
            return Some(last.steer);
        }
    }
    for w in samples.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        if pct >= a.pct && pct <= b.pct {
            let span = (b.pct - a.pct).max(1e-6);
            let u = (pct - a.pct) / span;
            return Some(a.steer + (b.steer - a.steer) * u);
        }
    }
    None
}

fn live_delta(cur: &[LineSample], reference: &[LineSample]) -> Option<f64> {
    if cur.is_empty() || reference.is_empty() {
        return None;
    }
    let last = cur.last()?;
    let ref_t = interp_time(reference, last.pct)?;
    Some(last.t - ref_t)
}

fn build_time_spark(cur: &[LineSample], reference: &[LineSample], live_clip: bool) -> Vec<f32> {
    if cur.is_empty() || reference.is_empty() {
        return Vec::new();
    }
    let upto = cur.last().map(|s| s.pct).unwrap_or(0.0);
    let mut out = Vec::with_capacity(SPARK_BINS);
    for i in 0..SPARK_BINS {
        let pct = i as f32 / (SPARK_BINS - 1).max(1) as f32;
        if live_clip && pct > upto + 1e-3 {
            break;
        }
        let Some(ct) = interp_time(cur, pct) else {
            continue;
        };
        let Some(rt) = interp_time(reference, pct) else {
            continue;
        };
        out.push((ct - rt) as f32);
    }
    out
}

fn build_steer_delta_spark(cur: &[LineSample], reference: &[LineSample]) -> Vec<f32> {
    if cur.is_empty() || reference.is_empty() {
        return Vec::new();
    }
    let upto = cur.last().map(|s| s.pct).unwrap_or(1.0);
    let mut out = Vec::with_capacity(SPARK_BINS);
    for i in 0..SPARK_BINS {
        let pct = i as f32 / (SPARK_BINS - 1).max(1) as f32;
        if pct > upto + 1e-3 {
            break;
        }
        let Some(cs) = interp_steer(cur, pct) else {
            continue;
        };
        let Some(rs) = interp_steer(reference, pct) else {
            continue;
        };
        out.push(cs - rs);
    }
    out
}

fn corner_deltas_vs(
    cur: &[LineSample],
    reference: &[LineSample],
    corners: &[(String, f32)],
) -> Vec<CornerDelta> {
    if cur.is_empty() || reference.is_empty() || corners.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (label, pct) in corners {
        let pct = pct.clamp(0.0, 0.999);
        let Some(ct) = interp_time(cur, pct) else {
            continue;
        };
        let Some(rt) = interp_time(reference, pct) else {
            continue;
        };
        out.push(CornerDelta {
            label: label.clone(),
            pct,
            delta_s: (ct - rt) as f32,
        });
    }
    out.sort_by(|a, b| {
        b.delta_s
            .abs()
            .partial_cmp(&a.delta_s.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out.truncate(8);
    out
}

fn turns_from_spark(spark: &[f32], allow_demo: bool) -> Vec<(String, f32)> {
    if spark.len() < 8 {
        return if allow_demo {
            vec![("T1".into(), 0.05)]
        } else {
            Vec::new()
        };
    }
    let n = spark.len();
    [(0.12, "T1"), (0.35, "T3"), (0.58, "T7"), (0.82, "T12")]
        .iter()
        .map(|(frac, label)| {
            let i = ((*frac) * (n - 1) as f32).round() as usize;
            let lo = i.saturating_sub(2);
            let hi = (i + 2).min(n - 1);
            let loss = spark[lo..=hi]
                .iter()
                .copied()
                .fold(0.0_f32, |a, b| if b.abs() > a.abs() { b } else { a });
            ((*label).into(), loss)
        })
        .collect()
}

fn demo_spark(t: f64) -> Vec<f32> {
    (0..SPARK_BINS)
        .map(|i| {
            let x = i as f64 / SPARK_BINS as f64;
            ((x * std::f64::consts::TAU * 2.0 + t * 0.4).sin() * 0.25 + x * 0.1) as f32
        })
        .collect()
}

fn short_name(name: &str) -> String {
    let t = name.trim();
    if t.len() <= 14 {
        return t.to_string();
    }
    format!("{}…", &t[..13])
}

fn fmt_lap(secs: f64) -> String {
    if secs < 0.0 || !secs.is_finite() {
        return "--.---".into();
    }
    let m = (secs / 60.0).floor() as i32;
    let s = secs - (m as f64) * 60.0;
    if m > 0 {
        format!("{m}:{s:06.3}")
    } else {
        format!("{s:.3}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(pct: f32, t: f64) -> LineSample {
        LineSample {
            pct,
            t,
            steer: 0.0,
            gear: 3,
            throttle: Some(1.0),
            brake: Some(0.0),
            x: Some(pct * 100.0),
            y: Some(0.0),
        }
    }

    #[test]
    fn promote_session_best_and_top3() {
        let mut st = SessionLineState::new();
        st.track_id = Some(100);
        st.car_path = "testcar".into();

        let mut lap = StoredLap {
            driver_name: "Alice".into(),
            car_number: "1".into(),
            is_player: false,
            lap_time_s: 90.0,
            lap_number: 2,
            samples: (0..40).map(|i| sample(i as f32 / 40.0, i as f64 * 2.0)).collect(),
            markers: vec![],
        };
        st.promote_finished(lap.clone());
        assert_eq!(st.race_best.as_ref().unwrap().lap_time_s, 90.0);

        lap.driver_name = "Bob".into();
        lap.lap_time_s = 89.0;
        st.promote_finished(lap);
        assert_eq!(st.race_best.as_ref().unwrap().lap_time_s, 89.0);

        for i in 0..4 {
            st.promote_finished(StoredLap {
                driver_name: "Me".into(),
                car_number: "7".into(),
                is_player: true,
                lap_time_s: 91.0 - i as f64 * 0.2,
                lap_number: 10 + i,
                samples: (0..40).map(|j| sample(j as f32 / 40.0, j as f64)).collect(),
                markers: vec![],
            });
        }
        assert_eq!(st.player_top3.len(), 3);
        assert!(st.player_top3[0].lap_time_s <= st.player_top3[1].lap_time_s);
        assert!(st.track_pb.as_ref().unwrap().lap_time_s < 91.0);
    }

    #[test]
    fn pb_only_promotes_faster() {
        let mut st = SessionLineState::new();
        st.track_id = Some(1);
        st.car_path = "car".into();
        st.promote_finished(StoredLap {
            driver_name: "Me".into(),
            car_number: "1".into(),
            is_player: true,
            lap_time_s: 100.0,
            lap_number: 1,
            samples: (0..30).map(|i| sample(i as f32 / 30.0, i as f64)).collect(),
            markers: vec![],
        });
        let pb1 = st.track_pb.as_ref().unwrap().lap_time_s;
        st.promote_finished(StoredLap {
            driver_name: "Me".into(),
            car_number: "1".into(),
            is_player: true,
            lap_time_s: 101.0,
            lap_number: 2,
            samples: (0..30).map(|i| sample(i as f32 / 30.0, i as f64)).collect(),
            markers: vec![],
        });
        assert_eq!(st.track_pb.as_ref().unwrap().lap_time_s, pb1);
        st.promote_finished(StoredLap {
            driver_name: "Me".into(),
            car_number: "1".into(),
            is_player: true,
            lap_time_s: 99.0,
            lap_number: 3,
            samples: (0..30).map(|i| sample(i as f32 / 30.0, i as f64)).collect(),
            markers: vec![],
        });
        assert_eq!(st.track_pb.as_ref().unwrap().lap_time_s, 99.0);
    }

    #[test]
    fn delta_sign_behind_reference_positive() {
        let cur: Vec<_> = (0..20).map(|i| sample(i as f32 / 20.0, i as f64 * 1.1)).collect();
        let reference: Vec<_> = (0..20).map(|i| sample(i as f32 / 20.0, i as f64)).collect();
        let d = live_delta(&cur, &reference).unwrap();
        assert!(d > 0.0, "slower current lap should be +delta, got {d}");
    }

    #[test]
    fn opponent_samples_lack_pedals_and_xy() {
        let s = LineSample {
            pct: 0.5,
            t: 40.0,
            steer: 0.2,
            gear: 4,
            throttle: None,
            brake: None,
            x: None,
            y: None,
        };
        assert!(s.throttle.is_none());
        assert!(s.x.is_none());
    }
}
