//! Dispatcher for Skia overlay panels (non-table / non-dash).

use super::anim::AnimStore;
use super::canvas::{Canvas, TextAlign};
use super::chrome::{draw_card, section_color};
use super::fuel_panel;
use super::inputs_panel;
use super::leaderboard_panel;
use super::map as map_panel;
use super::mid_panels;
use super::radar_panel;
use super::simple_panels;
use super::tokens::DesignTokens;
use super::types::{FontSpec, Rect};
use crate::config::OverlayConfig;
use crate::state::MapAuthoring;
use crate::telemetry::TelemetryFrame;

pub struct PanelPaintCtx<'a> {
    pub cfg: &'a OverlayConfig,
    pub frame: &'a TelemetryFrame,
    pub edit_mode: bool,
    pub demo: bool,
    pub map: &'a mut MapAuthoring,
    pub mono_secs: f64,
    pub anim: &'a mut AnimStore,
}

pub fn paint_panel(c: &mut Canvas, key: &str, ctx: &mut PanelPaintCtx<'_>) -> bool {
    match key {
        "relative" | "standings" | "dash" => false,
        "delta_bar" => simple_panels::paint_delta_bar(
            c,
            ctx.cfg,
            ctx.frame,
            &mut ctx.anim.delta,
            ctx.mono_secs,
            ctx.edit_mode,
        ),
        "flags" => simple_panels::paint_flags(
            c,
            ctx.cfg,
            ctx.frame,
            &mut ctx.anim.flags,
            ctx.mono_secs,
            ctx.edit_mode,
        ),
        "radio_tower" => simple_panels::paint_radio(
            c,
            ctx.cfg,
            ctx.frame,
            &mut ctx.anim.radio,
            ctx.mono_secs,
            ctx.edit_mode,
        ),
        "pit_advisor" => simple_panels::paint_pit_advisor(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "pace_caution" => {
            simple_panels::paint_pace_caution(c, ctx.cfg, ctx.frame, ctx.map, ctx.edit_mode)
        }
        "system_panel" => simple_panels::paint_system(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "tire_panel" => mid_panels::paint_tire(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "pit_board" => mid_panels::paint_pit_board(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "weather_panel" => mid_panels::paint_weather(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "ers_hybrid" => mid_panels::paint_ers(
            c,
            ctx.cfg,
            ctx.frame,
            &mut ctx.anim.ers,
            ctx.mono_secs,
            ctx.edit_mode,
        ),
        "sector_timing" => mid_panels::paint_sector_timing(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "lap_compare" => mid_panels::paint_lap_compare(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "laptime_log" => mid_panels::paint_laptime_log(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "inputs" => inputs_panel::paint_inputs(
            c,
            ctx.cfg,
            ctx.frame,
            &mut ctx.anim.inputs,
            ctx.mono_secs,
            ctx.edit_mode,
        ),
        "radar" => radar_panel::paint_radar(
            c,
            ctx.cfg,
            ctx.frame,
            &mut ctx.anim.radar,
            ctx.mono_secs,
            ctx.edit_mode,
        ),
        "fuel_calc" => fuel_panel::paint_fuel(c, ctx.cfg, ctx.frame, ctx.edit_mode),
        "leaderboard_strip" => {
            leaderboard_panel::paint_leaderboard(c, ctx.cfg, ctx.frame, ctx.edit_mode)
        }
        "map" => map_panel::paint(
            c,
            ctx.cfg,
            ctx.frame,
            ctx.map,
            ctx.mono_secs,
            ctx.demo,
            ctx.edit_mode,
        ),
        _ => {
            paint_generic(c, key, ctx);
            false
        }
    }
}

fn paint_generic(c: &mut Canvas, key: &str, ctx: &mut PanelPaintCtx<'_>) {
    let tokens = DesignTokens::for_section(ctx.cfg, key, c.height() as f32);
    let bounds = Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32);
    c.clear_transparent();
    draw_card(c, &tokens, bounds);
    let title = key.replace('_', " ").to_uppercase();
    c.text(
        &title,
        bounds.center().0,
        bounds.center().1 - 8.0,
        FontSpec::bold(14.0),
        tokens.muted,
        TextAlign::Center,
    );
    c.text(
        &format!(
            "P{} · Lap {}",
            ctx.frame.position.max(0),
            ctx.frame.lap.max(0)
        ),
        bounds.center().0,
        bounds.center().1 + 14.0,
        FontSpec::new(12.0),
        tokens.text,
        TextAlign::Center,
    );
    if ctx.edit_mode {
        c.stroke_rect(
            bounds.inset(3.0, 3.0),
            section_color(ctx.cfg, key, "accent", "#70df7a").with_alpha(160),
            tokens.radius,
            1.5,
        );
    }
}
