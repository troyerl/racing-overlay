//! `--log-track-path`: record driven laps to CSV for map geometry calibration.
//!
//! The map places a car by treating lap % as a fraction of the drawn loop's arc
//! length. That only holds if the drawn loop's length grows at the same rate as
//! real track distance everywhere, and an imported schematic can be right in
//! total while being locally short or long. Only a spatial reference exposes
//! that, since lap % alone carries no shape information.
//!
//! iRacing does not expose `Lat`/`Lon` in live telemetry, so the rows here carry
//! car-local velocity and heading instead; integrating those offline reconstructs
//! the driven path. Both are seated-car variables, so this needs you in the car —
//! spectating or watching a replay records nothing.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Context;

use crate::paths;
use crate::telemetry::TelemetryFrame;
use crate::tracks::calibrate::{self, Sample};

/// Enough laps to average out line variation without bloating the file.
const LAPS_TARGET: u32 = 3;
/// Lap % must move this far before a row is kept, so a stalled sim sample read
/// at overlay tick rate does not land dozens of identical rows.
const PCT_EPS: f32 = 1e-6;
/// Backwards jump this large is a lap wrap, not the car reversing.
const WRAP: f32 = 0.5;
/// Silence is indistinguishable from a broken recorder, so report the inputs
/// this often until the first row lands.
const IDLE_REPORT: Duration = Duration::from_secs(2);
/// Don't open a file until the car is actually rolling, or sitting in the garage
/// would produce a file full of a single stationary sample.
const MOVING_MPS: f32 = 1.0;

pub struct PathProbe {
    writer: Option<BufWriter<File>>,
    path: Option<PathBuf>,
    laps: u32,
    rows: u64,
    last_pct: Option<f32>,
    last_idle_report: Option<Instant>,
    track_id: Option<i32>,
    samples: Vec<Sample>,
    done: bool,
}

impl PathProbe {
    pub fn new(enabled: bool) -> Self {
        Self {
            writer: None,
            path: None,
            laps: 0,
            rows: 0,
            last_pct: None,
            last_idle_report: None,
            track_id: None,
            samples: Vec::new(),
            done: !enabled,
        }
    }

    pub fn observe(&mut self, frame: &TelemetryFrame) {
        if self.done {
            return;
        }
        let pct = frame.player_lap_dist_pct;
        let Some(motion) = frame.player_motion else {
            self.report_idle(frame, "sim exposes no motion telemetry");
            return;
        };
        if !frame.connected || !pct.is_finite() || pct < 0.0 {
            self.report_idle(frame, "no valid LapDistPct");
            return;
        }
        if self.writer.is_none() && frame.speed_mps < MOVING_MPS {
            self.report_idle(frame, "waiting for you to get rolling");
            return;
        }
        if let Some(prev) = self.last_pct {
            let d = pct - prev;
            if d.abs() < PCT_EPS {
                return;
            }
            if d < -WRAP {
                self.laps += 1;
                if let Some(w) = self.writer.as_mut() {
                    let _ = w.flush();
                }
                eprintln!(
                    "[gridglance] track-path: lap {}/{LAPS_TARGET} recorded ({} rows)",
                    self.laps, self.rows
                );
                if self.laps >= LAPS_TARGET {
                    self.finish();
                    return;
                }
            }
        }
        self.last_pct = Some(pct);

        if self.writer.is_none() && !self.open(frame) {
            return;
        }
        let on_pit = frame
            .cars
            .iter()
            .find(|c| c.is_player)
            .map(|c| c.on_pit)
            .unwrap_or(false);
        if let Some(w) = self.writer.as_mut() {
            // Dead reckoning integrates these, so drift is set by their precision:
            // velocities to 0.1 mm/s and headings to ~1e-6 rad keep a lap's worth
            // of accumulated rounding far below a car length.
            let (lat, lon) = (frame.player_lat, frame.player_lon);
            let _ = writeln!(
                w,
                "{:.4},{},{:.7},{:.3},{:.4},{:.4},{:.6},{:.6},{},{},{}",
                frame.session_time,
                frame.lap,
                pct,
                frame.speed_mps,
                motion.vx,
                motion.vy,
                motion.yaw,
                motion.yaw_north,
                lat.map(|v| format!("{v:.9}")).unwrap_or_default(),
                lon.map(|v| format!("{v:.9}")).unwrap_or_default(),
                on_pit as u8
            );
            self.rows += 1;
        }
        self.samples.push(Sample {
            t: frame.session_time,
            pct,
            vx: motion.vx,
            vy: motion.vy,
            yaw: motion.yaw,
        });
    }

