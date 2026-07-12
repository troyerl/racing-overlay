use super::WidgetCtx;
use super::table::paint_table;

pub fn paint(ui: &mut egui::Ui, ctx: &mut WidgetCtx<'_>) {
    paint_table(
        ui,
        ctx.cfg,
        "relative",
        &ctx.cfg.str_key("relative", "title", "RELATIVE"),
        &ctx.frame.cars,
    );
}
