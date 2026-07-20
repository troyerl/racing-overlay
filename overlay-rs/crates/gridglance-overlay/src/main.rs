//! GridGlance Rust overlay binary — primary process (no Python required).

// Release builds: no console window (tray / overlay only). Debug keeps a
// console for eprintln! / clap help.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(clippy::too_many_arguments)]

mod app_icon;
mod autostart;
mod chrome;
mod cloud;
mod config;
mod driver_groups;
mod host;
mod icons;
mod ipc;
mod irating;
mod iracing_results;
mod layered;
mod map_markers;
mod paths;
mod settings;
mod shell;
mod state;
mod sysstats;
mod telemetry;
mod track_path;
mod tracks;
mod updater;
mod widgets;
mod win_click;

use anyhow::Result;
use clap::Parser;
use gridglance_ipc::DEFAULT_IPC_PORT;
use host::OverlayApp;

const WINDOW_TITLE: &str = "GridGlance Overlay";

#[derive(Parser, Debug)]
#[command(name = "gridglance-overlay", about = "GridGlance race overlay (egui)")]
struct Args {
    /// Drive widgets from a built-in demo feed (no iRacing).
    #[arg(long)]
    demo: bool,

    /// Allow dragging panels (disables click-through).
    #[arg(long)]
    no_clickthrough: bool,

    /// IPC listen port (localhost).
    #[arg(long, default_value_t = DEFAULT_IPC_PORT)]
    ipc_port: u16,

    /// Start with overlay panels hidden until overlay.start.
    #[arg(long)]
    stopped: bool,

    /// Show overlay panels immediately (overrides start_overlay_on_launch=false).
    #[arg(long)]
    start: bool,

    /// Open the in-overlay Settings window on launch.
    #[arg(long)]
    settings: bool,

    /// Open Settings on the Track Scan page (requires write URI).
    #[arg(long)]
    track_scan: bool,

    /// Do not open Settings on launch (used by Start-at-login shortcuts).
    #[arg(long)]
    no_settings: bool,

    /// Skip system tray (useful for headless / CI).
    #[arg(long)]
    no_tray: bool,

    /// Emit once-per-second frame/readback/present timing to stderr.
    #[arg(long)]
    perf: bool,

    /// Import members HTML/SVG to track JSON and exit (replaces tools/svg_layers_to_track_v2.py).
    #[arg(long, value_name = "FILE")]
    import_track: Option<std::path::PathBuf>,

    /// TrackID for `--import-track` (defaults to id embedded in HTML when present).
    #[arg(long)]
    track_id: Option<String>,

    /// Display name for `--import-track`.
    #[arg(long)]
    name: Option<String>,

    /// Output directory for `--import-track` (default: GridGlance tracks dir).
    #[arg(long)]
    out_dir: Option<std::path::PathBuf>,

    /// Overwrite an existing track JSON when using `--import-track`.
    #[arg(long)]
    force: bool,

    /// Loop sample count for `--import-track` (default: 400).
    #[arg(long, default_value_t = 400)]
    samples: usize,

    /// Auto corner count when turn-numbers layer is missing (0 = skip).
    #[arg(long, default_value_t = 4)]
    corners: usize,

    /// Start/finish lap fraction for `--import-track`.
    #[arg(long, default_value_t = 0.0)]
    start_finish_pct: f32,
}

fn run_import_track(args: &Args) -> Result<()> {
    let path = args
        .import_track
        .as_ref()
        .expect("import_track set");
    let mut doc = tracks::import_track_source(path, args.samples, args.corners, args.start_finish_pct)?;
    if let Some(ref tid) = args.track_id {
        doc.track_id = Some(
            tid.parse::<i64>()
                .map_err(|_| anyhow::anyhow!("track-id must be an integer, got {tid}"))?,
        );
    }
    if let Some(ref name) = args.name {
        doc.name = name.clone();
    }
    let tid = doc
        .track_id
        .ok_or_else(|| anyhow::anyhow!("No TrackID — pass --track-id or use HTML with track-map-N"))?;
    let out_dir = args
        .out_dir
        .clone()
        .unwrap_or_else(paths::tracks_dir);
    std::fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join(format!("{tid}.json"));
    if out_path.exists() && !args.force {
        anyhow::bail!(
            "Refusing to overwrite {} (use --force)",
            out_path.display()
        );
    }
    let mut json = doc.to_json();
    if let Some(obj) = json.as_object_mut() {
        obj.insert("track_id".into(), serde_json::json!(tid));
        obj.insert("name".into(), serde_json::json!(doc.name));
    }
    let text = serde_json::to_string_pretty(&json)?;
    std::fs::write(&out_path, format!("{text}\n"))?;
    println!(
        "Wrote {} — {} loop pts (pit: draw manually in Track Scan)",
        out_path.display(),
        doc.points.len()
    );
    Ok(())
}

