//! Directional proximity radar HUD (Python `radar.py` parity).

use super::WidgetCtx;
use crate::chrome::{anim_dt, color_with_alpha, ease, full_rect, label, panel_card, still_easing};
use egui::{epaint::Vertex, Align2, Color32, Mesh, Pos2, Rect, Shape, Stroke, Ui, Vec2};

const SECTION: &str = "radar";

#[derive(Clone, Default)]
struct RadarAnim {
    left: f32,
    right: f32,
    ahead: f32,
    behind: f32,
    left_pos: f32,
    right_pos: f32,
    last_secs: f64,
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    if ctx.cfg.bool_key(SECTION, "show_panel", false) {
        panel_card(ui, ctx.cfg, SECTION, rect);
    }

    let w = rect.width();
    let h = rect.height();
    let cx = rect.center().x;
    let cy = rect.center().y;

    let car_w = (w * size_frac(ctx, "car_w", 0.36)).max(18.0);
    let car_h = (h * size_frac(ctx, "car_h", 0.48)).max(36.0);
    let kind = radar_car_kind(ctx.frame.car_path.as_deref());
    let (sprite_w, _) = car_sprite_fit(kind, car_w, car_h);
    let bar_h = car_h * size_frac(ctx, "bar_h", 0.78);
    let pad = (w * 0.032).max(7.0);
    let inner = (sprite_w * 0.5 + pad).min(w * 0.38);
    let nose_len = h * size_frac(ctx, "nose_len", 0.16);
    let glow_w = w * size_frac(ctx, "glow_w", 0.17);

    let d = &ctx.frame.radar;
    let show_front = ctx.cfg.bool_key(SECTION, "show_front", true);
    let show_rear = ctx.cfg.bool_key(SECTION, "show_rear", true);
    let side_tau = ctx.cfg.f64_key(SECTION, "ease_side_tau", 0.10) as f32;
    let glow_tau = ctx.cfg.f64_key(SECTION, "ease_glow_tau", 0.13) as f32;
    let prox = ctx.cfg.bool_key(SECTION, "side_proximity_color", true);

    let id = egui::Id::new("radar_anim");
    let mut a = ui
        .ctx()
        .data_mut(|data| data.get_temp::<RadarAnim>(id).unwrap_or_default());
    let dt = anim_dt(ctx.mono_secs, &mut a.last_secs);

    let t_left = if d.left { 1.0 } else { 0.0 };
    let t_right = if d.right { 1.0 } else { 0.0 };
    let t_ahead = if show_front {
        d.ahead.unwrap_or(0.0)
    } else {
        0.0
    };
    let t_behind = if show_rear {
        d.behind.unwrap_or(0.0)
    } else {
        0.0
    };
    a.left = ease(a.left, t_left, dt, side_tau);
    a.right = ease(a.right, t_right, dt, side_tau);
    a.ahead = ease(a.ahead, t_ahead, dt, glow_tau);
    a.behind = ease(a.behind, t_behind, dt, glow_tau);
    a.left_pos = ease(a.left_pos, d.left_pos, dt, side_tau);
    a.right_pos = ease(a.right_pos, d.right_pos, dt, side_tau);

