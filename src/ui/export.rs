use eframe::egui::Ui;
use rfd::FileDialog;

use crate::app::MigratorApp;

pub fn render(ui: &mut Ui, app: &mut MigratorApp) {
    ui.add_enabled_ui(!app.is_busy(), |ui| {
        ui.label("源 Codex 数据目录");
        ui.text_edit_singleline(&mut app.codex_home_input);

        ui.separator();
        ui.label("导出迁移包");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut app.export_output_input);
            if ui.button("浏览").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("Codex 历史迁移包", &["codexhist"])
                    .set_file_name("codex-history.codexhist")
                    .save_file()
                {
                    app.export_output_input = path.to_string_lossy().to_string();
                }
            }
        });

        ui.horizontal(|ui| {
            if ui.button("扫描").clicked() {
                app.run_scan();
            }
            if ui.button("开始导出").clicked() {
                app.run_export();
            }
        });
    });

    if let Some(title) = app.running_task_title() {
        ui.small(format!("{title}执行中，导出参数已暂时锁定。"));
    }

    if let Some(report) = &app.last_export_report {
        ui.separator();
        ui.label(format!("已导出线程：{}", report.thread_count));
        ui.label(format!("会话文件：{}", report.session_file_count));
        ui.label(format!("归档文件：{}", report.archived_file_count));
        ui.label(format!("缺失文件：{}", report.missing_file_count));
    }
}
