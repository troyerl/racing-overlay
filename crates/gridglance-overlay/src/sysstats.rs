//! Local-machine CPU / memory / GPU / WiFi for table slots (Python `sysstats`).

use crate::telemetry::TelemetryFrame;
use std::collections::HashMap;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::{
    CpuRefreshKind, MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind,
    System,
};

const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
/// `nvidia-smi` is a process spawn (~150 ms). Poll it far less often than CPU.
const GPU_INTERVAL: Duration = Duration::from_secs(3);
/// Stop probing after this many consecutive failures (no NVIDIA GPU present).
const GPU_MAX_MISSES: u32 = 3;

/// Display strings for one tracked process group.
#[derive(Clone)]
struct ProcSnap {
    cpu: String,
    mem: String,
    gpu: String,
}

impl Default for ProcSnap {
    fn default() -> Self {
        Self {
            cpu: "--".into(),
            mem: "--".into(),
            gpu: "--".into(),
        }
    }
}

/// Display strings published by the sampler thread.
#[derive(Clone, Default)]
struct Snapshot {
    cpu: String,
    mem: String,
    gpu: String,
    wifi: Option<String>,
    iracing: ProcSnap,
    overlay: ProcSnap,
    racelab: ProcSnap,
    ioverlay: ProcSnap,
    spotify: ProcSnap,
    apple_music: ProcSnap,
    youtube_music: ProcSnap,
}

pub struct SysStats {
    /// Latest values from the background sampler; reads never block the UI.
    shared: Arc<Mutex<Snapshot>>,
    cpu: String,
    mem: String,
    gpu: String,
    wifi: Option<String>,
    iracing: ProcSnap,
    overlay: ProcSnap,
    racelab: ProcSnap,
    ioverlay: ProcSnap,
    spotify: ProcSnap,
    apple_music: ProcSnap,
    youtube_music: ProcSnap,
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
        let dash = "--".to_string();
        let shared = Arc::new(Mutex::new(Snapshot {
            cpu: dash.clone(),
            mem: dash.clone(),
            gpu: dash,
            wifi: None,
            ..Default::default()
        }));
        spawn_sampler(Arc::clone(&shared));
        Self {
            shared,
            cpu: "--".into(),
            mem: "--".into(),
            gpu: "--".into(),
            wifi: None,
            iracing: ProcSnap::default(),
            overlay: ProcSnap::default(),
            racelab: ProcSnap::default(),
            ioverlay: ProcSnap::default(),
            spotify: ProcSnap::default(),
            apple_music: ProcSnap::default(),
            youtube_music: ProcSnap::default(),
        }
    }

    /// Write the latest sampled display strings onto the frame (never blocks).
    pub fn sample_into(&mut self, frame: &mut TelemetryFrame) {
        if let Ok(snap) = self.shared.try_lock() {
            self.cpu = snap.cpu.clone();
            self.mem = snap.mem.clone();
            self.gpu = snap.gpu.clone();
            self.wifi = snap.wifi.clone();
            self.iracing = snap.iracing.clone();
            self.overlay = snap.overlay.clone();
            self.racelab = snap.racelab.clone();
            self.ioverlay = snap.ioverlay.clone();
            self.spotify = snap.spotify.clone();
            self.apple_music = snap.apple_music.clone();
            self.youtube_music = snap.youtube_music.clone();
        }
        frame.cpu = Some(self.cpu.clone());
        frame.mem = Some(self.mem.clone());
        frame.gpu = Some(self.gpu.clone());
        apply_proc(frame, "iracing", &self.iracing);
        apply_proc(frame, "overlay", &self.overlay);
        apply_proc(frame, "racelab", &self.racelab);
        apply_proc(frame, "ioverlay", &self.ioverlay);
        apply_proc(frame, "spotify", &self.spotify);
        apply_proc(frame, "apple_music", &self.apple_music);
        apply_proc(frame, "youtube_music", &self.youtube_music);
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

fn apply_proc(frame: &mut TelemetryFrame, key: &str, p: &ProcSnap) {
    match key {
        "iracing" => {
            frame.cpu_iracing = Some(p.cpu.clone());
            frame.mem_iracing = Some(p.mem.clone());
            frame.gpu_iracing = Some(p.gpu.clone());
        }
        "overlay" => {
            frame.cpu_overlay = Some(p.cpu.clone());
            frame.mem_overlay = Some(p.mem.clone());
            frame.gpu_overlay = Some(p.gpu.clone());
        }
        "racelab" => {
            frame.cpu_racelab = Some(p.cpu.clone());
            frame.mem_racelab = Some(p.mem.clone());
            frame.gpu_racelab = Some(p.gpu.clone());
        }
        "ioverlay" => {
            frame.cpu_ioverlay = Some(p.cpu.clone());
            frame.mem_ioverlay = Some(p.mem.clone());
            frame.gpu_ioverlay = Some(p.gpu.clone());
        }
        "spotify" => {
            frame.cpu_spotify = Some(p.cpu.clone());
            frame.mem_spotify = Some(p.mem.clone());
            frame.gpu_spotify = Some(p.gpu.clone());
        }
        "apple_music" => {
            frame.cpu_apple_music = Some(p.cpu.clone());
            frame.mem_apple_music = Some(p.mem.clone());
            frame.gpu_apple_music = Some(p.gpu.clone());
        }
        "youtube_music" => {
            frame.cpu_youtube_music = Some(p.cpu.clone());
            frame.mem_youtube_music = Some(p.mem.clone());
            frame.gpu_youtube_music = Some(p.gpu.clone());
        }
        _ => {}
    }
}

fn fmt_pct(v: Option<f32>) -> String {
    match v {
        Some(n) if n.is_finite() => format!("{:.0}%", n.clamp(0.0, 100.0)),
        _ => "--".into(),
    }
}

fn fmt_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else if mb >= 1.0 {
        format!("{:.0} MB", mb)
    } else if bytes > 0 {
        format!("{:.0} KB", bytes as f64 / 1024.0)
    } else {
        "--".into()
    }
}

