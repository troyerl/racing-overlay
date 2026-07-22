use super::WidgetCtx;
use crate::chrome::{
    color_with_alpha, draw_card, draw_elegant_card, draw_section_header, full_rect, label,
    panel_pad,
};
use crate::config::PanelStyle;
use crate::icons;
use egui::{Align2, Color32, CornerRadius, Pos2, Stroke, Ui, Vec2};
use std::f32::consts::TAU;

const SECTION: &str = "weather_panel";

enum WeatherRow {
    Text {
        key: String,
        value: String,
    },
    Wet {
        track: Option<f32>,
        rain: Option<f32>,
    },
    Wind {
        dir_rad: Option<f32>,
        vel: Option<f32>,
    },
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    match ctx.cfg.panel_style(SECTION) {
        PanelStyle::Elegant => paint_elegant(ui, ctx),
        PanelStyle::Data => paint_data(ui, ctx),
    }
}

fn paint_data(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
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

    let rows = collect_rows(ctx);
    let n = rows.len().max(1) as f32;
    let avail = card.bottom() - pad - y;
    let rh = avail / n;
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let accent = ctx.cfg.color(SECTION, "accent", "#5aa9ff");
    let bar_bg = ctx.cfg.color(SECTION, "bar_bg", "#ffffff18");

    for row_data in rows {
        let row = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2(card.width() - 2.0 * pad, rh),
        );
        match row_data {
            WeatherRow::Text { key, value } => {
                label(
                    ui,
                    Pos2::new(row.left() + 6.0, row.center().y),
                    Align2::LEFT_CENTER,
                    &key,
                    rh * 0.32,
                    muted,
                    true,
                );
                label(
                    ui,
                    Pos2::new(row.right() - 6.0, row.center().y),
                    Align2::RIGHT_CENTER,
                    &value,
                    rh * 0.36,
                    text,
                    false,
                );
            }
            WeatherRow::Wet { track, rain } => {
                let wet_max = track.unwrap_or(0.0).max(rain.unwrap_or(0.0));
                if wet_max > 35.0 {
                    ui.painter().rect_filled(
                        row,
                        CornerRadius::same(4),
                        color_with_alpha(accent, 28),
                    );
                }
                label(
                    ui,
                    Pos2::new(row.left() + 6.0, row.center().y),
                    Align2::LEFT_CENTER,
                    "WET",
                    rh * 0.32,
                    muted,
                    true,
                );
                let label_w = (row.width() * 0.18).max(36.0);
                let bars_left = row.left() + label_w;
                let bars_right = row.right() - 6.0;
                let bars_w = (bars_right - bars_left).max(40.0);
                let bar_h = (rh * 0.22).clamp(4.0, 10.0);
                let gap = (rh * 0.08).max(2.0);
                let mid = row.center().y;
                paint_wet_bar(
                    ui,
                    bars_left,
                    mid - bar_h - gap * 0.5,
                    bars_w,
                    bar_h,
                    track,
                    "T",
                    accent,
                    bar_bg,
                    text,
                    muted,
                );
                paint_wet_bar(
                    ui,
                    bars_left,
                    mid + gap * 0.5,
                    bars_w,
                    bar_h,
                    rain,
                    "R",
                    accent,
                    bar_bg,
                    text,
                    muted,
                );
            }
            WeatherRow::Wind { dir_rad, vel } => {
                label(
                    ui,
                    Pos2::new(row.left() + 6.0, row.center().y),
                    Align2::LEFT_CENTER,
                    "WIND",
                    rh * 0.32,
                    muted,
                    true,
                );
                let wind_txt = match (dir_rad, vel) {
                    (Some(d), Some(v)) => {
                        let deg = wind_dir_degrees(d);
                        format!("{deg:.0}° @ {v:.1} m/s")
                    }
                    _ => "—".into(),
                };
                let tick_r = (rh * 0.22).clamp(6.0, 12.0);
                let tick_c = Pos2::new(row.right() - 6.0 - tick_r, row.center().y);
                if let Some(d) = dir_rad {
                    paint_wind_tick(ui, tick_c, tick_r, d, accent);
                }
                label(
                    ui,
                    Pos2::new(tick_c.x - tick_r - 4.0, row.center().y),
                    Align2::RIGHT_CENTER,
                    &wind_txt,
                    rh * 0.36,
                    text,
                    false,
                );
            }
        }
        y += rh;
    }
}

