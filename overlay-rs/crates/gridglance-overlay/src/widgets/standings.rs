use super::table::paint_table;
use super::WidgetCtx;

pub fn paint(ui: &mut egui::Ui, ctx: &mut WidgetCtx<'_>) {
    paint_table(
        ui,
        ctx.cfg,
        "standings",
        &ctx.frame.standings_cars,
        &ctx.frame.standings_slots,
        false,
    );
}