    let animating = still_easing(a.left, t_left, 0.01)
        || still_easing(a.right, t_right, 0.01)
        || still_easing(a.ahead, t_ahead, 0.01)
        || still_easing(a.behind, t_behind, 0.01)
        || still_easing(a.left_pos, d.left_pos, 0.01)
        || still_easing(a.right_pos, d.right_pos, 0.01);
    *ctx.panel_animating = animating;
    if animating {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(1));
    }
    ui.ctx().data_mut(|data| data.insert_temp(id, a.clone()));

    if show_front && a.ahead > 0.01 {
        v_glow(
            ui,
            ctx,
            cx,
            cy - car_h * 0.45,
            rect.top() + h * 0.06,
            a.ahead,
            glow_w,
            true,
        );
    }
    if show_rear && a.behind > 0.01 {
        v_glow(
            ui,
            ctx,
            cx,
            cy + car_h * 0.45,
            rect.top() + h * 0.94,
            a.behind,
            glow_w,
            false,
        );
    }

    let marker_h = bar_h.max(18.0);
    let travel = (h * 0.5 - marker_h * 0.5 - h * 0.06).max(0.0);

    if a.left > 0.01 {
        let yc = cy - a.left_pos * travel;
        side_marker(
            ui,
            ctx,
            rect.left() + w * 0.07,
            cx - inner,
            yc,
            marker_h,
            d.left2,
            true,
            a.left,
            if prox {
                Some(1.0 - a.left_pos.abs())
            } else {
                None
            },
            &d.left_label,
        );
    }
    if a.right > 0.01 {
        let yc = cy - a.right_pos * travel;
        side_marker(
            ui,
            ctx,
            cx + inner,
            rect.left() + w * 0.93,
            yc,
            marker_h,
            d.right2,
            false,
            a.right,
            if prox {
                Some(1.0 - a.right_pos.abs())
            } else {
                None
            },
            &d.right_label,
        );
    }

    if ctx.cfg.bool_key(SECTION, "show_clear_timer", false) {
        if let Some(secs) = d.clear_secs {
            if secs >= 0.0 {
                let txt = format!("Clear {secs:.0}s");
                label(
                    ui,
                    Pos2::new(cx, rect.bottom() - h * 0.08),
                    Align2::CENTER_CENTER,
                    &txt,
                    10.0,
                    ctx.cfg.color(SECTION, "nose", "#f4f6f8"),
                    true,
                );
            }
        }
    }

    if ctx.cfg.bool_key(SECTION, "show_axis", true) {
        let axis = ctx.cfg.color(SECTION, "axis", "#ffffff28");
        let sw = (w * 0.006).max(1.0);
        ui.painter().line_segment(
            [
                Pos2::new(rect.left() + w * 0.08, cy),
                Pos2::new(rect.left() + w * 0.92, cy),
            ],
            Stroke::new(sw, axis),
        );
        ui.painter().line_segment(
            [
                Pos2::new(cx, rect.top() + h * 0.10),
                Pos2::new(cx, rect.top() + h * 0.90),
            ],
            Stroke::new(sw, axis),
        );
    }
    if ctx.cfg.bool_key(SECTION, "show_nose", true) {
        let nose = ctx.cfg.color(SECTION, "nose", "#f4f6f8");
        let tip_y = cy - car_h * 0.50;
        ui.painter().line_segment(
            [Pos2::new(cx, tip_y), Pos2::new(cx, tip_y - nose_len)],
            Stroke::new((w * 0.012).max(1.5), nose),
        );
    }

    paint_car_sprite(ui, Pos2::new(cx, cy), car_w, car_h, kind);
}

const CAR_SPRITE_MAX_SIDE: u32 = 512;

/// Top-down radar body family, inferred from iRacing `CarPath`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RadarCarKind {
    Formula,
    Prototype,
    Gt,
    Touring,
    Stock,
    Truck,
    Sprint,
}

impl RadarCarKind {
    fn png_bytes(self) -> &'static [u8] {
        match self {
            Self::Formula => include_bytes!("../../../../assets/cars/formula.png"),
            Self::Prototype => include_bytes!("../../../../assets/cars/prototype.png"),
            Self::Gt => include_bytes!("../../../../assets/cars/gt.png"),
            Self::Touring => include_bytes!("../../../../assets/cars/touring.png"),
            Self::Stock => include_bytes!("../../../../assets/cars/nascar.png"),
            Self::Truck => include_bytes!("../../../../assets/cars/truck.png"),
            Self::Sprint => include_bytes!("../../../../assets/cars/sprint.png"),
        }
    }

    /// GT and sprint art is authored nose-down; rotate so ahead is up.
    pub(crate) fn sprite_rot_deg(self) -> f32 {
        match self {
            Self::Gt | Self::Sprint => 180.0,
            _ => 0.0,
        }
    }
}