/// Softer minimal layout: compact rows, same data, less empty chrome.
fn paint_elegant(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, _radius) = draw_elegant_card(ui, ctx.cfg, SECTION, rect);
    let pad_x = (card.width() * 0.05).clamp(8.0, 12.0);
    let pad_y = (card.height() * 0.05).clamp(6.0, 10.0);
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200);
    let accent = ctx.cfg.color(SECTION, "accent", "#5aa9ff");
    let bar_bg = color_with_alpha(text, 22);
    let f = ctx.frame;

    let show_title = ctx.cfg.bool_key(SECTION, "show_title", true);
    let show_sky = ctx.cfg.bool_key(SECTION, "show_skies", true);
    let show_wet = ctx.cfg.bool_key(SECTION, "show_rain", true);
    let show_temp = ctx.cfg.bool_key(SECTION, "show_temps", true);
    let show_wind = ctx.cfg.bool_key(SECTION, "show_wind", true);
    if !(show_title || show_sky || show_wet || show_temp || show_wind) {
        return;
    }

    let gap = 4.0;
    let mut y = card.top() + pad_y;
    let left = card.left() + pad_x;
    let width = card.width() - 2.0 * pad_x;

    if show_title {
        let title = ctx.cfg.str_key(SECTION, "title", "WEATHER");
        label(
            ui,
            Pos2::new(left, y + 5.0),
            Align2::LEFT_CENTER,
            &title,
            10.0,
            muted,
            false,
        );
        y += 12.0;
    }

    if show_sky {
        let skies = f.skies.as_deref().unwrap_or("—");
        let icon_sz = 12.0;
        let row_h = 18.0;
        let cy = y + row_h * 0.5;
        paint_icon(ui, left, cy, "weather", icon_sz, accent);
        let text_x = left + icon_sz + 6.0;
        let mut parts = vec![skies.to_string()];
        if let Some(h) = f.humidity {
            parts.push(format!("{h:.0}%"));
        }
        if let Some(fog) = f.fog {
            if fog > 0.0 {
                parts.push(format!("Fog {fog:.0}%"));
            }
        }
        label(
            ui,
            Pos2::new(text_x, cy),
            Align2::LEFT_CENTER,
            &parts.join(" · "),
            12.0,
            text,
            true,
        );
        y += row_h + gap;
    }

    if show_wet {
        let meter_h = 4.0;
        let meter_gap = 4.0;
        paint_soft_meter(
            ui,
            left,
            y,
            width,
            meter_h,
            f.track_wetness,
            accent,
            bar_bg,
            text,
            muted,
            "Trk",
        );
        paint_soft_meter(
            ui,
            left,
            y + meter_h + meter_gap,
            width,
            meter_h,
            f.rain_intensity,
            color_with_alpha(accent, 200),
            bar_bg,
            text,
            muted,
            "Rain",
        );
        y += meter_h * 2.0 + meter_gap + gap;
    }

    if show_temp || show_wind {
        let row_h = 16.0;
        let cy = y + row_h * 0.5;
        let mut x = left;
        if show_temp {
            let mut parts = Vec::new();
            if let Some(t) = f.track_temp {
                parts.push(format!("{:.0}°", ctx.cfg.conv_temp(t)));
            }
            if let Some(a) = f.air_temp {
                parts.push(format!("A {:.0}°", ctx.cfg.conv_temp(a)));
            }
            let val = if parts.is_empty() {
                "—".into()
            } else {
                parts.join(" ")
            };
            label(
                ui,
                Pos2::new(x, cy),
                Align2::LEFT_CENTER,
                &val,
                11.0,
                text,
                false,
            );
            x += 78.0;
        }
        if show_wind {
            let tick_r = 5.0;
            if let Some(d) = f.wind_dir {
                paint_wind_tick(ui, Pos2::new(x + tick_r, cy), tick_r, d, accent);
                x += tick_r * 2.0 + 4.0;
            }
            let wind_txt = match (f.wind_dir, f.wind_vel) {
                (Some(d), Some(v)) => format!("{:.0}° {:.0}m/s", wind_dir_degrees(d), v),
                _ => "—".into(),
            };
            label(
                ui,
                Pos2::new(x, cy),
                Align2::LEFT_CENTER,
                &wind_txt,
                11.0,
                text,
                false,
            );
        }
    }
}

fn collect_rows(ctx: &WidgetCtx<'_>) -> Vec<WeatherRow> {
    let f = ctx.frame;
    let mut rows: Vec<WeatherRow> = Vec::new();
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
        rows.push(WeatherRow::Text {
            key: "SKY".into(),
            value: format!(
                "{}  {}",
                f.skies.as_deref().unwrap_or("—"),
                extra.join("  ")
            )
            .trim()
            .into(),
        });
    }
    if ctx.cfg.bool_key(SECTION, "show_rain", true) {
        rows.push(WeatherRow::Wet {
            track: f.track_wetness,
            rain: f.rain_intensity,
        });
    }
    if ctx.cfg.bool_key(SECTION, "show_temps", true) {
        let mut ts = Vec::new();
        if let Some(t) = f.track_temp {
            ts.push(format!("T {t:.0}°"));
        }
        if let Some(a) = f.air_temp {
            ts.push(format!("A {a:.0}°"));
        }
        rows.push(WeatherRow::Text {
            key: "TEMP".into(),
            value: if ts.is_empty() {
                "—".into()
            } else {
                ts.join("  ")
            },
        });
    }
    if ctx.cfg.bool_key(SECTION, "show_wind", true) {
        rows.push(WeatherRow::Wind {
            dir_rad: f.wind_dir,
            vel: f.wind_vel,
        });
    }
    rows
}

