use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use eframe::egui;

use crate::core::export::export_package_with_progress;
use crate::core::import::import_package_with_progress;
use crate::core::provider_sync::{
    read_provider_sync_status, restore_latest_provider_backup_with_safety_backup,
    sync_threads_to_current_provider_with_backup,
};
use crate::core::scan::scan_codex_home_with_progress;
use crate::models::export_report::ExportReport;
use crate::models::import_report::ImportReport;
use crate::models::operation_progress::OperationProgress;
use crate::models::provider_sync_report::ProviderSyncReport;
use crate::models::provider_sync_status::ProviderSyncStatus;
use crate::models::scan_report::ScanReport;
use crate::ui::{export, import, logs, overview, sync};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Export,
    Import,
    Sync,
}

enum TaskMessage {
    Progress(OperationProgress),
    Finished(TaskCompletion),
}

enum TaskCompletion {
    Scan(Result<ScanReport, String>),
    Export(Result<ExportReport, String>),
    Import(Result<ImportReport, String>),
    ProviderStatus(Result<ProviderSyncStatus, String>),
    ProviderSync(Result<ProviderSyncTaskOutput, String>),
    ProviderRestore(Result<ProviderRestoreTaskOutput, String>),
}

struct ProviderSyncTaskOutput {
    report: ProviderSyncReport,
    refreshed_status: ProviderSyncStatus,
}

struct ProviderRestoreTaskOutput {
    restored_from: PathBuf,
    refreshed_status: ProviderSyncStatus,
}

struct RunningTask {
    title: String,
    progress: OperationProgress,
    receiver: Receiver<TaskMessage>,
}

pub struct MigratorApp {
    pub active_tab: ActiveTab,
    pub codex_home_input: String,
    pub export_output_input: String,
    pub import_package_input: String,
    pub create_backup_on_import: bool,
    pub create_backup_on_provider_sync: bool,
    pub create_safety_backup_on_restore: bool,
    pub logs: Vec<String>,
    pub last_scan: Option<ScanReport>,
    pub last_export_report: Option<ExportReport>,
    pub last_import_report: Option<ImportReport>,
    pub last_provider_sync_status: Option<ProviderSyncStatus>,
    pub last_provider_sync_report: Option<ProviderSyncReport>,
    running_task: Option<RunningTask>,
}

impl Default for MigratorApp {
    fn default() -> Self {
        let detected_codex_home = default_codex_home();
        let codex_home = detected_codex_home
            .as_ref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut logs = vec!["工具已就绪".to_string()];
        match &detected_codex_home {
            Some(path) => logs.push(format!("已自动识别 Codex 目录：{}", path.display())),
            None => logs.push("未能自动识别 Codex 目录，请手动输入或粘贴 .codex 路径".to_string()),
        }

        Self {
            active_tab: ActiveTab::Overview,
            codex_home_input: codex_home,
            export_output_input: String::new(),
            import_package_input: String::new(),
            create_backup_on_import: true,
            create_backup_on_provider_sync: true,
            create_safety_backup_on_restore: true,
            logs,
            last_scan: None,
            last_export_report: None,
            last_import_report: None,
            last_provider_sync_status: None,
            last_provider_sync_report: None,
            running_task: None,
        }
    }
}

impl MigratorApp {
    pub fn log(&mut self, message: impl Into<String>) {
        self.logs.push(message.into());
        if self.logs.len() > 200 {
            let overflow = self.logs.len() - 200;
            self.logs.drain(0..overflow);
        }
    }

    pub fn is_busy(&self) -> bool {
        self.running_task.is_some()
    }

    pub fn running_task_title(&self) -> Option<&str> {
        self.running_task.as_ref().map(|task| task.title.as_str())
    }

    pub fn codex_home_path(&self) -> PathBuf {
        PathBuf::from(self.codex_home_input.trim())
    }

    pub fn run_scan(&mut self) {
        let codex_home = self.codex_home_path();
        let codex_home_label = codex_home.display().to_string();
        self.start_background_task(
            "扫描",
            format!("正在扫描 {}", codex_home_label),
            move |sender| {
                let progress_sender = sender.clone();
                let result = scan_codex_home_with_progress(&codex_home, move |update| {
                    let _ = progress_sender.send(TaskMessage::Progress(update));
                })
                .map_err(|error| error.to_string());

                let _ = sender.send(TaskMessage::Finished(TaskCompletion::Scan(result)));
            },
        );
    }

