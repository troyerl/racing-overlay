//! GridGlance Rust overlay binary.

mod chrome;
mod config;
mod host;
mod icons;
mod ipc;
mod irating;
mod paths;
mod state;
mod telemetry;
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
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = config::OverlayConfig::load().unwrap_or_default();
    let click_through = !args.no_clickthrough;
    let state = state::new_state(config, click_through);
    if args.stopped {
        state.write().running = false;
    }

    ipc::spawn(state.clone(), args.ipc_port)?;

    let (vx, vy, vw, vh) = win_click::virtual_desktop_rect();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_position(egui::pos2(vx as f32, vy as f32))
            .with_inner_size([vw as f32, vh as f32])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_mouse_passthrough(click_through),
        // Glow + no MSAA: needed for per-pixel transparency on Windows.
        renderer: eframe::Renderer::Glow,
        multisampling: 1,
        ..Default::default()
    };

    let demo = args.demo;
    eframe::run_native(
        WINDOW_TITLE,
        options,
        Box::new(move |cc| {
            icons::install_fonts(&cc.egui_ctx);
            Ok(Box::new(OverlayApp::new(state, demo)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))?;
    Ok(())
}