fn process_stem(name: &str) -> String {
    name.rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .to_ascii_lowercase()
        .trim_end_matches(".exe")
        .to_string()
}

fn is_iracing_sim(stem: &str) -> bool {
    stem.starts_with("iracingsim")
}

fn is_racelab(stem: &str) -> bool {
    stem.contains("racelab")
}

fn is_ioverlay(stem: &str) -> bool {
    stem.starts_with("ioverlay")
}

fn is_spotify(stem: &str) -> bool {
    stem.starts_with("spotify")
}

fn is_apple_music(stem: &str) -> bool {
    let n = stem.replace([' ', '-', '_'], "");
    n.starts_with("applemusic")
}

fn is_youtube_music(stem: &str) -> bool {
    let n = stem.replace([' ', '-', '_'], "");
    n.contains("youtubemusic")
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

/// Best-effort per-PID GPU SM util via `nvidia-smi pmon -c 1`.
/// Returns empty map when unavailable (non-NVIDIA / graphics util not reported).
fn probe_gpu_by_pid() -> HashMap<u32, f32> {
    let mut cmd = Command::new("nvidia-smi");
    cmd.args(["pmon", "-c", "1"]);
    crate::win_process::no_window(&mut cmd);
    let out = match cmd.output() {
        Ok(o) if o.status.success() => o,
        _ => return HashMap::new(),
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut by_pid: HashMap<u32, f32> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<_> = line.split_whitespace().collect();
        // gpu pid type sm mem enc dec command
        if parts.len() < 4 {
            continue;
        }
        let pid: u32 = match parts[1].parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let sm = match parts[3].parse::<f32>() {
            Ok(v) if v.is_finite() => v,
            _ => continue,
        };
        by_pid
            .entry(pid)
            .and_modify(|e| *e = (*e + sm).min(100.0))
            .or_insert(sm.clamp(0.0, 100.0));
    }
    by_pid
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

struct ProcessTotals {
    cpu_pct: f32,
    mem_bytes: u64,
    pids: Vec<u32>,
    found: bool,
}

impl ProcessTotals {
    fn zero() -> Self {
        Self {
            cpu_pct: 0.0,
            mem_bytes: 0,
            pids: Vec::new(),
            found: false,
        }
    }

    fn add(&mut self, pid: u32, cpu_pct: f32, mem_bytes: u64) {
        self.found = true;
        self.cpu_pct += cpu_pct;
        self.mem_bytes = self.mem_bytes.saturating_add(mem_bytes);
        self.pids.push(pid);
    }

    fn to_snap(&self, gpu: &str) -> ProcSnap {
        ProcSnap {
            cpu: if self.found {
                fmt_pct(Some(self.cpu_pct))
            } else {
                "--".into()
            },
            mem: if self.found {
                fmt_bytes(self.mem_bytes)
            } else {
                "--".into()
            },
            gpu: gpu.to_string(),
        }
    }
}

struct TrackedProcesses {
    iracing: ProcessTotals,
    overlay: ProcessTotals,
    racelab: ProcessTotals,
    ioverlay: ProcessTotals,
    spotify: ProcessTotals,
    apple_music: ProcessTotals,
    youtube_music: ProcessTotals,
}

fn collect_tracked(sys: &System) -> TrackedProcesses {
    let self_pid = Pid::from_u32(std::process::id());
    let n_cpus = sys.cpus().len().max(1) as f32;
    let mut out = TrackedProcesses {
        iracing: ProcessTotals::zero(),
        overlay: ProcessTotals::zero(),
        racelab: ProcessTotals::zero(),
        ioverlay: ProcessTotals::zero(),
        spotify: ProcessTotals::zero(),
        apple_music: ProcessTotals::zero(),
        youtube_music: ProcessTotals::zero(),
    };

    for (pid, proc_) in sys.processes() {
        let stem = process_stem(&proc_.name().to_string_lossy());
        let cpu = (proc_.cpu_usage() / n_cpus).clamp(0.0, 100.0);
        let mem = proc_.memory();
        let pid_u = pid.as_u32();
        if *pid == self_pid {
            out.overlay.add(pid_u, cpu, mem);
        } else if is_iracing_sim(&stem) {
            out.iracing.add(pid_u, cpu, mem);
        } else if is_racelab(&stem) {
            out.racelab.add(pid_u, cpu, mem);
        } else if is_ioverlay(&stem) {
            out.ioverlay.add(pid_u, cpu, mem);
        } else if is_spotify(&stem) {
            out.spotify.add(pid_u, cpu, mem);
        } else if is_apple_music(&stem) {
            out.apple_music.add(pid_u, cpu, mem);
        } else if is_youtube_music(&stem) {
            out.youtube_music.add(pid_u, cpu, mem);
        }
    }
    out
}

fn gpu_for_pids(by_pid: &HashMap<u32, f32>, pids: &[u32]) -> String {
    if pids.is_empty() {
        return "--".into();
    }
    let mut sum = 0.0f32;
    let mut any = false;
    for pid in pids {
        if let Some(&v) = by_pid.get(pid) {
            sum += v;
            any = true;
        }
    }
    if any {
        fmt_pct(Some(sum.min(100.0)))
    } else {
        "--".into()
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
            let proc_kind = ProcessRefreshKind::nothing().with_cpu().with_memory();
            sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_kind);

            let mut gpu_at: Option<Instant> = None;
            let mut gpu_misses = 0u32;
            let mut gpu_iracing = "--".to_string();
            let mut gpu_overlay = "--".to_string();
            let mut gpu_racelab = "--".to_string();
            let mut gpu_ioverlay = "--".to_string();
            let mut gpu_spotify = "--".to_string();
            let mut gpu_apple_music = "--".to_string();
            let mut gpu_youtube_music = "--".to_string();

            loop {
                std::thread::sleep(SAMPLE_INTERVAL);
                sys.refresh_cpu_usage();
                sys.refresh_memory_specifics(MemoryRefreshKind::everything());
                sys.refresh_processes_specifics(ProcessesToUpdate::All, true, proc_kind);

                let cpu = fmt_pct(Some(sys.global_cpu_usage()));
                let total = sys.total_memory();
                let mem = fmt_pct(if total > 0 {
                    Some((sys.used_memory() as f64 / total as f64 * 100.0) as f32)
                } else {
                    None
                });

                let tracked = collect_tracked(&sys);

                // Publish the cheap values before the GPU process spawn, so a
                // slow (or absent) nvidia-smi never delays CPU/memory.
                let wifi = probe_wifi().map(|q| format!("{q:.0}%"));
                if let Ok(mut snap) = shared.lock() {
                    snap.cpu = cpu;
                    snap.mem = mem;
                    snap.wifi = wifi;
                    snap.iracing = tracked.iracing.to_snap(&gpu_iracing);
                    snap.overlay = tracked.overlay.to_snap(&gpu_overlay);
                    snap.racelab = tracked.racelab.to_snap(&gpu_racelab);
                    snap.ioverlay = tracked.ioverlay.to_snap(&gpu_ioverlay);
                    snap.spotify = tracked.spotify.to_snap(&gpu_spotify);
                    snap.apple_music = tracked.apple_music.to_snap(&gpu_apple_music);
                    snap.youtube_music = tracked.youtube_music.to_snap(&gpu_youtube_music);
                }

                let gpu_due = gpu_at.map(|t| t.elapsed() >= GPU_INTERVAL).unwrap_or(true);
                if gpu_due && gpu_misses < GPU_MAX_MISSES {
                    let gpu = match probe_gpu() {
                        Some(v) => {
                            gpu_misses = 0;
                            let by_pid = probe_gpu_by_pid();
                            gpu_iracing = gpu_for_pids(&by_pid, &tracked.iracing.pids);
                            gpu_overlay = gpu_for_pids(&by_pid, &tracked.overlay.pids);
                            gpu_racelab = gpu_for_pids(&by_pid, &tracked.racelab.pids);
                            gpu_ioverlay = gpu_for_pids(&by_pid, &tracked.ioverlay.pids);
                            gpu_spotify = gpu_for_pids(&by_pid, &tracked.spotify.pids);
                            gpu_apple_music = gpu_for_pids(&by_pid, &tracked.apple_music.pids);
                            gpu_youtube_music = gpu_for_pids(&by_pid, &tracked.youtube_music.pids);
                            fmt_pct(Some(v))
                        }
                        None => {
                            gpu_misses += 1;
                            gpu_iracing = "--".into();
                            gpu_overlay = "--".into();
                            gpu_racelab = "--".into();
                            gpu_ioverlay = "--".into();
                            gpu_spotify = "--".into();
                            gpu_apple_music = "--".into();
                            gpu_youtube_music = "--".into();
                            "--".to_string()
                        }
                    };
                    gpu_at = Some(Instant::now());
                    if let Ok(mut snap) = shared.lock() {
                        snap.gpu = gpu;
                        snap.iracing.gpu = gpu_iracing.clone();
                        snap.overlay.gpu = gpu_overlay.clone();
                        snap.racelab.gpu = gpu_racelab.clone();
                        snap.ioverlay.gpu = gpu_ioverlay.clone();
                        snap.spotify.gpu = gpu_spotify.clone();
                        snap.apple_music.gpu = gpu_apple_music.clone();
                        snap.youtube_music.gpu = gpu_youtube_music.clone();
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
        // Overlay process should resolve once process list is ready.
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            s.sample_into(&mut frame);
            if frame.cpu_overlay.as_deref().unwrap_or("--") != "--" {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let overlay_cpu = frame.cpu_overlay.as_deref().unwrap_or("--");
        let overlay_mem = frame.mem_overlay.as_deref().unwrap_or("--");
        assert!(
            overlay_cpu.ends_with('%') || overlay_cpu == "--",
            "cpu_overlay={overlay_cpu}"
        );
        assert!(
            overlay_mem.contains("MB") || overlay_mem.contains("GB") || overlay_mem == "--",
            "mem_overlay={overlay_mem}"
        );
    }

    #[test]
    fn process_name_match() {
        assert!(is_iracing_sim("iracingsim64dx11"));
        assert!(is_iracing_sim(&process_stem("iRacingSim64.exe")));
        assert!(!is_iracing_sim("iracingui"));
        assert!(is_racelab("racelab"));
        assert!(is_racelab(&process_stem("RaceLab.exe")));
        assert!(is_ioverlay(&process_stem("iOverlay.exe")));
        assert!(is_spotify(&process_stem("Spotify.exe")));
        assert!(is_apple_music(&process_stem("AppleMusic.exe")));
        assert!(is_apple_music("apple music"));
        assert!(is_youtube_music(&process_stem("YouTube Music.exe")));
        assert!(is_youtube_music("youtubemusic"));
        assert!(!is_youtube_music("youtube"));
        assert!(!is_spotify("chrome"));
    }

    #[test]
    fn fmt_bytes_scales() {
        assert_eq!(fmt_bytes(0), "--");
        assert_eq!(fmt_bytes(5 * 1024 * 1024), "5 MB");
        assert_eq!(fmt_bytes((4.1 * 1024.0 * 1024.0 * 1024.0) as u64), "4.1 GB");
    }
}
