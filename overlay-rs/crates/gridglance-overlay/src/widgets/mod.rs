//! Overlay widgets (egui ports of the Python PyQt panels).

mod dash;
mod delta_bar;
mod ers_hybrid;
mod flags;
mod fuel_calc;
mod inputs;
mod lap_compare;
mod laptime_log;
mod leaderboard_strip;
mod map;
mod pit_advisor;
mod pit_board;
mod radar;
mod radio_tower;
mod relative;
mod scoreboard_digits;
mod sector_timing;
mod standings;
mod system_panel;
mod table;
mod tire_panel;
mod weather_panel;

use crate::config::OverlayConfig;
use crate::state::MapAuthoring;
use crate::telemetry::TelemetryFrame;
use egui::Ui;

pub struct WidgetCtx<'a> {
    pub cfg: &'a OverlayConfig,
    pub frame: &'a TelemetryFrame,
    pub edit_mode: bool,
    pub map: &'a mut MapAuthoring,
}

pub fn paint(ui: &mut Ui, key: &str, ctx: &mut WidgetCtx<'_>) {
    match key {
        "flags" => flags::paint(ui, ctx),
        "system_panel" => system_panel::paint(ui, ctx),
        "weather_panel" => weather_panel::paint(ui, ctx),
        "delta_bar" => delta_bar::paint(ui, ctx),
        "inputs" => inputs::paint(ui, ctx),
        "dash" => dash::paint(ui, ctx),
        "relative" => relative::paint(ui, ctx),
        "standings" => standings::paint(ui, ctx),
        "leaderboard_strip" => leaderboard_strip::paint(ui, ctx),
        "radio_tower" => radio_tower::paint(ui, ctx),
        "fuel_calc" => fuel_calc::paint(ui, ctx),
        "pit_advisor" => pit_advisor::paint(ui, ctx),
        "pit_board" => pit_board::paint(ui, ctx),
        "laptime_log" => laptime_log::paint(ui, ctx),
        "sector_timing" => sector_timing::paint(ui, ctx),
        "lap_compare" => lap_compare::paint(ui, ctx),
        "tire_panel" => tire_panel::paint(ui, ctx),
        "ers_hybrid" => ers_hybrid::paint(ui, ctx),
        "radar" => radar::paint(ui, ctx),
        "map" => map::paint(ui, ctx),
        _ => {
            let rect = crate::chrome::full_rect(ui);
            crate::chrome::draw_card(ui, ctx.cfg, key, rect);
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                format!("unported: {key}"),
                egui::FontId::proportional(14.0),
                ctx.cfg.color(key, "muted", "#8b93a1"),
            );
        }
    }
}