    fn report_idle(&mut self, frame: &TelemetryFrame, why: &str) {
        if self.writer.is_some() {
            return;
        }
        let now = Instant::now();
        if self
            .last_idle_report
            .is_some_and(|t| now.duration_since(t) < IDLE_REPORT)
        {
            return;
        }
        self.last_idle_report = Some(now);
        eprintln!(
            "[gridglance] track-path: idle — {why} (connected={} pct={:.4} speed={:.1} motion={})",
            frame.connected,
            frame.player_lap_dist_pct,
            frame.speed_mps,
            frame.player_motion.is_some(),
        );
    }

    fn open(&mut self, frame: &TelemetryFrame) -> bool {
        self.track_id = frame.track_id;
        let tid = frame
            .track_id
            .map(|t| t.to_string())
            .unwrap_or_else(|| "unknown".into());
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let path = paths::data_dir().join(format!("trackpath-{tid}-{stamp}.csv"));
        let Ok(file) = File::create(&path) else {
            eprintln!("[gridglance] track-path: cannot write {}", path.display());
            self.done = true;
            return false;
        };
        let mut w = BufWriter::new(file);
        if writeln!(
            w,
            "session_time,lap,lap_dist_pct,speed_mps,vx,vy,yaw,yaw_north,lat,lon,on_pit"
        )
        .is_err()
        {
            self.done = true;
            return false;
        }
        eprintln!(
            "[gridglance] track-path: recording {LAPS_TARGET} laps into {}",
            path.display()
        );
        self.writer = Some(w);
        self.path = Some(path);
        true
    }

    fn finish(&mut self) {
        if let Some(mut w) = self.writer.take() {
            let _ = w.flush();
        }
        self.done = true;
        if let Some(p) = &self.path {
            eprintln!(
                "[gridglance] track-path: done — {} rows in {}",
                self.rows,
                p.display()
            );
        }
        let done = self
            .track_id
            .context("no TrackID for this session")
            .and_then(|tid| calibrate_track(tid, &self.samples));
        match done {
            Ok(msg) => eprintln!("[gridglance] track-path: {msg}"),
            Err(e) => eprintln!("[gridglance] track-path: calibration failed — {e}"),
        }
    }
}

/// Solve the lap-% → loop-arc table and store it on the track document.
pub fn calibrate_track(track_id: i32, samples: &[Sample]) -> anyhow::Result<String> {
    let file = crate::track_path::find_track_file(track_id)
        .with_context(|| format!("no local track file for {track_id}"))?;
    let tp = crate::track_path::load_points(&file, 720).context("track file has no loop")?;
    let table = calibrate::solve_pct_map(samples, &tp.points)
        .context("laps did not match the drawn loop well enough to calibrate")?;

    let text = std::fs::read_to_string(&file)?;
    let mut doc: serde_json::Value = serde_json::from_str(&text)?;
    let obj = doc
        .as_object_mut()
        .context("track file is not a JSON object")?;
    let rounded: Vec<serde_json::Value> = table
        .iter()
        .map(|v| serde_json::json!((*v as f64 * 1e6).round() / 1e6))
        .collect();
    obj.insert("pct_map".into(), serde_json::Value::Array(rounded));
    crate::cloud::write_json_atomic(&file, &doc)?;

    let worst = worst_offset(&table);
    Ok(format!(
        "calibrated {} — corrected up to {:.2}% of a lap; saved to {}",
        tp.name,
        worst * 100.0,
        file.display()
    ))
}

