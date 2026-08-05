//! Skia icon helpers — Font Awesome glyphs via embedded `fa-solid-900.ttf`.

use super::canvas::{Canvas, TextAlign};
use super::types::Rgba;
use crate::icons;

/// Draw a named FA icon (egui `icons::glyph` key). Returns advance width (0 if unknown).
pub fn paint(
    c: &mut Canvas,
    name: &str,
    x: f32,
    cy: f32,
    size: f32,
    color: Rgba,
    align: TextAlign,
) -> f32 {
    let Some(g) = icons::glyph(name) else {
        return 0.0;
    };
    c.icon(&g, x, cy, size, color, align)
}

/// Measure a named FA icon.
pub fn measure(c: &Canvas, name: &str, size: f32) -> f32 {
    let Some(g) = icons::glyph(name) else {
        return 0.0;
    };
    c.measure_icon(&g, size)
}

/// Whether a named icon exists in the FA map.
pub fn has(name: &str) -> bool {
    icons::has(name)
}
