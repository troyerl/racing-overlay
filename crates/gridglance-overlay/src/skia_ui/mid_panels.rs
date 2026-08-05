//! Skia painters for Phase B "medium" panels: tire, pit board, weather, ERS,
//! sector timing, lap compare, laptime log.
//!
//! Layout ports the egui widgets in `widgets/{tire_panel,pit_board,weather_panel,
//! ers_hybrid,sector_timing,lap_compare,laptime_log}.rs` (Data presentation) onto the
//! Skia `Canvas`. Local chrome helpers (`panel_card`, `panel_content_pad`, `panel_title`,
//! `panel_pad`, `is_elegant`) mirror `crate::chrome` since the Skia chrome module doesn't
//! carry them yet.

use super::canvas::{Canvas, TextAlign};
use super::chrome::{anim_dt, draw_dark_cell, ease, section_color, still_easing};
use super::types::{FontSpec, Rect, Rgba};
use crate::config::{OverlayConfig, PanelStyle};
use crate::telemetry::{
    signed_delta_1, CompareMarker, LapLogRow, MarkerKind, PitService, SectorCell, TelemetryFrame,
    TireCorner,
};

// ---------------------------------------------------------------------------
// Shared local chrome helpers (mirror `crate::chrome`, Skia-flavored).
// ---------------------------------------------------------------------------

/// Baseline y for text vertically centered at `center_y` (Canvas::text takes a baseline).
fn mid(center_y: f32, size: f32) -> f32 {
    center_y + size * 0.35
}

fn is_elegant(cfg: &OverlayConfig, section: &str) -> bool {
    cfg.panel_style(section) == PanelStyle::Elegant
}

fn panel_pad(h: f32) -> f32 {
    (h * 0.08).max(8.0)
}

fn panel_content_pad(cfg: &OverlayConfig, section: &str, card_h: f32) -> f32 {
    let base = panel_pad(card_h);
    if is_elegant(cfg, section) {
        (base * 0.75).max(6.0)
    } else {
        base
    }
}

fn cell_radius(row_h: f32) -> f32 {
    (row_h * 0.22).clamp(4.0, 8.0)
}

/// Data vs Elegant card fill. Returns `(card_rect, radius)`.
fn panel_card(c: &mut Canvas, cfg: &OverlayConfig, section: &str, bounds: Rect) -> (Rect, f32) {
    let h = bounds.height();
    if is_elegant(cfg, section) {
        let frac = cfg.f64_key(section, "corner_radius_frac", 0.0) as f32;
        let radius = (h * frac.max(0.10)).min(h * 0.22).max(4.0).min(h * 0.5);
        let top = section_color(cfg, section, "bg_top", "#1b1f26f2").with_alpha(108);
        let bottom = section_color(cfg, section, "bg_bottom", "#0f1216f2").with_alpha(88);
        let fill = top.lerp(bottom, 0.55);
        c.fill_rect(bounds, fill, radius);
        (bounds, radius)
    } else {
        let radius = (h * cfg.f64_key(section, "corner_radius_frac", 0.0) as f32).max(8.0);
        let top = section_color(cfg, section, "bg_top", "#1b1f26f2");
        let bottom = section_color(cfg, section, "bg_bottom", "#0f1216f2");
        c.fill_vertical_gradient(bounds, top, bottom, radius);
        (bounds, radius)
    }
}

fn draw_section_header(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    title: &str,
    radius_top: f32,
) {
    let bg = section_color(cfg, section, "header_bg", "#0b0e12bb");
    c.fill_rect(rect, bg, radius_top.max(0.0));
    let edge = section_color(cfg, section, "border", "#ffffff28").with_alpha(70);
    let inset = radius_top.max(0.0);
    c.line(
        rect.left() + inset,
        rect.top() + 0.5,
        rect.right() - inset,
        rect.top() + 0.5,
        edge,
        1.0,
    );
    c.line(
        rect.left() + inset,
        rect.bottom() - 0.5,
        rect.right() - inset,
        rect.bottom() - 0.5,
        edge,
        1.0,
    );
    let size = (rect.height() * 0.55).max(10.0);
    c.text(
        title,
        rect.left() + 10.0,
        mid(rect.center().1, size),
        FontSpec::bold(size),
        section_color(cfg, section, "title", "#f4f6f8"),
        TextAlign::Left,
    );
}

/// Draws optional title (Data = header band; Elegant = whisper label). Returns new y cursor.
fn panel_title(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    card: Rect,
    radius: f32,
    y: f32,
    pad: f32,
    default_title: &str,
) -> f32 {
    if !cfg.bool_key(section, "show_title", true) {
        return y;
    }
    let title = cfg.str_key(section, "title", default_title);
    if is_elegant(cfg, section) {
        let muted = section_color(cfg, section, "muted", "#8b93a1").with_alpha(200);
        let th = 12.0;
        c.text(
            &title,
            card.left() + pad,
            mid(y + th * 0.5, 10.0),
            FontSpec::new(10.0),
            muted,
            TextAlign::Left,
        );
        y + th + 4.0
    } else {
        let hh = (card.height() * 0.12).max(20.0);
        let hdr = Rect::from_xywh(card.left() + pad, y, card.width() - 2.0 * pad, hh);
        draw_section_header(c, cfg, section, hdr, &title, radius);
        y + hh + pad * 0.35
    }
}

fn draw_status_chip(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    text: &str,
    active: bool,
) {
    let r = (rect.height() * 0.35).min(10.0);
    let bg = if active {
        section_color(cfg, section, "active_bg", "#3d8bfd")
    } else {
        section_color(cfg, section, "cell_dark", "#0b0e12")
    };
    let fg = if active {
        section_color(cfg, section, "active_text", "#ffffff")
    } else {
        section_color(cfg, section, "muted", "#8b93a1")
    };
    c.fill_rect(rect, bg, r);
    let fs = (rect.height() * 0.48).clamp(10.0, 22.0);
    c.text(
        text,
        rect.center().0,
        mid(rect.center().1, fs),
        FontSpec::bold(fs),
        fg,
        TextAlign::Center,
    );
}

fn full_bounds(c: &Canvas) -> Rect {
    Rect::from_xywh(0.0, 0.0, c.width() as f32, c.height() as f32)
}

// ---------------------------------------------------------------------------
// Tire panel — 2x2 FL/FR/RL/RR wear + temp.
// ---------------------------------------------------------------------------

const TIRE_SECTION: &str = "tire_panel";
const TIRE_CORNERS: [(&str, usize); 4] = [("FL", 0), ("FR", 1), ("RL", 2), ("RR", 3)];
const TIRE_TEMP_COLD_C: f32 = 60.0;
const TIRE_TEMP_HOT_C: f32 = 105.0;

fn has_tire_data(corners: &[TireCorner; 4]) -> bool {
    corners
        .iter()
        .any(|c| c.wear.is_some() || c.temp.is_some() || c.pressure.is_some())
}

