use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label};
use egui::{Align2, Pos2, Sense, Stroke, Ui};

const SECTION: &str = "map";

/// Simple oval track path in normalized 0..1 space.
fn track_point(t: f32) -> Pos2 {
    let a = t * std::f32::consts::TAU;
    Pos2::new(0.5 + 0.38 * a.cos(), 0.5 + 0.28 * a.sin())
}

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = 12.0_f32;
    let plot = rect.shrink(pad);
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let track_col = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let accent = ctx.cfg.color(SECTION, "accent", "#70df7a");
    let player_col = ctx.cfg.color(SECTION, "faster", "#46df7a");
    let other = ctx.cfg.color(SECTION, "slower", "#ff9416");

    // Track outline
    let mut points = Vec::with_capacity(65);
    for i in 0..=64 {
        let n = track_point(i as f32 / 64.0);
        points.push(Pos2::new(
            plot.left() + n.x * plot.width(),
            plot.top() + n.y * plot.height(),
        ));
    }
    for w in points.windows(2) {
        ui.painter()
            .line_segment([w[0], w[1]], Stroke::new(3.0, track_col.gamma_multiply(0.55)));
    }

    // Cars
    for car in &ctx.frame.cars {
        let n = track_point(car.lap_dist_pct.fract());
        let p = Pos2::new(
            plot.left() + n.x * plot.width(),
            plot.top() + n.y * plot.height(),
        );
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

    // Authoring: pit points + click to add when pit_edit + interactive
    let pit_col = accent;
    for (i, (nx, ny)) in ctx.map.pit_points.iter().enumerate() {
        let p = Pos2::new(
            plot.left() + *nx * plot.width(),
            plot.top() + *ny * plot.height(),
        );
        ui.painter().circle_filled(p, 4.0, pit_col);
        if i > 0 {
            let (px, py) = ctx.map.pit_points[i - 1];
            let prev = Pos2::new(
                plot.left() + px * plot.width(),
                plot.top() + py * plot.height(),
            );
            ui.painter()
                .line_segment([prev, p], Stroke::new(2.0, pit_col));
        }
    }

    if ctx.map.pit_edit && ctx.map.interactive {
        let resp = ui.interact(plot, ui.id().with("map_pit"), Sense::click());
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let nx = ((pos.x - plot.left()) / plot.width()).clamp(0.0, 1.0);
                let ny = ((pos.y - plot.top()) / plot.height()).clamp(0.0, 1.0);
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

    let title = ctx
        .frame
        .track_name
        .as_deref()
        .unwrap_or("MAP");
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
