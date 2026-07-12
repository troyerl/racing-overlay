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

#[derive(Parser, Debug)]
#[command(name = "gridglance-overlay", about = "GridGlance race overlay (Rust)")]
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

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("GridGlance Overlay")
            .with_inner_size([280.0, 48.0])
            .with_decorations(true)
            .with_transparent(false)
            .with_visible(true),
        ..Default::default()
    };

    let demo = args.demo;
    eframe::run_native(
        "GridGlance Overlay",
        options,
        Box::new(move |cc| {
            icons::install_fonts(&cc.egui_ctx);
            Ok(Box::new(OverlayApp::new(state, demo)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))?;
    Ok(())
}
