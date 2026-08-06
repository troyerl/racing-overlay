use super::WidgetCtx;
use crate::chrome::{
    color_with_alpha, full_rect, is_elegant, label, panel_card, panel_content_pad, panel_title,
};
use crate::icons;
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "system_panel";

/// show_* config key → (text label, FA icon name).
const ROW_SPECS: &[(&str, &str, &str)] = &[
    ("show_cpu", "CPU", "cpu"),
    ("show_mem", "MEM", "mem"),
    ("show_gpu", "GPU", "gpu"),
    ("show_fps", "FPS", "fps"),
    ("show_network", "NET", "network"),
    ("show_ffb", "FFB", "ffb"),
];

fn push_child(
    rows: &mut Vec<(&'static str, &'static str, String, bool, u8)>,
    name: &'static str,
    val: Option<&String>,
) {
    let Some(v) = val
        .map(String::as_str)
        .filter(|v| !v.is_empty() && *v != "--" && *v != "—")
    else {
        return;
    };
    rows.push((name, "", v.to_string(), false, 1));
}

fn push_process_children(
    rows: &mut Vec<(&'static str, &'static str, String, bool, u8)>,
    cfg: &crate::config::OverlayConfig,
    iracing: (&'static str, Option<&String>),
    overlays: &[(&'static str, Option<&String>)],
    music: &[(&'static str, Option<&String>)],
) {
    if cfg.bool_key(SECTION, "show_subtask_iracing", true) {
        push_child(rows, iracing.0, iracing.1);
    }
    if cfg.bool_key(SECTION, "show_subtask_overlays", true) {
        for &(name, val) in overlays {
            push_child(rows, name, val);
        }
    }
    if cfg.bool_key(SECTION, "show_subtask_music", false) {
        for &(name, val) in music {
            push_child(rows, name, val);
        }
    }
}

/// (label, icon_key, value, warn, indent_level).
fn collect_rows(ctx: &WidgetCtx<'_>) -> Vec<(&'static str, &'static str, String, bool, u8)> {
    let f = ctx.frame;
    let breakdown = ctx.cfg.bool_key(SECTION, "show_process_breakdown", true);
    let mut rows = Vec::new();
    for &(cfg_key, text_label, icon_key) in ROW_SPECS {
        if !ctx.cfg.bool_key(SECTION, cfg_key, true) {
            continue;
        }
        let (value, warn) = match cfg_key {
            "show_cpu" => (f.cpu.clone().unwrap_or_else(|| "—".into()), false),
            "show_mem" => (f.mem.clone().unwrap_or_else(|| "—".into()), false),
            "show_gpu" => (f.gpu.clone().unwrap_or_else(|| "—".into()), false),
            "show_fps" => (
                f.fps.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
                false,
            ),
            "show_network" => {
                let v = if let Some(q) = f.chan_quality {
                    if q > 0.0 {
                        format!("{:.0}%", q)
                    } else {
                        "—".into()
                    }
                } else {
                    "—".into()
                };
                (v, false)
            }
            "show_ffb" => {
                if let Some(p) = f.ffb_pct.filter(|v| v.is_finite()) {
                    (format!("{:.0}%", p), p > 100.0)
                } else {
                    ("—".into(), false)
                }
            }
            _ => ("—".into(), false),
        };
        rows.push((text_label, icon_key, value, warn, 0));
        if breakdown {
            match cfg_key {
                "show_cpu" => push_process_children(
                    &mut rows,
                    ctx.cfg,
                    ("iRacing", f.cpu_iracing.as_ref()),
                    &[
                        ("GridGlance", f.cpu_overlay.as_ref()),
                        ("RaceLab", f.cpu_racelab.as_ref()),
                        ("iOverlay", f.cpu_ioverlay.as_ref()),
                    ],
                    &[
                        ("Spotify", f.cpu_spotify.as_ref()),
                        ("Apple Music", f.cpu_apple_music.as_ref()),
                        ("YouTube Music", f.cpu_youtube_music.as_ref()),
                    ],
                ),
                "show_mem" => push_process_children(
                    &mut rows,
                    ctx.cfg,
                    ("iRacing", f.mem_iracing.as_ref()),
                    &[
                        ("GridGlance", f.mem_overlay.as_ref()),
                        ("RaceLab", f.mem_racelab.as_ref()),
                        ("iOverlay", f.mem_ioverlay.as_ref()),
                    ],
                    &[
                        ("Spotify", f.mem_spotify.as_ref()),
                        ("Apple Music", f.mem_apple_music.as_ref()),
                        ("YouTube Music", f.mem_youtube_music.as_ref()),
                    ],
                ),
                "show_gpu" => push_process_children(
                    &mut rows,
                    ctx.cfg,
                    ("iRacing", f.gpu_iracing.as_ref()),
                    &[
                        ("GridGlance", f.gpu_overlay.as_ref()),
                        ("RaceLab", f.gpu_racelab.as_ref()),
                        ("iOverlay", f.gpu_ioverlay.as_ref()),
                    ],
                    &[
                        ("Spotify", f.gpu_spotify.as_ref()),
                        ("Apple Music", f.gpu_apple_music.as_ref()),
                        ("YouTube Music", f.gpu_youtube_music.as_ref()),
                    ],
                ),
                _ => {}
            }
        }
    }
    rows
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    if is_elegant(ctx.cfg, SECTION) {
        paint_elegant(ui, ctx);
    } else {
        paint_data(ui, ctx);
    }
}

fn paint_data(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    if !rect.is_positive() || rect.width() < 8.0 || rect.height() < 8.0 {
        return;
    }
    let (card, radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_content_pad(ctx.cfg, SECTION, card.height());
    let mut y = card.top() + pad;
    y = panel_title(ui, ctx.cfg, SECTION, card, radius, y, pad, "SYSTEM");

    let show_icons = ctx.cfg.bool_key(SECTION, "show_icons", false);
    let rows = collect_rows(ctx);
    // Fixed row height — panel HWND is live-fitted; don't stretch into empty space.
    let rh = (ctx.cfg.f64_key(SECTION, "row_height_px", 20.0) as f32).clamp(16.0, 28.0);
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let header = ctx.cfg.color(SECTION, "header", "#9aa3b2");
    let warn = ctx.cfg.color(SECTION, "warn", "#ff5b5b");
    let child_label = color_with_alpha(muted, 170);
    let child_value = color_with_alpha(muted, 210);
    for (text_label, icon_key, value, is_warn, indent) in rows {
        let is_child = indent > 0;
        let row = egui::Rect::from_min_size(
            Pos2::new(card.left() + pad, y),
            egui::vec2((card.width() - 2.0 * pad).max(1.0), rh),
        );
        let indent_px = if is_child { 18.0 } else { 0.0 };
        let label_x = row.left() + 8.0 + indent_px;
        if !is_child && show_icons && icons::has(icon_key) {
            if let Some(g) = icons::glyph(icon_key) {
                ui.painter().text(
                    Pos2::new(label_x, row.center().y),
                    Align2::LEFT_CENTER,
                    g,
                    icons::font_id((rh * 0.42).clamp(8.0, 22.0)),
                    header,
                );
            }
        } else if is_child {
            label(
                ui,
                Pos2::new(row.left() + 8.0, row.center().y),
                Align2::LEFT_CENTER,
                "–",
                (rh * 0.34).clamp(8.0, 14.0),
                child_label,
                false,
            );
            label(
                ui,
                Pos2::new(label_x + 4.0, row.center().y),
                Align2::LEFT_CENTER,
                text_label,
                (rh * 0.32).clamp(8.0, 15.0),
                child_label,
                false,
            );
        } else {
            label(
                ui,
                Pos2::new(label_x, row.center().y),
                Align2::LEFT_CENTER,
                text_label,
                (rh * 0.40).clamp(9.0, 20.0),
                header,
                true,
            );
        }
        label(
            ui,
            Pos2::new(row.right() - 8.0, row.center().y),
            Align2::RIGHT_CENTER,
            &value,
            if is_child {
                (rh * 0.34).clamp(8.0, 16.0)
            } else {
                (rh * 0.44).clamp(10.0, 22.0)
            },
            if is_warn {
                warn
            } else if is_child {
                child_value
            } else {
                text
            },
            !is_child,
        );
        y += rh;
    }
}

/// Soft metric stack: dense icon + value rows (same data, less chrome).
fn paint_elegant(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    let (card, _radius) = panel_card(ui, ctx.cfg, SECTION, rect);
    let pad_x = (card.width() * 0.05).clamp(8.0, 12.0);
    let pad_y = (card.height() * 0.05).clamp(6.0, 10.0);
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = color_with_alpha(ctx.cfg.color(SECTION, "muted", "#8b93a1"), 200);
    let accent = ctx.cfg.color(SECTION, "accent", "#9aa3b2");
    let rows = collect_rows(ctx);
    if rows.is_empty() {
        return;
    }

    let show_title = ctx.cfg.bool_key(SECTION, "show_title", true);
    let mut y = card.top() + pad_y;
    let left = card.left() + pad_x;
    let width = card.width() - 2.0 * pad_x;

    if show_title {
        let title = ctx.cfg.str_key(SECTION, "title", "SYSTEM");
        label(
            ui,
            Pos2::new(left, y + 6.0),
            Align2::LEFT_CENTER,
            &title,
            10.0,
            muted,
            false,
        );
        y += 14.0;
    }

    let row_h = 18.0;
    let warn_c = ctx.cfg.color(SECTION, "warn", "#ff5b5b");
    let child_label = color_with_alpha(muted, 160);
    let child_value = color_with_alpha(muted, 200);

    for (text_label, icon_key, value, is_warn, indent) in rows {
        let is_child = indent > 0;
        let cy = y + row_h * 0.5;
        let icon_sz = 12.0;
        let value_c = if is_warn {
            warn_c
        } else if is_child {
            child_value
        } else {
            text
        };
        if !is_child {
            if let Some(g) = icons::glyph(icon_key) {
                ui.painter().text(
                    Pos2::new(left, cy),
                    Align2::LEFT_CENTER,
                    g,
                    icons::font_id(icon_sz),
                    accent,
                );
                label(
                    ui,
                    Pos2::new(left + icon_sz + 6.0, cy),
                    Align2::LEFT_CENTER,
                    text_label,
                    11.0,
                    accent,
                    true,
                );
            } else {
                label(
                    ui,
                    Pos2::new(left, cy),
                    Align2::LEFT_CENTER,
                    text_label,
                    11.0,
                    accent,
                    true,
                );
            }
        } else {
            label(
                ui,
                Pos2::new(left + 10.0, cy),
                Align2::LEFT_CENTER,
                "–",
                10.0,
                child_label,
                false,
            );
            label(
                ui,
                Pos2::new(left + 20.0, cy),
                Align2::LEFT_CENTER,
                text_label,
                10.0,
                child_label,
                false,
            );
        }
        label(
            ui,
            Pos2::new(left + width, cy),
            Align2::RIGHT_CENTER,
            &value,
            if is_child { 11.0 } else { 13.0 },
            value_c,
            !is_child,
        );
        y += row_h;
    }
}
