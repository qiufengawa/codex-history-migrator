use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

use crate::core::export::export_package_with_progress;
use crate::core::import::import_package_with_progress;
use crate::core::manage::{
    delete_threads_to_trash, export_selected_threads_with_progress, filter_manage_rows,
    list_trash_batches, load_manage_rows, load_preview_entries, purge_all_trash, purge_trash_batch,
    rename_thread, restore_trash_batch, set_threads_archived,
};
use crate::core::provider_sync::{
    read_provider_sync_status, restore_latest_provider_backup_with_safety_backup,
    sync_threads_to_current_provider_with_backup,
};
use crate::core::scan::scan_codex_home_with_progress;
use crate::models::export_report::ExportReport;
use crate::models::import_report::ImportReport;
use crate::models::manage::{
    DeleteToTrashReport, ManageFilter, ManageRow, PreviewEntry, RestoreTrashReport,
    TrashBatchSummary,
};
use crate::models::operation_progress::OperationProgress;
use crate::models::provider_sync_report::ProviderSyncReport;
use crate::models::provider_sync_status::ProviderSyncStatus;
use crate::models::scan_report::ScanReport;
use crate::ui::{export, import, logs, manage, overview, sync};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Overview,
    Manage,
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
    ManageRefresh(Result<ManageSnapshot, String>),
    ManageRename(Result<ManageRenameTaskOutput, String>),
    ManageArchive(Result<ManageArchiveTaskOutput, String>),
    ManageDelete(Result<ManageDeleteTaskOutput, String>),
    ManageRestore(Result<ManageRestoreTaskOutput, String>),
    ManagePurgeBatch(Result<ManagePurgeBatchTaskOutput, String>),
    ManagePurgeAll(Result<ManagePurgeAllTaskOutput, String>),
    ManageExport(Result<ManageExportTaskOutput, String>),
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

#[derive(Debug, Clone)]
struct ManageSnapshot {
    rows: Vec<ManageRow>,
    trash_batches: Vec<TrashBatchSummary>,
}

struct ManageRenameTaskOutput {
    thread_id: String,
    new_title: String,
    snapshot: ManageSnapshot,
}

struct ManageArchiveTaskOutput {
    archived: bool,
    updated_ids: Vec<String>,
    snapshot: ManageSnapshot,
}

struct ManageDeleteTaskOutput {
    report: DeleteToTrashReport,
    snapshot: ManageSnapshot,
}

struct ManageRestoreTaskOutput {
    report: RestoreTrashReport,
    snapshot: ManageSnapshot,
}

struct ManagePurgeBatchTaskOutput {
    batch_id: String,
    snapshot: ManageSnapshot,
}

struct ManagePurgeAllTaskOutput {
    purged_count: usize,
    snapshot: ManageSnapshot,
}

struct ManageExportTaskOutput {
    output_path: PathBuf,
    report: ExportReport,
}

struct RunningTask {
    title: String,
    progress: OperationProgress,
    receiver: Receiver<TaskMessage>,
}

pub struct MigratorApp {
    pub active_tab: ActiveTab,
    pub codex_home_input: String,
    pub manage_filter: ManageFilter,
    pub manage_rows: Vec<ManageRow>,
    pub manage_trash_batches: Vec<TrashBatchSummary>,
    pub manage_selected_ids: BTreeSet<String>,
    pub manage_detail_thread_id: Option<String>,
    pub manage_preview_entries: Vec<PreviewEntry>,
    pub manage_rename_input: String,
    pub manage_has_auto_refreshed: bool,
    pub manage_confirm_delete_open: bool,
    pub manage_confirm_purge_batch_id: Option<String>,
    pub manage_confirm_purge_all_open: bool,
    pub export_output_input: String,
    pub import_package_input: String,
    pub create_backup_on_import: bool,
    pub create_backup_on_provider_sync: bool,
    pub create_safety_backup_on_restore: bool,
    pub create_backup_on_manage_delete: bool,
    pub manage_last_copied: Option<(String, Instant)>,
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
            manage_filter: ManageFilter::default(),
            manage_rows: Vec::new(),
            manage_trash_batches: Vec::new(),
            manage_selected_ids: BTreeSet::new(),
            manage_detail_thread_id: None,
            manage_preview_entries: Vec::new(),
            manage_rename_input: String::new(),
            manage_has_auto_refreshed: false,
            manage_confirm_delete_open: false,
            manage_confirm_purge_batch_id: None,
            manage_confirm_purge_all_open: false,
            export_output_input: String::new(),
            import_package_input: String::new(),
            create_backup_on_import: true,
            create_backup_on_provider_sync: true,
            create_safety_backup_on_restore: true,
            create_backup_on_manage_delete: true,
            manage_last_copied: None,
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