fn decode_car_png(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut img = image::load_from_memory(bytes).ok()?.into_rgba8();
    let long = img.width().max(img.height());
    if long > CAR_SPRITE_MAX_SIDE {
        let scale = CAR_SPRITE_MAX_SIDE as f32 / long as f32;
        let nw = (img.width() as f32 * scale).round().max(1.0) as u32;
        let nh = (img.height() as f32 * scale).round().max(1.0) as u32;
        img = image::imageops::resize(&img, nw, nh, image::imageops::FilterType::Triangle);
    }
    let (w, h) = img.dimensions();
    Some((w, h, img.into_raw()))
}

/// Cached RGBA8 (unpremultiplied), downscaled for the HUD. Art is nose-up.
pub(crate) fn car_sprite_rgba(kind: RadarCarKind) -> Option<&'static (u32, u32, Vec<u8>)> {
    use std::sync::OnceLock;
    type SpriteRgba = (u32, u32, Vec<u8>);
    static CACHE: OnceLock<[OnceLock<Option<SpriteRgba>>; 7]> = OnceLock::new();
    let slots = CACHE.get_or_init(|| std::array::from_fn(|_| OnceLock::new()));
    slots[kind as usize]
        .get_or_init(|| decode_car_png(kind.png_bytes()))
        .as_ref()
}

/// Drawn size of the sprite contain-fit into the radar car box.
pub(crate) fn car_sprite_fit(kind: RadarCarKind, box_w: f32, box_h: f32) -> (f32, f32) {
    let Some((iw, ih, _)) = car_sprite_rgba(kind) else {
        return (box_w, box_h);
    };
    let scale = (box_w / *iw as f32).min(box_h / *ih as f32);
    (*iw as f32 * scale, *ih as f32 * scale)
}

pub(crate) fn radar_car_kind(car_path: Option<&str>) -> RadarCarKind {
    let p = car_path.unwrap_or("").to_ascii_lowercase();
    let has = |n: &str| p.contains(n);
    if has("sprint") || has("midget") || has("winged") {
        RadarCarKind::Sprint
    } else if has("truck") || has("silverado") || has("tundra") || has("f150") || has("f-150") {
        RadarCarKind::Truck
    } else if has("nascar")
        || has("nextgen")
        || has("next gen")
        || has("stockcar")
        || has("latemodel")
        || has("late model")
        || has("streetstock")
        || has("street stock")
        || has("arca")
        || has("modified")
        || has("legends")
    {
        RadarCarKind::Stock
    } else if has("ir18") || has("dw12") || has("indycar") || has("indy car") {
        RadarCarKind::Formula
    } else if has("lmp")
        || has("gtp")
        || has("prototype")
        || has("arx")
        || has("jsp")
        || has("p217")
        || has("963")
        || has("vseries")
        || has("v-series")
        || has("hybrid v8")
        || has("lmh")
        || has("lm dh")
    {
        RadarCarKind::Prototype
    } else if has("formula")
        || has("formulavee")
        || has("usf")
        || has("ir-04")
        || has("ir04")
        || has("ir-01")
        || has("ir01")
        || has("sf23")
        || has("fw31")
        || has("mp4")
        || p.contains("f3")
        || p.contains("f4")
        || has("superformula")
        || has("super formula")
        || has("ff1600")
        || has("formula ford")
    {
        RadarCarKind::Formula
    } else if has("gt3") || has("gt4") || has("gte") || has("gtd") || has("gt1") {
        RadarCarKind::Gt
    } else if has("mx5")
        || has("mx-5")
        || has("gr86")
        || has("tcr")
        || has("touring")
        || has("civic")
        || has("globalmazda")
    {
        RadarCarKind::Touring
    } else {
        RadarCarKind::Gt
    }
}

