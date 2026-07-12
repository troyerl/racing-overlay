use super::WidgetCtx;
use super::table::paint_table;

pub fn paint(ui: &mut egui::Ui, ctx: &mut WidgetCtx<'_>) {
    paint_table(
        ui,
        ctx.cfg,
        "standings",
        &ctx.cfg.str_key("standings", "title", "STANDINGS"),
        &ctx.frame.cars,
    );
}
