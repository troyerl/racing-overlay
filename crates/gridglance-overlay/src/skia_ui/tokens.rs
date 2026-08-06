//! Design tokens matching egui table chrome.

use super::types::Rgba;
use crate::config::OverlayConfig;

#[derive(Clone, Debug)]
pub struct DesignTokens {
    pub row_height: f32,
    pub pad: f32,
    pub radius: f32,
    pub header_h: f32,
    pub footer_h: f32,
    pub font_scale: f32,
    pub gap_font_scale: f32,
    pub row_slide_s: f32,
    pub card_top: Rgba,
    pub card_bottom: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub header_bg: Rgba,
    pub footer_bg: Rgba,
}

impl DesignTokens {
    pub fn for_section(cfg: &OverlayConfig, section: &str, panel_h: f32) -> Self {
        let rh_cfg = cfg.f64_key(section, "row_height_px", 28.0) as f32;
        let rh = if (rh_cfg - 36.0).abs() < 0.01 {
            28.0
        } else {
            rh_cfg.max(16.0)
        };
        let font_scale_cfg = cfg.f64_key(section, "font_scale", 0.48) as f32;
        let font_scale = if (font_scale_cfg - 0.40).abs() < 0.001 {
            0.48
        } else {
            font_scale_cfg
        };
        let scale = cfg.text_scale(section);
        let hscale = cfg.f64_key(section, "header_font_scale", 1.0) as f32;
        let fscale = cfg.f64_key(section, "footer_font_scale", 1.0) as f32;
        let show_footer = cfg.bool_key(section, "show_footer", true);
        // Match egui draw_card: max(8, h * corner_radius_frac).
        let radius = (panel_h * cfg.f64_key(section, "corner_radius_frac", 0.0) as f32).max(8.0);
        // Horizontal inset for band slots / row body — keep modest so edges aren't flush.
        let pad = if rh > 0.0 {
            6.0
        } else {
            (panel_h * 0.02).max(6.0)
        };
        Self {
            row_height: rh,
            pad,
            radius,
            // Compact chrome bands (no extra vertical pad stacked into the band).
            header_h: (rh * 0.50 * scale * hscale.max(0.3)).max(15.0),
            footer_h: if show_footer {
                (rh * 0.48 * scale * fscale.max(0.3)).max(14.0)
            } else {
                0.0
            },
            font_scale,
            gap_font_scale: cfg.f64_key(section, "gap_font_scale", 1.12) as f32,
            row_slide_s: cfg.f64_key(section, "row_ease_tau", 0.24) as f32,
            card_top: rgba(cfg, section, "bg_top", "#1b1f26f2"),
            card_bottom: rgba(cfg, section, "bg_bottom", "#0f1216f2"),
            border: rgba(cfg, section, "border", "#ffffff28"),
            text: rgba(cfg, section, "text", "#f4f6f8"),
            muted: rgba(cfg, section, "muted", "#8b93a1"),
            header_bg: rgba(cfg, section, "header_bg", "#0b0e12bb"),
            footer_bg: rgba(cfg, section, "footer_bg", "#0f1216"),
        }
    }
}

pub fn rgba(cfg: &OverlayConfig, section: &str, key: &str, fallback: &str) -> Rgba {
    Rgba::from_egui(cfg.color(section, key, fallback))
}
