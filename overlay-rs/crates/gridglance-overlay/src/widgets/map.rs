use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label};
use crate::track_path;
use egui::{Align2, Pos2, Rect, Sense, Stroke, Ui};

const SECTION: &str = "map";

/// Aspect-preserving map from path bounds → plot pixels.
struct PlotXform {
    origin: Pos2,
    scale: f32,
    min: (f32, f32),
}

impl PlotXform {
    fn fit(plot: Rect, pts: &[(f32, f32)]) -> Self {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for &(x, y) in pts {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let w = (max_x - min_x).max(1e-6);
        let h = (max_y - min_y).max(1e-6);
        let pad = 0.06_f32;
        let avail_w = plot.width() * (1.0 - 2.0 * pad);
        let avail_h = plot.height() * (1.0 - 2.0 * pad);
        let scale = (avail_w / w).min(avail_h / h);
        let drawn_w = w * scale;
        let drawn_h = h * scale;
        let origin = Pos2::new(
            plot.center().x - drawn_w * 0.5,
            plot.center().y - drawn_h * 0.5,
        );
        Self {
            origin,
            scale,
            min: (min_x, min_y),
        }
    }

    fn map(&self, x: f32, y: f32) -> Pos2 {
        Pos2::new(
            self.origin.x + (x - self.min.0) * self.scale,
            self.origin.y + (y - self.min.1) * self.scale,
        )
    }

    /// Inverse for pit-edit clicks (normalized against path bounds → raw coords).
    fn unmap(&self, p: Pos2) -> (f32, f32) {
        let x = self.min.0 + (p.x - self.origin.x) / self.scale;
        let y = self.min.1 + (p.y - self.origin.y) / self.scale;
        (x, y)
    }
}

fn ensure_path_cached(ctx: &mut WidgetCtx<'_>) {
    let tid = ctx.frame.track_id;
    if tid == ctx.map.cached_track_id && !ctx.map.cached_path.is_empty() {
        return;
    }
    ctx.map.cached_track_id = tid;
    ctx.map.cached_path.clear();
    ctx.map.cached_track_name.clear();
    if let Some(id) = tid {
        if let Some(tp) = track_path::load_for_track_id(id) {
            ctx.map.cached_track_name = tp.name.clone();
            ctx.map.cached_path = tp.points;
            return;
        }
    }
    ctx.map.cached_path = track_path::oval_path(64);
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    ensure_path_cached(ctx);

    let rect = full_rect(ui);
    if ctx.cfg.bool_key(SECTION, "show_panel", false) {
        draw_card(ui, ctx.cfg, SECTION, rect);
    }
    let pad = 12.0_f32;
    let plot = rect.shrink(pad);
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let track_col = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let accent = ctx.cfg.color(SECTION, "accent", "#70df7a");
    let player_col = ctx.cfg.color(SECTION, "faster", "#46df7a");
    let other = ctx.cfg.color(SECTION, "slower", "#ff9416");

    let path = ctx.map.cached_path.clone();
    let xform = PlotXform::fit(plot, &path);

    // Closed polyline
    let screen: Vec<Pos2> = path.iter().map(|&(x, y)| xform.map(x, y)).collect();
    if screen.len() >= 2 {
        let stroke = Stroke::new(3.0_f32, track_col.gamma_multiply(0.55));
        for w in screen.windows(2) {
            ui.painter().line_segment([w[0], w[1]], stroke);
        }
        // Close loop
        if let (Some(&a), Some(&b)) = (screen.last(), screen.first()) {
            ui.painter().line_segment([a, b], stroke);
        }
    }

    // Cars by LapDistPct along path
    for car in &ctx.frame.cars {
        let (nx, ny) = track_path::point_at(&path, car.lap_dist_pct);
        let p = xform.map(nx, ny);
        let col = if car.is_player {
            player_col
        } else if car.on_pit {
            muted
        } else {
            other
        };
        let r = if car.is_player { 7.0 } else { 5.0 };
        ui.painter().circle_filled(p, r, col);
    }

    // Authoring: pit points in path/normalized space (same as before for oval 0..1)
    let pit_col = accent;
    for (i, (nx, ny)) in ctx.map.pit_points.iter().enumerate() {
        let p = xform.map(*nx, *ny);
        ui.painter().circle_filled(p, 4.0, pit_col);
        if i > 0 {
            let (px, py) = ctx.map.pit_points[i - 1];
            let prev = xform.map(px, py);
            ui.painter()
                .line_segment([prev, p], Stroke::new(2.0_f32, pit_col));
        }
    }

    if ctx.map.pit_edit && ctx.map.interactive {
        let resp = ui.interact(plot, ui.id().with("map_pit"), Sense::click());
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let (nx, ny) = xform.unmap(pos);
                ctx.map.pit_points.push((nx, ny));
            }
        }
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            &format!("PIT EDIT ({})", ctx.map.phase),
            12.0,
            accent,
            true,
        );
    } else if ctx.map.corner_edit {
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            "CORNER EDIT",
            12.0,
            accent,
            true,
        );
    } else if ctx.map.sf_edit {
        label(
            ui,
            Pos2::new(plot.left() + 6.0, plot.top() + 6.0),
            Align2::LEFT_TOP,
            "S/F EDIT",
            12.0,
            accent,
            true,
        );
    }

    let title = if !ctx.map.cached_track_name.is_empty() {
        ctx.map.cached_track_name.as_str()
    } else {
        ctx.frame.track_name.as_deref().unwrap_or("MAP")
    };
    label(
        ui,
        Pos2::new(rect.center().x, rect.bottom() - 8.0),
        Align2::CENTER_BOTTOM,
        title,
        11.0,
        muted,
        true,
    );
}
