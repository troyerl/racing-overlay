//! Local-machine CPU / memory / GPU / WiFi for table slots (Python `sysstats`).

use crate::telemetry::TelemetryFrame;
use std::process::Command;
use std::time::{Duration, Instant};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

pub struct SysStats {
    sys: System,
    last_sample: Option<Instant>,
    cpu: String,
    mem: String,
    gpu: String,
    wifi: Option<String>,
    primed: bool,
}

impl Default for SysStats {
    fn default() -> Self {
        Self::new()
    }
}

impl SysStats {
    pub fn new() -> Self {
        let kind = RefreshKind::nothing()
            .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
            .with_memory(MemoryRefreshKind::everything());
        let mut sys = System::new_with_specifics(kind);
        // Prime CPU counters so the next refresh yields a real delta.
        sys.refresh_cpu_usage();
        Self {
            sys,
            last_sample: None,
            cpu: "--".into(),
            mem: "--".into(),
            gpu: "--".into(),
            wifi: None,
            primed: true,
        }
    }

    fn fmt_pct(v: Option<f32>) -> String {
        match v {
            Some(n) if n.is_finite() => format!("{:.0}%", n.clamp(0.0, 100.0)),
            _ => "--".into(),
        }
    }

    fn probe_gpu() -> Option<f32> {
        // nvidia-smi works on Linux/Windows when present; macOS typically unavailable.
        let out = Command::new("nvidia-smi")
            .args([
                "--query-gpu=utilization.gpu",
                "--format=csv,noheader,nounits",
            ])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let mut max = None;
        for line in text.lines() {
            if let Ok(v) = line.trim().parse::<f32>() {
                max = Some(max.map_or(v, |m: f32| m.max(v)));
            }
        }
        max
    }

    fn probe_wifi() -> Option<f32> {
        #[cfg(target_os = "linux")]
        {
            let text = std::fs::read_to_string("/proc/net/wireless").ok()?;
            for line in text.lines().skip(2) {
                // iface: status link level noise …
                let parts: Vec<_> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    let link = parts[2].trim_end_matches('.').parse::<f32>().ok()?;
                    // link is typically 0..70; map roughly to 0..100.
                    return Some((link / 70.0 * 100.0).clamp(0.0, 100.0));
                }
            }
            None
        }
        #[cfg(not(target_os = "linux"))]
        {
            None
        }
    }

    fn refresh(&mut self) {
        self.sys.refresh_cpu_usage();
        self.sys
            .refresh_memory_specifics(MemoryRefreshKind::everything());

        let cpu = if self.primed {
            let u = self.sys.global_cpu_usage();
            if u > 0.0 || self.last_sample.is_some() {
                Some(u)
            } else {
                Some(0.0)
            }
        } else {
            Some(self.sys.global_cpu_usage())
        };

        let mem = {
            let total = self.sys.total_memory();
            let used = self.sys.used_memory();
            if total > 0 {
                Some((used as f64 / total as f64 * 100.0) as f32)
            } else {
                None
            }
        };

        self.cpu = Self::fmt_pct(cpu);
        self.mem = Self::fmt_pct(mem);
        self.gpu = Self::fmt_pct(Self::probe_gpu());
        self.wifi = Self::probe_wifi().map(|q| format!("{:.0}%", q));
        self.last_sample = Some(Instant::now());
    }

    /// Write cached (or freshly sampled) display strings onto the frame.
    pub fn sample_into(&mut self, frame: &mut TelemetryFrame) {
        let due = self
            .last_sample
            .map(|t| t.elapsed() >= SAMPLE_INTERVAL)
            .unwrap_or(true);
        if due {
            self.refresh();
        }
        frame.cpu = Some(self.cpu.clone());
        frame.mem = Some(self.mem.clone());
        frame.gpu = Some(self.gpu.clone());
        // WiFi fills channel quality when iRacing channel stats are unavailable.
        let chan_dead = frame.chan_quality.map(|q| q <= 0.0).unwrap_or(true)
            && frame.chan_latency.map(|l| l <= 0.0).unwrap_or(true);
        if chan_dead {
            if let Some(ref w) = self.wifi {
                if let Ok(q) = w.trim_end_matches('%').parse::<f32>() {
                    frame.chan_quality = Some(q);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_into_sets_pct_strings() {
        let mut s = SysStats::new();
        let mut frame = TelemetryFrame::default();
        // Second sample after a short wait gets a real CPU delta.
        std::thread::sleep(Duration::from_millis(50));
        s.sample_into(&mut frame);
        let cpu = frame.cpu.as_deref().unwrap_or("");
        let mem = frame.mem.as_deref().unwrap_or("");
        assert!(cpu.ends_with('%') || cpu == "--", "cpu={cpu}");
        assert!(mem.ends_with('%') || mem == "--", "mem={mem}");
        // Memory should almost always resolve on desktop.
        assert!(mem.ends_with('%'), "expected mem percent, got {mem}");
    }
}
