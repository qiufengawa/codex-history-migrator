use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use rusqlite::{Connection, params};
use tempfile::tempdir;
use walkdir::WalkDir;

use crate::core::checksum::compute_sha256_hex;
use crate::core::merge::{MergeDecision, merge_thread};
use crate::core::path_rewrite::rewrite_rollout_path;
use crate::db::threads::load_threads;
use crate::fs::package::unpack_zip_to_dir_with_progress;
use crate::models::import_report::ImportReport;
use crate::models::manifest::Manifest;
use crate::models::operation_progress::OperationProgress;
use crate::models::thread_record::ThreadRecord;

pub fn import_package(
    package: &Path,
    target_home: &Path,
    make_backup: bool,
) -> Result<ImportReport> {
    import_package_with_progress(package, target_home, make_backup, |_| {})
}

pub fn import_package_with_progress<F>(
    package: &Path,
    target_home: &Path,
    make_backup: bool,
    mut progress: F,
) -> Result<ImportReport>
where
    F: FnMut(OperationProgress),
{
    let temp = tempdir()?;
    emit_progress(
        &mut progress,
        0.03,
        format!("导入：解压迁移包 {}", package.display()),
    );
    unpack_zip_to_dir_with_progress(package, temp.path(), |done, total| {
        let fraction = remap_fraction(ratio(done, total), 0.03, 0.18);
        emit_progress(
            &mut progress,
            fraction,
            format!("导入：解压迁移包 {done}/{total}"),
        );
    })?;

    emit_progress(&mut progress, 0.22, "导入：读取迁移包清单");
    let manifest: Manifest = serde_json::from_slice(&fs::read(temp.path().join("manifest.json"))?)?;

    emit_progress(&mut progress, 0.28, "导入：校验迁移包完整性");
    validate_import_package(temp.path(), &manifest)?;

    emit_progress(&mut progress, 0.32, "导入：读取线程数据库");
    let imported_threads = load_threads(&temp.path().join("db").join("threads.sqlite"))?;
    ensure_target_home_ready(target_home)?;

    if make_backup {
        emit_progress(
            &mut progress,
            0.36,
            format!("导入：创建目标备份 {}", target_home.display()),
        );
        create_chat_layer_backup(target_home)?;
    } else {
        emit_progress(&mut progress, 0.36, "导入：已跳过目标备份");
    }

    let session_source = temp.path().join("sessions");
    let archived_source = temp.path().join("archived_sessions");
    let total_payload_files =
        count_payload_files(&session_source)? + count_payload_files(&archived_source)?;
    let mut copied_payloads = 0usize;

    if total_payload_files == 0 {
        emit_progress(&mut progress, 0.62, "导入：迁移包中没有会话文件");
    }

    copy_payload_tree_with_progress(&session_source, &target_home.join("sessions"), |done, _| {
        copied_payloads = done;
        let fraction = remap_fraction(ratio(copied_payloads, total_payload_files), 0.42, 0.62);
        emit_progress(
            &mut progress,
            fraction,
            format!("导入：复制会话文件 {}/{}", copied_payloads, total_payload_files),
        );
    })?;

    let copied_after_sessions = copied_payloads;
    copy_payload_tree_with_progress(
        &archived_source,
        &target_home.join("archived_sessions"),
        |done, _| {
            copied_payloads = copied_after_sessions + done;
            let fraction =
                remap_fraction(ratio(copied_payloads, total_payload_files), 0.42, 0.62);
            emit_progress(
                &mut progress,
                fraction,
                format!("导入：复制会话文件 {}/{}", copied_payloads, total_payload_files),
            );
        },
    )?;

    if temp
        .path()
        .join("index")
        .join("session_index.jsonl")
        .exists()
    {
        emit_progress(&mut progress, 0.68, "导入：合并会话索引");
        merge_session_index(
            &temp.path().join("index").join("session_index.jsonl"),
            &target_home.join("session_index.jsonl"),
        )?;
    } else {
        emit_progress(&mut progress, 0.68, "导入：迁移包中没有会话索引");
    }

    let total_threads = imported_threads.len();
    let mut conn = Connection::open(target_home.join("state_5.sqlite"))?;
    let tx = conn.transaction()?;

    let old_root = manifest.source_root_prefix;
    let new_root = target_home.to_string_lossy().to_string();
    let mut inserted_threads = 0;
    let mut updated_threads = 0;
    let mut skipped_threads = 0;
    let mut repaired_paths = 0;

    if total_threads == 0 {
        emit_progress(&mut progress, 0.96, "导入：迁移包中没有线程记录");
    }

    for (index, mut imported) in imported_threads.into_iter().enumerate() {
        if let Some(rewritten) = rewrite_rollout_path(&imported.rollout_path, &old_root, &new_root)
        {
            imported.rollout_path = rewritten;
            repaired_paths += 1;
        }

        match fetch_existing_thread(&tx, &imported.id)? {
            None => {
                insert_thread(&tx, &imported)?;
                inserted_threads += 1;
            }
            Some(existing) => match merge_thread(&existing, &imported) {
                MergeDecision::InsertImported => {
                    insert_thread(&tx, &imported)?;
                    inserted_threads += 1;
                }
                MergeDecision::UpdateExisting => {
                    update_thread(&tx, &imported)?;
                    updated_threads += 1;
                }
                MergeDecision::KeepExisting => {
                    skipped_threads += 1;
                }
            },
        }

        let fraction = remap_fraction(ratio(index + 1, total_threads), 0.78, 0.98);
        emit_progress(
            &mut progress,
            fraction,
            format!("导入：合并线程 {}/{}", index + 1, total_threads),
        );
    }

    tx.commit()?;

    let report = ImportReport {
        inserted_threads,
        updated_threads,
        skipped_threads,
        repaired_paths,
    };
    emit_progress(
        &mut progress,
        1.0,
        format!(
            "导入完成：新增 {}，更新 {}，跳过 {}，修复路径 {}",
            report.inserted_threads,
            report.updated_threads,
            report.skipped_threads,
            report.repaired_paths
        ),
    );

    Ok(report)
}

