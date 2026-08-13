//! Skia CPU surface → premul BGRA for UpdateLayeredWindow.

use super::types::{FontSpec, Rect, Rgba};
use skia_safe::{
    surfaces, AlphaType, BlendMode, Color, Color4f, ColorType, Font, FontMgr, FontStyle, ImageInfo,
    Paint, PaintStyle, Point, Surface, TextBlob, TileMode, Typeface,
};

pub struct Canvas {
    surface: Surface,
    width: i32,
    height: i32,
    font_mgr: FontMgr,
    face_regular: Option<Typeface>,
    face_bold: Option<Typeface>,
    face_icons: Option<Typeface>,
}

impl Canvas {
    pub fn new(width: i32, height: i32) -> Option<Self> {
        if width <= 0 || height <= 0 {
            return None;
        }
        let info = ImageInfo::new(
            (width, height),
            ColorType::BGRA8888,
            AlphaType::Premul,
            None,
        );
        let surface = surfaces::raster(&info, None, None)?;
        let font_mgr = FontMgr::new();
        let face_regular = font_mgr.new_from_data(crate::icons::noto_regular_bytes(), None);
        let face_bold = font_mgr.new_from_data(crate::icons::noto_bold_bytes(), None);
        let face_icons = font_mgr.new_from_data(crate::icons::fa_ttf_bytes(), None);
        Some(Self {
            surface,
            width,
            height,
            font_mgr,
            face_regular,
            face_bold,
            face_icons,
        })
    }

