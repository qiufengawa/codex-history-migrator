use eframe::egui::{self, Ui};

use crate::app::MigratorApp;

pub fn render(ui: &mut Ui, app: &mut MigratorApp) {
    ui.heading("日志");
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for entry in &app.logs {
                ui.label(entry);
            }
        });
}
