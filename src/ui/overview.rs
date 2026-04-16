use eframe::egui::{self, Ui};

use crate::app::MigratorApp;

pub fn render(ui: &mut Ui, app: &mut MigratorApp) {
    ui.add_enabled_ui(!app.is_busy(), |ui| {
        ui.label("Codex 数据目录");
        ui.text_edit_singleline(&mut app.codex_home_input);

        ui.horizontal(|ui| {
            if ui.button("扫描当前聊天记录").clicked() {
                app.run_scan();
            }
            if ui.button("切换到导出页").clicked() {
                app.active_tab = crate::app::ActiveTab::Export;
            }
            if ui.button("切换到导入页").clicked() {
                app.active_tab = crate::app::ActiveTab::Import;
            }
        });
    });

    if let Some(title) = app.running_task_title() {
        ui.small(format!("{title}执行中，当前页面操作已暂时禁用。"));
    }

    ui.separator();
    if let Some(scan) = &app.last_scan {
        egui::Grid::new("overview_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("线程数");
                ui.label(scan.threads.len().to_string());
                ui.end_row();

                ui.label("会话文件数");
                ui.label(scan.session_payloads.len().to_string());
                ui.end_row();

                ui.label("缺失引用数");
                ui.label(scan.missing_payloads.len().to_string());
                ui.end_row();

                ui.label("侧栏索引");
                ui.label(if scan.session_index_path.is_some() {
                    "存在"
                } else {
                    "缺失"
                });
                ui.end_row();

                ui.label("历史根路径");
                ui.label(&scan.source_root_prefix);
                ui.end_row();
            });
    } else {
        ui.label("还没有扫描结果。");
    }
}
