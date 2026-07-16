//! Python ConfigEditor color palette + egui Visuals for the Settings viewport.

use egui::{
    self, Color32, ColorImage, CornerRadius, FontFamily, FontId, TextStyle, TextureHandle,
    TextureOptions, Visuals,
};

pub const ACCENT: Color32 = Color32::from_rgb(0x46, 0xdf, 0x7a);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(0x2f, 0x9d, 0x56);
pub const BLUE: Color32 = Color32::from_rgb(0x4c, 0x9a, 0xff);
pub const ORANGE: Color32 = Color32::from_rgb(0xff, 0x94, 0x16);
pub const YELLOW: Color32 = Color32::from_rgb(0xff, 0xd2, 0x3a);
pub const TEXT: Color32 = Color32::from_rgb(0xd7, 0xda, 0xe0);
pub const TITLE: Color32 = Color32::from_rgb(0xf4, 0xf6, 0xf8);
pub const MUTED: Color32 = Color32::from_rgb(0x8b, 0x93, 0xa1);
pub const ROW_LABEL: Color32 = Color32::from_rgb(0xc7, 0xcd, 0xd6);
pub const NAV_SECTION: Color32 = Color32::from_rgb(0x6b, 0x72, 0x80);
pub const NAV_IDLE: Color32 = Color32::from_rgb(0xaa, 0xb2, 0xbf);
pub const BG: Color32 = Color32::from_rgb(0x0d, 0x0f, 0x12);
pub const CARD_BORDER: Color32 = Color32::from_rgb(0x26, 0x2b, 0x34);
pub const NAV_BORDER: Color32 = Color32::from_rgb(0x20, 0x24, 0x2c);
pub const INPUT_BORDER: Color32 = Color32::from_rgb(0x2c, 0x31, 0x3b);
pub const BTN_BORDER: Color32 = Color32::from_rgb(0x2f, 0x35, 0x40);
pub const GROOVE: Color32 = Color32::from_rgb(0x23, 0x28, 0x31);
pub const POPUP_BG: Color32 = Color32::from_rgb(0x16, 0x1a, 0x20);

#[inline]
pub fn card_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(18, 21, 27, 217)
}

#[inline]
pub fn rail_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(13, 15, 19, 199)
}

#[inline]
pub fn input_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(20, 23, 28, 217)
}

#[inline]
pub fn button_bg() -> Color32 {
    Color32::from_rgba_unmultiplied(34, 39, 50, 230)
}

#[inline]
pub fn button_hover_bg() -> Color32 {
    Color32::from_rgb(0x2a, 0x31, 0x40)
}

pub const NAV_WIDTH: f32 = 196.0;
pub const LABEL_WIDTH: f32 = 170.0;
pub const CARD_RADIUS: f32 = 13.0;
pub const ACCORDION_RADIUS: f32 = 11.0;
pub const FIELD_RADIUS: f32 = 9.0;
pub const SEARCH_RADIUS: f32 = 11.0;
pub const BTN_RADIUS: f32 = 10.0;
pub const NAV_ITEM_H: f32 = 40.0;

const CARBON_TILE_PX: usize = 12;

/// Approximate letter-spacing for uppercase labels (egui has no tracking API).
pub fn spaced_upper(s: &str) -> String {
    s.chars()
        .map(|c| c.to_uppercase().to_string())
        .collect::<Vec<_>>()
        .join("\u{2009}") // thin space
}

pub fn parse_hex(s: &str) -> Color32 {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).unwrap_or(0xaa);
        let g = u8::from_str_radix(&s[2..4], 16).unwrap_or(0xaa);
        let b = u8::from_str_radix(&s[4..6], 16).unwrap_or(0xb2);
        return Color32::from_rgb(r, g, b);
    }
    ACCENT
}