    pub fn run_export(&mut self) {
        let codex_home = self.codex_home_path();
        let output = PathBuf::from(self.export_output_input.trim());
        if output.as_os_str().is_empty() {
            self.log("导出已跳过：请先选择输出包路径。");
            return;
        }

        let codex_home_label = codex_home.display().to_string();
        let output_label = output.display().to_string();
        self.start_background_task(
            "导出",
            format!("正在从 {codex_home_label} 导出到 {output_label}"),
            move |sender| {
                let progress_sender = sender.clone();
                let result = export_package_with_progress(&codex_home, &output, move |update| {
                    let _ = progress_sender.send(TaskMessage::Progress(update));
                })
                .map_err(|error| error.to_string());

                let _ = sender.send(TaskMessage::Finished(TaskCompletion::Export(result)));
            },
        );
    }

    pub fn run_import(&mut self) {
        let package = PathBuf::from(self.import_package_input.trim());
        let target_home = self.codex_home_path();
        let create_backup = self.create_backup_on_import;
        if package.as_os_str().is_empty() {
            self.log("导入已跳过：请先选择迁移包。");
            return;
        }

        let package_label = package.display().to_string();
        let target_label = target_home.display().to_string();
        self.start_background_task(
            "导入",
            format!("正在把迁移包 {package_label} 导入到 {target_label}"),
            move |sender| {
                let progress_sender = sender.clone();
                let result = import_package_with_progress(
                    &package,
                    &target_home,
                    create_backup,
                    move |update| {
                        let _ = progress_sender.send(TaskMessage::Progress(update));
                    },
                )
                .map_err(|error| error.to_string());

                let _ = sender.send(TaskMessage::Finished(TaskCompletion::Import(result)));
            },
        );
    }

