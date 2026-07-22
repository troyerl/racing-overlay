//! Sector splits derived from lap-distance crossings (Python `SectorTimer`).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectorCell {
    pub time: Option<f64>,
    pub status: String,
    pub active: bool,
    pub delta: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SectorSnapshot {
    pub cur_lap: Option<f64>,
    pub last_lap: Option<f64>,
    pub best_lap: Option<f64>,
    pub predicted_lap: Option<f64>,
    pub sectors: Vec<SectorCell>,
    pub active_idx: usize,
    /// Sector start percentages (lap_dist_pct), for map highlight.
    #[serde(default)]
    pub starts: Vec<f64>,
}

/// Derives sector splits from lap-distance crossings (owned by the host).
#[derive(Debug, Clone)]
pub struct SectorTimer {
    starts: Option<Vec<f64>>,
    cur: Vec<Option<f64>>,
    last: Vec<Option<f64>>,
    best: Vec<Option<f64>>,
    session_best: Vec<Option<f64>>,
    idx: usize,
    seg_start_t: f64,
    cur_lap_t: f64,
    prev_pct: Option<f64>,
}

impl Default for SectorTimer {
    fn default() -> Self {
        Self::new()
    }
}

impl SectorTimer {
    /// Default: three equal sectors (0, 1/3, 2/3).
    pub fn new() -> Self {
        Self {
            starts: Some(vec![0.0, 1.0 / 3.0, 2.0 / 3.0]),
            cur: Vec::new(),
            last: Vec::new(),
            best: Vec::new(),
            session_best: Vec::new(),
            idx: 0,
            seg_start_t: 0.0,
            cur_lap_t: 0.0,
            prev_pct: None,
        }
    }

    /// Set sector start percentages. Re-inits in-lap state when they change.
    pub fn set_boundaries(&mut self, starts: &[f64]) {
        if starts.is_empty() {
            return;
        }
        let mut s: Vec<f64> = starts.to_vec();
        s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        if self.starts.as_ref() != Some(&s) {
            self.starts = Some(s);
            self.cur.clear();
            self.idx = 0;
            self.seg_start_t = 0.0;
            self.prev_pct = None;
        }
    }

    #[allow(dead_code)]
    pub fn reset_session(&mut self) {
        self.session_best.clear();
    }

    pub fn update(&mut self, pct: f32, cur_lap_time: Option<f64>, last_lap_time: Option<f64>) {
        if self.starts.is_none() {
            self.starts = Some(vec![0.0, 1.0 / 3.0, 2.0 / 3.0]);
        }
        let pct = pct as f64;
        if pct < 0.0 {
            return;
        }
        if let Some(t) = cur_lap_time {
            self.cur_lap_t = t;
        }
        let n = self.starts.as_ref().map(|s| s.len()).unwrap_or(0);

        // Lap rollover: big backward jump in lap distance = crossed start/finish.
        if let Some(prev) = self.prev_pct {
            if pct + 0.5 < prev {
                if let Some(last) = last_lap_time.filter(|t| *t > 0.0) {
                    self.finish_sector(Some(last - self.seg_start_t));
                }
                if !self.cur.is_empty() {
                    self.last = self.cur.clone();
                }
                self.cur.clear();
                self.idx = 0;
                self.seg_start_t = 0.0;
                self.prev_pct = Some(pct);
                return;
            }
        }

        let starts = self.starts.as_ref().expect("starts set above");
        let mut new_idx = 0usize;
        for (i, &start) in starts.iter().enumerate().take(n) {
            if pct >= start {
                new_idx = i;
            }
        }
        if new_idx > self.idx {
            if let Some(t) = cur_lap_time {
                self.finish_sector(Some(t - self.seg_start_t));
                self.seg_start_t = t;
                self.idx = new_idx;
            }
        }
        self.prev_pct = Some(pct);
    }

    fn finish_sector(&mut self, t: Option<f64>) {
        let i = self.cur.len();
        let Some(t) = t.filter(|v| *v > 0.0) else {
            self.cur.push(None);
            return;
        };
        self.cur.push(Some(t));
        while self.best.len() <= i {
            self.best.push(None);
        }
        if self.best[i].is_none_or(|b| t < b) {
            self.best[i] = Some(t);
        }
        while self.session_best.len() <= i {
            self.session_best.push(None);
        }
        if self.session_best[i].is_none_or(|b| t < b) {
            self.session_best[i] = Some(t);
        }
    }

