//! Compatibility entry for the widget menu; canvas owns its rich in-place UI.
pub fn show(app: &mut crate::FigApp, ctx: &egui::Context) {
    let rect = app.canvas.rect;
    egui::Area::new(egui::Id::unique("widget_bar_compat")).fixed_pos(rect.min).show(ctx, |ui| crate::widget_ui::menu_bar(app, ui, rect));
}