fn radar_car_texture(ui: &mut Ui, kind: RadarCarKind) -> Option<egui::TextureHandle> {
    let id = egui::Id::new(("radar_car_tex", kind as u8));
    if let Some(tex) = ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(id)) {
        return Some(tex);
    }
    let (w, h, rgba) = car_sprite_rgba(kind)?;
    let color = egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], rgba);
    let tex = ui.ctx().load_texture(
        format!("radar_car_{}", kind as u8),
        color,
        egui::TextureOptions::LINEAR,
    );
    ui.ctx().data_mut(|d| d.insert_temp(id, tex.clone()));
    Some(tex)
}

/// Source PNGs are portrait, nose up. Contain-fit into the radar car box.
fn paint_car_sprite(ui: &mut Ui, c: Pos2, box_w: f32, box_h: f32, kind: RadarCarKind) {
    let Some(tex) = radar_car_texture(ui, kind) else {
        return;
    };
    let size = tex.size_vec2();
    if size.x <= 0.0 || size.y <= 0.0 || box_w <= 0.0 || box_h <= 0.0 {
        return;
    }
    let scale = (box_w / size.x).min(box_h / size.y);
    let vis = Vec2::new(size.x * scale, size.y * scale);
    let rect = Rect::from_center_size(c, vis);
    let mut mesh = Mesh::with_texture(tex.id());
    let white = Color32::WHITE;
    let uvs = if kind.sprite_rot_deg() == 180.0 {
        [
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
        ]
    } else {
        [
            Pos2::new(0.0, 0.0),
            Pos2::new(1.0, 0.0),
            Pos2::new(1.0, 1.0),
            Pos2::new(0.0, 1.0),
        ]
    };
    mesh.vertices.extend_from_slice(&[
        Vertex {
            pos: rect.left_top(),
            uv: uvs[0],
            color: white,
        },
        Vertex {
            pos: rect.right_top(),
            uv: uvs[1],
            color: white,
        },
        Vertex {
            pos: rect.right_bottom(),
            uv: uvs[2],
            color: white,
        },
        Vertex {
            pos: rect.left_bottom(),
            uv: uvs[3],
            color: white,
        },
    ]);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(Shape::mesh(mesh));
}

fn size_frac(ctx: &WidgetCtx<'_>, key: &str, default: f32) -> f32 {
    let v = ctx
        .cfg
        .section(SECTION)
        .get("sizes")
        .and_then(|s| s.get(key))
        .and_then(|v| v.as_f64())
        .map(|v| v as f32)
        .unwrap_or(default);
    match key {
        "car_w" if (v - 0.20).abs() < 1e-4 || (v - 0.13).abs() < 1e-4 => default,
        "car_h" if (v - 0.26).abs() < 1e-4 || (v - 0.20).abs() < 1e-4 => default,
        _ => v,
    }
}

fn unpremultiply(c: Color32) -> (u8, u8, u8) {
    let a = c.a();
    if a == 0 {
        return (0, 0, 0);
    }
    (
        (c.r() as u16 * 255 / a as u16).min(255) as u8,
        (c.g() as u16 * 255 / a as u16).min(255) as u8,
        (c.b() as u16 * 255 / a as u16).min(255) as u8,
    )
}

fn punch_alert(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    // Push hue toward neon so saved muted reds/ambers still pop off-axis.
    let max = r.max(g).max(b).max(1) as f32;
    let scale = 255.0 / max;
    (
        (r as f32 * scale).round().min(255.0) as u8,
        (g as f32 * scale).round().min(255.0) as u8,
        (b as f32 * scale).round().min(255.0) as u8,
    )
}

fn prox_color(ctx: &WidgetCtx<'_>, closeness: f32, alpha: u8) -> Color32 {
    let c = closeness.clamp(0.0, 1.0);
    let (yr, yg, yb) = punch_alert_unpremul(ctx.cfg.color(SECTION, "yellow", "#ffe033"));
    let (rr, rg, rb) = punch_alert_unpremul(ctx.cfg.color(SECTION, "red", "#ff2424"));
    Color32::from_rgba_unmultiplied(
        (yr as f32 + (rr as f32 - yr as f32) * c) as u8,
        (yg as f32 + (rg as f32 - yg as f32) * c) as u8,
        (yb as f32 + (rb as f32 - yb as f32) * c) as u8,
        alpha,
    )
}