/// Apply dark carbon theme to this viewport only.
pub fn apply_settings_visuals(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = BG;
    visuals.extreme_bg_color = Color32::from_rgb(0x0b, 0x0d, 0x11);
    visuals.faint_bg_color = card_bg();
    visuals.widgets.noninteractive.bg_fill = card_bg();
    visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);
    visuals.widgets.inactive.bg_fill = input_bg();
    visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, INPUT_BORDER);
    visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0_f32, NAV_IDLE);
    visuals.widgets.hovered.bg_fill = button_hover_bg();
    visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0_f32, BTN_BORDER);
    visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0_f32, TITLE);
    visuals.widgets.active.bg_fill = Color32::from_rgba_unmultiplied(70, 223, 122, 64);
    visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0_f32, TITLE);
    visuals.widgets.open.bg_fill = input_bg();
    visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(70, 223, 122, 80);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.window_shadow = egui::epaint::Shadow::NONE;
    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.animation_time = 0.14;
        style
            .text_styles
            .insert(TextStyle::Body, FontId::new(12.0, FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(12.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(16.0, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Small,
            FontId::new(11.0, FontFamily::Proportional),
        );
    });
    ctx.options_mut(|opts| {
        opts.line_scroll_speed = 28.0;
    });
}

fn carbon_tile_image() -> ColorImage {
    let n = CARBON_TILE_PX;
    let cell = n / 2;
    // Quieter weave — closer to BG so cards read first.
    let c_hi = Color32::from_rgb(0x16, 0x19, 0x1e);
    let c_mid = Color32::from_rgb(0x11, 0x14, 0x18);
    let c_lo = Color32::from_rgb(0x0d, 0x0f, 0x12);
    let mut pixels = vec![c_lo; n * n];
    for y in 0..n {
        for x in 0..n {
            let i = y / cell;
            let j = x / cell;
            let flip = (i + j) % 2 == 1;
            let base = if flip { c_lo } else { c_hi };
            let lx = x % cell;
            let ly = y % cell;
            let inset = if flip { 1 } else { 2 };
            let in_band = lx >= inset && ly >= inset && lx < cell - inset && ly < cell - inset;
            pixels[y * n + x] = if in_band { c_mid } else { base };
        }
    }
    ColorImage {
        size: [n, n],
        pixels,
    }
}

fn carbon_texture(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::new("gg_settings_carbon_tex_v2");
    if let Some(tex) = ctx.data(|d| d.get_temp::<TextureHandle>(id)) {
        return tex;
    }
    let tex = ctx.load_texture(
        "gg_settings_carbon_v2",
        carbon_tile_image(),
        TextureOptions::NEAREST_REPEAT,
    );
    ctx.data_mut(|d| d.insert_temp(id, tex.clone()));
    tex
}

/// Carbon weave + soft vertical sheen behind the whole settings window.
pub fn paint_background(ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::ZERO, BG);

    let tex = carbon_texture(ui.ctx());
    let tile = CARBON_TILE_PX as f32;
    let uv = egui::Rect::from_min_size(
        egui::pos2(0.0, 0.0),
        egui::vec2(rect.width() / tile, rect.height() / tile),
    );
    painter.image(tex.id(), rect, uv, Color32::WHITE);

    // Smooth vertical sheen via GPU-interpolated vertex colors (no banding).
    // Peak highlight at t≈0.0525 (alpha 14); fades out by t≈0.175; vignette after 0.65.
    let h = rect.height().max(1.0);
    let strip = |t0: f32, t1: f32| {
        egui::Rect::from_min_max(
            egui::pos2(rect.left(), rect.top() + h * t0),
            egui::pos2(rect.right(), rect.top() + h * t1),
        )
    };
    let white = |a: u8| Color32::from_rgba_unmultiplied(255, 255, 255, a);
    let black = |a: u8| Color32::from_rgba_unmultiplied(0, 0, 0, a);
    gradient_quad(painter, strip(0.0, 0.0525), white(8), white(14));
    gradient_quad(painter, strip(0.0525, 0.175), white(14), white(0));
    gradient_quad(painter, strip(0.65, 1.0), black(0), black(55));
}

/// Vertical gradient quad: top color → bottom color, GPU-lerped per pixel.
fn gradient_quad(painter: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32) {
    if rect.height() <= 0.0 || rect.width() <= 0.0 {
        return;
    }
    let mut mesh = egui::Mesh::default();
    let i = mesh.vertices.len() as u32;
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_top(),
        uv: egui::epaint::WHITE_UV,
        color: top,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_top(),
        uv: egui::epaint::WHITE_UV,
        color: top,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.right_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: bottom,
    });
    mesh.vertices.push(egui::epaint::Vertex {
        pos: rect.left_bottom(),
        uv: egui::epaint::WHITE_UV,
        color: bottom,
    });
    mesh.indices
        .extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    painter.add(egui::Shape::mesh(mesh));
}