    pub fn run_provider_sync_status(&mut self) {
        let codex_home = self.codex_home_path();
        let codex_home_label = codex_home.display().to_string();
        self.start_background_task(
            "检查 Provider",
            format!("正在读取当前 Provider 状态：{codex_home_label}"),
            move |sender| {
                send_task_progress(&sender, 0.25, "正在读取 provider 配置和线程分布");
                let result =
                    read_provider_sync_status(&codex_home).map_err(|error| error.to_string());
                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ProviderStatus(
                    result,
                )));
            },
        );
    }

    pub fn run_provider_sync(&mut self) {
        let codex_home = self.codex_home_path();
        let create_backup = self.create_backup_on_provider_sync;
        let codex_home_label = codex_home.display().to_string();
        self.start_background_task(
            "同步 Provider",
            format!("正在把旧 provider 线程同步到当前 provider：{codex_home_label}"),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.15, "正在读取当前 provider 状态");
                    send_task_progress(&sender, 0.55, "正在同步线程 provider");
                    let report =
                        sync_threads_to_current_provider_with_backup(&codex_home, create_backup)?;
                    send_task_progress(&sender, 0.90, "正在刷新 provider 状态");
                    let refreshed_status = read_provider_sync_status(&codex_home)?;
                    Ok(ProviderSyncTaskOutput {
                        report,
                        refreshed_status,
                    })
                })()
                .map_err(|error: anyhow::Error| error.to_string());

                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ProviderSync(result)));
            },
        );
    }

    pub fn run_restore_latest_provider_backup(&mut self) {
        let codex_home = self.codex_home_path();
        let create_backup = self.create_safety_backup_on_restore;
        let codex_home_label = codex_home.display().to_string();
        self.start_background_task(
            "恢复 Provider 备份",
            format!("正在恢复最近一次 provider 同步备份：{codex_home_label}"),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.20, "正在查找最近一次 provider 备份");
                    let restored_from = restore_latest_provider_backup_with_safety_backup(
                        &codex_home,
                        create_backup,
                    )?;
                    send_task_progress(&sender, 0.85, "正在刷新 provider 状态");
                    let refreshed_status = read_provider_sync_status(&codex_home)?;
                    Ok(ProviderRestoreTaskOutput {
                        restored_from,
                        refreshed_status,
                    })
                })()
                .map_err(|error: anyhow::Error| error.to_string());

                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ProviderRestore(
                    result,
                )));
            },
        );
    }

    fn start_background_task<F>(&mut self, title: &str, initial_message: String, task: F)
    where
        F: FnOnce(Sender<TaskMessage>) + Send + 'static,
    {
        if self.is_busy() {
            self.log("已有任务正在执行，请等待当前任务完成。");
            return;
        }

        let (sender, receiver) = mpsc::channel();
        let initial_progress = OperationProgress::new(0.0, initial_message);
        self.running_task = Some(RunningTask {
            title: title.to_string(),
            progress: initial_progress.clone(),
            receiver,
        });
        self.log(format!("{title}已开始。"));

        thread::spawn(move || {
            let _ = sender.send(TaskMessage::Progress(initial_progress));
            task(sender);
        });
    }

    fn poll_background_task(&mut self) {
        let mut latest_progress = None;
        let mut completion = None;
        let mut disconnected = false;

        if let Some(task) = &mut self.running_task {
            loop {
                match task.receiver.try_recv() {
                    Ok(TaskMessage::Progress(progress)) => latest_progress = Some(progress),
                    Ok(TaskMessage::Finished(result)) => completion = Some(result),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }

            if let Some(progress) = latest_progress {
                task.progress = progress;
            }
        }

        if let Some(result) = completion {
            self.running_task = None;
            self.apply_task_completion(result);
        } else if disconnected {
            let title = self
                .running_task
                .as_ref()
                .map(|task| task.title.clone())
                .unwrap_or_else(|| "后台任务".to_string());
            self.running_task = None;
            self.log(format!("{title}意外结束，请重试。"));
        }
    }

    fn apply_task_completion(&mut self, completion: TaskCompletion) {
        match completion {
            TaskCompletion::Scan(result) => match result {
                Ok(scan) => {
                    self.log(format!(
                        "扫描完成：{} 个线程，{} 个会话文件，{} 个缺失引用。",
                        scan.threads.len(),
                        scan.session_payloads.len(),
                        scan.missing_payloads.len()
                    ));
                    self.last_scan = Some(scan);
                }
                Err(error) => self.log(format!("扫描失败：{error}")),
            },
            TaskCompletion::Export(result) => match result {
                Ok(report) => {
                    self.log(format!(
                        "导出完成：{} 个线程，{} 个会话文件，{} 个归档文件。",
                        report.thread_count, report.session_file_count, report.archived_file_count
                    ));
                    self.last_export_report = Some(report);
                }
                Err(error) => self.log(format!("导出失败：{error}")),
            },
            TaskCompletion::Import(result) => match result {
                Ok(report) => {
                    self.log(format!(
                        "导入完成：新增 {}，更新 {}，跳过 {}，修复路径 {}。",
                        report.inserted_threads,
                        report.updated_threads,
                        report.skipped_threads,
                        report.repaired_paths
                    ));
                    self.last_import_report = Some(report);
                }
                Err(error) => self.log(format!("导入失败：{error}")),
            },
            TaskCompletion::ProviderStatus(result) => match result {
                Ok(status) => {
                    self.log(format!(
                        "当前 provider：{}，共 {} 个线程，其中 {} 个需要重挂。",
                        status.current_provider, status.total_threads, status.movable_threads
                    ));
                    self.last_provider_sync_status = Some(status);
                }
                Err(error) => self.log(format!("读取 provider 状态失败：{error}")),
            },
            TaskCompletion::ProviderSync(result) => match result {
                Ok(output) => {
                    let backup_message = output
                        .report
                        .backup_path
                        .as_ref()
                        .map(|path| format!("备份保存在 {}", path.display()))
                        .unwrap_or_else(|| "未创建备份".to_string());
                    self.log(format!(
                        "同步完成：{} 个线程已切换到 {}，{}。",
                        output.report.updated_threads,
                        output.report.current_provider,
                        backup_message
                    ));
                    self.last_provider_sync_status = Some(output.refreshed_status);
                    self.last_provider_sync_report = Some(output.report);
                }
                Err(error) => self.log(format!("同步 provider 失败：{error}")),
            },
            TaskCompletion::ProviderRestore(result) => match result {
                Ok(output) => {
                    self.log(format!(
                        "恢复完成：已从 {} 恢复。",
                        output.restored_from.display()
                    ));
                    self.last_provider_sync_report = None;
                    self.last_provider_sync_status = Some(output.refreshed_status);
                }
                Err(error) => self.log(format!("恢复 provider 备份失败：{error}")),
            },
        }
    }
}