fn tire_temp_wash(cfg: &OverlayConfig, temp_c: f32) -> Rgba {
    let cold = section_color(cfg, TIRE_SECTION, "temp_cold", "#5aa9ff");
    let mid_c = section_color(cfg, TIRE_SECTION, "wear", "#46df7a");
    let hot = section_color(cfg, TIRE_SECTION, "temp_hot", "#ff9416");
    let t = ((temp_c - TIRE_TEMP_COLD_C) / (TIRE_TEMP_HOT_C - TIRE_TEMP_COLD_C)).clamp(0.0, 1.0);
    if t < 0.5 {
        cold.lerp(mid_c, t * 2.0)
    } else {
        mid_c.lerp(hot, (t - 0.5) * 2.0)
    }
}

pub fn paint_tire(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let corners = &frame.tire_corners;
    if !has_tire_data(corners) && !edit_mode {
        return false;
    }
    let bounds = full_bounds(c);
    let (card, radius) = panel_card(c, cfg, TIRE_SECTION, bounds);
    let pad = panel_content_pad(cfg, TIRE_SECTION, card.height());
    let mut y = card.top() + pad;
    y = panel_title(c, cfg, TIRE_SECTION, card, radius, y, pad, "TIRES");

    let iw = card.width() - 2.0 * pad;
    let ih = card.bottom() - pad - y;
    let gap = (iw * 0.04).max(4.0);
    let cw = (iw - gap) / 2.0;
    let ch = (ih - gap) / 2.0;
    let warn = cfg.f64_key(TIRE_SECTION, "warn_wear_pct", 30.0) as f32;
    let header_c = section_color(cfg, TIRE_SECTION, "header", "#c5ccd6");
    let text_c = section_color(cfg, TIRE_SECTION, "text", "#f4f6f8");
    let muted = section_color(cfg, TIRE_SECTION, "muted", "#8b93a1");
    let bar_bg = section_color(cfg, TIRE_SECTION, "bar_bg", "#ffffff18");
    let wear_c = section_color(cfg, TIRE_SECTION, "wear", "#70df7a");
    let warn_c = section_color(cfg, TIRE_SECTION, "warn", "#e23b3b");
    let show_wear = cfg.bool_key(TIRE_SECTION, "show_wear", true);
    let show_temp = cfg.bool_key(TIRE_SECTION, "show_temp", true);
    let show_pressure = cfg.bool_key(TIRE_SECTION, "show_pressure", false);
    let rad = cell_radius(cw.min(ch) * 0.4);

    for (i, (lbl, idx)) in TIRE_CORNERS.iter().enumerate() {
        let col_i = (i % 2) as f32;
        let row_i = (i / 2) as f32;
        let x = card.left() + pad + col_i * (cw + gap);
        let cy = y + row_i * (ch + gap);
        let cell = Rect::from_xywh(x, cy, cw, ch);
        draw_dark_cell(c, cfg, TIRE_SECTION, cell, rad);

        let cdata = &corners[*idx];
        if let Some(temp) = cdata.temp {
            let hotness =
                ((temp - TIRE_TEMP_COLD_C) / (TIRE_TEMP_HOT_C - TIRE_TEMP_COLD_C)).clamp(0.0, 1.0);
            if hotness > 0.55 {
                c.fill_rect(
                    Rect::from_xywh(x, cy, 3.0, ch),
                    tire_temp_wash(cfg, temp).with_alpha(200),
                    0.0,
                );
            }
        }

        let label_fs = (ch * 0.22).clamp(10.0, 16.0);
        c.text(
            lbl,
            x + 8.0,
            mid(cy + ch * 0.11 + 4.0, label_fs),
            FontSpec::bold(label_fs),
            header_c,
            TextAlign::Left,
        );

        let mut ty = cy + ch * 0.28;
        if show_temp {
            let (ts, tcol) = if let Some(temp) = cdata.temp {
                (
                    format!("{:.0}°", cfg.conv_temp(temp)),
                    tire_temp_wash(cfg, temp),
                )
            } else if edit_mode {
                ("—".into(), muted)
            } else {
                ("--".into(), muted)
            };
            let fs = (ch * 0.26).clamp(11.0, 17.0);
            c.text(
                &ts,
                x + 6.0,
                mid(ty + ch * 0.10, fs),
                FontSpec::bold(fs),
                tcol,
                TextAlign::Left,
            );
            ty += ch * 0.28;
        }
        if show_pressure {
            let ps = if let Some(pr) = cdata.pressure {
                format!("{pr:.0} kPa")
            } else if edit_mode {
                "—".into()
            } else {
                "--".into()
            };
            let fs = (ch * 0.18).clamp(9.0, 13.0);
            c.text(
                &ps,
                x + 6.0,
                mid(ty + ch * 0.08, fs),
                FontSpec::new(fs),
                muted,
                TextAlign::Left,
            );
        }
        if show_wear {
            let bar = Rect::from_xywh(x + 8.0, cy + ch - ch * 0.26, cw - 16.0, ch * 0.16);
            c.fill_rect(bar, bar_bg, 3.0);
            let val = if let Some(wear) = cdata.wear {
                let pct = (wear * 100.0).clamp(0.0, 100.0);
                let fill_w = bar.width() * pct / 100.0;
                let fcol = if pct <= warn { warn_c } else { wear_c };
                c.fill_rect(
                    Rect::from_xywh(bar.x, bar.y, fill_w, bar.height()),
                    fcol,
                    3.0,
                );
                format!("{pct:.0}%")
            } else if edit_mode {
                "—".into()
            } else {
                "--".into()
            };
            let fs = (bar.height() * 0.85).clamp(9.0, 14.0);
            c.text(
                &val,
                bar.center().0,
                mid(bar.center().1, fs),
                FontSpec::bold(fs),
                text_c,
                TextAlign::Center,
            );
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Pit board — checklist rows with check marks.
// ---------------------------------------------------------------------------

const PIT_SECTION: &str = "pit_board";

fn preview_pit_services() -> Vec<PitService> {
    vec![
        PitService {
            key: "lf_tire".into(),
            label: "LF tire".into(),
            checked: true,
        },
        PitService {
            key: "fuel".into(),
            label: "Fuel".into(),
            checked: true,
        },
        PitService {
            key: "rf_tire".into(),
            label: "RF tire".into(),
            checked: false,
        },
    ]
}

pub fn paint_pit_board(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let mut services = frame.pit_services.clone();
    if services.is_empty() && edit_mode {
        services = preview_pit_services();
    }
    if services.is_empty() && !edit_mode && !frame.pit_active {
        return false;
    }
    let bounds = full_bounds(c);
    let (card, radius) = panel_card(c, cfg, PIT_SECTION, bounds);
    let pad = panel_content_pad(cfg, PIT_SECTION, card.height());
    let h = card.height();
    let mut y = card.top() + pad;

    if frame.pit_active && cfg.bool_key(PIT_SECTION, "show_pit_banner", true) {
        let banner_h = (h * 0.12).max(22.0);
        let banner = Rect::from_xywh(card.left() + pad, y, card.width() - 2.0 * pad, banner_h);
        let text = cfg.str_key(PIT_SECTION, "pit_banner_text", "PIT STOP ACTIVE");
        draw_status_chip(c, cfg, PIT_SECTION, banner, &text, true);
        y += banner_h + pad * 0.4;
    }

    y = panel_title(c, cfg, PIT_SECTION, card, radius, y, pad, "PIT SERVICES");

    let n = services.len().max(1);
    let extras_h = h * 0.14;
    let body_h = (card.bottom() - pad - y - extras_h).max(n as f32 * 18.0);
    let fixed_rh = cfg.f64_key(PIT_SECTION, "row_height_px", 0.0) as f32;
    let mut row_h = if fixed_rh > 0.0 {
        fixed_rh
    } else {
        let max_frac = cfg.f64_key(PIT_SECTION, "max_row_height_frac", 0.0) as f32;
        let mut rh = body_h / n as f32;
        if max_frac > 0.0 {
            rh = rh.min(h * max_frac);
        }
        rh
    };
    row_h = row_h.max(18.0);
    let rad = cell_radius(row_h);
    let mark_w = (row_h * 0.45).max(18.0);
    let checked_c = section_color(cfg, PIT_SECTION, "checked", "#70df7a");
    let muted = section_color(cfg, PIT_SECTION, "muted", "#8b93a1");
    let text_c = section_color(cfg, PIT_SECTION, "text", "#f4f6f8");
    let row_dividers = cfg.bool_key(PIT_SECTION, "row_dividers", true);
    let border = section_color(cfg, PIT_SECTION, "border", "#ffffff28");

    for (i, svc) in services.iter().enumerate() {
        let row = Rect::from_xywh(card.left() + pad, y, card.width() - 2.0 * pad, row_h - 2.0);
        draw_dark_cell(c, cfg, PIT_SECTION, row, rad);
        let mark = if svc.checked { "✓" } else { "–" };
        let mark_fs = (row_h * 0.42).clamp(11.0, 20.0);
        c.text(
            mark,
            row.left() + 8.0 + mark_w * 0.5,
            mid(row.center().1, mark_fs),
            FontSpec::bold(mark_fs),
            if svc.checked { checked_c } else { muted },
            TextAlign::Center,
        );
        let label_fs = (row_h * 0.42).clamp(11.0, 18.0);
        c.text(
            &svc.label,
            row.left() + mark_w + 4.0,
            mid(row.center().1, label_fs),
            FontSpec::new(label_fs),
            if svc.checked { text_c } else { muted },
            TextAlign::Left,
        );
        y += row_h;
        if row_dividers && i + 1 < services.len() {
            let edge = border.with_alpha(((border.a as f32) * 0.55).max(30.0) as u8);
            c.line(
                card.left() + pad,
                y - 2.0,
                card.left() + pad + card.width() - 2.0 * pad,
                y - 2.0,
                edge,
                1.0,
            );
        }
    }

    let mut extras: Vec<String> = Vec::new();
    if cfg.bool_key(PIT_SECTION, "show_compound", true) {
        if let Some(cmp) = frame.pit_compound {
            extras.push(format!("Set {cmp}"));
        }
    }
    if let Some(fuel_l) = frame.pit_fuel_add_l.or(frame.pit_fuel_to_add) {
        let v = cfg.conv_fuel(fuel_l);
        extras.push(format!("+{v:.1} {}", cfg.fuel_unit()));
    }
    if cfg.bool_key(PIT_SECTION, "show_fast_repairs", true) {
        if let Some(r) = frame.pit_repairs {
            extras.push(format!("Repairs {r}"));
        }
    }
    if !extras.is_empty() {
        let fs = (row_h * 0.38).clamp(10.0, 14.0);
        c.text(
            &extras.join("  •  "),
            card.left() + pad,
            mid(y + extras_h * 0.5, fs),
            FontSpec::new(fs),
            muted,
            TextAlign::Left,
        );
    }
    false
}

// ---------------------------------------------------------------------------
// Weather — sky/wet bars/temp/wind.
// ---------------------------------------------------------------------------

const WEATHER_SECTION: &str = "weather_panel";

enum WeatherRow {
    Text(String, String),
    Wet(Option<f32>, Option<f32>),
    Wind(Option<f32>, Option<f32>),
}

fn wind_dir_degrees(dir: f32) -> f32 {
    if dir.abs() <= std::f32::consts::TAU + 0.25 {
        dir.to_degrees().rem_euclid(360.0)
    } else {
        dir.rem_euclid(360.0)
    }
}

fn paint_wind_tick(c: &mut Canvas, cx: f32, cy: f32, r: f32, dir_rad: f32, color: Rgba) {
    let ang = if dir_rad.abs() <= std::f32::consts::TAU + 0.25 {
        dir_rad
    } else {
        dir_rad.to_radians()
    };
    let screen = ang - std::f32::consts::FRAC_PI_2;
    let tip = (cx + screen.cos() * r, cy + screen.sin() * r);
    let back = (cx - screen.cos() * r * 0.55, cy - screen.sin() * r * 0.55);
    let perp = (-screen.sin(), screen.cos());
    let left = (back.0 + perp.0 * r * 0.4, back.1 + perp.1 * r * 0.4);
    let right = (back.0 - perp.0 * r * 0.4, back.1 - perp.1 * r * 0.4);
    c.circle_ex(cx, cy, r * 0.95, color.with_alpha(140), false, 1.0);
    c.line(back.0, back.1, tip.0, tip.1, color, 1.6);
    c.line(left.0, left.1, tip.0, tip.1, color, 1.4);
    c.line(right.0, right.1, tip.0, tip.1, color, 1.4);
}

fn paint_wet_bar(
    c: &mut Canvas,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    pct: Option<f32>,
    tag: &str,
    fill: Rgba,
    bg: Rgba,
    text: Rgba,
    muted: Rgba,
) {
    let tag_w = (h * 1.6).max(10.0);
    c.text(
        tag,
        x,
        mid(y + h * 0.5, h * 0.95),
        FontSpec::bold(h * 0.95),
        muted,
        TextAlign::Left,
    );
    let bar = Rect::from_xywh(x + tag_w, y, (w - tag_w - 34.0).max(20.0), h);
    c.fill_rect(bar, bg, 3.0);
    if let Some(p) = pct {
        let frac = (p / 100.0).clamp(0.0, 1.0);
        let fill_w = bar.width() * frac;
        if fill_w > 0.5 {
            c.fill_rect(
                Rect::from_xywh(bar.x, bar.y, fill_w, bar.height()),
                fill,
                3.0,
            );
        }
        c.text(
            &format!("{p:.0}%"),
            bar.right() + 4.0,
            mid(y + h * 0.5, h * 0.95),
            FontSpec::new(h * 0.95),
            text,
            TextAlign::Left,
        );
    } else {
        c.text(
            "—",
            bar.right() + 4.0,
            mid(y + h * 0.5, h * 0.95),
            FontSpec::new(h * 0.95),
            muted,
            TextAlign::Left,
        );
    }
}

pub fn paint_weather(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    let _ = edit_mode;
    c.clear_transparent();
    let bounds = full_bounds(c);
    let (card, radius) = panel_card(c, cfg, WEATHER_SECTION, bounds);
    let pad = panel_pad(card.height());
    let mut y = card.top() + pad;
    if cfg.bool_key(WEATHER_SECTION, "show_title", true) {
        let hh = (card.height() * 0.12).max(22.0);
        let hdr = Rect::from_xywh(card.left() + pad, y, card.width() - 2.0 * pad, hh);
        draw_section_header(
            c,
            cfg,
            WEATHER_SECTION,
            hdr,
            &cfg.str_key(WEATHER_SECTION, "title", "WEATHER"),
            radius,
        );
        y += hh + pad * 0.35;
    }

    let mut rows: Vec<WeatherRow> = Vec::new();
    if cfg.bool_key(WEATHER_SECTION, "show_skies", true) {
        let mut extra = Vec::new();
        if let Some(h) = frame.humidity {
            extra.push(format!("{h:.0}% RH"));
        }
        if let Some(fog) = frame.fog {
            if fog > 0.0 {
                extra.push(format!("Fog {fog:.0}%"));
            }
        }
        rows.push(WeatherRow::Text(
            "SKY".into(),
            format!(
                "{}  {}",
                frame.skies.as_deref().unwrap_or("—"),
                extra.join("  ")
            )
            .trim()
            .to_string(),
        ));
    }
    if cfg.bool_key(WEATHER_SECTION, "show_rain", true) {
        rows.push(WeatherRow::Wet(frame.track_wetness, frame.rain_intensity));
    }
    if cfg.bool_key(WEATHER_SECTION, "show_temps", true) {
        let mut ts = Vec::new();
        if let Some(t) = frame.track_temp {
            ts.push(format!("T {t:.0}°"));
        }
        if let Some(a) = frame.air_temp {
            ts.push(format!("A {a:.0}°"));
        }
        rows.push(WeatherRow::Text(
            "TEMP".into(),
            if ts.is_empty() {
                "—".into()
            } else {
                ts.join("  ")
            },
        ));
    }
    if cfg.bool_key(WEATHER_SECTION, "show_wind", true) {
        rows.push(WeatherRow::Wind(frame.wind_dir, frame.wind_vel));
    }

    let n = rows.len().max(1) as f32;
    let avail = card.bottom() - pad - y;
    let rh = avail / n;
    let text = section_color(cfg, WEATHER_SECTION, "text", "#f4f6f8");
    let muted = section_color(cfg, WEATHER_SECTION, "muted", "#8b93a1");
    let accent = section_color(cfg, WEATHER_SECTION, "accent", "#5aa9ff");
    let bar_bg = section_color(cfg, WEATHER_SECTION, "bar_bg", "#ffffff18");

    for row in rows {
        let r = Rect::from_xywh(card.left() + pad, y, card.width() - 2.0 * pad, rh);
        match row {
            WeatherRow::Text(key, value) => {
                c.text(
                    &key,
                    r.left() + 6.0,
                    mid(r.center().1, rh * 0.32),
                    FontSpec::bold(rh * 0.32),
                    muted,
                    TextAlign::Left,
                );
                c.text(
                    &value,
                    r.right() - 6.0,
                    mid(r.center().1, rh * 0.36),
                    FontSpec::new(rh * 0.36),
                    text,
                    TextAlign::Right,
                );
            }
            WeatherRow::Wet(track, rain) => {
                let wet_max = track.unwrap_or(0.0).max(rain.unwrap_or(0.0));
                if wet_max > 35.0 {
                    c.fill_rect(r, accent.with_alpha(28), 4.0);
                }
                c.text(
                    "WET",
                    r.left() + 6.0,
                    mid(r.center().1, rh * 0.32),
                    FontSpec::bold(rh * 0.32),
                    muted,
                    TextAlign::Left,
                );
                let label_w = (r.width() * 0.18).max(36.0);
                let bars_left = r.left() + label_w;
                let bars_w = (r.right() - 6.0 - bars_left).max(40.0);
                let bar_h = (rh * 0.22).clamp(4.0, 10.0);
                let gap = (rh * 0.08).max(2.0);
                let mid_y = r.center().1;
                paint_wet_bar(
                    c,
                    bars_left,
                    mid_y - bar_h - gap * 0.5,
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
                    c,
                    bars_left,
                    mid_y + gap * 0.5,
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
            WeatherRow::Wind(dir, vel) => {
                c.text(
                    "WIND",
                    r.left() + 6.0,
                    mid(r.center().1, rh * 0.32),
                    FontSpec::bold(rh * 0.32),
                    muted,
                    TextAlign::Left,
                );
                let wind_txt = match (dir, vel) {
                    (Some(d), Some(v)) => format!("{:.0}° @ {v:.1} m/s", wind_dir_degrees(d)),
                    _ => "—".into(),
                };
                let tick_r = (rh * 0.22).clamp(6.0, 12.0);
                let tick_cx = r.right() - 6.0 - tick_r;
                let tick_cy = r.center().1;
                if let Some(d) = dir {
                    paint_wind_tick(c, tick_cx, tick_cy, tick_r, d, accent);
                }
                c.text(
                    &wind_txt,
                    tick_cx - tick_r - 4.0,
                    mid(r.center().1, rh * 0.36),
                    FontSpec::new(rh * 0.36),
                    text,
                    TextAlign::Right,
                );
            }
        }
        y += rh;
    }
    false
}

// ---------------------------------------------------------------------------
// ERS / hybrid — battery bar + BOOST/P2P chips.
// ---------------------------------------------------------------------------

const ERS_SECTION: &str = "ers_hybrid";

/// Eased battery-fill state, owned by the caller across frames.
#[derive(Default)]
pub struct ErsAnim {
    fill: f32,
    last_secs: f64,
}

fn draw_metric_row(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    lab: &str,
    value: &str,
) {
    let lh = rect.height();
    let lw = rect.width();
    let lab_fs = (lh * 0.38).clamp(10.0, 16.0);
    c.text(
        lab,
        rect.left(),
        mid(rect.center().1, lab_fs),
        FontSpec::bold(lab_fs),
        section_color(cfg, section, "header", "#c5ccd6"),
        TextAlign::Left,
    );
    let val_fs = (lh * 0.42).clamp(11.0, 18.0);
    c.text(
        value,
        rect.left() + lw * 0.22,
        mid(rect.center().1, val_fs),
        FontSpec::bold(val_fs),
        section_color(cfg, section, "text", "#f4f6f8"),
        TextAlign::Left,
    );
}

pub fn paint_ers(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    anim: &mut ErsAnim,
    mono_secs: f64,
    edit_mode: bool,
) -> bool {
    c.clear_transparent();
    let bounds = full_bounds(c);
    let (card, radius) = panel_card(c, cfg, ERS_SECTION, bounds);
    let pad = panel_content_pad(cfg, ERS_SECTION, card.height());
    let h = card.height();

    if !frame.have_hybrid && !edit_mode {
        let empty = cfg.str_key(ERS_SECTION, "empty_text", "No hybrid data");
        let fs = (h * 0.22).clamp(12.0, 20.0);
        c.text(
            &empty,
            card.center().0,
            mid(card.center().1, fs),
            FontSpec::new(fs),
            section_color(cfg, ERS_SECTION, "muted", "#8b93a1"),
            TextAlign::Center,
        );
        return false;
    }

    let mut y = card.top() + pad;
    y = panel_title(c, cfg, ERS_SECTION, card, radius, y, pad, "HYBRID");

    let mut animating = false;
    if cfg.bool_key(ERS_SECTION, "show_battery", true) {
        let mut pct = if frame.have_hybrid {
            frame.ers_battery_pct.or(frame.ers_pct)
        } else {
            None
        };
        if edit_mode && pct.is_none() {
            pct = Some(62.0);
        }
        let bar_h = (h * 0.28).max(28.0);
        let bar = Rect::from_xywh(card.left() + pad, y, card.width() - 2.0 * pad, bar_h);
        draw_dark_cell(c, cfg, ERS_SECTION, bar, cell_radius(bar_h));
        let inner = bar.inset(6.0, 6.0);
        c.fill_rect(
            inner,
            section_color(cfg, ERS_SECTION, "gauge_bg", "#ffffff18"),
            4.0,
        );
        if let Some(p) = pct {
            let target = (p / 100.0).clamp(0.0, 1.0);
            let dt = anim_dt(mono_secs, &mut anim.last_secs);
            anim.fill = ease(anim.fill, target, dt, 0.14);
            if still_easing(anim.fill, target, 0.005) {
                animating = true;
            }
            let fw = inner.width() * anim.fill;
            c.fill_rect(
                Rect::from_xywh(inner.x, inner.y, fw, inner.height()),
                section_color(cfg, ERS_SECTION, "gauge_fill", "#70df7a"),
                4.0,
            );
        }
        let lbl = if let Some(p) = pct {
            format!("{p:.0}%")
        } else if frame.have_hybrid {
            "—".into()
        } else {
            "--".into()
        };
        let metric = bar.inset(8.0, 0.0);
        let bat_lab = cfg.str_key(ERS_SECTION, "label_battery", "ERS");
        draw_metric_row(c, cfg, ERS_SECTION, metric, &bat_lab, &lbl);
        y += bar_h + pad * 0.4;
    }

    let chip_h = (h * 0.12).max(18.0);
    let chip_w = (card.width() - 2.0 * pad - pad * 0.5) / 2.0;
    let mut x = card.left() + pad;
    if cfg.bool_key(ERS_SECTION, "show_boost", true) {
        let lab = cfg.str_key(ERS_SECTION, "label_boost", "BOOST");
        draw_status_chip(
            c,
            cfg,
            ERS_SECTION,
            Rect::from_xywh(x, y, chip_w, chip_h),
            &lab,
            frame.ers_boost_active,
        );
        x += chip_w + pad * 0.5;
    }
    if cfg.bool_key(ERS_SECTION, "show_p2p", true) {
        let lab = cfg.str_key(ERS_SECTION, "label_p2p", "P2P");
        draw_status_chip(
            c,
            cfg,
            ERS_SECTION,
            Rect::from_xywh(x, y, chip_w, chip_h),
            &lab,
            frame.ers_p2p_active,
        );
    }

    animating
}

// ---------------------------------------------------------------------------
// Sector / lap timing — current clock + last/best + sector cells.
// ---------------------------------------------------------------------------

const SECTOR_SECTION: &str = "sector_timing";

fn fmt_clock(sec: Option<f64>) -> String {
    match sec {
        Some(s) if s > 0.0 => {
            let m = (s / 60.0).floor() as i32;
            let rem = s - m as f64 * 60.0;
            format!("{m}:{rem:06.3}")
        }
        _ => "--:--.---".into(),
    }
}

fn fmt_sec(sec: Option<f64>) -> String {
    match sec {
        Some(s) if s > 0.0 => format!("{s:.1}"),
        _ => "--.-".into(),
    }
}

fn signed_delta_2(d: f64) -> String {
    format!("{d:+.2}")
}

fn draw_sector_metric_pair(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    lab: &str,
    value: &str,
) {
    let lab_fs = (rect.height() * 0.38).clamp(9.0, 14.0);
    c.text(
        lab,
        rect.left() + 10.0,
        mid(rect.center().1, lab_fs),
        FontSpec::bold(lab_fs),
        section_color(cfg, SECTOR_SECTION, "muted", "#8b93a1"),
        TextAlign::Left,
    );
    let val_fs = (rect.height() * 0.46).clamp(11.0, 18.0);
    c.text(
        value,
        rect.right() - 10.0,
        mid(rect.center().1, val_fs),
        FontSpec::bold(val_fs),
        section_color(cfg, SECTOR_SECTION, "text", "#f4f6f8"),
        TextAlign::Right,
    );
}

fn paint_sector_cell(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    num: usize,
    cell: &SectorCell,
    show_delta: bool,
) {
    let inner = rect.inset(1.0, 1.0);
    let rad = (rect.height() * 0.18).clamp(4.0, 10.0);
    let status = cell.status.as_str();
    let bg_key = match status {
        "best" => ("sec_best", "#6b39c8"),
        "running" => ("sec_running", "#1d3a2a"),
        "done" => ("sec_done", "#22303f"),
        _ => ("sec_idle", "#161a20"),
    };
    if status == "idle" || status.is_empty() {
        draw_dark_cell(c, cfg, SECTOR_SECTION, inner, rad);
    } else {
        c.fill_rect(
            inner,
            section_color(cfg, SECTOR_SECTION, bg_key.0, bg_key.1),
            rad,
        );
        c.stroke_rect(
            inner,
            section_color(cfg, SECTOR_SECTION, "cell_border", "#ffffff28"),
            rad,
            1.0,
        );
        if cell.active {
            c.stroke_rect(
                inner,
                section_color(cfg, SECTOR_SECTION, "sec_running_edge", "#46df7a"),
                rad,
                1.6,
            );
        }
    }
    let sec_text = section_color(cfg, SECTOR_SECTION, "sec_text", "#dfe3ea");
    let num_fs = (rect.height() * 0.26).clamp(9.0, 16.0);
    c.text(
        &format!("S{num}"),
        rect.center().0,
        mid(rect.top() + rect.height() * 0.28, num_fs),
        FontSpec::new(num_fs),
        sec_text,
        TextAlign::Center,
    );
    let time_fs = (rect.height() * 0.34).clamp(11.0, 20.0);
    c.text(
        &fmt_sec(cell.time),
        rect.center().0,
        mid(rect.center().1 + rect.height() * 0.12, time_fs),
        FontSpec::bold(time_fs),
        sec_text,
        TextAlign::Center,
    );
    if show_delta {
        if let Some(d) = cell.delta.filter(|d| d.abs() >= 0.005) {
            let dc = if d > 0.0 {
                section_color(cfg, SECTOR_SECTION, "slower", "#e23b3b")
            } else {
                section_color(cfg, SECTOR_SECTION, "faster", "#46df7a")
            };
            let fs = (rect.height() * 0.22).clamp(8.0, 14.0);
            c.text(
                &signed_delta_2(d),
                rect.center().0,
                mid(rect.bottom() - rect.height() * 0.16, fs),
                FontSpec::new(fs),
                dc,
                TextAlign::Center,
            );
        }
    }
}

pub fn paint_sector_timing(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    let _ = edit_mode;
    c.clear_transparent();
    let bounds = full_bounds(c);
    let (card, _radius) = panel_card(c, cfg, SECTOR_SECTION, bounds);
    let pad = panel_pad(card.height());
    let h = card.height();
    let snap = &frame.sectors_ui;
    let show_pred = cfg.bool_key(SECTOR_SECTION, "show_predicted_lap", false);
    let show_delta = cfg.bool_key(SECTOR_SECTION, "show_sector_delta", false);
    let iw = card.width() - 2.0 * pad;
    let text = section_color(cfg, SECTOR_SECTION, "text", "#f4f6f8");
    let muted = section_color(cfg, SECTOR_SECTION, "muted", "#8b93a1");

    let cur_h = if show_pred { h * 0.26 } else { h * 0.30 };
    let clock_fs = (cur_h * 0.72).clamp(14.0, 32.0);
    c.text(
        &fmt_clock(snap.cur_lap),
        card.center().0,
        mid(card.top() + pad + cur_h * 0.5, clock_fs),
        FontSpec::bold(clock_fs),
        text,
        TextAlign::Center,
    );

    if show_pred {
        if let Some(pred) = snap.predicted_lap.filter(|t| *t > 0.0) {
            let fs = (h * 0.07).clamp(9.0, 13.0);
            c.text(
                &format!("Pred {}", fmt_clock(Some(pred))),
                card.center().0,
                mid(card.top() + pad + cur_h * 0.95, fs),
                FontSpec::new(fs),
                muted.with_alpha(200),
                TextAlign::Center,
            );
        }
    }

    let sub_top = card.top() + pad + if show_pred { h * 0.34 } else { h * 0.30 };
    let fixed_rh = cfg.f64_key(SECTOR_SECTION, "row_height_px", 0.0) as f32;
    let mut sub_h = if fixed_rh > 0.0 { fixed_rh } else { h * 0.18 };
    let max_frac = cfg.f64_key(SECTOR_SECTION, "max_row_height_frac", 0.0) as f32;
    if max_frac > 0.0 {
        sub_h = sub_h.min(h * max_frac);
    }
    sub_h = sub_h.max(16.0);
    let sub = Rect::from_xywh(card.left() + pad, sub_top, iw, sub_h);

    c.fill_rect(
        sub,
        section_color(cfg, SECTOR_SECTION, "header_bg", "#0b0e12bb"),
        0.0,
    );
    let border = section_color(cfg, SECTOR_SECTION, "border", "#ffffff28");
    c.line(
        sub.left(),
        sub.bottom(),
        sub.right(),
        sub.bottom(),
        border,
        1.0,
    );

    let half = sub.width() * 0.5;
    draw_sector_metric_pair(
        c,
        cfg,
        Rect::from_xywh(sub.left(), sub.top(), half, sub.height()),
        "LAST",
        &fmt_clock(snap.last_lap),
    );
    draw_sector_metric_pair(
        c,
        cfg,
        Rect::from_xywh(sub.left() + half, sub.top(), half, sub.height()),
        "BEST",
        &fmt_clock(snap.best_lap),
    );

    let sectors = &snap.sectors;
    if sectors.is_empty() {
        return false;
    }
    let top = sub.bottom() + h * 0.04;
    let avail = (card.bottom() - pad - top).max(18.0);
    let ch_ = avail.max(24.0);
    let gap = iw * 0.03;
    let n = sectors.len() as f32;
    let cw = (iw - gap * (n - 1.0).max(0.0)) / n.max(1.0);
    let mut x = card.left() + pad;
    for (i, cell) in sectors.iter().enumerate() {
        let cell_rect = Rect::from_xywh(x, top, cw, ch_);
        paint_sector_cell(c, cfg, cell_rect, i + 1, cell, show_delta);
        x += cw + gap;
    }
    false
}

// ---------------------------------------------------------------------------
// Lap compare — delta header + sparkline + turn losses.
// ---------------------------------------------------------------------------

const LAPCMP_SECTION: &str = "lap_compare";

fn signed_delta_lap(d: f64) -> String {
    format!("{d:+.2}")
}

fn lapcmp_delta_color(cfg: &OverlayConfig, d: Option<f64>) -> Rgba {
    match d {
        Some(v) if v < -0.005 => section_color(cfg, LAPCMP_SECTION, "faster", "#46df7a"),
        Some(v) if v > 0.005 => section_color(cfg, LAPCMP_SECTION, "slower", "#e23b3b"),
        _ => section_color(cfg, LAPCMP_SECTION, "muted", "#8b93a1"),
    }
}

fn draw_spark(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    rect: Rect,
    spark: &[f32],
    markers: &[CompareMarker],
) {
    draw_dark_cell(c, cfg, LAPCMP_SECTION, rect, 5.0);
    let mid_y = rect.center().1;
    c.line(
        rect.left(),
        mid_y,
        rect.right(),
        mid_y,
        section_color(cfg, LAPCMP_SECTION, "grid", "#ffffff1f"),
        1.0,
    );
    if spark.is_empty() {
        return;
    }
    let peak = spark
        .iter()
        .map(|v| v.abs())
        .fold(0.15_f32, f32::max)
        .max(0.15);
    let n = spark.len().max(1) as f32;
    let mut pts = Vec::with_capacity(spark.len());
    for (i, &v) in spark.iter().enumerate() {
        let x = rect.left() + (i as f32 / (n - 1.0).max(1.0)) * rect.width();
        let y = mid_y - (v / peak) * (rect.height() * 0.5 - 2.0);
        pts.push((x, y));
    }
    c.polyline(
        &pts,
        section_color(cfg, LAPCMP_SECTION, "graph_line", "#ffd23a"),
        1.8,
        false,
    );
    let show_brake = cfg.bool_key(LAPCMP_SECTION, "show_brake_markers", true);
    let show_lift = cfg.bool_key(LAPCMP_SECTION, "show_lift_markers", true);
    let brake_col = section_color(cfg, LAPCMP_SECTION, "marker_brake", "#ff5050");
    let lift_col = section_color(cfg, LAPCMP_SECTION, "marker_lift", "#3aa0ff");
    for m in markers {
        let col = match m.kind {
            MarkerKind::Brake if show_brake => brake_col,
            MarkerKind::Lift if show_lift => lift_col,
            _ => continue,
        };
        let x = rect.left() + m.pct.clamp(0.0, 1.0) * rect.width();
        c.line(x, rect.top() + 2.0, x, rect.bottom() - 2.0, col, 1.2);
    }
}

pub fn paint_lap_compare(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    let _ = edit_mode;
    c.clear_transparent();
    let bounds = full_bounds(c);
    let (card, radius) = panel_card(c, cfg, LAPCMP_SECTION, bounds);
    let pad = panel_pad(card.height()).max(7.0);
    let h = card.height();
    let iw = card.width() - 2.0 * pad;
    let view = &frame.lap_compare;
    let mut y = card.top() + pad * 0.35;

    let ref_label = if view.ref_label.is_empty() {
        "VS BEST"
    } else {
        view.ref_label.as_str()
    };
    let delta = view.delta;

    let hh = h * 0.12;
    let band = Rect::from_xywh(card.left(), y, card.width(), hh);
    c.fill_rect(
        band,
        section_color(cfg, LAPCMP_SECTION, "header_bg", "#0b0e12bb"),
        radius,
    );
    let ref_fs = (hh * 0.55).clamp(11.0, 18.0);
    c.text(
        ref_label,
        card.left() + pad,
        mid(y + hh * 0.5, ref_fs),
        FontSpec::bold(ref_fs),
        section_color(cfg, LAPCMP_SECTION, "accent", "#e23b3b"),
        TextAlign::Left,
    );
    y += hh;

    let bh = h * 0.20;
    let delta_str = delta
        .map(signed_delta_lap)
        .unwrap_or_else(|| "--.--".into());
    let delta_fs = (bh * 0.72).clamp(18.0, 40.0);
    c.text(
        &delta_str,
        card.center().0,
        mid(y + bh * 0.45, delta_fs),
        FontSpec::bold(delta_fs),
        lapcmp_delta_color(cfg, delta),
        TextAlign::Center,
    );
    y += bh;

    if cfg.bool_key(LAPCMP_SECTION, "show_graph", true) && !view.spark.is_empty() {
        let gh = h * 0.16;
        let graph = Rect::from_xywh(card.left() + pad, y, iw, gh);
        draw_spark(c, cfg, graph, &view.spark, &view.markers);
        y += gh + pad * 0.4;
    }

    let turns = &view.turns;
    if turns.is_empty() {
        let fs = (h * 0.06).clamp(11.0, 16.0);
        c.text(
            "Drive a clean lap to set your benchmark",
            card.center().0,
            mid((y + card.bottom() - pad) * 0.5, fs),
            FontSpec::new(fs),
            section_color(cfg, LAPCMP_SECTION, "muted", "#8b93a1"),
            TextAlign::Center,
        );
        return false;
    }

    let body_h = (card.bottom() - pad - y).max(18.0);
    let max_turns = cfg.f64_key(LAPCMP_SECTION, "max_turns", 6.0).max(1.0) as usize;
    let shown: Vec<_> = turns.iter().take(max_turns).collect();
    let max_rh = (body_h * 0.35).max(18.0);
    let rh = (body_h / shown.len().max(1) as f32).clamp(18.0, max_rh);
    let mut row_y = y;
    let muted = section_color(cfg, LAPCMP_SECTION, "muted", "#8b93a1");
    let text = section_color(cfg, LAPCMP_SECTION, "text", "#f4f6f8");
    let alt_shading = cfg.bool_key(LAPCMP_SECTION, "alt_row_shading", true);
    let row_alt = section_color(cfg, LAPCMP_SECTION, "row_alt", "#ffffff0a");

    for (i, (name, loss)) in shown.iter().enumerate() {
        let row = Rect::from_xywh(card.left() + pad, row_y, iw, rh);
        if alt_shading && i % 2 == 1 {
            c.fill_rect(row, row_alt, 0.0);
        }
        let chip_w = row.width() * 0.18;
        let chip = Rect::from_xywh(row.left(), row.top() + rh * 0.12, chip_w, rh * 0.76);
        draw_dark_cell(c, cfg, LAPCMP_SECTION, chip, 5.0);
        let name_fs = (rh * 0.32).clamp(10.0, 16.0);
        c.text(
            name,
            chip.center().0,
            mid(chip.center().1, name_fs),
            FontSpec::bold(name_fs),
            text,
            TextAlign::Center,
        );
        let d = *loss as f64;
        let d_fs = (rh * 0.34).clamp(11.0, 18.0);
        c.text(
            &signed_delta_lap(d),
            chip.right() + row.width() * 0.04,
            mid(row.center().1, d_fs),
            FontSpec::bold(d_fs),
            lapcmp_delta_color(cfg, Some(d)),
            TextAlign::Left,
        );
        let tip = if d.abs() < 0.02 {
            "on pace"
        } else if d > 0.0 {
            "time lost"
        } else {
            "time gained"
        };
        let tip_fs = (rh * 0.26).clamp(9.0, 14.0);
        c.text(
            tip,
            row.right() - 4.0,
            mid(row.center().1, tip_fs),
            FontSpec::new(tip_fs),
            muted,
            TextAlign::Right,
        );
        row_y += rh;
    }
    false
}

// ---------------------------------------------------------------------------
// Laptime log — header + rows from column_order.
// ---------------------------------------------------------------------------

const LAPLOG_SECTION: &str = "laptime_log";
const LAPLOG_DEFAULT_COLS: &[&str] = &["lap", "time", "delta", "temp"];
const LAPLOG_HEADERS: &[(&str, &str)] = &[
    ("lap", "LAP"),
    ("time", "TIME"),
    ("delta", "DELTA"),
    ("temp", "TEMP."),
    ("sectors", "SECT"),
    ("fuel", "FUEL"),
    ("tires", "TIRE"),
    ("incidents", "INC"),
    ("tag", "TAG"),
];

fn laplog_col_header(key: &str) -> &str {
    LAPLOG_HEADERS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, h)| *h)
        .unwrap_or(key)
}

fn laplog_col_weight(key: &str) -> f32 {
    match key {
        "lap" => 0.10,
        "time" => 0.22,
        "delta" => 0.14,
        "temp" => 0.14,
        "sectors" => 0.18,
        "fuel" => 0.10,
        "tires" | "incidents" | "tag" => 0.08,
        _ => 0.12,
    }
}

fn laplog_column_order(cfg: &OverlayConfig) -> Vec<String> {
    if let Some(arr) = cfg
        .section(LAPLOG_SECTION)
        .get("column_order")
        .and_then(|v| v.as_array())
    {
        let cols: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        if !cols.is_empty() {
            return cols;
        }
    }
    LAPLOG_DEFAULT_COLS
        .iter()
        .map(|s| (*s).to_string())
        .collect()
}

fn laplog_col_layout(order: &[String]) -> Vec<(String, f32)> {
    let weights: Vec<f32> = order.iter().map(|k| laplog_col_weight(k)).collect();
    let total: f32 = weights.iter().sum::<f32>().max(1e-6);
    order
        .iter()
        .zip(weights)
        .map(|(k, w)| (k.clone(), w / total))
        .collect()
}

fn laplog_cell_value(row: &LapLogRow, key: &str) -> String {
    match key {
        "lap" => row.lap.to_string(),
        "time" => row.time.clone(),
        "delta" => row.delta.clone(),
        "temp" => {
            if row.temp.is_empty() {
                "—".into()
            } else {
                row.temp.clone()
            }
        }
        "fuel" => row.fuel.clone().unwrap_or_else(|| "—".into()),
        "tires" => row.tires.clone().unwrap_or_else(|| "—".into()),
        "incidents" => row.incidents.clone().unwrap_or_else(|| "—".into()),
        "tag" => row.tag.clone().unwrap_or_default(),
        "sectors" => "—".into(),
        _ => "—".into(),
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_laplog_row(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    row: &LapLogRow,
    cells: &[(String, f32, f32)],
    y: f32,
    row_h: f32,
    data_size: f32,
    text: Rgba,
    muted: Rgba,
    faster: Rgba,
    slower: Rgba,
) {
    for (key, x, cw) in cells {
        let rect = Rect::from_xywh(*x, y, *cw, row_h);
        match key.as_str() {
            "delta" => match row.delta_seconds() {
                None => {
                    c.text(
                        "—",
                        rect.center().0,
                        mid(rect.center().1, data_size),
                        FontSpec::new(data_size),
                        muted,
                        TextAlign::Center,
                    );
                }
                Some(d) => {
                    let col = if d < 0.0 { faster } else { slower };
                    let txt = if row.delta.is_empty() || row.delta == "—" {
                        signed_delta_1(d)
                    } else {
                        row.delta.clone()
                    };
                    c.text(
                        &txt,
                        rect.center().0,
                        mid(rect.center().1, data_size),
                        FontSpec::new(data_size),
                        col,
                        TextAlign::Center,
                    );
                }
            },
            "tag" => {
                let val = laplog_cell_value(row, "tag");
                if !val.is_empty() {
                    let chip =
                        Rect::from_xywh(*x + *cw * 0.1, y + row_h * 0.22, *cw * 0.8, row_h * 0.56);
                    draw_dark_cell(c, cfg, LAPLOG_SECTION, chip, 4.0);
                    c.text(
                        &val,
                        chip.center().0,
                        mid(chip.center().1, row_h * 0.30),
                        FontSpec::bold(row_h * 0.30),
                        text,
                        TextAlign::Center,
                    );
                }
            }
            _ => {
                let val = laplog_cell_value(row, key);
                c.text(
                    &val,
                    rect.center().0,
                    mid(rect.center().1, data_size),
                    FontSpec::new(data_size),
                    text,
                    TextAlign::Center,
                );
            }
        }
    }
}

pub fn paint_laptime_log(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    frame: &TelemetryFrame,
    edit_mode: bool,
) -> bool {
    let _ = edit_mode;
    c.clear_transparent();
    let bounds = full_bounds(c);
    let (card, radius) = panel_card(c, cfg, LAPLOG_SECTION, bounds);
    let pad = panel_pad(card.height()).max((card.height() * 0.03).max(8.0));

    let order = laplog_column_order(cfg);
    let cols = laplog_col_layout(&order);
    let show_header = cfg.bool_key(LAPLOG_SECTION, "show_header", true);
    let hscale = cfg
        .f64_key(LAPLOG_SECTION, "header_font_scale", 1.0)
        .max(0.3) as f32;
    let n = cfg.f64_key(LAPLOG_SECTION, "rows", 8.0).max(1.0) as usize;
    let fixed_rh = cfg.f64_key(LAPLOG_SECTION, "row_height_px", 0.0) as f32;
    let max_frac = cfg.f64_key(LAPLOG_SECTION, "max_row_height_frac", 0.14) as f32;
    let font_scale = cfg.f64_key(LAPLOG_SECTION, "font_scale", 0.42) as f32;
    let text_scale = cfg.text_scale(LAPLOG_SECTION);

    let (header_h, row_h) = if fixed_rh > 0.0 {
        let hh = if show_header {
            (fixed_rh * 1.1 * hscale).round()
        } else {
            0.0
        };
        (hh, fixed_rh)
    } else {
        let hh = if show_header {
            (card.height() * 0.12).max(22.0)
        } else {
            0.0
        };
        let body_top_est = card.top() + pad + hh;
        let est_body = (card.bottom() - pad - body_top_est).max(1.0);
        let mut rh = est_body / n as f32;
        if max_frac > 0.0 {
            rh = rh.min(card.height() * max_frac);
        }
        (hh, rh.max(18.0))
    };

    let body_top = card.top() + pad + header_h;
    let inner_w = card.width() - 2.0 * pad;
    let inner_x = card.left() + pad;

    let mut cells: Vec<(String, f32, f32)> = Vec::with_capacity(cols.len());
    let mut cx = inner_x;
    for (key, frac) in &cols {
        let cw = inner_w * frac;
        cells.push((key.clone(), cx, cw));
        cx += cw;
    }

    let header_col = section_color(cfg, LAPLOG_SECTION, "header", "#ffd23a");
    let text = section_color(cfg, LAPLOG_SECTION, "text", "#f4f6f8");
    let muted = section_color(cfg, LAPLOG_SECTION, "muted", "#8b93a1");
    let faster = section_color(cfg, LAPLOG_SECTION, "faster", "#46df7a");
    let slower = section_color(cfg, LAPLOG_SECTION, "slower", "#e23b3b");
    let row_alt = section_color(cfg, LAPLOG_SECTION, "row_alt", "#ffffff08");
    let divider = section_color(cfg, LAPLOG_SECTION, "border", "#ffffff28");

    if show_header {
        let hdr = Rect::from_xywh(inner_x, card.top() + pad, inner_w, header_h);
        c.fill_rect(
            hdr,
            section_color(cfg, LAPLOG_SECTION, "header_bg", "#0b0e12bb"),
            radius,
        );
        c.line(
            hdr.left(),
            hdr.bottom(),
            hdr.right(),
            hdr.bottom(),
            divider,
            1.0,
        );
        let hs = header_h * 0.42 * hscale * text_scale;
        for (key, x, cw) in &cells {
            c.text(
                laplog_col_header(key),
                x + cw * 0.5,
                mid(hdr.center().1, hs),
                FontSpec::bold(hs),
                header_col,
                TextAlign::Center,
            );
        }
    }

    let shown: Vec<&LapLogRow> = frame.lap_log.iter().take(n).collect();
    let alt = cfg.bool_key(LAPLOG_SECTION, "alt_row_shading", true);
    let dividers = cfg.bool_key(LAPLOG_SECTION, "row_dividers", true);
    let data_size = row_h * font_scale * text_scale;

    for (i, row) in shown.iter().enumerate() {
        let y = body_top + i as f32 * row_h;
        if y + row_h > card.bottom() - pad * 0.5 {
            break;
        }
        let row_rect = Rect::from_xywh(inner_x, y, inner_w, row_h);
        if alt && i % 2 == 1 {
            c.fill_rect(row_rect, row_alt, 0.0);
        }
        draw_laplog_row(
            c, cfg, row, &cells, y, row_h, data_size, text, muted, faster, slower,
        );
        if dividers && i + 1 < shown.len() {
            c.line(
                inner_x,
                y + row_h,
                inner_x + inner_w,
                y + row_h,
                divider,
                1.0,
            );
        }
    }
    false
}