fn punch_alert_unpremul(c: Color32) -> (u8, u8, u8) {
    let (r, g, b) = unpremultiply(c);
    punch_alert(r, g, b)
}

fn alert_red(ctx: &WidgetCtx<'_>) -> Color32 {
    let (r, g, b) = punch_alert_unpremul(ctx.cfg.color(SECTION, "red", "#ff2424"));
    Color32::from_rgba_unmultiplied(r, g, b, 255)
}

/// Tent map 0→1→0 across [0, 1] (Python `_feather_mask` edge dissolve).
fn tent(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t <= 0.5 {
        (t * 2.0).clamp(0.0, 1.0)
    } else {
        ((1.0 - t) * 2.0).clamp(0.0, 1.0)
    }
}

fn side_marker(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    x0: f32,
    x1: f32,
    yc: f32,
    marker_h: f32,
    strong: bool,
    to_left: bool,
    opacity: f32,
    closeness: Option<f32>,
    label_txt: &str,
) {
    let left = x0.min(x1);
    let right = x0.max(x1);
    let w = (right - left).max(1.0);
    let h = marker_h.max(1.0);
    let peak = ((if strong { 255.0 } else { 230.0 }) * opacity.clamp(0.0, 1.0)) as u8;
    let base = if let Some(c) = closeness {
        prox_color(ctx, c, 255)
    } else {
        alert_red(ctx)
    };
    let core = Rect::from_min_max(
        Pos2::new(left, yc - h * 0.5),
        Pos2::new(right, yc + h * 0.5),
    );
    let bloom_h = h * 1.7;
    let extra_w = w * 0.09;
    let bloom = Rect::from_min_max(
        Pos2::new(left - extra_w, yc - bloom_h * 0.5),
        Pos2::new(right + extra_w, yc + bloom_h * 0.5),
    );
    paint_feather_mesh(
        ui,
        bloom,
        base,
        (peak as f32 * 0.55) as u8,
        FeatherKind::HorizontalTowardCar { to_left },
    );
    // Smooth tent × linear fade via vertex-colored mesh (GPU interpolates).
    paint_feather_mesh(
        ui,
        core,
        base,
        peak,
        FeatherKind::HorizontalTowardCar { to_left },
    );
    if !label_txt.is_empty() {
        label(
            ui,
            Pos2::new((left + right) * 0.5, yc),
            Align2::CENTER_CENTER,
            label_txt,
            (w.min(h) * 0.38).max(6.0),
            Color32::WHITE,
            true,
        );
    }
}

fn v_glow(
    ui: &mut Ui,
    ctx: &WidgetCtx<'_>,
    cx: f32,
    y_inner: f32,
    y_outer: f32,
    closeness: f32,
    half_w: f32,
    _up: bool,
) {
    let top = y_inner.min(y_outer);
    let bottom = y_inner.max(y_outer);
    let peak = (170.0 + 85.0 * closeness.clamp(0.0, 1.0)) as u8;
    let base = prox_color(ctx, closeness, 255);
    let kind = FeatherKind::VerticalFromInner {
        opaque_at_top: y_inner <= y_outer,
    };
    paint_feather_mesh(
        ui,
        Rect::from_min_max(
            Pos2::new(cx - half_w * 1.85, top),
            Pos2::new(cx + half_w * 1.85, bottom),
        ),
        base,
        (peak as f32 * 0.50) as u8,
        kind,
    );
    paint_feather_mesh(
        ui,
        Rect::from_min_max(Pos2::new(cx - half_w, top), Pos2::new(cx + half_w, bottom)),
        base,
        peak,
        kind,
    );
}

#[derive(Clone, Copy)]
enum FeatherKind {
    VerticalFromInner { opaque_at_top: bool },
    HorizontalTowardCar { to_left: bool },
}