/// Calibrate from a CSV this probe wrote earlier, so a recording can be reused
/// without driving the laps again.
pub fn calibrate_from_csv(csv: &std::path::Path) -> anyhow::Result<String> {
    let track_id = track_id_from_name(csv)
        .with_context(|| format!("cannot tell the track from {}", csv.display()))?;
    let text = std::fs::read_to_string(csv)?;
    let mut lines = text.lines();
    let header = lines.next().context("empty file")?;
    let cols: Vec<&str> = header.split(',').collect();
    let col = |name: &str| {
        cols.iter()
            .position(|c| *c == name)
            .with_context(|| format!("no `{name}` column"))
    };
    let (t, pct, vx, vy, yaw) = (
        col("session_time")?,
        col("lap_dist_pct")?,
        col("vx")?,
        col("vy")?,
        col("yaw")?,
    );
    let mut samples = Vec::new();
    for line in lines {
        let f: Vec<&str> = line.split(',').collect();
        let get = |i: usize| f.get(i).and_then(|v| v.parse::<f64>().ok());
        let (Some(t), Some(pct), Some(vx), Some(vy), Some(yaw)) =
            (get(t), get(pct), get(vx), get(vy), get(yaw))
        else {
            continue;
        };
        samples.push(Sample {
            t,
            pct: pct as f32,
            vx: vx as f32,
            vy: vy as f32,
            yaw: yaw as f32,
        });
    }
    if samples.is_empty() {
        anyhow::bail!("no usable rows");
    }
    calibrate_track(track_id, &samples)
}

/// Recordings are named `trackpath-<track id>-<stamp>.csv`; nothing inside the
/// file says which track it is.
fn track_id_from_name(csv: &std::path::Path) -> Option<i32> {
    csv.file_stem()?
        .to_str()?
        .strip_prefix("trackpath-")?
        .split('-')
        .next()?
        .parse()
        .ok()
}

/// Largest gap between the table and the uncorrected lap-%-as-arc assumption.
fn worst_offset(table: &[f32]) -> f32 {
    let n = table.len();
    (0..n)
        .map(|i| {
            let want = i as f32 / n as f32;
            let got = (table[i] - table[0]).rem_euclid(1.0);
            let d = (got - want).abs();
            d.min(1.0 - d)
        })
        .fold(0.0, f32::max)
}

impl Drop for PathProbe {
    fn drop(&mut self) {
        if self.writer.is_some() {
            self.finish();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driving_frame() -> TelemetryFrame {
        TelemetryFrame {
            connected: true,
            player_lap_dist_pct: 0.25,
            speed_mps: 50.0,
            player_motion: Some(crate::telemetry::PlayerMotion {
                vx: 50.0,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn disabled_probe_never_opens_a_file() {
        let mut p = PathProbe::new(false);
        p.observe(&driving_frame());
        assert!(p.writer.is_none());
        assert_eq!(p.rows, 0);
    }

    #[test]
    fn a_parked_car_does_not_start_a_recording() {
        let mut p = PathProbe::new(true);
        p.observe(&TelemetryFrame {
            speed_mps: 0.0,
            ..driving_frame()
        });
        assert!(p.writer.is_none(), "garage idling should not open a file");
        assert!(!p.done);
    }

    #[test]
    fn missing_motion_is_skipped_without_consuming_pct() {
        let mut p = PathProbe::new(true);
        let frame = TelemetryFrame {
            player_motion: None,
            ..driving_frame()
        };
        p.observe(&frame);
        assert!(p.last_pct.is_none(), "no motion means no sample was taken");
        assert!(!p.done, "a spectated session should keep waiting");
    }
}