    pub fn width(&self) -> i32 {
        self.width
    }
    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn clear_transparent(&mut self) {
        self.surface.canvas().clear(Color::TRANSPARENT);
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Rgba, radius: f32) {
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Fill);
        let r = rect.to_skia();
        if radius > 0.5 {
            self.surface
                .canvas()
                .draw_round_rect(r, radius, radius, &paint);
        } else {
            self.surface.canvas().draw_rect(r, &paint);
        }
    }

    pub fn fill_vertical_gradient(&mut self, rect: Rect, top: Rgba, bottom: Rgba, radius: f32) {
        let shader = skia_safe::Shader::linear_gradient(
            (
                Point::new(rect.x, rect.y),
                Point::new(rect.x, rect.bottom()),
            ),
            [
                Color::from_argb(top.a, top.r, top.g, top.b),
                Color::from_argb(bottom.a, bottom.r, bottom.g, bottom.b),
            ]
            .as_ref(),
            None,
            TileMode::Clamp,
            None,
            None,
        );
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        if let Some(sh) = shader {
            paint.set_shader(sh);
        }
        let r = rect.to_skia();
        if radius > 0.5 {
            self.surface
                .canvas()
                .draw_round_rect(r, radius, radius, &paint);
        } else {
            self.surface.canvas().draw_rect(r, &paint);
        }
    }

    /// Left→right linear gradient. `stops` are `(fraction 0..1, color)`.
    pub fn fill_horizontal_gradient(&mut self, rect: Rect, stops: &[(f32, Rgba)]) {
        if stops.len() < 2 {
            return;
        }
        let colors: Vec<Color> = stops
            .iter()
            .map(|(_, c)| Color::from_argb(c.a, c.r, c.g, c.b))
            .collect();
        let positions: Vec<f32> = stops.iter().map(|(f, _)| f.clamp(0.0, 1.0)).collect();
        let shader = skia_safe::Shader::linear_gradient(
            (
                Point::new(rect.left(), rect.top()),
                Point::new(rect.right(), rect.top()),
            ),
            colors.as_slice(),
            positions.as_slice(),
            TileMode::Clamp,
            None,
            None,
        );
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        if let Some(sh) = shader {
            paint.set_shader(sh);
        }
        self.surface.canvas().draw_rect(rect.to_skia(), &paint);
    }

    /// Soft glow: horizontal tent (peak at center) × vertical linear fade.
    /// `opaque_at_top`: peak alpha on the top edge, fading out toward the bottom.
    pub fn fill_tent_vertical_fade(&mut self, rect: Rect, color: Rgba, opaque_at_top: bool) {
        if rect.width() < 0.5 || rect.height() < 0.5 || color.a < 2 {
            return;
        }
        let peak = Color::from_argb(color.a, color.r, color.g, color.b);
        let clear = Color::from_argb(0, color.r, color.g, color.b);
        let Some(h_shader) = skia_safe::Shader::linear_gradient(
            (
                Point::new(rect.left(), rect.top()),
                Point::new(rect.right(), rect.top()),
            ),
            [clear, peak, clear].as_ref(),
            [0.0_f32, 0.5, 1.0].as_ref(),
            TileMode::Clamp,
            None,
            None,
        ) else {
            return;
        };
        let (top_a, bot_a) = if opaque_at_top { (255, 0) } else { (0, 255) };
        let Some(v_shader) = skia_safe::Shader::linear_gradient(
            (
                Point::new(rect.left(), rect.top()),
                Point::new(rect.left(), rect.bottom()),
            ),
            [
                Color::from_argb(top_a, 255, 255, 255),
                Color::from_argb(bot_a, 255, 255, 255),
            ]
            .as_ref(),
            None,
            TileMode::Clamp,
            None,
            None,
        ) else {
            return;
        };
        let shader = skia_safe::shaders::blend(BlendMode::Modulate, h_shader, v_shader);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_shader(shader);
        self.surface.canvas().draw_rect(rect.to_skia(), &paint);
    }

    /// Soft glow: vertical tent (peak at mid-height) × horizontal linear fade.
    /// `opaque_at_left`: peak alpha on the left edge, fading out toward the right.
    pub fn fill_tent_horizontal_fade(&mut self, rect: Rect, color: Rgba, opaque_at_left: bool) {
        if rect.width() < 0.5 || rect.height() < 0.5 || color.a < 2 {
            return;
        }
        let peak = Color::from_argb(color.a, color.r, color.g, color.b);
        let clear = Color::from_argb(0, color.r, color.g, color.b);
        let Some(v_shader) = skia_safe::Shader::linear_gradient(
            (
                Point::new(rect.left(), rect.top()),
                Point::new(rect.left(), rect.bottom()),
            ),
            [clear, peak, clear].as_ref(),
            [0.0_f32, 0.5, 1.0].as_ref(),
            TileMode::Clamp,
            None,
            None,
        ) else {
            return;
        };
        let (left_a, right_a) = if opaque_at_left { (255, 0) } else { (0, 255) };
        let Some(h_shader) = skia_safe::Shader::linear_gradient(
            (
                Point::new(rect.left(), rect.top()),
                Point::new(rect.right(), rect.top()),
            ),
            [
                Color::from_argb(left_a, 255, 255, 255),
                Color::from_argb(right_a, 255, 255, 255),
            ]
            .as_ref(),
            None,
            TileMode::Clamp,
            None,
            None,
        ) else {
            return;
        };
        let shader = skia_safe::shaders::blend(BlendMode::Modulate, v_shader, h_shader);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        paint.set_shader(shader);
        self.surface.canvas().draw_rect(rect.to_skia(), &paint);
    }

    pub fn stroke_rect(&mut self, rect: Rect, color: Rgba, radius: f32, width: f32) {
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(width);
        let r = rect.to_skia();
        if radius > 0.5 {
            self.surface
                .canvas()
                .draw_round_rect(r, radius, radius, &paint);
        } else {
            self.surface.canvas().draw_rect(r, &paint);
        }
    }

    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, color: Rgba, width: f32) {
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(width);
        self.surface
            .canvas()
            .draw_line(Point::new(x0, y0), Point::new(x1, y1), &paint);
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: Rgba, fill: bool) {
        self.circle_ex(cx, cy, radius, color, fill, 1.5);
    }

    pub fn circle_ex(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        color: Rgba,
        fill: bool,
        stroke_width: f32,
    ) {
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        paint.set_style(if fill {
            PaintStyle::Fill
        } else {
            PaintStyle::Stroke
        });
        if !fill {
            paint.set_stroke_width(stroke_width);
        }
        self.surface
            .canvas()
            .draw_circle(Point::new(cx, cy), radius, &paint);
    }

    /// Stroke an arc from `a0` to `a1` (radians, 0=east, CCW; y-down screen).
    pub fn stroke_arc(
        &mut self,
        cx: f32,
        cy: f32,
        r: f32,
        a0: f32,
        a1: f32,
        color: Rgba,
        width: f32,
        steps: usize,
    ) {
        let steps = steps.max(2);
        let mut pts = Vec::with_capacity(steps + 1);
        for s in 0..=steps {
            let t = s as f32 / steps as f32;
            let a = a0 + (a1 - a0) * t;
            pts.push((cx + r * a.cos(), cy - r * a.sin()));
        }
        self.polyline(&pts, color, width, false);
    }

    pub fn polyline(&mut self, pts: &[(f32, f32)], color: Rgba, width: f32, closed: bool) {
        if pts.len() < 2 {
            return;
        }
        let mut path = skia_safe::Path::new();
        path.move_to(Point::new(pts[0].0, pts[0].1));
        for p in &pts[1..] {
            path.line_to(Point::new(p.0, p.1));
        }
        if closed {
            path.close();
        }
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Stroke);
        paint.set_stroke_width(width);
        paint.set_stroke_join(skia_safe::PaintJoin::Round);
        paint.set_stroke_cap(skia_safe::PaintCap::Round);
        self.surface.canvas().draw_path(&path, &paint);
    }

    /// Fill a closed polygon (Skia winding fill — safe for concave track outlines).
    pub fn fill_closed_path(&mut self, pts: &[(f32, f32)], color: Rgba) {
        if pts.len() < 3 {
            return;
        }
        let mut path = skia_safe::Path::new();
        path.move_to(Point::new(pts[0].0, pts[0].1));
        for p in &pts[1..] {
            path.line_to(Point::new(p.0, p.1));
        }
        path.close();
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        paint.set_style(PaintStyle::Fill);
        self.surface.canvas().draw_path(&path, &paint);
    }

    /// Dashed open polyline with continuous dash phase across segments.
    pub fn dashed_polyline(
        &mut self,
        pts: &[(f32, f32)],
        color: Rgba,
        width: f32,
        dash: f32,
        gap: f32,
    ) {
        if pts.len() < 2 {
            return;
        }
        let dash = dash.max(1.0);
        let gap = gap.max(0.5);
        let pattern = dash + gap;
        let mut phase = 0.0_f32;
        for w in pts.windows(2) {
            let a = w[0];
            let b = w[1];
            let dx = b.0 - a.0;
            let dy = b.1 - a.1;
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-3 {
                continue;
            }
            let ux = dx / len;
            let uy = dy / len;
            let mut consumed = 0.0_f32;
            while consumed < len {
                let in_dash = phase < dash;
                let remain = if in_dash {
                    dash - phase
                } else {
                    pattern - phase
                };
                let step = remain.min(len - consumed);
                if in_dash && step > 1e-4 {
                    let t0 = consumed;
                    let t1 = consumed + step;
                    self.line(
                        a.0 + ux * t0,
                        a.1 + uy * t0,
                        a.0 + ux * t1,
                        a.1 + uy * t1,
                        color,
                        width,
                    );
                }
                consumed += step;
                phase += step;
                if phase >= pattern - 1e-6 {
                    phase = 0.0;
                }
            }
        }
    }

    fn font(&self, spec: FontSpec) -> Font {
        let style = if spec.bold {
            FontStyle::bold()
        } else {
            FontStyle::normal()
        };
        let typeface = if spec.bold {
            self.face_bold.clone().or_else(|| self.face_regular.clone())
        } else {
            self.face_regular.clone()
        }
        .or_else(|| self.font_mgr.match_family_style("Segoe UI", style))
        .or_else(|| self.font_mgr.match_family_style("Arial", style))
        .unwrap_or_else(|| {
            self.font_mgr
                .legacy_make_typeface(None, style)
                .expect("default typeface")
        });
        let mut font = Font::new(typeface, spec.size);
        font.set_edging(skia_safe::font::Edging::AntiAlias);
        font.set_subpixel(true);
        font
    }

    fn icon_font(&self, size: f32) -> Option<Font> {
        let face = self.face_icons.clone()?;
        let mut font = Font::new(face, size.max(6.0));
        font.set_edging(skia_safe::font::Edging::AntiAlias);
        font.set_subpixel(true);
        Some(font)
    }

    pub fn measure_text(&self, text: &str, spec: FontSpec) -> f32 {
        let font = self.font(spec);
        let (_, rect) = font.measure_str(text, None);
        rect.width()
    }

    /// Draw `text` so its tight measured ink box is centred on `(cx, cy)`.
    ///
    /// Good for small map-dot digits. For large dash gear, prefer
    /// [`Self::text_path_centered`] (glyph-path union; blob/measure leave
    /// "1" looking left-shifted).
    pub fn text_ink_centered(
        &mut self,
        text: &str,
        cx: f32,
        cy: f32,
        spec: FontSpec,
        color: Rgba,
    ) {
        if text.is_empty() {
            return;
        }
        let font = self.font(spec);
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        let Some(blob) = TextBlob::from_str(text, &font) else {
            return;
        };
        let (_, measured) = font.measure_str(text, Some(&paint));
        let rect = if measured.width() > 0.0 && measured.height() > 0.0 {
            measured
        } else {
            *blob.bounds()
        };
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        let draw_x = cx - (rect.left + rect.right) * 0.5;
        let baseline = cy - (rect.top + rect.bottom) * 0.5;
        self.surface
            .canvas()
            .draw_text_blob(&blob, (draw_x, baseline), &paint);
    }

    /// Draw `text` centred on the union of glyph path bounds (baseline origin).
    ///
    /// This is the most reliable centre for large bold digits like gear "1",
    /// where TextBlob / measure boxes carry asymmetric side bearings.
    pub fn text_path_centered(
        &mut self,
        text: &str,
        cx: f32,
        cy: f32,
        spec: FontSpec,
        color: Rgba,
    ) {
        if text.is_empty() {
            return;
        }
        let font = self.font(spec);
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        let Some(blob) = TextBlob::from_str(text, &font) else {
            return;
        };

        let glyphs = font.str_to_glyphs_vec(text);
        let mut widths = vec![0.0; glyphs.len()];
        let mut glyph_bounds = vec![skia_safe::Rect::default(); glyphs.len()];
        font.get_widths_bounds(&glyphs, Some(&mut widths), Some(&mut glyph_bounds), Some(&paint));

        let mut pen_x = 0.0_f32;
        let mut min_x = f32::INFINITY;
        let mut min_y = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_y = f32::NEG_INFINITY;
        for (i, gb) in glyph_bounds.iter().enumerate() {
            if gb.width() > 0.0 && gb.height() > 0.0 {
                min_x = min_x.min(pen_x + gb.left);
                min_y = min_y.min(gb.top);
                max_x = max_x.max(pen_x + gb.right);
                max_y = max_y.max(gb.bottom);
            } else if let Some(path) = font.get_path(glyphs[i]) {
                let pb = path.bounds();
                min_x = min_x.min(pen_x + pb.left);
                min_y = min_y.min(pb.top);
                max_x = max_x.max(pen_x + pb.right);
                max_y = max_y.max(pb.bottom);
            }
            pen_x += widths[i];
        }

        let (draw_x, baseline) = if min_x.is_finite() && max_x > min_x && max_y > min_y {
            (
                cx - (min_x + max_x) * 0.5,
                cy - (min_y + max_y) * 0.5,
            )
        } else {
            // Fallback: measured string box.
            let (_, measured) = font.measure_str(text, Some(&paint));
            if measured.width() <= 0.0 || measured.height() <= 0.0 {
                return;
            }
            (
                cx - (measured.left + measured.right) * 0.5,
                cy - (measured.top + measured.bottom) * 0.5,
            )
        };
        self.surface
            .canvas()
            .draw_text_blob(&blob, (draw_x, baseline), &paint);
    }

    /// Measure a Font Awesome glyph string (from `crate::icons::glyph`).
    pub fn measure_icon(&self, glyph: &str, size: f32) -> f32 {
        let Some(font) = self.icon_font(size) else {
            return size * 0.7;
        };
        let (_, rect) = font.measure_str(glyph, None);
        rect.width()
    }

    pub fn text(
        &mut self,
        text: &str,
        x: f32,
        y: f32,
        spec: FontSpec,
        color: Rgba,
        align: TextAlign,
    ) {
        if text.is_empty() {
            return;
        }
        let font = self.font(spec);
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        let blob = TextBlob::from_str(text, &font);
        let Some(blob) = blob else { return };
        let width = self.measure_text(text, spec);
        let draw_x = match align {
            TextAlign::Left => x,
            TextAlign::Center => x - width * 0.5,
            TextAlign::Right => x - width,
        };
        // y is baseline; callers pass mid-row → adjust with size*0.35
        self.surface
            .canvas()
            .draw_text_blob(&blob, (draw_x, y), &paint);
    }

    /// Draw a Font Awesome glyph. `y` is visual mid-line (same as `text_at`).
    pub fn icon(
        &mut self,
        glyph: &str,
        x: f32,
        y: f32,
        size: f32,
        color: Rgba,
        align: TextAlign,
    ) -> f32 {
        if glyph.is_empty() {
            return 0.0;
        }
        let Some(font) = self.icon_font(size) else {
            // Fallback: no FA face — skip silently.
            return 0.0;
        };
        let width = self.measure_icon(glyph, size);
        let draw_x = match align {
            TextAlign::Left => x,
            TextAlign::Center => x - width * 0.5,
            TextAlign::Right => x - width,
        };
        let baseline = y + size * 0.35;
        let mut paint = Paint::new(
            Color4f::new(
                color.r as f32 / 255.0,
                color.g as f32 / 255.0,
                color.b as f32 / 255.0,
                color.a as f32 / 255.0,
            ),
            None,
        );
        paint.set_anti_alias(true);
        if let Some(blob) = TextBlob::from_str(glyph, &font) {
            self.surface
                .canvas()
                .draw_text_blob(&blob, (draw_x, baseline), &paint);
        }
        width
    }

    /// Draw a Font Awesome glyph rotated clockwise by `angle_rad` around `(cx, cy)`.
    pub fn icon_rotated(
        &mut self,
        glyph: &str,
        cx: f32,
        cy: f32,
        size: f32,
        color: Rgba,
        angle_rad: f32,
    ) {
        if glyph.is_empty() {
            return;
        }
        {
            let canvas = self.surface.canvas();
            canvas.save();
            canvas.translate(Point::new(cx, cy));
            canvas.rotate(angle_rad.to_degrees(), None);
        }
        self.icon(glyph, 0.0, 0.0, size, color, TextAlign::Center);
        self.surface.canvas().restore();
    }

    pub fn clip_rect(&mut self, rect: Rect, f: impl FnOnce(&mut Self)) {
        self.surface.canvas().save();
        self.surface.canvas().clip_rect(rect.to_skia(), None, true);
        f(self);
        self.surface.canvas().restore();
    }

    /// Decode a PNG and draw it centered inside `dest` (aspect-fit, rounded).
    ///
    /// Country flags use a shared display footprint ([`crate::country_flags::FLAG_DISPLAY_ASPECT`])
    /// so wide (US) and square (CH) assets read the same size. The image is
    /// scaled to cover that box (center-cropped) rather than letterboxed.
    pub fn draw_png_fit(&mut self, png: &[u8], dest: Rect) {
        use skia_safe::{AlphaType, ColorType, Data, Image, ImageInfo, RRect};
        // Prefer Skia decode; fall back through `image` for palette PNGs Skia rejects.
        let img = Image::from_encoded(Data::new_copy(png)).or_else(|| {
            let rgba = image::load_from_memory(png).ok()?.into_rgba8();
            let w = rgba.width() as i32;
            let h = rgba.height() as i32;
            let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Unpremul, None);
            let row_bytes = (w as usize) * 4;
            skia_safe::images::raster_from_data(&info, Data::new_copy(rgba.as_raw()), row_bytes)
        });
        let Some(img) = img else {
            return;
        };
        let iw = img.width() as f32;
        let ih = img.height() as f32;
        if iw <= 0.0 || ih <= 0.0 || dest.width() <= 0.0 || dest.height() <= 0.0 {
            return;
        }
        let (box_w, box_h) = crate::country_flags::flag_display_box(dest.width(), dest.height());
        if box_w <= 0.0 || box_h <= 0.0 {
            return;
        }
        // Cover: scale so the image fills the box, then center-crop.
        let scale = (box_w / iw).max(box_h / ih);
        let src_w = box_w / scale;
        let src_h = box_h / scale;
        let src_x = (iw - src_w) * 0.5;
        let src_y = (ih - src_h) * 0.5;
        let src = skia_safe::Rect::from_xywh(src_x, src_y, src_w, src_h);
        let x = dest.left() + (dest.width() - box_w) * 0.5;
        let y = dest.top() + (dest.height() - box_h) * 0.5;
        let dst = skia_safe::Rect::from_xywh(x, y, box_w, box_h);
        let radius = (box_h * 0.18).clamp(1.5, 3.5);
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let canvas = self.surface.canvas();
        canvas.save();
        canvas.clip_rrect(RRect::new_rect_xy(dst, radius, radius), None, true);
        canvas.draw_image_rect(&img, Some((&src, skia_safe::canvas::SrcRectConstraint::Strict)), dst, &paint);
        canvas.restore();
    }

    /// Cached radar car sprite for `kind`. Source art is nose-up portrait.
    pub fn draw_radar_car(
        &mut self,
        cx: f32,
        cy: f32,
        box_w: f32,
        box_h: f32,
        kind: crate::widgets::radar::RadarCarKind,
    ) {
        use skia_safe::{AlphaType, ColorType, Data, ImageInfo};
        use std::sync::OnceLock;
        static CACHE: OnceLock<[OnceLock<Option<skia_safe::Image>>; 7]> = OnceLock::new();
        let slots = CACHE.get_or_init(|| std::array::from_fn(|_| OnceLock::new()));
        let Some(img) = slots[kind as usize]
            .get_or_init(|| {
                let (w, h, rgba) = crate::widgets::radar::car_sprite_rgba(kind)?;
                let info = ImageInfo::new(
                    (*w as i32, *h as i32),
                    ColorType::RGBA8888,
                    AlphaType::Unpremul,
                    None,
                );
                skia_safe::images::raster_from_data(&info, Data::new_copy(rgba), (*w as usize) * 4)
            })
            .clone()
        else {
            return;
        };
        let iw = img.width() as f32;
        let ih = img.height() as f32;
        if iw <= 0.0 || ih <= 0.0 || box_w <= 0.0 || box_h <= 0.0 {
            return;
        }
        let scale = (box_w / iw).min(box_h / ih);
        let draw_w = iw * scale;
        let draw_h = ih * scale;
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        let canvas = self.surface.canvas();
        canvas.save();
        canvas.translate(Point::new(cx, cy));
        let rot = kind.sprite_rot_deg();
        if rot != 0.0 {
            canvas.rotate(rot, None);
        }
        let dst = skia_safe::Rect::from_xywh(-draw_w * 0.5, -draw_h * 0.5, draw_w, draw_h);
        canvas.draw_image_rect(&img, None, dst, &paint);
        canvas.restore();
    }

    /// Overwrite the whole surface with top-down premul BGRA pixels.
    /// Used to seed the hot map path with its cached static track.
    pub fn write_bgra(&mut self, bgra: &[u8]) -> bool {
        let row_bytes = (self.width as usize) * 4;
        if bgra.len() < row_bytes.saturating_mul(self.height as usize) {
            return false;
        }
        let info = ImageInfo::new(
            (self.width, self.height),
            ColorType::BGRA8888,
            AlphaType::Premul,
            None,
        );
        self.surface
            .canvas()
            .write_pixels(&info, bgra, row_bytes, (0, 0))
    }

    /// Snapshot as top-down premul BGRA (matches `layered::present_bgra`).
    pub fn to_bgra(&mut self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.read_bgra_into(&mut buf);
        buf
    }

    /// `to_bgra` that reuses `out`'s allocation (per-frame hot path).
    pub fn read_bgra_into(&mut self, out: &mut Vec<u8>) {
        let image = self.surface.image_snapshot();
        let info = ImageInfo::new(
            (self.width, self.height),
            ColorType::BGRA8888,
            AlphaType::Premul,
            None,
        );
        let row_bytes = (self.width as usize) * 4;
        let need = row_bytes * self.height as usize;
        out.clear();
        out.resize(need, 0);
        let buf = out;
        if let Some(pixmap) = image.peek_pixels() {
            let src = pixmap.bytes().unwrap_or(&[]);
            let src_rb = pixmap.row_bytes();
            for y in 0..self.height as usize {
                let s = y * src_rb;
                let d = y * row_bytes;
                let n = row_bytes.min(src.len().saturating_sub(s));
                if n > 0 {
                    buf[d..d + n].copy_from_slice(&src[s..s + n]);
                }
            }
        } else {
            let _ = image.read_pixels(
                &info,
                buf,
                row_bytes,
                (0, 0),
                skia_safe::image::CachingHint::Allow,
            );
        }
        // Near-black alpha punch (parity with layered GL path).
        for px in buf.chunks_exact_mut(4) {
            if px[0] == 0 && px[1] == 0 && px[2] == 0 && px[3] < 8 {
                px[3] = 0;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}
