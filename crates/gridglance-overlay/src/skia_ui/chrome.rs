//! Shared Skia chrome matching egui table/card look.

use super::canvas::{Canvas, TextAlign};
use super::tokens::{rgba, DesignTokens};
use super::types::{FontSpec, Rect, Rgba};
use crate::config::OverlayConfig;
use crate::telemetry::{slot_label, TableSlotItem};

pub fn draw_card(c: &mut Canvas, tokens: &DesignTokens, bounds: Rect) {
    // Egui draw_card: fill only — no outer frame stroke.
    c.fill_vertical_gradient(bounds, tokens.card_top, tokens.card_bottom, tokens.radius);
}

/// Egui `chrome::panel_pad`.
pub fn panel_pad(h: f32) -> f32 {
    (h * 0.08).max(8.0)
}

/// True when this section uses the softer Elegant presentation.
pub fn is_elegant(cfg: &OverlayConfig, section: &str) -> bool {
    cfg.panel_style(section) == crate::config::PanelStyle::Elegant
}

/// `panel_opacity` when configured for this section (Radio / System).
/// `1.0` means a fully opaque card fill.
fn panel_fill_opacity(cfg: &OverlayConfig, section: &str) -> Option<f32> {
    cfg.section(section)
        .get("panel_opacity")
        .and_then(|v| v.as_f64())
        .map(|v| (v as f32).clamp(0.0, 1.0))
}

/// Egui `chrome::draw_card` (per-section variant, single panel not a table).
pub fn draw_data_card(c: &mut Canvas, cfg: &OverlayConfig, section: &str, rect: Rect) -> f32 {
    let h = rect.height();
    let radius = (h * cfg.f64_key(section, "corner_radius_frac", 0.0) as f32).max(8.0);
    let mut top = rgba(cfg, section, "bg_top", "#1b1f26f2");
    let mut bottom = rgba(cfg, section, "bg_bottom", "#0f1216f2");
    if let Some(o) = panel_fill_opacity(cfg, section) {
        let a = (o * 255.0).round().clamp(0.0, 255.0) as u8;
        top = top.with_alpha(a);
        bottom = bottom.with_alpha(a);
    }
    c.fill_vertical_gradient(rect, top, bottom, radius);
    radius
}

/// Egui `chrome::draw_elegant_card` — single flat fill (no banded gradient).
pub fn draw_elegant_card(c: &mut Canvas, cfg: &OverlayConfig, section: &str, rect: Rect) -> f32 {
    let h = rect.height();
    let frac = cfg.f64_key(section, "corner_radius_frac", 0.0) as f32;
    let radius = (h * frac.max(0.10)).min(h * 0.22).max(4.0).min(h * 0.5);
    let (top, bottom) = if let Some(o) = panel_fill_opacity(cfg, section) {
        let a = (o * 255.0).round().clamp(0.0, 255.0) as u8;
        (
            rgba(cfg, section, "bg_top", "#1b1f26f2").with_alpha(a),
            rgba(cfg, section, "bg_bottom", "#0f1216f2").with_alpha(a),
        )
    } else {
        (
            rgba(cfg, section, "bg_top", "#1b1f26f2").with_alpha(108),
            rgba(cfg, section, "bg_bottom", "#0f1216f2").with_alpha(88),
        )
    };
    let fill = top.lerp(bottom, 0.55);
    let ru = radius
        .min(rect.width() * 0.5)
        .min(rect.height() * 0.5)
        .max(0.0);
    c.fill_rect(rect, fill, ru);
    radius
}

/// Data vs Elegant card for a section — returns the card radius.
pub fn panel_card(c: &mut Canvas, cfg: &OverlayConfig, section: &str, rect: Rect) -> f32 {
    if is_elegant(cfg, section) {
        draw_elegant_card(c, cfg, section, rect)
    } else {
        draw_data_card(c, cfg, section, rect)
    }
}

/// Horizontal inset for panel content (tighter in Elegant so panels can stay compact).
pub fn panel_content_pad(cfg: &OverlayConfig, section: &str, card_h: f32) -> f32 {
    let base = panel_pad(card_h);
    if is_elegant(cfg, section) {
        (base * 0.75).max(6.0)
    } else {
        base
    }
}

/// Egui `chrome::draw_section_header` — header band with rounded top corners
/// (approximated: full-radius fill + square-off strip at the bottom, matching
/// the `draw_edge_band` pattern already used above).
pub fn draw_section_header(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    rect: Rect,
    title: &str,
    radius_top: f32,
) {
    let bg = rgba(cfg, section, "header_bg", "#0b0e12bb");
    c.fill_rect(rect, bg, radius_top);
    let r = radius_top.max(0.0);
    if r > 0.5 {
        c.fill_rect(
            Rect::from_xywh(rect.x, rect.bottom() - r, rect.w, r),
            bg,
            0.0,
        );
    }
    let edge = rgba(cfg, section, "border", "#ffffff28").with_alpha(70);
    let inset = r;
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
    text_at(
        c,
        rect.left() + 10.0,
        rect.center().1,
        title,
        size,
        rgba(cfg, section, "title", "#f4f6f8"),
        true,
        TextAlign::Left,
    );
}

