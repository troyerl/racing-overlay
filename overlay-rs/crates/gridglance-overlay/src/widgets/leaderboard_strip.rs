use super::WidgetCtx;
use crate::chrome::{draw_card, full_rect, label, panel_pad};
use egui::{Align2, Pos2, Ui};

const SECTION: &str = "leaderboard_strip";

pub fn paint(ui: &mut Ui, ctx: &mut WidgetCtx<'_>) {
    let rect = full_rect(ui);
    draw_card(ui, ctx.cfg, SECTION, rect);
    let pad = panel_pad(rect.height());
    let text = ctx.cfg.color(SECTION, "text", "#f4f6f8");
    let muted = ctx.cfg.color(SECTION, "muted", "#8b93a1");
    let player = ctx.cfg.color(SECTION, "player_row", "#ff941670");

    let cars = &ctx.frame.cars;
    let n = cars.len().min(10);
    if n == 0 {
        label(
            ui,
            rect.center(),
            Align2::CENTER_CENTER,
            "—",
            16.0,
            muted,
            false,
        );
        return;
    }
    let cell_w = (rect.width() - 2.0 * pad) / n as f32;
    for (i, car) in cars.iter().take(n).enumerate() {
        let cell = egui::Rect::from_min_size(
            Pos2::new(rect.left() + pad + i as f32 * cell_w, rect.top() + pad),
            egui::vec2(cell_w - 4.0, rect.height() - 2.0 * pad),
        );
        if car.is_player {
            ui.painter()
                .rect_filled(cell, egui::CornerRadius::same(6), player);
        }
        label(
            ui,
            Pos2::new(cell.center().x, cell.top() + cell.height() * 0.35),
            Align2::CENTER_CENTER,
            &format!("P{}", car.position),
            cell.height() * 0.28,
            muted,
            true,
        );
        label(
            ui,
            Pos2::new(cell.center().x, cell.top() + cell.height() * 0.68),
            Align2::CENTER_CENTER,
            &car.car_number,
            cell.height() * 0.32,
            text,
            true,
        );
    }
}
