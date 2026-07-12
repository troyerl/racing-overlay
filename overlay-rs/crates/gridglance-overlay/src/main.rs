//! GridGlance Rust overlay binary.

mod chrome;
mod config;
mod host;
mod icons;
mod ipc;
mod irating;
mod layered;
mod paths;
mod state;
mod telemetry;
mod track_path;
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

    // Tiny hidden root — panels are separate immediate viewports. Never cover
    // the desktop with a failed-transparent fullscreen surface.
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(WINDOW_TITLE)
            .with_inner_size([1.0, 1.0])
            .with_position(egui::pos2(-32000.0, -32000.0))
            .with_decorations(false)
            .with_transparent(true)
            .with_taskbar(false)
            .with_visible(false),
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
            let mut visuals = cc.egui_ctx.style().visuals.clone();
            visuals.panel_fill = egui::Color32::TRANSPARENT;
            visuals.window_fill = egui::Color32::TRANSPARENT;
            visuals.extreme_bg_color = egui::Color32::TRANSPARENT;
            visuals.faint_bg_color = egui::Color32::TRANSPARENT;
            cc.egui_ctx.set_visuals(visuals);
            Ok(Box::new(OverlayApp::new(state, demo, cc.gl.clone())))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))?;
    Ok(())
}