fn validate_import_package(package_root: &Path, manifest: &Manifest) -> Result<()> {
    if manifest.format_version != 1 {
        bail!("unsupported package format version: {}", manifest.format_version);
    }

    let threads_db = package_root.join("db").join("threads.sqlite");
    if !threads_db.exists() {
        bail!("package is missing db/threads.sqlite");
    }

    let checksums_path = package_root.join("checksums.json");
    if !checksums_path.exists() {
        bail!("package is missing checksums.json");
    }

    let checksums: BTreeMap<String, String> = serde_json::from_slice(&fs::read(&checksums_path)?)?;
    if checksums.is_empty() {
        bail!("package checksums are empty");
    }

    for (relative_path, expected_checksum) in checksums {
        let safe_relative_path = sanitize_package_relative_path(&relative_path)?;
        let file_path = package_root.join(&safe_relative_path);
        if !file_path.exists() {
            bail!("package file missing for checksum: {}", safe_relative_path.display());
        }

        let actual_checksum = compute_sha256_hex(&file_path)?;
        if !actual_checksum.eq_ignore_ascii_case(expected_checksum.trim()) {
            bail!("package checksum mismatch: {}", safe_relative_path.display());
        }
    }

    Ok(())
}

fn sanitize_package_relative_path(relative_path: &str) -> Result<PathBuf> {
    let mut sanitized = PathBuf::new();

    for component in Path::new(relative_path).components() {
        match component {
            Component::Normal(part) => sanitized.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("package checksum path is invalid: {relative_path}")
            }
        }
    }

    if sanitized.as_os_str().is_empty() {
        bail!("package checksum path is empty");
    }

    Ok(sanitized)
}

fn ensure_target_home_ready(target_home: &Path) -> Result<()> {
    let state_db = target_home.join("state_5.sqlite");
    if !state_db.exists() {
        bail!("target state_5.sqlite not found at {}", state_db.display());
    }
    Ok(())
}

