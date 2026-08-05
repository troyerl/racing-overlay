//! Local-machine CPU / memory / GPU / WiFi for table slots (Python `sysstats`).

use crate::telemetry::TelemetryFrame;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
/// `nvidia-smi` is a process spawn (~150 ms). Poll it far less often than CPU.
const GPU_INTERVAL: Duration = Duration::from_secs(3);
/// Stop probing after this many consecutive failures (no NVIDIA GPU present).
const GPU_MAX_MISSES: u32 = 3;

/// Display strings published by the sampler thread.
#[derive(Clone, Default)]
struct Snapshot {
    cpu: String,
    mem: String,
    gpu: String,
    wifi: Option<String>,
}

pub struct SysStats {
    /// Latest values from the background sampler; reads never block the UI.
    shared: Arc<Mutex<Snapshot>>,
    cpu: String,
    mem: String,
    gpu: String,
    wifi: Option<String>,
}

impl Default for SysStats {
    fn default() -> Self {
        Self::new()
    }
}

impl SysStats {
    /// Start the background sampler. Nothing here runs on the caller's thread
    /// again — `nvidia-smi` alone is a ~150 ms process spawn and used to stall
    /// the overlay twice a second from inside the host frame.
    pub fn new() -> Self {
        let shared = Arc::new(Mutex::new(Snapshot {
            cpu: "--".into(),
            mem: "--".into(),
            gpu: "--".into(),
            wifi: None,
        }));
        spawn_sampler(Arc::clone(&shared));
        Self {
            shared,
            cpu: "--".into(),
            mem: "--".into(),
            gpu: "--".into(),
            wifi: None,
        }
    }

    /// Write the latest sampled display strings onto the frame (never blocks).
    pub fn sample_into(&mut self, frame: &mut TelemetryFrame) {
        if let Ok(snap) = self.shared.try_lock() {
            self.cpu = snap.cpu.clone();
            self.mem = snap.mem.clone();
            self.gpu = snap.gpu.clone();
            self.wifi = snap.wifi.clone();
        }
        frame.cpu = Some(self.cpu.clone());
        frame.mem = Some(self.mem.clone());
        frame.gpu = Some(self.gpu.clone());
        // WiFi fills channel quality when iRacing channel stats are unavailable.
        if frame.chan_quality.map(|q| q <= 0.0).unwrap_or(true) {
            if let Some(ref w) = self.wifi {
                if let Ok(q) = w.trim_end_matches('%').parse::<f32>() {
                    frame.chan_quality = Some(q);
                }
            }
        }
    }
}

fn fmt_pct(v: Option<f32>) -> String {
    match v {
        Some(n) if n.is_finite() => format!("{:.0}%", n.clamp(0.0, 100.0)),
        _ => "--".into(),
    }
}

/// Spawn `nvidia-smi` and read GPU utilisation. Costs ~150 ms — sampler only.
fn probe_gpu() -> Option<f32> {
    // nvidia-smi works on Linux/Windows when present; macOS typically unavailable.
    // On Windows, CREATE_NO_WINDOW is required or a console flashes every sample.
    let mut cmd = Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=utilization.gpu",
        "--format=csv,noheader,nounits",
    ]);
    crate::win_process::no_window(&mut cmd);
    let out = cmd.output().ok()?;
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

/// Sample CPU/memory every `SAMPLE_INTERVAL` and GPU every `GPU_INTERVAL`,
/// publishing formatted strings for the host to pick up for free.
fn spawn_sampler(shared: Arc<Mutex<Snapshot>>) {
    std::thread::Builder::new()
        .name("sysstats".into())
        .spawn(move || {
            let kind = RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::nothing().with_cpu_usage())
                .with_memory(MemoryRefreshKind::everything());
            let mut sys = System::new_with_specifics(kind);
            // Prime CPU counters so the first refresh yields a real delta.
            sys.refresh_cpu_usage();

            let mut gpu_at: Option<Instant> = None;
            let mut gpu_misses = 0u32;

            loop {
                std::thread::sleep(SAMPLE_INTERVAL);
                sys.refresh_cpu_usage();
                sys.refresh_memory_specifics(MemoryRefreshKind::everything());

                let cpu = fmt_pct(Some(sys.global_cpu_usage()));
                let total = sys.total_memory();
                let mem = fmt_pct(if total > 0 {
                    Some((sys.used_memory() as f64 / total as f64 * 100.0) as f32)
                } else {
                    None
                });

                // Publish the cheap values before the GPU process spawn, so a
                // slow (or absent) nvidia-smi never delays CPU/memory.
                let wifi = probe_wifi().map(|q| format!("{q:.0}%"));
                if let Ok(mut snap) = shared.lock() {
                    snap.cpu = cpu;
                    snap.mem = mem;
                    snap.wifi = wifi;
                }

                let gpu_due = gpu_at.map(|t| t.elapsed() >= GPU_INTERVAL).unwrap_or(true);
                if gpu_due && gpu_misses < GPU_MAX_MISSES {
                    let gpu = match probe_gpu() {
                        Some(v) => {
                            gpu_misses = 0;
                            fmt_pct(Some(v))
                        }
                        None => {
                            gpu_misses += 1;
                            "--".to_string()
                        }
                    };
                    gpu_at = Some(Instant::now());
                    if let Ok(mut snap) = shared.lock() {
                        snap.gpu = gpu;
                    }
                }
            }
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_into_sets_pct_strings() {
        let mut s = SysStats::new();
        let mut frame = TelemetryFrame::default();
        // Poll until the background sampler publishes (timing is not exact).
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            s.sample_into(&mut frame);
            if frame.mem.as_deref().unwrap_or("--") != "--" {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let cpu = frame.cpu.as_deref().unwrap_or("");
        let mem = frame.mem.as_deref().unwrap_or("");
        assert!(cpu.ends_with('%') || cpu == "--", "cpu={cpu}");
        assert!(mem.ends_with('%') || mem == "--", "mem={mem}");
        // Memory should almost always resolve on desktop.
        assert!(mem.ends_with('%'), "expected mem percent, got {mem}");
    }
}