impl eframe::App for MigratorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_background_task();
        if self.is_busy() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Codex 历史迁移与同步工具");
                ui.separator();
                ui.label(format!("v{}", env!("CARGO_PKG_VERSION")));
                if self.is_busy() {
                    ui.separator();
                    ui.label("任务执行中");
                }
            });

            if let Some(task) = &self.running_task {
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("当前任务：{}", task.title));
                    ui.add(
                        egui::ProgressBar::new(task.progress.fraction)
                            .show_percentage()
                            .desired_width(260.0),
                    );
                });
                ui.label(&task.progress.message);
            } else {
                ui.add_space(6.0);
                ui.label("状态：就绪");
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.active_tab, ActiveTab::Overview, "概览");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Export, "导出");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Import, "导入");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Sync, "同步修复");
            });
            ui.separator();

            match self.active_tab {
                ActiveTab::Overview => overview::render(ui, self),
                ActiveTab::Export => export::render(ui, self),
                ActiveTab::Import => import::render(ui, self),
                ActiveTab::Sync => sync::render(ui, self),
            }
        });

        egui::TopBottomPanel::bottom("logs")
            .resizable(true)
            .default_height(180.0)
            .show(ctx, |ui| logs::render(ui, self));
    }
}

fn default_codex_home() -> Option<PathBuf> {
    detect_codex_home_with(|key| env::var_os(key))
}

fn detect_codex_home_with<F>(get_var: F) -> Option<PathBuf>
where
    F: Fn(&str) -> Option<OsString>,
{
    if let Some(codex_home) = non_empty_path(get_var("CODEX_HOME")) {
        return Some(codex_home);
    }

    if let Some(user_profile) = non_empty_path(get_var("USERPROFILE")) {
        return Some(user_profile.join(".codex"));
    }

    if let Some(home) = non_empty_path(get_var("HOME")) {
        return Some(home.join(".codex"));
    }

    match (
        non_empty_path(get_var("HOMEDRIVE")),
        non_empty_path(get_var("HOMEPATH")),
    ) {
        (Some(home_drive), Some(home_path)) => {
            let combined = format!(
                "{}{}",
                home_drive.to_string_lossy(),
                home_path.to_string_lossy()
            );
            Some(Path::new(&combined).join(".codex"))
        }
        _ => None,
    }
}

fn non_empty_path(value: Option<OsString>) -> Option<PathBuf> {
    let value = value?;
    if value.to_string_lossy().trim().is_empty() {
        return None;
    }

    Some(PathBuf::from(value))
}

fn send_task_progress(sender: &Sender<TaskMessage>, fraction: f32, message: impl Into<String>) {
    let _ = sender.send(TaskMessage::Progress(OperationProgress::new(
        fraction, message,
    )));
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{detect_codex_home_with, ActiveTab, MigratorApp};

    #[test]
    fn default_app_state_starts_on_overview_tab() {
        let app = MigratorApp::default();
        assert_eq!(app.active_tab, ActiveTab::Overview);
    }

    #[test]
    fn detect_codex_home_prefers_explicit_codex_home_env() {
        let detected = detect_codex_home_with(|key| match key {
            "CODEX_HOME" => Some(OsString::from(r"D:\Portable\CodexData")),
            "USERPROFILE" => Some(OsString::from(r"C:\Users\Alice")),
            _ => None,
        });

        assert_eq!(
            detected,
            Some(std::path::PathBuf::from(r"D:\Portable\CodexData"))
        );
    }

    #[test]
    fn detect_codex_home_falls_back_to_windows_profile_directory() {
        let detected = detect_codex_home_with(|key| match key {
            "USERPROFILE" => Some(OsString::from(r"C:\Users\Alice")),
            _ => None,
        });

        assert_eq!(
            detected,
            Some(std::path::PathBuf::from(r"C:\Users\Alice\.codex"))
        );
    }

    #[test]
    fn detect_codex_home_falls_back_to_home_drive_and_path() {
        let detected = detect_codex_home_with(|key| match key {
            "HOMEDRIVE" => Some(OsString::from("D:")),
            "HOMEPATH" => Some(OsString::from(r"\Users\Bob")),
            _ => None,
        });

        assert_eq!(
            detected,
            Some(std::path::PathBuf::from(r"D:\Users\Bob\.codex"))
        );
    }
}