    /// Sum of best sectors + current sector pace.
    pub fn predicted_lap(&self) -> Option<f64> {
        let n = self.starts.as_ref().map(|s| s.len()).unwrap_or(0);
        if n == 0 {
            return None;
        }
        let mut total = 0.0;
        let mut have = false;
        for i in 0..n {
            if i < self.cur.len() {
                if let Some(t) = self.cur[i].filter(|t| *t > 0.0) {
                    total += t;
                    have = true;
                }
            } else if i == self.idx {
                let running = (self.cur_lap_t - self.seg_start_t).max(0.0);
                if running > 0.0 {
                    total += running;
                    have = true;
                }
            } else {
                let ref_t = self
                    .session_best
                    .get(i)
                    .copied()
                    .flatten()
                    .or_else(|| self.best.get(i).copied().flatten());
                if let Some(r) = ref_t.filter(|t| *t > 0.0) {
                    total += r;
                    have = true;
                }
            }
        }
        have.then_some(total)
    }

    pub fn snapshot(
        &self,
        cur_lap: Option<f64>,
        last_lap: Option<f64>,
        best_lap: Option<f64>,
        show_delta: bool,
    ) -> SectorSnapshot {
        let n = self.starts.as_ref().map(|s| s.len()).unwrap_or(3).max(1);
        let mut sectors = Vec::with_capacity(n);
        for i in 0..n {
            if i < self.cur.len() {
                let t = self.cur[i];
                let best = self.best.get(i).copied().flatten();
                let status = if t.is_some() && best.is_some() && t.unwrap() <= best.unwrap() + 1e-6
                {
                    "best"
                } else {
                    "done"
                };
                let delta = if show_delta {
                    match (t, best) {
                        (Some(tv), Some(bv)) => Some(tv - bv),
                        _ => None,
                    }
                } else {
                    None
                };
                sectors.push(SectorCell {
                    time: t,
                    status: status.into(),
                    active: false,
                    delta,
                });
            } else if i == self.idx {
                let running = (self.cur_lap_t - self.seg_start_t).max(0.0);
                let best = self.best.get(i).copied().flatten();
                let delta = if show_delta && running > 0.0 {
                    best.map(|b| running - b)
                } else {
                    None
                };
                sectors.push(SectorCell {
                    time: Some(running),
                    status: "running".into(),
                    active: true,
                    delta,
                });
            } else {
                let last = self.last.get(i).copied().flatten();
                sectors.push(SectorCell {
                    time: last,
                    status: "idle".into(),
                    active: false,
                    delta: None,
                });
            }
        }
        SectorSnapshot {
            cur_lap,
            last_lap,
            best_lap,
            predicted_lap: self.predicted_lap(),
            sectors,
            active_idx: self.idx,
            starts: self.starts.clone().unwrap_or_default(),
        }
    }

    /// Equal divisions when SplitTimeInfo is unavailable (Python fallback).
    pub fn equal_starts(n: usize) -> Vec<f64> {
        let n = n.max(1);
        (0..n).map(|i| i as f64 / n as f64).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sector_advance_records_split() {
        let mut t = SectorTimer::new();
        t.update(0.05, Some(5.0), None);
        assert_eq!(t.idx, 0);
        // Cross into sector 1 (~0.333)
        t.update(0.40, Some(30.0), None);
        assert_eq!(t.idx, 1);
        assert_eq!(t.cur.len(), 1);
        assert!((t.cur[0].unwrap() - 30.0).abs() < 1e-6);
        // Cross into sector 2
        t.update(0.70, Some(60.0), None);
        assert_eq!(t.idx, 2);
        assert_eq!(t.cur.len(), 2);
        assert!((t.cur[1].unwrap() - 30.0).abs() < 1e-6);
    }

    #[test]
    fn wrap_at_start_finish_rolls_lap() {
        let mut t = SectorTimer::new();
        t.update(0.10, Some(8.0), None);
        t.update(0.40, Some(32.0), None);
        t.update(0.72, Some(65.0), None);
        assert_eq!(t.idx, 2);
        assert_eq!(t.cur.len(), 2);

        // Cross S/F: pct drops; last lap time finishes the final sector.
        t.update(0.02, Some(1.0), Some(90.0));
        assert_eq!(t.idx, 0);
        assert!(t.cur.is_empty());
        assert_eq!(t.last.len(), 3);
        assert!((t.last[2].unwrap() - (90.0 - 65.0)).abs() < 1e-6);

        let snap = t.snapshot(Some(1.0), Some(90.0), Some(88.0), false);
        assert_eq!(snap.active_idx, 0);
        assert_eq!(snap.sectors[0].status, "running");
        assert_eq!(snap.sectors[1].status, "idle");
    }
}