fn paint_icon(ui: &mut Ui, x: f32, cy: f32, name: &str, size: f32, color: Color32) {
    if let Some(g) = icons::glyph(name) {
        ui.painter().text(
            Pos2::new(x, cy),
            Align2::LEFT_CENTER,
            g,
            icons::font_id(size),
            color,
        );
    }
}

fn paint_soft_meter(
    ui: &mut Ui,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    pct: Option<f32>,
    fill: Color32,
    bg: Color32,
    text: Color32,
    muted: Color32,
    caption: &str,
) {
    let cap_w = if caption.len() <= 4 { 28.0 } else { 40.0 };
    label(
        ui,
        Pos2::new(x, y + h * 0.5),
        Align2::LEFT_CENTER,
        caption,
        10.0,
        muted,
        false,
    );
    let pct_w = 28.0;
    let bar = egui::Rect::from_min_size(
        Pos2::new(x + cap_w, y),
        Vec2::new((w - cap_w - pct_w).max(24.0), h),
    );
    let rad = (h * 0.5).round().clamp(0.0, 255.0) as u8;
    ui.painter().rect_filled(bar, CornerRadius::same(rad), bg);
    if let Some(p) = pct {
        let frac = (p / 100.0).clamp(0.0, 1.0);
        let fill_w = bar.width() * frac;
        if fill_w > 0.5 {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(bar.min, Vec2::new(fill_w, bar.height())),
                CornerRadius::same(rad),
                fill,
            );
        }
        label(
            ui,
            Pos2::new(bar.right() + 6.0, y + h * 0.5),
            Align2::LEFT_CENTER,
            &format!("{p:.0}%"),
            10.0,
            text,
            false,
        );
    } else {
        label(
            ui,
            Pos2::new(bar.right() + 6.0, y + h * 0.5),
            Align2::LEFT_CENTER,
            "—",
            10.0,
            muted,
            false,
        );
    }
}

fn paint_wet_bar(
    ui: &mut Ui,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    pct: Option<f32>,
    tag: &str,
    fill: Color32,
    bg: Color32,
    text: Color32,
    muted: Color32,
) {
    let tag_w = (h * 1.6).max(10.0);
    label(
        ui,
        Pos2::new(x, y + h * 0.5),
        Align2::LEFT_CENTER,
        tag,
        h * 0.95,
        muted,
        true,
    );
    let bar = egui::Rect::from_min_size(
        Pos2::new(x + tag_w, y),
        Vec2::new((w - tag_w - 34.0).max(20.0), h),
    );
    ui.painter().rect_filled(bar, CornerRadius::same(3), bg);
    if let Some(p) = pct {
        let frac = (p / 100.0).clamp(0.0, 1.0);
        let fill_w = bar.width() * frac;
        if fill_w > 0.5 {
            ui.painter().rect_filled(
                egui::Rect::from_min_size(bar.min, Vec2::new(fill_w, bar.height())),
                CornerRadius::same(3),
                fill,
            );
        }
        label(
            ui,
            Pos2::new(bar.right() + 4.0, y + h * 0.5),
            Align2::LEFT_CENTER,
            &format!("{p:.0}%"),
            h * 0.95,
            text,
            false,
        );
    } else {
        label(
            ui,
            Pos2::new(bar.right() + 4.0, y + h * 0.5),
            Align2::LEFT_CENTER,
            "—",
            h * 0.95,
            muted,
            false,
        );
    }
}

/// iRacing `WindDir` is radians; tolerate already-degrees values.
fn wind_dir_degrees(dir: f32) -> f32 {
    if dir.abs() <= TAU + 0.25 {
        dir.to_degrees().rem_euclid(360.0)
    } else {
        dir.rem_euclid(360.0)
    }
}

fn paint_wind_tick(ui: &mut Ui, c: Pos2, r: f32, dir_rad: f32, color: Color32) {
    let ang = if dir_rad.abs() <= TAU + 0.25 {
        dir_rad
    } else {
        dir_rad.to_radians()
    };
    // Screen: 0° north-up, clockwise from telemetry (math angle from +X, CCW).
    // Convert meteorological "from" direction (0 = north) to screen vector.
    let screen = ang - std::f32::consts::FRAC_PI_2;
    let tip = Pos2::new(c.x + screen.cos() * r, c.y + screen.sin() * r);
    let back = Pos2::new(c.x - screen.cos() * r * 0.55, c.y - screen.sin() * r * 0.55);
    let perp = Pos2::new(-screen.sin(), screen.cos());
    let left = Pos2::new(back.x + perp.x * r * 0.4, back.y + perp.y * r * 0.4);
    let right = Pos2::new(back.x - perp.x * r * 0.4, back.y - perp.y * r * 0.4);
    ui.painter().circle_stroke(
        c,
        r * 0.95,
        Stroke::new(1.0_f32, color_with_alpha(color, 140)),
    );
    ui.painter()
        .line_segment([back, tip], Stroke::new(1.6_f32, color));
    ui.painter()
        .line_segment([left, tip], Stroke::new(1.4_f32, color));
    ui.painter()
        .line_segment([right, tip], Stroke::new(1.4_f32, color));
}
