use super::WidgetCtx;
use crate::chrome::{draw_card, draw_section_header, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "weather_panel";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, radius) = draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(card.height());
    let mut y = card.top() + pad;
    if ctx.cfg.bool_key(SECTION, "show_title", true) {
        let hh = (card.height() * 0.12).max(22.0);
        let hdr = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, hh),
        );
        draw_section_header(
            ui,
            ctx.cfg,
            SECTION,
            hdr,
            &ctx.cfg.str_key(SECTION, "title", "WEATHER"),
            radius,
        );
        y += hh + pad * 0.35;
    }

    let f = ctx.frame;
    let mut lines: Vec<(String, String)> = Vec::new();
    if ctx.cfg.bool_key(SECTION, "show_skies", true) {
        let mut extra = Vec::new();
        if let Some(h) = f.humidity {
            extra.push(format!("{h:.0}% RH"));
        }
        if let Some(fog) = f.fog {
            if fog > 0.0 {
                extra.push(format!("Fog {fog:.0}%"));
            }
        }
        lines.push((
            "SKY".into(),
            format!(
                "{}  {}",
                f.skies.as_deref().unwrap_or("—"),
                extra.join("  ")
            )
            .trim()
            .into(),
        ));
    }
    if ctx.cfg.bool_key(SECTION, "show_rain", true) {
        let mut parts = Vec::new();
        if let Some(w) = f.track_wetness {
            parts.push(format!("Track {w:.0}%"));
        }
        if let Some(r) = f.rain_intensity {
            parts.push(format!("Rain {r:.0}%"));
        }
        lines.push((
            "WET".into(),
            if parts.is_empty() {
                "—".into()
            } else {
                parts.join("  ")
            },
        ));
    }
    if ctx.cfg.bool_key(SECTION, "show_temps", true) {
        let mut ts = Vec::new();
        if let Some(t) = f.track_temp {
            ts.push(format!("T {t:.0}°"));
        }
        if let Some(a) = f.air_temp {
            ts.push(format!("A {a:.0}°"));
        }
        lines.push((
            "TEMP".into(),
            if ts.is_empty() {
                "—".into()
            } else {
                ts.join("  ")
            },
        ));
    }
    if ctx.cfg.bool_key(SECTION, "show_wind", true) {
        let wind = match (f.wind_dir, f.wind_vel) {
            (Some(d), Some(v)) => format!("{d:.0}° @ {v:.1} m/s"),
            _ => "—".into(),
        };
        lines.push(("WIND".into(), wind));
    }

    let n = lines.len().max(1) as f32;
    let avail = card.bottom() - pad - y;
    let rh = avail / n;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    for (k, v) in lines {
        let row = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, rh),
        );
        label(
            ui,
            Pos2::new(row.left() + 6.0, row.center().y),
            Align2::LEFT_CENTER,
            &k,
            rh * 0.32,
            muted,
            true,
        );
        label(
            ui,
            Pos2::new(row.right() - 6.0, row.center().y),
            Align2::RIGHT_CENTER,
            &v,
            rh * 0.36,
            text,
            false,
        );
        y += rh;
    }
}