fn create_chat_layer_backup(target_home: &Path) -> Result<()> {
    let backup_root = target_home.join(".backups").join(unix_timestamp_string());
    fs::create_dir_all(&backup_root)?;

    for name in ["state_5.sqlite", "session_index.jsonl"] {
        let source = target_home.join(name);
        if source.exists() {
            fs::copy(&source, backup_root.join(name))?;
        }
    }

    copy_payload_tree(&target_home.join("sessions"), &backup_root.join("sessions"))?;
    copy_payload_tree(
        &target_home.join("archived_sessions"),
        &backup_root.join("archived_sessions"),
    )?;

    Ok(())
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn copy_payload_tree(source: &Path, target: &Path) -> Result<()> {
    copy_payload_tree_with_progress(source, target, |_, _| {})
}

fn copy_payload_tree_with_progress<F>(source: &Path, target: &Path, mut progress: F) -> Result<()>
where
    F: FnMut(usize, usize),
{
    if !source.exists() {
        progress(0, 0);
        return Ok(());
    }

    let files = collect_payload_files(source)?;
    let total_files = files.len();
    if total_files == 0 {
        progress(0, 0);
    }

    for (index, path) in files.iter().enumerate() {
        let relative = path.strip_prefix(source)?;
        let out_path = target.join(relative);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(path, out_path)?;
        progress(index + 1, total_files);
    }

    Ok(())
}

fn count_payload_files(source: &Path) -> Result<usize> {
    Ok(collect_payload_files(source)?.len())
}

fn collect_payload_files(source: &Path) -> Result<Vec<PathBuf>> {
    if !source.exists() {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();
    for entry in WalkDir::new(source)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if path.is_file() {
            files.push(path.to_path_buf());
        }
    }

    Ok(files)
}

fn merge_session_index(source_index: &Path, target_index: &Path) -> Result<()> {
    if !target_index.exists() {
        if let Some(parent) = target_index.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source_index, target_index)?;
        return Ok(());
    }

    let mut entries = BTreeMap::<String, String>::new();

    for line in fs::read_to_string(target_index)?.lines() {
        if let Some(id) = extract_index_id(line) {
            entries.insert(id, line.to_string());
        }
    }
    for line in fs::read_to_string(source_index)?.lines() {
        if let Some(id) = extract_index_id(line) {
            entries.insert(id, line.to_string());
        }
    }

    let body = entries.into_values().collect::<Vec<_>>().join("\n");
    fs::write(target_index, format!("{body}\n"))?;
    Ok(())
}

fn extract_index_id(line: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()?
        .get("id")?
        .as_str()
        .map(ToOwned::to_owned)
}

fn fetch_existing_thread(conn: &Connection, thread_id: &str) -> Result<Option<ThreadRecord>> {
    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, rollout_path, created_at, updated_at, source, model_provider, cwd, title,
            sandbox_policy, approval_mode, tokens_used, has_user_event, archived, archived_at,
            git_sha, git_branch, git_origin_url, cli_version, first_user_message, agent_nickname,
            agent_role, memory_mode, model, reasoning_effort, agent_path
        FROM threads
        WHERE id = ?1
        "#,
    )?;

    let mut rows = stmt.query(params![thread_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(ThreadRecord {
            id: row.get(0)?,
            rollout_path: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            source: row.get(4)?,
            model_provider: row.get(5)?,
            cwd: row.get(6)?,
            title: row.get(7)?,
            sandbox_policy: row.get(8)?,
            approval_mode: row.get(9)?,
            tokens_used: row.get(10)?,
            has_user_event: row.get::<_, i64>(11)? != 0,
            archived: row.get::<_, i64>(12)? != 0,
            archived_at: row.get(13)?,
            git_sha: row.get(14)?,
            git_branch: row.get(15)?,
            git_origin_url: row.get(16)?,
            cli_version: row.get(17)?,
            first_user_message: row.get(18)?,
            agent_nickname: row.get(19)?,
            agent_role: row.get(20)?,
            memory_mode: row.get(21)?,
            model: row.get(22)?,
            reasoning_effort: row.get(23)?,
            agent_path: row.get(24)?,
        }))
    } else {
        Ok(None)
    }
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

fn update_thread(conn: &Connection, thread: &ThreadRecord) -> Result<()> {
    conn.execute(
        r#"
        UPDATE threads
        SET
            rollout_path = ?2,
            created_at = ?3,
            updated_at = ?4,
            source = ?5,
            model_provider = ?6,
            cwd = ?7,
            title = ?8,
            sandbox_policy = ?9,
            approval_mode = ?10,
            tokens_used = ?11,
            has_user_event = ?12,
            archived = ?13,
            archived_at = ?14,
            git_sha = ?15,
            git_branch = ?16,
            git_origin_url = ?17,
            cli_version = ?18,
            first_user_message = ?19,
            agent_nickname = ?20,
            agent_role = ?21,
            memory_mode = ?22,
            model = ?23,
            reasoning_effort = ?24,
            agent_path = ?25
        WHERE id = ?1
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