fn main() -> Result<()> {
    cloud::load_dotenv();
    app_icon::set_windows_app_user_model_id();

    let args = Args::parse();
    if args.import_track.is_some() {
        return run_import_track(&args);
    }

    // Single-instance: second launch activates Settings on the primary.
    let activate_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let activate_flag2 = activate_flag.clone();
    let _guard = match shell::acquire_instance(move || {
        activate_flag2.store(true, std::sync::atomic::Ordering::SeqCst);
    }) {
        Some(g) => g,
        None => {
            // Peer activated — exit quietly.
            return Ok(());
        }
    };

    let config = config::OverlayConfig::load().unwrap_or_default();
    let start_on_launch = config
        .cfg
        .get("start_overlay_on_launch")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let check_updates = config
        .cfg
        .get("check_updates_on_launch")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let click_through = !args.no_clickthrough;
    let state = state::new_state(config, click_through, args.demo);
    // Python parity: widgets stay hidden unless --start / --demo / pref / not --stopped.
    let start_now = (args.start || args.demo || start_on_launch) && !args.stopped;
    if !start_now {
        state.write().running = false;
    }
    let open_settings = (args.settings || args.track_scan) && !args.no_settings;
    if open_settings {
        state.write().settings_open = true;
    }
    if args.track_scan {
        state.write().settings_section = "__scan__".into();
    }

    // Background cloud track sync + app-settings cache (pro ★ badges).
    cloud::sync_down_async(paths::tracks_dir());
    cloud::fetch_app_settings_async();

    if check_updates {
        let state_upd = state.clone();
        std::thread::spawn(move || match updater::fetch_latest(8) {
            Ok(Some(info)) if updater::is_newer(&info.version, updater::VERSION) => {
                eprintln!(
                    "[gridglance] update available: {} (current {})",
                    info.version,
                    updater::VERSION
                );
                if let Some(mut st) = state_upd.try_write() {
                    st.pending_update = Some((info.version, info.url));
                    st.settings_open = true;
                    st.settings_section = "__app__".into();
                }
            }
            Ok(_) => {}
            Err(e) => eprintln!("[gridglance] update check: {e}"),
        });
    }

    ipc::spawn(state.clone(), args.ipc_port)?;

    let tray: Option<shell::TrayHandle> = if args.no_tray {
        None
    } else {
        match shell::spawn_tray() {
            Ok(handle) => Some(handle),
            Err(e) => {
                eprintln!("[gridglance] tray unavailable: {e}");
                None
            }
        }
    };

    // Tiny hidden root — panels are separate immediate viewports.
    let mut root_viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size([1.0, 1.0])
        .with_position(egui::pos2(-32000.0, -32000.0))
        .with_decorations(false)
        .with_transparent(true)
        .with_taskbar(false)
        .with_visible(false);
    if let Some(icon) = app_icon::egui_icon() {
        root_viewport = root_viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport: root_viewport,
        renderer: eframe::Renderer::Glow,
        multisampling: 1,
        ..Default::default()
    };

    let demo = args.demo;
    let perf = args.perf;
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            icons::install_fonts(&cc.egui_ctx);
            let mut visuals = cc.egui_ctx.style().visuals.clone();
            visuals.panel_fill = egui::Color32::TRANSPARENT;
            visuals.window_fill = egui::Color32::TRANSPARENT;
            visuals.extreme_bg_color = egui::Color32::TRANSPARENT;
            visuals.faint_bg_color = egui::Color32::TRANSPARENT;
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(OverlayApp::new(
                state,
                demo,
                perf,
                cc.gl.clone(),
                tray,
                activate_flag,
            )))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))?;
    Ok(())
}
