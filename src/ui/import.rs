use eframe::egui::Ui;
use rfd::FileDialog;

use crate::app::MigratorApp;

pub fn render(ui: &mut Ui, app: &mut MigratorApp) {
    ui.add_enabled_ui(!app.is_busy(), |ui| {
        ui.label("目标 Codex 数据目录");
        ui.text_edit_singleline(&mut app.codex_home_input);

        ui.separator();
        ui.label("迁移包");
        ui.horizontal(|ui| {
            ui.text_edit_singleline(&mut app.import_package_input);
            if ui.button("浏览").clicked() {
                if let Some(path) = FileDialog::new()
                    .add_filter("Codex 历史迁移包", &["codexhist"])
                    .pick_file()
                {
                    app.import_package_input = path.to_string_lossy().to_string();
                }
            }
        });

        ui.checkbox(&mut app.create_backup_on_import, "导入前先创建目标备份");

        if ui.button("开始导入").clicked() {
            app.run_import();
        }
    });

    if let Some(title) = app.running_task_title() {
        ui.small(format!("{title}执行中，导入参数已暂时锁定。"));
    }

    if let Some(report) = &app.last_import_report {
        ui.separator();
        ui.label(format!("新增线程：{}", report.inserted_threads));
        ui.label(format!("更新线程：{}", report.updated_threads));
        ui.label(format!("跳过线程：{}", report.skipped_threads));
        ui.label(format!("修复路径：{}", report.repaired_paths));
    }
}
