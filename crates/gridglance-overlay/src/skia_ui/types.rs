#![allow(dead_code)]

//! Neutral paint types (no egui).

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::new(r, g, b, 255)
    }

    pub const TRANSPARENT: Self = Self::new(0, 0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const BLACK: Self = Self::rgb(0, 0, 0);

    pub fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    pub fn mul_alpha(self, factor: f32) -> Self {
        let a = ((self.a as f32) * factor.clamp(0.0, 1.0)).round() as u8;
        self.with_alpha(a)
    }

    /// Premultiplied BGRA bytes for layered present.
    pub fn to_premul_bgra(self) -> [u8; 4] {
        let af = self.a as f32 / 255.0;
        [
            (self.b as f32 * af).round() as u8,
            (self.g as f32 * af).round() as u8,
            (self.r as f32 * af).round() as u8,
            self.a,
        ]
    }

    pub fn from_egui(c: egui::Color32) -> Self {
        // egui Color32 is premultiplied; un-premultiply for Skia Color4f paints.
        let a = c.a();
        if a == 0 {
            return Self::TRANSPARENT;
        }
        if a == 255 {
            return Self::rgb(c.r(), c.g(), c.b());
        }
        let af = a as f32;
        Self::new(
            ((c.r() as f32) * 255.0 / af).round().clamp(0.0, 255.0) as u8,
            ((c.g() as f32) * 255.0 / af).round().clamp(0.0, 255.0) as u8,
            ((c.b() as f32) * 255.0 / af).round().clamp(0.0, 255.0) as u8,
            a,
        )
    }

    pub fn parse(s: &str) -> Self {
        let t = s.trim().trim_start_matches('#');
        match t.len() {
            6 => {
                let n = u32::from_str_radix(t, 16).unwrap_or(0);
                Self::rgb(
                    ((n >> 16) & 0xff) as u8,
                    ((n >> 8) & 0xff) as u8,
                    (n & 0xff) as u8,
                )
            }
            8 => {
                let n = u32::from_str_radix(t, 16).unwrap_or(0);
                Self::new(
                    ((n >> 24) & 0xff) as u8,
                    ((n >> 16) & 0xff) as u8,
                    ((n >> 8) & 0xff) as u8,
                    (n & 0xff) as u8,
                )
            }
            _ => Self::rgb(244, 246, 248),
        }
    }

    pub fn luminance(self) -> f32 {
        (0.299 * self.r as f32 + 0.587 * self.g as f32 + 0.114 * self.b as f32) / 255.0
    }

    pub fn contrast_text(self) -> Self {
        if self.luminance() > 0.6 {
            Self::rgb(20, 22, 26)
        } else {
            Self::WHITE
        }
    }

    pub fn lerp(self, other: Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Self::new(
            mix(self.r, other.r),
            mix(self.g, other.g),
            mix(self.b, other.b),
            mix(self.a, other.a),
        )
    }

    pub fn soften(self, toward: Self, mix: f32) -> Self {
        self.lerp(toward, mix.clamp(0.0, 1.0))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_ltrb(l: f32, t: f32, r: f32, b: f32) -> Self {
        Self {
            x: l,
            y: t,
            w: (r - l).max(0.0),
            h: (b - t).max(0.0),
        }
    }

    pub fn left(self) -> f32 {
        self.x
    }
    pub fn top(self) -> f32 {
        self.y
    }
    pub fn right(self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(self) -> f32 {
        self.y + self.h
    }
    pub fn width(self) -> f32 {
        self.w
    }
    pub fn height(self) -> f32 {
        self.h
    }
    pub fn center(self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn inset(self, dx: f32, dy: f32) -> Self {
        Self::from_xywh(
            self.x + dx,
            self.y + dy,
            (self.w - 2.0 * dx).max(0.0),
            (self.h - 2.0 * dy).max(0.0),
        )
    }

    pub fn contains(self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.right() && py < self.bottom()
    }

    pub fn to_skia(self) -> skia_safe::Rect {
        skia_safe::Rect::from_xywh(self.x, self.y, self.w, self.h)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FontSpec {
    pub size: f32,
    pub bold: bool,
}

impl FontSpec {
    pub fn new(size: f32) -> Self {
        Self { size, bold: false }
    }

    pub fn bold(size: f32) -> Self {
        Self { size, bold: true }
    }
}