    pub fn manage_filtered_rows(&self) -> Vec<ManageRow> {
        filter_manage_rows(&self.manage_rows, &self.manage_filter)
    }

    pub fn manage_provider_options(&self) -> Vec<String> {
        let mut providers = self
            .manage_rows
            .iter()
            .map(|row| row.model_provider.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        providers.sort();
        providers
    }

    pub fn manage_detail_row(&self) -> Option<&ManageRow> {
        let thread_id = self.manage_detail_thread_id.as_ref()?;
        self.manage_rows.iter().find(|row| &row.id == thread_id)
    }

    pub fn set_manage_detail_thread(&mut self, thread_id: Option<String>) {
        self.manage_detail_thread_id = thread_id;
        if let Some((row_id, row_title)) = self
            .manage_detail_row()
            .map(|row| (row.id.clone(), row.title_display.clone()))
        {
            self.manage_rename_input = row_title;
            self.manage_preview_entries =
                load_preview_entries(&self.codex_home_path(), &row_id, 12).unwrap_or_default();
        } else {
            self.manage_rename_input.clear();
            self.manage_preview_entries.clear();
        }
    }

    pub fn select_only_manage_row(&mut self, thread_id: String) {
        self.manage_selected_ids.clear();
        self.manage_selected_ids.insert(thread_id.clone());
        self.set_manage_detail_thread(Some(thread_id));
    }

    pub fn toggle_manage_selection(&mut self, thread_id: String, selected: bool) {
        if selected {
            self.manage_selected_ids.insert(thread_id.clone());
            self.set_manage_detail_thread(Some(thread_id));
        } else {
            self.manage_selected_ids.remove(&thread_id);
            if self.manage_detail_thread_id.as_deref() == Some(thread_id.as_str()) {
                let next = self.manage_selected_ids.iter().next().cloned();
                self.set_manage_detail_thread(next);
            }
        }
    }

    pub fn clear_manage_selection(&mut self) {
        self.manage_selected_ids.clear();
        self.set_manage_detail_thread(None);
    }

    pub fn select_all_filtered_manage_rows(&mut self) {
        let filtered_ids = self
            .manage_filtered_rows()
            .into_iter()
            .map(|row| row.id)
            .collect::<Vec<_>>();
        if filtered_ids.is_empty() {
            self.log("全选已跳过：当前筛选结果为空。");
            return;
        }

        self.manage_selected_ids = filtered_ids.iter().cloned().collect();
        let next_detail = self
            .manage_detail_thread_id
            .clone()
            .filter(|thread_id| self.manage_selected_ids.contains(thread_id))
            .or_else(|| filtered_ids.first().cloned());
        self.set_manage_detail_thread(next_detail);
    }

    pub fn record_manage_copy(&mut self, copy_key: &str) {
        self.record_manage_copy_at(copy_key, Instant::now());
    }

    pub fn record_manage_copy_at(&mut self, copy_key: &str, now: Instant) {
        self.manage_last_copied = Some((copy_key.to_string(), now));
    }

    pub fn manage_copy_feedback_active(&self, copy_key: &str) -> bool {
        self.manage_copy_feedback_active_at(copy_key, Instant::now())
    }

    pub fn manage_copy_feedback_active_at(&self, copy_key: &str, now: Instant) -> bool {
        const WINDOW: Duration = Duration::from_millis(1200);
        self.manage_last_copied
            .as_ref()
            .is_some_and(|(last_key, copied_at)| {
                last_key == copy_key && now.saturating_duration_since(*copied_at) <= WINDOW
            })
    }

    pub fn maybe_auto_refresh_manage(&mut self) -> bool {
        let should_start = self.active_tab == ActiveTab::Manage
            && !self.manage_has_auto_refreshed
            && !self.is_busy()
            && !self.codex_home_input.trim().is_empty();

        if should_start {
            self.manage_has_auto_refreshed = true;
            self.run_manage_refresh();
        }

        should_start
    }

    pub fn request_manage_delete_confirmation(&mut self) {
        if self.manage_selected_ids.is_empty() {
            self.log("删除已跳过：请先选择至少一条会话。");
            return;
        }

        self.manage_confirm_delete_open = true;
    }

    pub fn request_manage_delete_for_detail(&mut self) {
        let Some(thread_id) = self.manage_detail_thread_id.clone() else {
            self.log("删除已跳过：请先在详情区选择一条会话。");
            return;
        };

        self.manage_selected_ids.clear();
        self.manage_selected_ids.insert(thread_id);
        self.request_manage_delete_confirmation();
    }

    pub fn cancel_manage_delete_confirmation(&mut self) {
        self.manage_confirm_delete_open = false;
    }

    pub fn confirm_manage_delete(&mut self) {
        self.manage_confirm_delete_open = false;
        self.run_manage_delete();
    }

    pub fn request_manage_purge_batch_confirmation(&mut self, batch_id: String) {
        if self
            .manage_trash_batches
            .iter()
            .any(|batch| batch.batch_id == batch_id)
        {
            self.manage_confirm_purge_batch_id = Some(batch_id);
        } else {
            self.log("清空回收站批次已跳过：未找到对应批次。");
        }
    }

    pub fn cancel_manage_purge_batch_confirmation(&mut self) {
        self.manage_confirm_purge_batch_id = None;
    }

    pub fn confirm_manage_purge_batch(&mut self) {
        let Some(batch_id) = self.manage_confirm_purge_batch_id.take() else {
            self.log("清空回收站批次已跳过：当前没有待确认的批次。");
            return;
        };

        self.run_manage_purge_batch(batch_id);
    }

    pub fn request_manage_purge_all_confirmation(&mut self) {
        if self.manage_trash_batches.is_empty() {
            self.log("清空全部回收站已跳过：当前没有回收站批次。");
            return;
        }

        self.manage_confirm_purge_all_open = true;
    }

    pub fn cancel_manage_purge_all_confirmation(&mut self) {
        self.manage_confirm_purge_all_open = false;
    }

    pub fn confirm_manage_purge_all(&mut self) {
        self.manage_confirm_purge_all_open = false;
        self.run_manage_purge_all();
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

    pub fn run_manage_refresh(&mut self) {
        let codex_home = self.codex_home_path();
        let codex_home_label = codex_home.display().to_string();
        self.start_background_task(
            "管理扫描",
            format!("正在刷新聊天记录管理视图：{codex_home_label}"),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.20, "正在读取聊天线程");
                    let snapshot = load_manage_snapshot(&codex_home)?;
                    send_task_progress(&sender, 1.0, "管理视图刷新完成");
                    Ok(snapshot)
                })()
                .map_err(|error: anyhow::Error| error.to_string());

                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManageRefresh(result)));
            },
        );
    }

    pub fn run_manage_rename(&mut self) {
        let Some(thread_id) = self.manage_detail_thread_id.clone() else {
            self.log("改标题已跳过：请先选择一条会话。");
            return;
        };
        let new_title = self.manage_rename_input.trim().to_string();
        if new_title.is_empty() {
            self.log("改标题已跳过：标题不能为空。");
            return;
        }

        let codex_home = self.codex_home_path();
        self.start_background_task(
            "改标题",
            format!("正在修改会话标题：{thread_id}"),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.35, "正在更新数据库和索引");
                    rename_thread(&codex_home, &thread_id, &new_title)?;
                    send_task_progress(&sender, 0.75, "正在刷新管理视图");
                    let snapshot = load_manage_snapshot(&codex_home)?;
                    Ok(ManageRenameTaskOutput {
                        thread_id,
                        new_title,
                        snapshot,
                    })
                })()
                .map_err(|error: anyhow::Error| error.to_string());
                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManageRename(result)));
            },
        );
    }

    pub fn run_manage_archive_toggle(&mut self, archived: bool) {
        let selected_ids = self.manage_selected_ids.iter().cloned().collect::<Vec<_>>();
        if selected_ids.is_empty() {
            self.log("批量归档已跳过：请先选择至少一条会话。");
            return;
        }

        let codex_home = self.codex_home_path();
        let title = if archived {
            "批量归档"
        } else {
            "取消归档"
        };
        let action_label = if archived { "归档" } else { "取消归档" };
        self.start_background_task(
            title,
            format!("正在{action_label} {} 条会话", selected_ids.len()),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.30, format!("正在{action_label}会话文件"));
                    let report = set_threads_archived(&codex_home, &selected_ids, archived)?;
                    send_task_progress(&sender, 0.75, "正在刷新管理视图");
                    let snapshot = load_manage_snapshot(&codex_home)?;
                    Ok(ManageArchiveTaskOutput {
                        archived,
                        updated_ids: report.updated_ids,
                        snapshot,
                    })
                })()
                .map_err(|error: anyhow::Error| error.to_string());
                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManageArchive(result)));
            },
        );
    }

    pub fn run_manage_delete(&mut self) {
        let selected_ids = self.manage_selected_ids.iter().cloned().collect::<Vec<_>>();
        if selected_ids.is_empty() {
            self.log("删除已跳过：请先选择至少一条会话。");
            return;
        }

        let create_backup = self.create_backup_on_manage_delete;
        let codex_home = self.codex_home_path();
        self.start_background_task(
            "删除到回收站",
            format!("正在把 {} 条会话移入回收站", selected_ids.len()),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.20, "正在写入回收站批次");
                    let report =
                        delete_threads_to_trash(&codex_home, &selected_ids, create_backup)?;
                    send_task_progress(&sender, 0.78, "正在刷新管理视图");
                    let snapshot = load_manage_snapshot(&codex_home)?;
                    Ok(ManageDeleteTaskOutput { report, snapshot })
                })()
                .map_err(|error: anyhow::Error| error.to_string());
                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManageDelete(result)));
            },
        );
    }

    pub fn run_manage_restore_batch(&mut self, batch_id: String) {
        let codex_home = self.codex_home_path();
        self.start_background_task(
            "恢复回收站批次",
            format!("正在恢复批次：{batch_id}"),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.30, "正在检查冲突并恢复数据");
                    let report = restore_trash_batch(&codex_home, &batch_id)?;
                    send_task_progress(&sender, 0.82, "正在刷新管理视图");
                    let snapshot = load_manage_snapshot(&codex_home)?;
                    Ok(ManageRestoreTaskOutput { report, snapshot })
                })()
                .map_err(|error: anyhow::Error| error.to_string());
                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManageRestore(result)));
            },
        );
    }

    pub fn run_manage_purge_batch(&mut self, batch_id: String) {
        let codex_home = self.codex_home_path();
        self.start_background_task(
            "清空回收站批次",
            format!("正在永久清空批次：{batch_id}"),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.45, "正在清理回收站批次");
                    purge_trash_batch(&codex_home, &batch_id)?;
                    send_task_progress(&sender, 0.82, "正在刷新管理视图");
                    let snapshot = load_manage_snapshot(&codex_home)?;
                    Ok(ManagePurgeBatchTaskOutput { batch_id, snapshot })
                })()
                .map_err(|error: anyhow::Error| error.to_string());
                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManagePurgeBatch(
                    result,
                )));
            },
        );
    }

    pub fn run_manage_purge_all(&mut self) {
        let codex_home = self.codex_home_path();
        self.start_background_task(
            "清空全部回收站",
            "正在永久清空全部回收站批次".to_string(),
            move |sender| {
                let result = (|| {
                    send_task_progress(&sender, 0.45, "正在清理全部回收站内容");
                    let purged_count = purge_all_trash(&codex_home)?;
                    send_task_progress(&sender, 0.82, "正在刷新管理视图");
                    let snapshot = load_manage_snapshot(&codex_home)?;
                    Ok(ManagePurgeAllTaskOutput {
                        purged_count,
                        snapshot,
                    })
                })()
                .map_err(|error: anyhow::Error| error.to_string());
                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManagePurgeAll(
                    result,
                )));
            },
        );
    }

    pub fn run_manage_export(&mut self, output_path: PathBuf) {
        let selected_ids = self.manage_selected_ids.iter().cloned().collect::<Vec<_>>();
        if selected_ids.is_empty() {
            self.log("导出所选已跳过：请先选择至少一条会话。");
            return;
        }

        let codex_home = self.codex_home_path();
        let output_label = output_path.display().to_string();
        self.start_background_task(
            "导出所选会话",
            format!("正在导出所选会话到 {output_label}"),
            move |sender| {
                let progress_sender = sender.clone();
                let result = export_selected_threads_with_progress(
                    &codex_home,
                    &output_path,
                    &selected_ids,
                    move |done, total| {
                        let fraction = if total == 0 {
                            1.0
                        } else {
                            done as f32 / total as f32
                        };
                        let _ =
                            progress_sender.send(TaskMessage::Progress(OperationProgress::new(
                                fraction,
                                format!("正在导出所选会话 {done}/{total}"),
                            )));
                    },
                )
                .map(|report| ManageExportTaskOutput {
                    output_path,
                    report,
                })
                .map_err(|error| error.to_string());

                let _ = sender.send(TaskMessage::Finished(TaskCompletion::ManageExport(result)));
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

    fn apply_manage_snapshot(&mut self, snapshot: ManageSnapshot) {
        self.manage_rows = snapshot.rows;
        self.manage_trash_batches = snapshot.trash_batches;

        self.manage_selected_ids
            .retain(|thread_id| self.manage_rows.iter().any(|row| row.id == *thread_id));

        let next_detail = self
            .manage_detail_thread_id
            .clone()
            .filter(|thread_id| self.manage_rows.iter().any(|row| row.id == *thread_id))
            .or_else(|| self.manage_selected_ids.iter().next().cloned());

        self.set_manage_detail_thread(next_detail);
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
            TaskCompletion::ManageRefresh(result) => match result {
                Ok(snapshot) => {
                    self.apply_manage_snapshot(snapshot);
                    self.log(format!(
                        "管理视图已刷新：{} 条会话，{} 个回收站批次。",
                        self.manage_rows.len(),
                        self.manage_trash_batches.len()
                    ));
                }
                Err(error) => self.log(format!("管理视图刷新失败：{error}")),
            },
            TaskCompletion::ManageRename(result) => match result {
                Ok(output) => {
                    self.apply_manage_snapshot(output.snapshot);
                    self.manage_selected_ids.clear();
                    self.manage_selected_ids.insert(output.thread_id.clone());
                    self.set_manage_detail_thread(Some(output.thread_id.clone()));
                    self.log(format!(
                        "标题修改完成：{} -> {}",
                        output.thread_id, output.new_title
                    ));
                }
                Err(error) => self.log(format!("修改标题失败：{error}")),
            },
            TaskCompletion::ManageArchive(result) => match result {
                Ok(output) => {
                    let affected = output.updated_ids.len();
                    let affected_ids = output.updated_ids.clone();
                    self.apply_manage_snapshot(output.snapshot);
                    self.manage_selected_ids = affected_ids.into_iter().collect();
                    let verb = if output.archived {
                        "归档"
                    } else {
                        "取消归档"
                    };
                    self.log(format!("{verb}完成：共处理 {affected} 条会话。"));
                }
                Err(error) => self.log(format!("批量归档操作失败：{error}")),
            },
            TaskCompletion::ManageDelete(result) => match result {
                Ok(output) => {
                    let deleted_count = output.report.deleted_count;
                    let batch_id = output.report.batch_id.clone();
                    let backup_message = output
                        .report
                        .backup_path
                        .as_ref()
                        .map(|path| format!("，安全备份：{}", path.display()))
                        .unwrap_or_default();
                    self.apply_manage_snapshot(output.snapshot);
                    self.clear_manage_selection();
                    self.log(format!(
                        "已删除到回收站：{deleted_count} 条会话，批次 {batch_id}{backup_message}"
                    ));
                }
                Err(error) => self.log(format!("删除到回收站失败：{error}")),
            },
            TaskCompletion::ManageRestore(result) => match result {
                Ok(output) => {
                    let restored = output.report.restored_ids.len();
                    let conflicts = output.report.conflict_ids.len();
                    self.apply_manage_snapshot(output.snapshot);
                    self.log(format!(
                        "回收站恢复完成：恢复 {restored} 条，冲突跳过 {conflicts} 条。"
                    ));
                }
                Err(error) => self.log(format!("恢复回收站批次失败：{error}")),
            },
            TaskCompletion::ManagePurgeBatch(result) => match result {
                Ok(output) => {
                    self.apply_manage_snapshot(output.snapshot);
                    self.log(format!("已永久清空回收站批次：{}", output.batch_id));
                }
                Err(error) => self.log(format!("清空回收站批次失败：{error}")),
            },
            TaskCompletion::ManagePurgeAll(result) => match result {
                Ok(output) => {
                    self.apply_manage_snapshot(output.snapshot);
                    self.log(format!(
                        "已永久清空全部回收站批次：{} 个。",
                        output.purged_count
                    ));
                }
                Err(error) => self.log(format!("清空全部回收站失败：{error}")),
            },
            TaskCompletion::ManageExport(result) => match result {
                Ok(output) => {
                    self.last_export_report = Some(output.report.clone());
                    self.log(format!(
                        "所选会话导出完成：{} 条线程，输出到 {}",
                        output.report.thread_count,
                        output.output_path.display()
                    ));
                }
                Err(error) => self.log(format!("导出所选会话失败：{error}")),
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
                ui.selectable_value(&mut self.active_tab, ActiveTab::Manage, "管理");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Overview, "概览");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Export, "导出");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Import, "导入");
                ui.selectable_value(&mut self.active_tab, ActiveTab::Sync, "同步修复");
            });
            ui.separator();
            self.maybe_auto_refresh_manage();

            match self.active_tab {
                ActiveTab::Manage => manage::render(ui, self),
                ActiveTab::Overview => overview::render(ui, self),
                ActiveTab::Export => export::render(ui, self),
                ActiveTab::Import => import::render(ui, self),
                ActiveTab::Sync => sync::render(ui, self),
            }
        });

        egui::TopBottomPanel::bottom("logs")
            .resizable(true)
            .default_height(140.0)
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