/// Draw optional title. Data = section header band; Elegant = whisper label.
/// Returns the y cursor below the title (or `y` unchanged when hidden).
pub fn panel_title(
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
        let muted = rgba(cfg, section, "muted", "#8b93a1").with_alpha(200);
        let th = 12.0;
        text_at(
            c,
            card.left() + pad,
            y + th * 0.5,
            &title,
            10.0,
            muted,
            false,
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

/// Baseline-adjusted text draw (`y` is a visual mid-line, not the glyph baseline).
pub fn text_at(
    c: &mut Canvas,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: Rgba,
    bold: bool,
    align: TextAlign,
) {
    let baseline = y + size * 0.35;
    c.text(
        text,
        x,
        baseline,
        if bold {
            FontSpec::bold(size)
        } else {
            FontSpec::new(size)
        },
        color,
        align,
    );
}

/// Nested panel fill (egui `draw_panel_rect`) — radius = min(w,h) * corner_radius_frac.
pub fn draw_panel_rect(c: &mut Canvas, cfg: &OverlayConfig, section: &str, rect: Rect) -> f32 {
    let frac = cfg.f64_key(section, "corner_radius_frac", 0.0) as f32;
    let radius = rect.width().min(rect.height()) * frac;
    let top = rgba(cfg, section, "bg_top", "#1b1f26f2");
    let bottom = rgba(cfg, section, "bg_bottom", "#0f1216f2");
    c.fill_vertical_gradient(rect, top, bottom, radius);
    radius
}

/// Dark pill / cell (egui `draw_dark_cell`).
pub fn draw_dark_cell(c: &mut Canvas, cfg: &OverlayConfig, section: &str, rect: Rect, radius: f32) {
    c.fill_rect(rect, rgba(cfg, section, "cell_dark", "#0b0e12"), radius);
    c.stroke_rect(
        rect,
        rgba(cfg, section, "cell_border", "#ffffff20"),
        radius,
        1.0,
    );
}

pub fn ease(cur: f32, target: f32, dt: f32, tau: f32) -> f32 {
    if !tau.is_finite() || tau <= 1e-6 {
        return target;
    }
    let a = (1.0 - (-dt / tau).exp()).clamp(0.0, 1.0);
    cur + (target - cur) * a
}

pub fn anim_dt(now: f64, last: &mut f64) -> f32 {
    let dt = if *last > 0.0 {
        ((now - *last) as f32).clamp(0.0, 0.1)
    } else {
        1.0 / 60.0
    };
    *last = now;
    dt
}

pub fn still_easing(cur: f32, target: f32, eps: f32) -> bool {
    (cur - target).abs() > eps
}

/// Left accent stripe + horizontal alpha wash + top/bottom rim (egui `draw_row_tint`).
pub fn draw_row_tint(c: &mut Canvas, rect: Rect, accent: Rgba) {
    let h = rect.height();
    let stripe_w = (h * 0.07).max(2.5);
    let edge_a = ((accent.a as u16) + 50).min(255) as u8;
    c.fill_rect(
        Rect::from_xywh(rect.left(), rect.top() + h * 0.12, stripe_w, h * 0.76),
        accent.with_alpha(edge_a),
        2.0,
    );
    // Match egui mesh stops with a true linear gradient (no band steps).
    let wash: [(f32, Rgba); 4] = [
        (0.0, accent.with_alpha((accent.a as f32 * 0.42) as u8)),
        (0.35, accent.with_alpha((accent.a as f32 * 0.22) as u8)),
        (0.72, accent.with_alpha((accent.a as f32 * 0.08) as u8)),
        (1.0, accent.with_alpha(0)),
    ];
    c.fill_horizontal_gradient(rect, &wash);
    let rim = accent.with_alpha(((accent.a as f32) * 0.55) as u8);
    let inset = rect.inset(0.5, 0.0);
    c.line(
        inset.left(),
        inset.top(),
        inset.right(),
        inset.top(),
        rim,
        1.0,
    );
    c.line(
        inset.left(),
        inset.bottom(),
        inset.right(),
        inset.bottom(),
        rim,
        1.0,
    );
}

pub fn draw_edge_band(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    tokens: &DesignTokens,
    band: Rect,
    content: Rect,
    is_header: bool,
    left: &TableSlotItem,
    center: &TableSlotItem,
    right: &TableSlotItem,
) {
    let bg = if is_header {
        tokens.header_bg
    } else {
        tokens.footer_bg
    };
    // Rounded only on outer corners of the card edge (egui CornerRadius nw/ne or sw/se).
    let r = tokens.radius;
    c.fill_rect(band, bg, r);
    if is_header {
        c.fill_rect(
            Rect::from_xywh(band.x, band.bottom() - r.max(1.0), band.w, r.max(1.0)),
            bg,
            0.0,
        );
    } else {
        c.fill_rect(Rect::from_xywh(band.x, band.y, band.w, r.max(1.0)), bg, 0.0);
    }

    let scale = cfg.text_scale(section);
    // Fill the compact band — was 0.42 with extra pad stacked outside content.
    let fs = (content.height() * 0.52).clamp(9.0, 16.0) * scale;
    let icons_group = if is_header {
        "header_icons"
    } else {
        "footer_icons"
    };
    let cy = content.center().1 + fs * 0.35;
    paint_band_slot(
        c,
        cfg,
        section,
        icons_group,
        "left",
        left,
        content.left(),
        cy,
        TextAlign::Left,
        fs,
        tokens.muted,
        tokens.text,
    );
    paint_band_slot(
        c,
        cfg,
        section,
        icons_group,
        "center",
        center,
        content.center().0,
        cy,
        TextAlign::Center,
        fs,
        tokens.muted,
        tokens.text,
    );
    paint_band_slot(
        c,
        cfg,
        section,
        icons_group,
        "right",
        right,
        content.right(),
        cy,
        TextAlign::Right,
        fs,
        tokens.muted,
        tokens.text,
    );
}

fn paint_band_slot(
    c: &mut Canvas,
    cfg: &OverlayConfig,
    section: &str,
    icons_group: &str,
    pos: &str,
    item: &TableSlotItem,
    anchor_x: f32,
    cy: f32,
    align: TextAlign,
    fs: f32,
    muted: Rgba,
    text: Rgba,
) {
    if item.key.is_empty() {
        return;
    }
    let _use_icon = cfg.nested_bool(section, icons_group, pos, false);

    if item.key == "title" {
        c.text(&item.value, anchor_x, cy, FontSpec::bold(fs), text, align);
        return;
    }
    if item.key == "order_pill" {
        let pill_w = fs * 2.8;
        let pill_h = fs * 1.15;
        let x0 = match align {
            TextAlign::Left => anchor_x,
            TextAlign::Right => anchor_x - pill_w,
            TextAlign::Center => anchor_x - pill_w * 0.5,
        };
        let pill = Rect::from_xywh(x0, cy - fs * 0.35 - pill_h * 0.5, pill_w, pill_h);
        c.stroke_rect(pill, muted, 3.0, 1.0);
        c.text(
            "ORDER",
            pill.center().0,
            cy,
            FontSpec::bold(fs * 0.72),
            muted,
            TextAlign::Center,
        );
        return;
    }
    if item.key == "count" || item.key == "track_name" {
        c.text(&item.value, anchor_x, cy, FontSpec::bold(fs), muted, align);
        return;
    }

    let use_icon = _use_icon && super::icons::has(&item.key);
    let icon_sz = fs * 0.85;
    let lead_w = if use_icon {
        super::icons::measure(c, &item.key, icon_sz)
    } else {
        let lead = slot_label(&item.key);
        if lead.is_empty() {
            0.0
        } else {
            c.measure_text(lead, FontSpec::new(fs * 0.62))
        }
    };
    let gap = if lead_w > 0.0 { fs * 0.35 } else { 0.0 };
    let val_w = c.measure_text(&item.value, FontSpec::new(fs * 0.9));
    let total_w = lead_w + gap + val_w;
    let x0 = match align {
        TextAlign::Left => anchor_x,
        TextAlign::Right => anchor_x - total_w,
        TextAlign::Center => anchor_x - total_w * 0.5,
    };
    if lead_w > 0.0 {
        if use_icon {
            super::icons::paint(
                c,
                &item.key,
                x0,
                cy - fs * 0.35,
                icon_sz,
                muted,
                TextAlign::Left,
            );
        } else {
            let lead = slot_label(&item.key);
            c.text(
                lead,
                x0,
                cy,
                FontSpec::new(fs * 0.62),
                muted,
                TextAlign::Left,
            );
        }
    }
    c.text(
        &item.value,
        x0 + lead_w + gap,
        cy,
        FontSpec::new(fs * 0.9),
        text,
        TextAlign::Left,
    );
}

pub fn section_color(cfg: &OverlayConfig, section: &str, key: &str, fallback: &str) -> Rgba {
    rgba(cfg, section, key, fallback)
}

pub fn parse_rgba(s: &str) -> Rgba {
    Rgba::parse(s)
}
