use std::path::{Component, Path, PathBuf};

use anyhow::{Result, bail};

use crate::db::threads::load_threads;
use crate::fs::codex_home::CodexHomePaths;
use crate::models::operation_progress::OperationProgress;
use crate::models::scan_report::{ScanReport, SessionPayload};

pub fn scan_codex_home(codex_home: &Path) -> Result<ScanReport> {
    scan_codex_home_with_progress(codex_home, |_| {})
}

pub fn scan_codex_home_with_progress<F>(codex_home: &Path, mut progress: F) -> Result<ScanReport>
where
    F: FnMut(OperationProgress),
{
    let paths = CodexHomePaths::resolve(codex_home);
    emit_progress(
        &mut progress,
        0.05,
        format!("扫描：检查数据库 {}", paths.state_db.display()),
    );
    if !paths.state_db.exists() {
        bail!("state_5.sqlite not found at {}", paths.state_db.display());
    }

    emit_progress(&mut progress, 0.15, "扫描：读取线程记录");
    let threads = load_threads(&paths.state_db)?;
    let total_threads = threads.len();
    let mut session_payloads = Vec::new();
    let mut missing_payloads = Vec::new();

    if total_threads == 0 {
        emit_progress(&mut progress, 0.80, "扫描：未发现线程记录");
    }

    for (index, thread) in threads.iter().enumerate() {
        let absolute_path = PathBuf::from(&thread.rollout_path);

        match relative_payload_path(codex_home, &absolute_path) {
            Some(relative_path) if absolute_path.exists() => {
                session_payloads.push(SessionPayload {
                    archived: thread.archived,
                    absolute_path,
                    relative_path,
                });
            }
            _ => {
                missing_payloads.push(thread.rollout_path.clone());
            }
        }

        let fraction = 0.20 + 0.70 * ((index + 1) as f32 / total_threads.max(1) as f32);
        emit_progress(
            &mut progress,
            fraction,
            format!("扫描：检查会话文件 {}/{}", index + 1, total_threads),
        );
    }

    let source_root_prefix = codex_home.to_string_lossy().to_string();
    let report = ScanReport {
        codex_home: codex_home.to_path_buf(),
        threads,
        session_payloads,
        missing_payloads,
        session_index_path: paths.session_index.exists().then_some(paths.session_index),
        source_root_prefix,
    };

    emit_progress(
        &mut progress,
        1.0,
        format!(
            "扫描完成：{} 个线程，{} 个会话文件，{} 个缺失引用",
            report.threads.len(),
            report.session_payloads.len(),
            report.missing_payloads.len()
        ),
    );

    Ok(report)
}

fn relative_payload_path(codex_home: &Path, absolute_path: &Path) -> Option<PathBuf> {
    let stripped = absolute_path.strip_prefix(codex_home).ok()?;
    let mut relative = PathBuf::new();

    for component in stripped.components() {
        match component {
            Component::Normal(part) => relative.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    let top_level = relative.components().next()?;
    let Component::Normal(top_level) = top_level else {
        return None;
    };

    if top_level != "sessions" && top_level != "archived_sessions" {
        return None;
    }

    if relative.as_os_str().is_empty() {
        return None;
    }

    Some(relative)
}

fn emit_progress<F>(progress: &mut F, fraction: f32, message: impl Into<String>)
where
    F: FnMut(OperationProgress),
{
    progress(OperationProgress::new(fraction, message));
}
