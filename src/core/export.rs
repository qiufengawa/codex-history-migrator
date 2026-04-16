use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use rusqlite::{Connection, params};
use tempfile::tempdir;

use crate::core::checksum::compute_sha256_hex;
use crate::core::scan::scan_codex_home_with_progress;
use crate::fs::package::write_zip_from_dir_with_progress;
use crate::models::export_report::ExportReport;
use crate::models::manifest::{Manifest, PackageCounts};
use crate::models::operation_progress::OperationProgress;
use crate::models::scan_report::ScanReport;
use crate::models::thread_record::ThreadRecord;

pub fn export_package(source_home: &Path, output_file: &Path) -> Result<ExportReport> {
    export_package_with_progress(source_home, output_file, |_| {})
}

pub fn export_package_with_progress<F>(
    source_home: &Path,
    output_file: &Path,
    mut progress: F,
) -> Result<ExportReport>
where
    F: FnMut(OperationProgress),
{
    emit_progress(
        &mut progress,
        0.02,
        format!("导出：扫描源目录 {}", source_home.display()),
    );
    let scan = scan_codex_home_with_progress(source_home, |update| {
        progress(OperationProgress::new(
            remap_fraction(update.fraction, 0.05, 0.28),
            format!("导出：{}", update.message),
        ));
    })?;

    let temp = tempdir()?;
    let package_root = temp.path();
    emit_progress(&mut progress, 0.32, "导出：准备临时目录");
    fs::create_dir_all(package_root.join("db"))?;
    fs::create_dir_all(package_root.join("index"))?;

    emit_progress(&mut progress, 0.38, "导出：写入线程数据库");
    write_threads_sqlite(
        &scan,
        &package_root.join("db").join("threads.sqlite"),
        |done, total| {
            let fraction = remap_fraction(ratio(done, total), 0.38, 0.56);
            emit_progress(
                &mut progress,
                fraction,
                format!("导出：写入线程数据库 {done}/{total}"),
            );
        },
    )?;

    let total_payloads = scan.session_payloads.len();
    if total_payloads == 0 {
        emit_progress(&mut progress, 0.78, "导出：没有可复制的会话文件");
    }

    for (index, payload) in scan.session_payloads.iter().enumerate() {
        let target = package_root.join(&payload.relative_path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&payload.absolute_path, &target)?;

        let fraction = remap_fraction(ratio(index + 1, total_payloads), 0.58, 0.78);
        emit_progress(
            &mut progress,
            fraction,
            format!("导出：复制会话文件 {}/{}", index + 1, total_payloads),
        );
    }

    if let Some(session_index_path) = &scan.session_index_path {
        emit_progress(&mut progress, 0.82, "导出：复制会话索引");
        fs::copy(
            session_index_path,
            package_root.join("index").join("session_index.jsonl"),
        )?;
    } else {
        emit_progress(&mut progress, 0.82, "导出：未找到会话索引，已跳过");
    }

    let manifest = Manifest {
        format_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: unix_timestamp_string(),
        source_codex_home: scan.codex_home.to_string_lossy().to_string(),
        source_root_prefix: scan.source_root_prefix.clone(),
        counts: PackageCounts {
            thread_count: scan.threads.len(),
            session_file_count: scan
                .session_payloads
                .iter()
                .filter(|payload| !payload.archived)
                .count(),
            archived_file_count: scan
                .session_payloads
                .iter()
                .filter(|payload| payload.archived)
                .count(),
            missing_file_count: scan.missing_payloads.len(),
        },
    };
    emit_progress(&mut progress, 0.88, "导出：写入清单文件");
    fs::write(
        package_root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;

    emit_progress(&mut progress, 0.92, "导出：计算校验和");
    let mut checksums = BTreeMap::new();
    checksums.insert(
        "db/threads.sqlite".to_string(),
        compute_sha256_hex(&package_root.join("db").join("threads.sqlite"))?,
    );
    if package_root
        .join("index")
        .join("session_index.jsonl")
        .exists()
    {
        checksums.insert(
            "index/session_index.jsonl".to_string(),
            compute_sha256_hex(&package_root.join("index").join("session_index.jsonl"))?,
        );
    }
    for payload in &scan.session_payloads {
        let relative_path = payload.relative_path.to_string_lossy().replace('\\', "/");
        checksums.insert(
            relative_path,
            compute_sha256_hex(&package_root.join(&payload.relative_path))?,
        );
    }
    fs::write(
        package_root.join("checksums.json"),
        serde_json::to_vec_pretty(&checksums)?,
    )?;

    emit_progress(
        &mut progress,
        0.95,
        format!("导出：封装迁移包 {}", output_file.display()),
    );
    write_zip_from_dir_with_progress(package_root, output_file, |done, total| {
        let fraction = remap_fraction(ratio(done, total), 0.95, 0.99);
        emit_progress(
            &mut progress,
            fraction,
            format!("导出：封装迁移包 {done}/{total}"),
        );
    })?;

    let report = ExportReport {
        thread_count: scan.threads.len(),
        session_file_count: manifest.counts.session_file_count,
        archived_file_count: manifest.counts.archived_file_count,
        missing_file_count: manifest.counts.missing_file_count,
    };
    emit_progress(
        &mut progress,
        1.0,
        format!(
            "导出完成：{} 个线程，{} 个会话文件，{} 个归档文件",
            report.thread_count, report.session_file_count, report.archived_file_count
        ),
    );

    Ok(report)
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn write_threads_sqlite<F>(scan: &ScanReport, output_path: &Path, mut progress: F) -> Result<()>
where
    F: FnMut(usize, usize),
{
    let conn = Connection::open(output_path)?;
    conn.execute_batch(
        r#"
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            rollout_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            source TEXT NOT NULL,
            model_provider TEXT NOT NULL,
            cwd TEXT NOT NULL,
            title TEXT NOT NULL,
            sandbox_policy TEXT NOT NULL,
            approval_mode TEXT NOT NULL,
            tokens_used INTEGER NOT NULL DEFAULT 0,
            has_user_event INTEGER NOT NULL DEFAULT 0,
            archived INTEGER NOT NULL DEFAULT 0,
            archived_at INTEGER,
            git_sha TEXT,
            git_branch TEXT,
            git_origin_url TEXT,
            cli_version TEXT NOT NULL DEFAULT '',
            first_user_message TEXT NOT NULL DEFAULT '',
            agent_nickname TEXT,
            agent_role TEXT,
            memory_mode TEXT NOT NULL DEFAULT 'enabled',
            model TEXT,
            reasoning_effort TEXT,
            agent_path TEXT
        );
        "#,
    )?;

    let total_threads = scan.threads.len();
    if total_threads == 0 {
        progress(0, 0);
    }

    for (index, thread) in scan.threads.iter().enumerate() {
        insert_thread(&conn, thread)?;
        progress(index + 1, total_threads);
    }

    Ok(())
}

fn insert_thread(conn: &Connection, thread: &ThreadRecord) -> Result<()> {
    conn.execute(
        r#"
        INSERT INTO threads (
            id, rollout_path, created_at, updated_at, source, model_provider, cwd, title,
            sandbox_policy, approval_mode, tokens_used, has_user_event, archived, archived_at,
            git_sha, git_branch, git_origin_url, cli_version, first_user_message, agent_nickname,
            agent_role, memory_mode, model, reasoning_effort, agent_path
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25
        )
        "#,
        params![
            thread.id,
            thread.rollout_path,
            thread.created_at,
            thread.updated_at,
            thread.source,
            thread.model_provider,
            thread.cwd,
            thread.title,
            thread.sandbox_policy,
            thread.approval_mode,
            thread.tokens_used,
            if thread.has_user_event { 1_i64 } else { 0_i64 },
            if thread.archived { 1_i64 } else { 0_i64 },
            thread.archived_at,
            thread.git_sha,
            thread.git_branch,
            thread.git_origin_url,
            thread.cli_version,
            thread.first_user_message,
            thread.agent_nickname,
            thread.agent_role,
            thread.memory_mode,
            thread.model,
            thread.reasoning_effort,
            thread.agent_path,
        ],
    )?;

    Ok(())
}

fn emit_progress<F>(progress: &mut F, fraction: f32, message: impl Into<String>)
where
    F: FnMut(OperationProgress),
{
    progress(OperationProgress::new(fraction, message));
}

fn remap_fraction(fraction: f32, start: f32, end: f32) -> f32 {
    start + (end - start) * fraction.clamp(0.0, 1.0)
}

fn ratio(done: usize, total: usize) -> f32 {
    if total == 0 {
        1.0
    } else {
        done as f32 / total as f32
    }
}