/// Soft glow mesh: tent × linear fade with enough verts for smooth interpolation.
fn paint_feather_mesh(ui: &mut Ui, rect: Rect, base: Color32, peak: u8, kind: FeatherKind) {
    if rect.width() < 0.5 || rect.height() < 0.5 || peak < 2 {
        return;
    }
    // Odd counts so a column/row sits on the tent peak (t=0.5).
    let (nx, ny) = match kind {
        FeatherKind::VerticalFromInner { .. } => (17usize, 9usize),
        FeatherKind::HorizontalTowardCar { .. } => (9usize, 17usize),
    };
    let mut mesh = Mesh::default();
    for iy in 0..ny {
        let ty = iy as f32 / (ny - 1) as f32;
        for ix in 0..nx {
            let tx = ix as f32 / (nx - 1) as f32;
            let a_mul = match kind {
                FeatherKind::VerticalFromInner { opaque_at_top } => {
                    let a_h = tent(tx);
                    let a_v = if opaque_at_top { 1.0 - ty } else { ty };
                    a_h * a_v
                }
                FeatherKind::HorizontalTowardCar { to_left } => {
                    let a_v = tent(ty);
                    let a_h = if to_left { 1.0 - tx } else { tx };
                    a_h * a_v
                }
            };
            let a = (peak as f32 * a_mul) as u8;
            mesh.vertices.push(Vertex {
                pos: Pos2::new(
                    rect.left() + rect.width() * tx,
                    rect.top() + rect.height() * ty,
                ),
                uv: egui::epaint::WHITE_UV,
                color: color_with_alpha(base, a),
            });
        }
    }
    for iy in 0..(ny - 1) {
        for ix in 0..(nx - 1) {
            let i = (iy * nx + ix) as u32;
            let nx_u = nx as u32;
            mesh.indices
                .extend_from_slice(&[i, i + 1, i + nx_u, i + 1, i + 1 + nx_u, i + nx_u]);
        }
    }
    ui.painter().add(Shape::mesh(mesh));
}

#[cfg(test)]
mod tests {
    use super::{car_sprite_rgba, radar_car_kind, RadarCarKind};

    #[test]
    fn car_pngs_decode_portrait() {
        for kind in [
            RadarCarKind::Formula,
            RadarCarKind::Prototype,
            RadarCarKind::Gt,
            RadarCarKind::Touring,
            RadarCarKind::Stock,
            RadarCarKind::Truck,
            RadarCarKind::Sprint,
        ] {
            let (w, h, rgba) = car_sprite_rgba(kind).unwrap_or_else(|| panic!("{kind:?} png"));
            assert!(*w > 8 && *h > 8, "{kind:?}");
            assert_eq!(rgba.len(), (*w as usize) * (*h as usize) * 4, "{kind:?}");
            assert!(*h > *w, "{kind:?} should be nose-up portrait");
        }
    }

    #[test]
    fn classifies_session_car_paths() {
        assert_eq!(radar_car_kind(Some("dallara f3")), RadarCarKind::Formula);
        assert_eq!(radar_car_kind(Some("dallarair18")), RadarCarKind::Formula);
        assert_eq!(radar_car_kind(Some("ferrari 296 gt3")), RadarCarKind::Gt);
        assert_eq!(radar_car_kind(Some("mx5 cup")), RadarCarKind::Touring);
        assert_eq!(
            radar_car_kind(Some("nASCAR cup series next gen chevrolet camaro z l 1")),
            RadarCarKind::Stock
        );
        assert_eq!(
            radar_car_kind(Some("stockcars fordmustang2022")),
            RadarCarKind::Stock
        );
        assert_eq!(
            radar_car_kind(Some("stockcars2 chevroletcamaro2018")),
            RadarCarKind::Stock
        );
        assert_eq!(
            radar_car_kind(Some("porsche 963 gtp")),
            RadarCarKind::Prototype
        );
        assert_eq!(
            radar_car_kind(Some("dirt sprint car")),
            RadarCarKind::Sprint
        );
        assert_eq!(radar_car_kind(Some("nascar truck")), RadarCarKind::Truck);
        assert_eq!(radar_car_kind(None), RadarCarKind::Gt);
    }
}
