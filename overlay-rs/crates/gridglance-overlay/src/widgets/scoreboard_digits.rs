//! 7-segment scoreboard digits (Python `scoreboard_digits.py`).

use egui::{Color32, CornerRadius, Pos2, Rect, Ui, Vec2};

/// Segment masks a–g for digits 0–9.
const DIGITS: [[u8; 7]; 10] = [
    [1, 1, 1, 1, 1, 1, 0], // 0
    [0, 1, 1, 0, 0, 0, 0], // 1
    [1, 1, 0, 1, 1, 0, 1], // 2
    [1, 1, 1, 1, 0, 0, 1], // 3
    [0, 1, 1, 0, 0, 1, 1], // 4
    [1, 0, 1, 1, 0, 1, 1], // 5
    [1, 0, 1, 1, 1, 1, 1], // 6
    [1, 1, 1, 0, 0, 0, 0], // 7
    [1, 1, 1, 1, 1, 1, 1], // 8
    [1, 1, 1, 1, 0, 1, 1], // 9
];

// Horizontal segment endpoints as fractions (x0,y0,x1,y1): a, g, d.
const H_SEGS: [(f32, f32, f32, f32); 3] = [
    (0.12, 0.06, 0.88, 0.06),
    (0.12, 0.50, 0.88, 0.50),
    (0.12, 0.94, 0.88, 0.94),
];
// Vertical: f, b, e, c as (x, y0, y1).
const V_SEGS: [(f32, f32, f32); 4] = [
    (0.06, 0.10, 0.46),
    (0.94, 0.10, 0.46),
    (0.06, 0.54, 0.90),
    (0.94, 0.54, 0.90),
];

fn digit_size(digit_h: f32) -> (f32, f32) {
    (digit_h * 0.62, digit_h)
}

fn normalize_digits(text: &str) -> String {
    text.chars().filter(|c| c.is_ascii_digit()).collect()
}

#[allow(dead_code)]
pub fn scoreboard_text_width(text: &str, digit_h: f32, min_digits: usize) -> f32 {
    let digits = normalize_digits(text);
    let n = digits
        .len()
        .max(min_digits)
        .max(if digits.is_empty() { 0 } else { 1 });
    if n == 0 {
        return 0.0;
    }
    let (dw, _) = digit_size(digit_h);
    let gap = dw * 0.10;
    n as f32 * dw + (n.saturating_sub(1) as f32) * gap
}

fn bulb_count(length: f32, stroke: f32, horizontal: bool) -> usize {
    let pitch = stroke * if horizontal { 1.55 } else { 1.40 };
    let target = if horizontal { 7 } else { 4 };
    ((length / pitch) as usize).clamp(3, target)
}

fn draw_segment_bulbs(
    ui: &mut Ui,
    p0: Pos2,
    p1: Pos2,
    color: Color32,
    stroke: f32,
    horizontal: bool,
    glow: bool,
) {
    let dx = p1.x - p0.x;
    let dy = p1.y - p0.y;
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-3 {
        return;
    }
    let n = bulb_count(length, stroke, horizontal);
    let (bulb_w, bulb_h) = if horizontal {
        (
            (length / n as f32) * 0.62,
            stroke * if glow { 1.25 } else { 1.05 },
        )
    } else {
        (
            stroke * if glow { 1.02 } else { 0.82 },
            (length / n as f32) * 0.62,
        )
    };
    let radius = (bulb_w.min(bulb_h) * 0.38).clamp(1.0, 8.0) as u8;
    for i in 0..n {
        let t = (i as f32 + 0.5) / n as f32;
        let cx = p0.x + dx * t;
        let cy = p0.y + dy * t;
        let rect = Rect::from_center_size(Pos2::new(cx, cy), Vec2::new(bulb_w, bulb_h));
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius), color);
    }
}

fn draw_scoreboard_digit(ui: &mut Ui, x: f32, y: f32, w: f32, h: f32, ch: char, color: Color32) {
    let Some(d) = ch.to_digit(10) else {
        return;
    };
    let mask = DIGITS[d as usize];
    let stroke = (h * 0.11).max(1.4);
    let glow = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 80);
    let names = ["a", "b", "c", "d", "e", "f", "g"];
    for (seg_color, is_glow) in [(glow, true), (color, false)] {
        for (on, name) in mask.iter().zip(names.iter()) {
            if *on == 0 {
                continue;
            }
            let (p0, p1, horizontal) = match *name {
                "a" | "g" | "d" => {
                    let idx = match *name {
                        "a" => 0,
                        "g" => 1,
                        _ => 2,
                    };
                    let (x0, y0, x1, y1) = H_SEGS[idx];
                    (
                        Pos2::new(x + x0 * w, y + y0 * h),
                        Pos2::new(x + x1 * w, y + y1 * h),
                        true,
                    )
                }
                _ => {
                    let idx = match *name {
                        "f" => 0,
                        "b" => 1,
                        "e" => 2,
                        _ => 3,
                    };
                    let (sx, y0f, y1f) = V_SEGS[idx];
                    (
                        Pos2::new(x + sx * w, y + y0f * h),
                        Pos2::new(x + sx * w, y + y1f * h),
                        false,
                    )
                }
            };
            draw_segment_bulbs(ui, p0, p1, seg_color, stroke, horizontal, is_glow);
        }
    }
}

/// Draw digits right-aligned inside `rect` (IMS pylon car-number style).
pub fn draw_scoreboard_text(
    ui: &mut Ui,
    rect: Rect,
    text: &str,
    color: Color32,
    min_digits: usize,
) {
    let digits = normalize_digits(text);
    if digits.is_empty() && min_digits == 0 {
        return;
    }
    let digit_h = rect.height() * 0.88;
    let (dw, dh) = digit_size(digit_h);
    let gap = dw * 0.10;
    let n = digits.len().max(min_digits);
    let total_w = n as f32 * dw + (n.saturating_sub(1) as f32) * gap;
    let mut x = rect.right() - total_w;
    let y = rect.center().y - dh * 0.5;
    let padded = if digits.is_empty() {
        " ".repeat(n)
    } else {
        format!("{digits:>n$}")
    };
    for ch in padded.chars() {
        if ch == ' ' {
            x += dw + gap;
            continue;
        }
        draw_scoreboard_digit(ui, x, y, dw, dh, ch, color);
        x += dw + gap;
    }
}