fn load_manage_snapshot(codex_home: &Path) -> Result<ManageSnapshot, anyhow::Error> {
    Ok(ManageSnapshot {
        rows: load_manage_rows(codex_home, &ManageFilter::default())?,
        trash_batches: list_trash_batches(codex_home)?,
    })
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::{ActiveTab, MigratorApp, detect_codex_home_with};
    use crate::models::manage::{ManageHealth, ManageRow, TrashBatchSummary};

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

        assert_eq!(detected, Some(std::path::Path::new(r"C:\Users\Alice").join(".codex")));
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
            Some(std::path::Path::new(r"D:\Users\Bob").join(".codex"))
        );
    }

    #[test]
    fn manage_tab_auto_refresh_only_starts_once() {
        let temp = tempfile::tempdir().unwrap();
        let mut app = MigratorApp::default();
        app.codex_home_input = temp.path().to_string_lossy().to_string();
        app.active_tab = ActiveTab::Manage;

        assert!(app.maybe_auto_refresh_manage());
        assert!(app.manage_has_auto_refreshed);
        assert!(!app.maybe_auto_refresh_manage());
    }

    #[test]
    fn delete_confirmation_requires_selection() {
        let mut app = MigratorApp::default();

        app.request_manage_delete_confirmation();
        assert!(!app.manage_confirm_delete_open);
        assert!(
            app.logs
                .last()
                .is_some_and(|line| line.contains("请先选择至少一条会话"))
        );

        app.manage_selected_ids.insert("thread-a".to_string());
        app.request_manage_delete_confirmation();
        assert!(app.manage_confirm_delete_open);
    }

    #[test]
    fn purge_all_confirmation_requires_existing_batches() {
        let mut app = MigratorApp::default();

        app.request_manage_purge_all_confirmation();
        assert!(!app.manage_confirm_purge_all_open);

        app.manage_trash_batches.push(TrashBatchSummary {
            batch_id: "batch-001".to_string(),
            path: PathBuf::from(r"C:\Temp\history_manager_trash\batch-001"),
            deleted_at: 1_713_304_800,
            thread_count: 2,
            payload_count: 2,
        });

        app.request_manage_purge_all_confirmation();
        assert!(app.manage_confirm_purge_all_open);
    }

    #[test]
    fn manage_copy_feedback_expires_after_window() {
        let mut app = MigratorApp::default();
        let start = Instant::now();

        app.record_manage_copy_at("thread-id", start);
        assert!(app.manage_copy_feedback_active_at("thread-id", start));
        assert!(
            app.manage_copy_feedback_active_at("thread-id", start + Duration::from_millis(1000))
        );
        assert!(
            !app.manage_copy_feedback_active_at("thread-id", start + Duration::from_millis(1300))
        );
        assert!(!app.manage_copy_feedback_active_at("cwd", start));
    }

    #[test]
    fn detail_delete_request_selects_current_thread_and_opens_confirmation() {
        let mut app = MigratorApp::default();
        app.manage_detail_thread_id = Some("thread-a".to_string());

        app.request_manage_delete_for_detail();

        assert!(app.manage_confirm_delete_open);
        assert_eq!(
            app.manage_selected_ids.iter().cloned().collect::<Vec<_>>(),
            vec!["thread-a".to_string()]
        );
    }

    #[test]
    fn select_all_filtered_manage_rows_respects_current_filter() {
        let mut app = MigratorApp::default();
        app.manage_rows = vec![
            test_manage_row("thread-a", "openai", "Alpha"),
            test_manage_row("thread-b", "anthropic", "Beta"),
            test_manage_row("thread-c", "openai", "Gamma"),
        ];
        app.manage_filter.provider = Some("openai".to_string());

        app.select_all_filtered_manage_rows();

        assert_eq!(
            app.manage_selected_ids.iter().cloned().collect::<Vec<_>>(),
            vec!["thread-a".to_string(), "thread-c".to_string()]
        );
        assert_eq!(app.manage_detail_thread_id.as_deref(), Some("thread-a"));
    }

    fn test_manage_row(id: &str, provider: &str, title: &str) -> ManageRow {
        ManageRow {
            id: id.to_string(),
            title: title.to_string(),
            title_display: title.to_string(),
            title_detail: None,
            first_user_message: title.to_string(),
            first_user_message_display: title.to_string(),
            updated_at: 1,
            model_provider: provider.to_string(),
            model: Some("gpt-5.4".to_string()),
            cwd: "C:/Projects/Test".to_string(),
            cwd_display: "C:/Projects/Test".to_string(),
            archived: false,
            archived_at: None,
            rollout_path: PathBuf::from("C:/Projects/Test/rollout.jsonl"),
            rollout_path_display: "C:/Projects/Test/rollout.jsonl".to_string(),
            relative_rollout_path: Some(PathBuf::from("sessions/rollout.jsonl")),
            payload_exists: true,
            preview_available: true,
            can_open_payload: true,
            can_toggle_archive: true,
            can_delete: true,
            health: ManageHealth::Healthy,
        }
    }
}
