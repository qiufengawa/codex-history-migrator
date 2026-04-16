use eframe::egui::{self, Ui};

use crate::app::MigratorApp;
use crate::models::provider_count::ProviderCount;

pub fn render(ui: &mut Ui, app: &mut MigratorApp) {
    ui.add_enabled_ui(!app.is_busy(), |ui| {
        ui.label("Codex 数据目录");
        ui.text_edit_singleline(&mut app.codex_home_input);

        ui.separator();
        ui.checkbox(
            &mut app.create_backup_on_provider_sync,
            "同步到当前 Provider 前先创建备份",
        );
        ui.checkbox(
            &mut app.create_safety_backup_on_restore,
            "恢复备份前先创建安全备份",
        );

        ui.horizontal(|ui| {
            if ui.button("检查当前 Provider").clicked() {
                app.run_provider_sync_status();
            }
            if ui.button("一键同步到当前 Provider").clicked() {
                app.run_provider_sync();
            }
            if ui.button("恢复最近同步备份").clicked() {
                app.run_restore_latest_provider_backup();
            }
        });
    });

    if let Some(title) = app.running_task_title() {
        ui.small(format!("{title}执行中，同步相关设置已暂时锁定。"));
    }

    ui.separator();

    if let Some(status) = &app.last_provider_sync_status {
        egui::Grid::new("provider_sync_status_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("当前 Provider");
                ui.label(&status.current_provider);
                ui.end_row();

                ui.label("当前模型");
                ui.label(status.current_model.as_deref().unwrap_or("未配置"));
                ui.end_row();

                ui.label("线程总数");
                ui.label(status.total_threads.to_string());
                ui.end_row();

                ui.label("可重挂线程");
                ui.label(status.movable_threads.to_string());
                ui.end_row();

                ui.label("同步备份数");
                ui.label(status.backup_count.to_string());
                ui.end_row();

                ui.label("最近备份");
                ui.label(
                    status
                        .latest_backup_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string())
                        .unwrap_or_else(|| "暂无".to_string()),
                );
                ui.end_row();
            });

        ui.separator();
        ui.label("Provider 线程分布");
        render_provider_counts(ui, "provider_count_grid", &status.provider_counts);
    } else {
        ui.label("还没有检查结果。");
    }

    if let Some(report) = &app.last_provider_sync_report {
        ui.separator();
        ui.label("最近一次同步结果");
        egui::Grid::new("provider_sync_report_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("目标 Provider");
                ui.label(&report.current_provider);
                ui.end_row();

                ui.label("更新线程数");
                ui.label(report.updated_threads.to_string());
                ui.end_row();

                ui.label("同步前备份");
                ui.label(
                    report
                        .backup_path
                        .as_ref()
                        .map(|path| path.to_string_lossy().to_string())
                        .unwrap_or_else(|| "未创建（已关闭备份）".to_string()),
                );
                ui.end_row();
            });

        ui.separator();
        ui.label("同步前分布");
        render_provider_counts(ui, "provider_sync_before_grid", &report.before_counts);

        ui.separator();
        ui.label("同步后分布");
        render_provider_counts(ui, "provider_sync_after_grid", &report.after_counts);
    }
}

fn render_provider_counts(ui: &mut Ui, grid_id: &str, counts: &[ProviderCount]) {
    egui::Grid::new(grid_id).num_columns(2).show(ui, |ui| {
        ui.strong("Provider");
        ui.strong("线程数");
        ui.end_row();

        for item in counts {
            ui.label(&item.provider);
            ui.label(item.count.to_string());
            ui.end_row();
        }
    });
}
