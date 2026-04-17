use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tempfile::tempdir;

use crate::core::checksum::compute_sha256_hex;
use crate::db::sqlite::{backup_database, open_connection};
use crate::db::threads::load_threads;
use crate::fs::codex_home::CodexHomePaths;
use crate::fs::package::write_zip_from_dir_with_progress;
use crate::models::export_report::ExportReport;
use crate::models::manage::{
    ArchiveUpdateReport, ArchivedFilter, DeleteToTrashReport, HealthFilter, ManageFilter,
    ManageHealth, ManageRow, PreviewEntry, RestoreTrashReport, TrashBatchSummary,
};
use crate::models::manifest::{Manifest, PackageCounts};
use crate::models::thread_record::ThreadRecord;

pub fn load_manage_rows(codex_home: &Path, filter: &ManageFilter) -> Result<Vec<ManageRow>> {
    let paths = CodexHomePaths::resolve(codex_home);
    let rows = load_threads(&paths.state_db)?
        .into_iter()
        .map(|thread| build_manage_row(codex_home, thread))
        .collect::<Vec<_>>();
    Ok(filter_manage_rows(&rows, filter))
}

pub fn filter_manage_rows(rows: &[ManageRow], filter: &ManageFilter) -> Vec<ManageRow> {
    let mut filtered = rows
        .iter()
        .filter(|row| matches_manage_filter(row, filter))
        .cloned()
        .collect::<Vec<_>>();
    filtered.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    filtered
}

pub fn load_preview_entries(
    codex_home: &Path,
    thread_id: &str,
    limit: usize,
) -> Result<Vec<PreviewEntry>> {
    let paths = CodexHomePaths::resolve(codex_home);
    let conn = open_connection(&paths.state_db)?;
    let thread =
        fetch_thread(&conn, thread_id)?.ok_or_else(|| anyhow!("thread not found: {thread_id}"))?;
    let payload_path = PathBuf::from(&thread.rollout_path);

    if !payload_path.exists() {
        return Ok(Vec::new());
    }

    let body = fs::read_to_string(&payload_path)
        .with_context(|| format!("failed to read {}", payload_path.display()))?;
    let mut entries = body
        .lines()
        .enumerate()
        .map(|(index, line)| parse_preview_entry(index + 1, line))
        .collect::<Vec<_>>();

    if limit > 0 && entries.len() > limit {
        let start = entries.len() - limit;
        entries = entries.split_off(start);
    }

    Ok(entries)
}

pub fn rename_thread(codex_home: &Path, thread_id: &str, new_title: &str) -> Result<()> {
    let title = new_title.trim();
    if title.is_empty() {
        bail!("thread title cannot be empty");
    }

    let paths = CodexHomePaths::resolve(codex_home);
    let conn = open_connection(&paths.state_db)?;
    conn.execute(
        "UPDATE threads SET title = ?1 WHERE id = ?2",
        params![title, thread_id],
    )?;

    let mut index_file = read_session_index_file(&paths.session_index)?;
    if let Some(line) = index_file.entries.get_mut(thread_id) {
        let mut value: Value = serde_json::from_str(line)?;
        if let Some(object) = value.as_object_mut() {
            object.insert("thread_name".to_string(), Value::String(title.to_string()));
            *line = serde_json::to_string(&value)?;
        }
    }
    write_session_index_file(&paths.session_index, &index_file)?;
    Ok(())
}

pub fn set_threads_archived(
    codex_home: &Path,
    thread_ids: &[String],
    archived: bool,
) -> Result<ArchiveUpdateReport> {
    let ids = normalize_ids(thread_ids);
    if ids.is_empty() {
        bail!("no thread ids selected");
    }

    let paths = CodexHomePaths::resolve(codex_home);
    let conn = open_connection(&paths.state_db)?;
    let archive_timestamp = archived.then_some(current_unix_timestamp());
    let mut operations = Vec::new();

    for thread_id in &ids {
        let thread = fetch_thread(&conn, thread_id)?
            .ok_or_else(|| anyhow!("thread not found: {thread_id}"))?;
        let source = PathBuf::from(&thread.rollout_path);
        let relative = relative_payload_path(codex_home, &source)
            .ok_or_else(|| anyhow!("thread has invalid rollout path: {thread_id}"))?;
        if !source.exists() {
            bail!("thread payload does not exist: {}", source.display());
        }

        let target_relative = replace_payload_root(&relative, archived)?;
        let target = codex_home.join(&target_relative);
        if source != target && target.exists() {
            bail!("target payload already exists: {}", target.display());
        }

        operations.push((thread_id.clone(), source, target, archive_timestamp));
    }

    let mut conn = open_connection(&paths.state_db)?;
    let tx = conn.transaction()?;
    let mut moved = Vec::new();

    let result: Result<()> = (|| {
        for (thread_id, source, target, archived_at) in &operations {
            if source != target {
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(source, target)?;
                moved.push((target.clone(), source.clone()));
            }

            tx.execute(
                r#"
                UPDATE threads
                SET archived = ?1, archived_at = ?2, rollout_path = ?3
                WHERE id = ?4
                "#,
                params![
                    if archived { 1_i64 } else { 0_i64 },
                    archived_at,
                    target.to_string_lossy().to_string(),
                    thread_id
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    })();

    if let Err(error) = result {
        revert_moves(&moved);
        return Err(error);
    }

    Ok(ArchiveUpdateReport {
        updated_ids: operations
            .into_iter()
            .map(|(thread_id, _, _, _)| thread_id)
            .collect(),
    })
}

pub fn list_trash_batches(codex_home: &Path) -> Result<Vec<TrashBatchSummary>> {
    let paths = CodexHomePaths::resolve(codex_home);
    if !paths.trash_dir.exists() {
        return Ok(Vec::new());
    }

    let mut summaries = Vec::new();
    for entry in fs::read_dir(&paths.trash_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let batch_path = entry.path();
        let manifest = read_trash_manifest(&batch_path.join("manifest.json"))?;
        summaries.push(TrashBatchSummary {
            batch_id: manifest.batch_id,
            path: batch_path,
            deleted_at: manifest.deleted_at,
            thread_count: manifest.items.len(),
            payload_count: manifest
                .items
                .iter()
                .filter(|item| item.payload_present)
                .count(),
        });
    }

    summaries.sort_by(|left, right| {
        right
            .deleted_at
            .cmp(&left.deleted_at)
            .then_with(|| right.batch_id.cmp(&left.batch_id))
    });
    Ok(summaries)
}

pub fn restore_trash_batch(codex_home: &Path, batch_id: &str) -> Result<RestoreTrashReport> {
    let paths = CodexHomePaths::resolve(codex_home);
    let batch_dir = paths.trash_dir.join(batch_id);
    let manifest_path = batch_dir.join("manifest.json");
    let batch_index_path = batch_dir.join("session_index.jsonl");

    let manifest = read_trash_manifest(&manifest_path)?;
    let batch_index = read_session_index_file(&batch_index_path)?;
    let original_index = read_session_index_file(&paths.session_index)?;
    let mut merged_index = original_index.clone();

    let mut conn = open_connection(&paths.state_db)?;
    let tx = conn.transaction()?;
    let mut moved = Vec::new();
    let mut restored_ids = Vec::new();
    let mut conflict_ids = Vec::new();
    let mut remaining_items = Vec::new();

    let result: Result<()> = (|| {
        for item in manifest.items {
            let thread_id = item.thread.id.clone();
            let has_db_conflict = fetch_thread(&tx, &thread_id)?.is_some();
            let has_index_conflict = merged_index.entries.contains_key(&thread_id);
            let target_path = codex_home.join(&item.relative_rollout_path);
            let source_path = batch_dir.join("payloads").join(&item.relative_rollout_path);
            let has_payload_conflict = item.payload_present && target_path.exists();
            let missing_trash_payload = item.payload_present && !source_path.exists();

            if has_db_conflict
                || has_index_conflict
                || has_payload_conflict
                || missing_trash_payload
            {
                conflict_ids.push(thread_id);
                remaining_items.push(item);
                continue;
            }

            insert_thread(&tx, &item.thread)?;
            if let Some(line) = batch_index.entries.get(&item.thread.id) {
                merged_index
                    .entries
                    .insert(item.thread.id.clone(), line.clone());
            }

            if item.payload_present {
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&source_path, &target_path)?;
                moved.push((target_path, source_path));
            }

            restored_ids.push(item.thread.id.clone());
        }

        write_session_index_file(&paths.session_index, &merged_index)?;
        tx.commit()?;
        Ok(())
    })();

    if let Err(error) = result {
        write_session_index_file(&paths.session_index, &original_index).ok();
        revert_moves(&moved);
        return Err(error);
    }

    if remaining_items.is_empty() {
        if batch_dir.exists() {
            fs::remove_dir_all(&batch_dir)?;
        }
    } else {
        let remaining_ids = remaining_items
            .iter()
            .map(|item| item.thread.id.clone())
            .collect::<BTreeSet<_>>();
        let remaining_manifest = TrashManifest {
            format_version: 1,
            batch_id: batch_id.to_string(),
            deleted_at: current_unix_timestamp(),
            items: remaining_items,
        };
        write_trash_manifest(&manifest_path, &remaining_manifest)?;

        let mut remaining_index = SessionIndexFile::default();
        for (thread_id, line) in batch_index.entries {
            if remaining_ids.contains(&thread_id) {
                remaining_index.entries.insert(thread_id, line);
            }
        }
        write_session_index_file(&batch_index_path, &remaining_index)?;
    }

    Ok(RestoreTrashReport {
        restored_ids,
        conflict_ids,
    })
}

pub fn purge_trash_batch(codex_home: &Path, batch_id: &str) -> Result<()> {
    let paths = CodexHomePaths::resolve(codex_home);
    let batch_dir = paths.trash_dir.join(batch_id);
    if batch_dir.exists() {
        fs::remove_dir_all(batch_dir)?;
    }
    Ok(())
}

pub fn purge_all_trash(codex_home: &Path) -> Result<usize> {
    let batches = list_trash_batches(codex_home)?;
    for batch in &batches {
        if batch.path.exists() {
            fs::remove_dir_all(&batch.path)?;
        }
    }
    Ok(batches.len())
}

pub fn export_selected_threads(
    source_home: &Path,
    output_file: &Path,
    selected_ids: &[String],
) -> Result<ExportReport> {
    export_selected_threads_with_progress(source_home, output_file, selected_ids, |_, _| {})
}

pub fn export_selected_threads_with_progress<F>(
    source_home: &Path,
    output_file: &Path,
    selected_ids: &[String],
    mut progress: F,
) -> Result<ExportReport>
where
    F: FnMut(usize, usize),
{
    let ids = normalize_ids(selected_ids);
    if ids.is_empty() {
        bail!("no thread ids selected");
    }

    let paths = CodexHomePaths::resolve(source_home);
    let all_threads = load_threads(&paths.state_db)?;
    let selected_threads = all_threads
        .into_iter()
        .filter(|thread| ids.contains(&thread.id))
        .collect::<Vec<_>>();
    if selected_threads.len() != ids.len() {
        bail!("some selected threads were not found");
    }

    let temp = tempdir()?;
    let package_root = temp.path();
    fs::create_dir_all(package_root.join("db"))?;
    fs::create_dir_all(package_root.join("index"))?;

    write_threads_sqlite(
        &selected_threads,
        &package_root.join("db").join("threads.sqlite"),
    )?;

    let mut checksums = BTreeMap::new();
    checksums.insert(
        "db/threads.sqlite".to_string(),
        compute_sha256_hex(&package_root.join("db").join("threads.sqlite"))?,
    );

    let mut session_file_count = 0usize;
    let mut archived_file_count = 0usize;
    let mut missing_file_count = 0usize;

    if selected_threads.is_empty() {
        progress(0, 0);
    }

    for (index, thread) in selected_threads.iter().enumerate() {
        let payload_path = PathBuf::from(&thread.rollout_path);
        match relative_payload_path(source_home, &payload_path) {
            Some(relative_path) if payload_path.exists() => {
                let target_path = package_root.join(&relative_path);
                if let Some(parent) = target_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(&payload_path, &target_path)?;
                checksums.insert(
                    relative_path.to_string_lossy().replace('\\', "/"),
                    compute_sha256_hex(&target_path)?,
                );
                if thread.archived {
                    archived_file_count += 1;
                } else {
                    session_file_count += 1;
                }
            }
            _ => {
                missing_file_count += 1;
            }
        }

        progress(index + 1, selected_threads.len());
    }

    let selected_index = filter_session_index_file(&paths.session_index, &ids)?;
    if !selected_index.entries.is_empty() || !selected_index.passthrough_lines.is_empty() {
        let index_path = package_root.join("index").join("session_index.jsonl");
        write_session_index_file(&index_path, &selected_index)?;
        checksums.insert(
            "index/session_index.jsonl".to_string(),
            compute_sha256_hex(&index_path)?,
        );
    }

    let manifest = Manifest {
        format_version: 1,
        tool_version: env!("CARGO_PKG_VERSION").to_string(),
        exported_at: current_unix_timestamp().to_string(),
        source_codex_home: source_home.to_string_lossy().to_string(),
        source_root_prefix: source_home.to_string_lossy().to_string(),
        counts: PackageCounts {
            thread_count: selected_threads.len(),
            session_file_count,
            archived_file_count,
            missing_file_count,
        },
    };
    fs::write(
        package_root.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    checksums.insert(
        "manifest.json".to_string(),
        compute_sha256_hex(&package_root.join("manifest.json"))?,
    );

    fs::write(
        package_root.join("checksums.json"),
        serde_json::to_vec_pretty(&checksums)?,
    )?;

    write_zip_from_dir_with_progress(package_root, output_file, |done, total| {
        progress(done, total.max(1));
    })?;

    Ok(ExportReport {
        thread_count: selected_threads.len(),
        session_file_count,
        archived_file_count,
        missing_file_count,
    })
}

#[derive(Debug, Clone, Default)]
struct SessionIndexFile {
    passthrough_lines: Vec<String>,
    entries: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrashManifest {
    format_version: u32,
    batch_id: String,
    deleted_at: i64,
    items: Vec<TrashManifestItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TrashManifestItem {
    thread: ThreadRecord,
    relative_rollout_path: String,
    payload_present: bool,
}

#[derive(Debug, Clone)]
struct DisplaySummary {
    primary: String,
    secondary: Option<String>,
    is_fallback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SummaryContext {
    ThreadTitle,
    ThreadSnippet,
    Preview,
}

fn build_manage_row(codex_home: &Path, thread: ThreadRecord) -> ManageRow {
    let rollout_path = PathBuf::from(&thread.rollout_path);
    let relative_rollout_path = relative_payload_path(codex_home, &rollout_path);
    let payload_exists = rollout_path.exists();
    let health = if relative_rollout_path.is_none() {
        ManageHealth::InvalidPath
    } else if !archive_state_matches(thread.archived, relative_rollout_path.as_ref()) {
        ManageHealth::ArchiveStateMismatch
    } else if !payload_exists {
        ManageHealth::MissingPayload
    } else {
        ManageHealth::Healthy
    };
    let can_toggle_archive = payload_exists && relative_rollout_path.is_some();
    let can_delete = relative_rollout_path.is_some();
    let title_summary = summarize_text_for_context(&thread.title, SummaryContext::ThreadTitle);
    let message_summary =
        summarize_text_for_context(&thread.first_user_message, SummaryContext::ThreadSnippet);
    let cwd_display = normalize_display_path(&thread.cwd);
    let rollout_path_display = normalize_display_path(&rollout_path.to_string_lossy());

    ManageRow {
        id: thread.id,
        title: thread.title,
        title_display: title_summary.primary,
        title_detail: title_summary.secondary,
        first_user_message: thread.first_user_message,
        first_user_message_display: message_summary.primary,
        updated_at: thread.updated_at,
        model_provider: thread.model_provider,
        model: thread.model,
        cwd: thread.cwd,
        cwd_display,
        archived: thread.archived,
        archived_at: thread.archived_at,
        rollout_path,
        rollout_path_display,
        relative_rollout_path,
        payload_exists,
        preview_available: payload_exists,
        can_open_payload: payload_exists,
        can_toggle_archive,
        can_delete,
        health,
    }
}

fn matches_manage_filter(row: &ManageRow, filter: &ManageFilter) -> bool {
    let keyword = filter.keyword.trim().to_ascii_lowercase();
    if !keyword.is_empty() {
        let haystacks = [
            row.title.to_ascii_lowercase(),
            row.first_user_message.to_ascii_lowercase(),
            row.id.to_ascii_lowercase(),
            row.cwd.to_ascii_lowercase(),
            row.model_provider.to_ascii_lowercase(),
            row.model.clone().unwrap_or_default().to_ascii_lowercase(),
        ];
        if !haystacks.iter().any(|value| value.contains(&keyword)) {
            return false;
        }
    }

    match filter.archived {
        ArchivedFilter::All => {}
        ArchivedFilter::ActiveOnly if row.archived => return false,
        ArchivedFilter::ArchivedOnly if !row.archived => return false,
        _ => {}
    }

    if let Some(provider) = filter
        .provider
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        if !row.model_provider.eq_ignore_ascii_case(provider.trim()) {
            return false;
        }
    }

    match filter.health {
        HealthFilter::All => true,
        HealthFilter::HealthyOnly => row.health == ManageHealth::Healthy,
        HealthFilter::NeedsAttentionOnly => row.health != ManageHealth::Healthy,
        HealthFilter::MissingPayloadOnly => row.health == ManageHealth::MissingPayload,
        HealthFilter::InvalidPathOnly => row.health == ManageHealth::InvalidPath,
        HealthFilter::ArchiveStateMismatchOnly => row.health == ManageHealth::ArchiveStateMismatch,
    }
}

fn parse_preview_entry(line_number: usize, line: &str) -> PreviewEntry {
    match serde_json::from_str::<Value>(line) {
        Ok(value) => {
            let entry_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("json")
                .to_string();
            let summary = preview_summary_from_value(&entry_type, &value);
            PreviewEntry {
                line_number,
                display_type: preview_type_label(&entry_type).to_string(),
                entry_type,
                text: summary.primary,
                is_fallback: summary.is_fallback,
            }
        }
        Err(_) => PreviewEntry {
            line_number,
            entry_type: "invalid_json".to_string(),
            display_type: "原始文本".to_string(),
            text: truncate_text(&collapse_whitespace(line.trim()), 180),
            is_fallback: true,
        },
    }
}

fn preview_summary_from_value(entry_type: &str, value: &Value) -> DisplaySummary {
    if let Some(summary) = summarize_tool_result_value(value, SummaryContext::Preview, entry_type) {
        return summary;
    }

    for field in summary_fields_for_entry(entry_type) {
        if let Some(candidate) = value.get(*field) {
            return summarize_value_for_context(candidate, SummaryContext::Preview, entry_type);
        }
    }

    summarize_value_for_context(value, SummaryContext::Preview, entry_type)
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
}

fn summarize_text_for_context(text: &str, context: SummaryContext) -> DisplaySummary {
    let trimmed = collapse_whitespace(text.trim());
    if trimmed.is_empty() {
        return DisplaySummary {
            primary: String::new(),
            secondary: None,
            is_fallback: false,
        };
    }

    if let Ok(value) = serde_json::from_str::<Value>(&trimmed) {
        return summarize_value_for_context(&value, context, "json");
    }

    DisplaySummary {
        primary: truncate_text(
            &trimmed,
            match context {
                SummaryContext::ThreadTitle => 64,
                SummaryContext::ThreadSnippet => 72,
                SummaryContext::Preview => 180,
            },
        ),
        secondary: None,
        is_fallback: false,
    }
}

fn summarize_value_for_context(
    value: &Value,
    context: SummaryContext,
    entry_type: &str,
) -> DisplaySummary {
    let normalized = normalize_summary_value(value);

    if let Some(summary) = summarize_mcp_value(&normalized, context) {
        return summary;
    }

    if let Some(summary) = summarize_codex_error_value(&normalized) {
        return summary;
    }

    if let Some(summary) = summarize_completion_value(&normalized) {
        return summary;
    }

    if let Some(summary) = summarize_thread_rollback_value(&normalized) {
        return summary;
    }

    if let Some(summary) = summarize_token_usage_value(&normalized) {
        return summary;
    }

    if let Some(summary) = summarize_tool_result_value(&normalized, context, entry_type) {
        return summary;
    }

    if let Some(summary) = summarize_known_text_field(&normalized, context, entry_type) {
        return summary;
    }

    if let Some(summary) = summarize_flat_json(&normalized, context) {
        return summary;
    }

    match normalized {
        Value::String(ref text) => DisplaySummary {
            primary: truncate_text(&collapse_whitespace(text), preview_limit_for(context)),
            secondary: None,
            is_fallback: false,
        },
        Value::Array(ref items) => DisplaySummary {
            primary: summarize_array(items, context),
            secondary: None,
            is_fallback: true,
        },
        Value::Object(ref object) => summarize_unknown_object(object, context),
        _ => DisplaySummary {
            primary: truncate_text(&compact_json(&normalized), preview_limit_for(context)),
            secondary: None,
            is_fallback: true,
        },
    }
}

fn normalize_summary_value(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| value.clone())
        }
        _ => value.clone(),
    }
}

fn summarize_mcp_value(value: &Value, context: SummaryContext) -> Option<DisplaySummary> {
    let object = find_mcp_object(value)?;
    let name = object.get("name")?.as_str()?.trim();
    if name.is_empty() {
        return None;
    }

    let transport = object
        .get("transport")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let command_name = object
        .get("command")
        .and_then(Value::as_str)
        .and_then(command_display_name);

    let mut details = Vec::new();
    if let Some(transport) = transport {
        details.push(transport.to_string());
    }
    if let Some(command_name) = command_name {
        details.push(command_name);
    }

    Some(match context {
        SummaryContext::ThreadTitle => DisplaySummary {
            primary: name.to_string(),
            secondary: Some(if details.is_empty() {
                "MCP 服务".to_string()
            } else {
                format!("MCP 服务 · {}", details.join(" · "))
            }),
            is_fallback: false,
        },
        SummaryContext::ThreadSnippet => DisplaySummary {
            primary: name.to_string(),
            secondary: None,
            is_fallback: false,
        },
        SummaryContext::Preview => DisplaySummary {
            primary: if details.is_empty() {
                format!("MCP 服务：{name}")
            } else {
                format!("MCP 服务：{name} · {}", details.join(" · "))
            },
            secondary: None,
            is_fallback: false,
        },
    })
}

fn summarize_codex_error_value(value: &Value) -> Option<DisplaySummary> {
    let info = value.get("codex_error_info")?;
    let object = info.as_object()?;
    let mut error_code = None;
    let mut http_status = object.get("http_status_code").and_then(Value::as_i64);

    for (key, nested) in object {
        if key == "http_status_code" {
            continue;
        }
        error_code = Some(key.as_str());
        if http_status.is_none() {
            http_status = nested
                .get("http_status_code")
                .and_then(Value::as_i64)
                .or_else(|| nested.as_i64());
        }
        break;
    }

    let mut text = match error_code {
        Some("response_too_many_failed_attempts") => "请求失败次数过多".to_string(),
        Some(code) => format!("请求失败：{code}"),
        None => "请求失败".to_string(),
    };
    if let Some(status) = http_status {
        text.push_str(&format!("（HTTP {status}）"));
    }

    Some(DisplaySummary {
        primary: text,
        secondary: None,
        is_fallback: false,
    })
}

fn summarize_completion_value(value: &Value) -> Option<DisplaySummary> {
    let object = value.as_object()?;
    if !object.contains_key("completed_at") && !object.contains_key("duration_ms") {
        return None;
    }

    let mut text = "任务完成".to_string();
    if let Some(duration_ms) = object.get("duration_ms").and_then(Value::as_i64) {
        text.push_str(&format!("，耗时 {duration_ms} ms"));
    }

    Some(DisplaySummary {
        primary: text,
        secondary: None,
        is_fallback: false,
    })
}

fn summarize_thread_rollback_value(value: &Value) -> Option<DisplaySummary> {
    let object = value.as_object()?;
    if object.get("type").and_then(Value::as_str) != Some("thread_rolled_back") {
        return None;
    }

    let turns = object.get("num_turns").and_then(Value::as_i64);
    let primary = match turns {
        Some(turns) if turns > 0 => format!("会话已回滚（{turns} 个回合）"),
        _ => "会话已回滚".to_string(),
    };
    Some(DisplaySummary {
        primary,
        secondary: None,
        is_fallback: false,
    })
}

fn summarize_token_usage_value(value: &Value) -> Option<DisplaySummary> {
    let usage = value
        .get("info")
        .and_then(|info| info.get("last_token_usage"))
        .or_else(|| value.get("last_token_usage"))?
        .as_object()?;

    let input = usage
        .get("input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let output = usage
        .get("output_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let cached = usage
        .get("cached_input_tokens")
        .and_then(Value::as_i64)
        .unwrap_or(0);

    Some(DisplaySummary {
        primary: format!("Token 使用：输入 {input}，输出 {output}，缓存 {cached}"),
        secondary: None,
        is_fallback: false,
    })
}

fn summarize_known_text_field(
    value: &Value,
    context: SummaryContext,
    entry_type: &str,
) -> Option<DisplaySummary> {
    let object = value.as_object()?;
    for field in summary_fields_for_entry(entry_type) {
        if let Some(candidate) = object.get(*field) {
            let summary = summarize_value_for_context(candidate, context, entry_type);
            if !summary.primary.is_empty() {
                return Some(summary);
            }
        }
    }
    None
}

fn summarize_flat_json(value: &Value, context: SummaryContext) -> Option<DisplaySummary> {
    let object = value.as_object()?;
    if object.is_empty() || object.len() > 3 {
        return None;
    }

    let mut parts = Vec::new();
    for (key, value) in object {
        let primitive = primitive_value_text(value)?;
        parts.push(format!("{key}: {primitive}"));
    }

    Some(DisplaySummary {
        primary: truncate_text(&parts.join(" · "), preview_limit_for(context)),
        secondary: None,
        is_fallback: false,
    })
}

fn summarize_array(items: &[Value], context: SummaryContext) -> String {
    let primitive_items = items
        .iter()
        .filter_map(primitive_value_text)
        .take(3)
        .collect::<Vec<_>>();
    if primitive_items.is_empty() {
        format!("数组（{} 项）", items.len())
    } else {
        truncate_text(&primitive_items.join(" · "), preview_limit_for(context))
    }
}

fn summary_fields_for_entry(entry_type: &str) -> &'static [&'static str] {
    match entry_type {
        "event_msg" => &["text", "message", "content", "body", "payload"],
        "tool_result" => &["content", "text", "message", "body", "payload"],
        _ => &["payload", "text", "content", "message", "body"],
    }
}

fn summarize_tool_result_value(
    value: &Value,
    context: SummaryContext,
    entry_type: &str,
) -> Option<DisplaySummary> {
    if entry_type != "tool_result" {
        return None;
    }

    let object = value.as_object()?;
    let tool_name = object
        .get("tool_name")
        .or_else(|| object.get("name"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned);
    let content_text = object.get("content").and_then(extract_tool_result_text);

    if tool_name.is_none() && content_text.is_none() {
        return None;
    }

    let primary = match (tool_name, content_text) {
        (Some(name), Some(text)) => format!("{name}: {text}"),
        (Some(name), None) => name,
        (None, Some(text)) => text,
        (None, None) => String::new(),
    };

    Some(DisplaySummary {
        primary: truncate_text(&primary, preview_limit_for(context)),
        secondary: None,
        is_fallback: false,
    })
}

fn extract_tool_result_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let collapsed = collapse_whitespace(text);
            if collapsed.is_empty() {
                None
            } else {
                Some(format_path_hint(&collapsed))
            }
        }
        Value::Array(items) => {
            let fragments = items
                .iter()
                .filter_map(extract_tool_result_text)
                .filter(|fragment| !fragment.is_empty())
                .take(3)
                .collect::<Vec<_>>();
            if fragments.is_empty() {
                None
            } else {
                Some(fragments.join(" 路 "))
            }
        }
        Value::Object(object) => {
            for key in [
                "text",
                "output_text",
                "content",
                "value",
                "path",
                "file",
                "file_path",
            ] {
                if let Some(candidate) = object.get(key) {
                    if let Some(text) = extract_tool_result_text(candidate) {
                        if !text.is_empty() {
                            return Some(text);
                        }
                    }
                }
            }
            None
        }
        _ => primitive_value_text(value),
    }
}

fn summarize_unknown_object(
    object: &Map<String, Value>,
    context: SummaryContext,
) -> DisplaySummary {
    if object.is_empty() {
        return DisplaySummary {
            primary: "{}".to_string(),
            secondary: None,
            is_fallback: true,
        };
    }

    let mut keys = object.keys().take(4).cloned().collect::<Vec<_>>();
    keys.sort();
    let key_part = if object.len() > 4 {
        format!("未知对象字段：{} 等 {} 项", keys.join("、"), object.len())
    } else {
        format!("未知对象字段：{}", keys.join("、"))
    };
    let path_hint = object.values().find_map(path_like_hint);
    let mut primary = key_part;
    if let Some(path_hint) = path_hint {
        primary.push_str(" 路 ");
        primary.push_str(&path_hint);
    }

    DisplaySummary {
        primary: truncate_text(&primary, preview_limit_for(context)),
        secondary: None,
        is_fallback: true,
    }
}

fn path_like_hint(value: &Value) -> Option<String> {
    let text = value.as_str()?;
    let normalized = normalize_display_path(text);
    if normalized.contains('\\') || normalized.contains('/') {
        Some(format_path_hint(&normalized))
    } else {
        None
    }
}

fn format_path_hint(text: &str) -> String {
    let normalized = normalize_display_path(text);
    let Some(file_name) = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
    else {
        return normalized;
    };

    if file_name == normalized {
        return file_name;
    }
    format!("{file_name} ({normalized})")
}

fn find_mcp_object(value: &Value) -> Option<&Map<String, Value>> {
    find_mcp_object_with_depth(value, 0)
}

fn find_mcp_object_with_depth(value: &Value, depth: usize) -> Option<&Map<String, Value>> {
    if depth > 4 {
        return None;
    }

    let object = value.as_object()?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty());
    if name.is_some()
        && (object.contains_key("command")
            || object.contains_key("transport")
            || object.contains_key("args"))
    {
        return Some(object);
    }

    for key in [
        "mcp",
        "event_msg",
        "input",
        "server",
        "payload",
        "text",
        "content",
        "body",
    ] {
        if let Some(nested) = object.get(key) {
            if let Some(found) = find_mcp_object_with_depth(nested, depth + 1) {
                return Some(found);
            }
        }
    }
    for nested in object.values() {
        if let Some(found) = find_mcp_object_with_depth(nested, depth + 1) {
            return Some(found);
        }
    }
    None
}

fn primitive_value_text(value: &Value) -> Option<String> {
    match value {
        Value::Null => Some("null".to_string()),
        Value::Bool(flag) => Some(if *flag { "true" } else { "false" }.to_string()),
        Value::Number(number) => Some(number.to_string()),
        Value::String(text) => Some(collapse_whitespace(text)),
        _ => None,
    }
}

fn preview_type_label(entry_type: &str) -> &'static str {
    match entry_type {
        "user_message" => "用户",
        "assistant_message" => "助手",
        "tool_result" => "工具结果",
        "event_msg" => "系统事件",
        "invalid_json" => "原始文本",
        _ => "记录",
    }
}

fn preview_limit_for(context: SummaryContext) -> usize {
    match context {
        SummaryContext::ThreadTitle => 64,
        SummaryContext::ThreadSnippet => 72,
        SummaryContext::Preview => 180,
    }
}

fn truncate_text(text: &str, max_chars: usize) -> String {
    let mut output = String::new();
    let mut count = 0usize;
    for ch in text.chars() {
        if count >= max_chars {
            output.push('…');
            return output;
        }
        output.push(ch);
        count += 1;
    }
    output
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_display_path(text: &str) -> String {
    text.trim()
        .strip_prefix(r"\\?\")
        .unwrap_or(text.trim())
        .to_string()
}

fn command_display_name(command: &str) -> Option<String> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return None;
    }

    Path::new(trimmed)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .or_else(|| Some(trimmed.to_string()))
}

fn normalize_ids(thread_ids: &[String]) -> BTreeSet<String> {
    thread_ids
        .iter()
        .map(|thread_id| thread_id.trim())
        .filter(|thread_id| !thread_id.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn read_session_index_file(path: &Path) -> Result<SessionIndexFile> {
    if !path.exists() {
        return Ok(SessionIndexFile::default());
    }

    let mut file = SessionIndexFile::default();
    for line in fs::read_to_string(path)?.lines() {
        match extract_index_id(line) {
            Some(id) => {
                file.entries.insert(id, line.to_string());
            }
            None => file.passthrough_lines.push(line.to_string()),
        }
    }
    Ok(file)
}

fn filter_session_index_file(
    path: &Path,
    selected_ids: &BTreeSet<String>,
) -> Result<SessionIndexFile> {
    let mut filtered = SessionIndexFile::default();
    let original = read_session_index_file(path)?;
    filtered.passthrough_lines = original.passthrough_lines;
    filtered.entries = original
        .entries
        .into_iter()
        .filter(|(thread_id, _)| selected_ids.contains(thread_id))
        .collect();
    Ok(filtered)
}

fn write_session_index_file(path: &Path, file: &SessionIndexFile) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut lines = file.passthrough_lines.clone();
    lines.extend(file.entries.values().cloned());
    let body = if lines.is_empty() {
        String::new()
    } else {
        format!("{}\n", lines.join("\n"))
    };
    fs::write(path, body)?;
    Ok(())
}

fn extract_index_id(line: &str) -> Option<String> {
    serde_json::from_str::<Value>(line)
        .ok()?
        .get("id")?
        .as_str()
        .map(ToOwned::to_owned)
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

fn archive_state_matches(archived: bool, relative_path: Option<&PathBuf>) -> bool {
    let Some(relative_path) = relative_path else {
        return false;
    };
    let Some(Component::Normal(top_level)) = relative_path.components().next() else {
        return false;
    };

    if archived {
        top_level == "archived_sessions"
    } else {
        top_level == "sessions"
    }
}

fn replace_payload_root(relative_path: &Path, archived: bool) -> Result<PathBuf> {
    let mut components = relative_path.components();
    let first = components
        .next()
        .ok_or_else(|| anyhow!("relative payload path is empty"))?;
    let Component::Normal(_) = first else {
        bail!("relative payload path is invalid");
    };

    let mut replaced = PathBuf::new();
    replaced.push(if archived {
        "archived_sessions"
    } else {
        "sessions"
    });
    for component in components {
        match component {
            Component::Normal(part) => replaced.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                bail!("relative payload path is invalid")
            }
        }
    }
    Ok(replaced)
}

fn fetch_thread(conn: &Connection, thread_id: &str) -> Result<Option<ThreadRecord>> {
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

    stmt.query_row(params![thread_id], |row| {
        Ok(ThreadRecord {
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
        })
    })
    .optional()
    .map_err(Into::into)
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

fn write_threads_sqlite(threads: &[ThreadRecord], output_path: &Path) -> Result<()> {
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

    for thread in threads {
        insert_thread(&conn, thread)?;
    }

    Ok(())
}

fn write_trash_manifest(path: &Path, manifest: &TrashManifest) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(manifest)?)?;
    Ok(())
}

fn read_trash_manifest(path: &Path) -> Result<TrashManifest> {
    serde_json::from_slice(&fs::read(path)?).map_err(Into::into)
}

fn create_manage_backup(paths: &CodexHomePaths, prefix: &str) -> Result<PathBuf> {
    fs::create_dir_all(&paths.backup_dir)?;
    let backup_dir = paths
        .backup_dir
        .join(format!("{prefix}-{}", generate_batch_id()));
    fs::create_dir_all(&backup_dir)?;

    backup_database(&paths.state_db, &backup_dir.join("state_5.sqlite"))?;
    if paths.session_index.exists() {
        fs::copy(&paths.session_index, backup_dir.join("session_index.jsonl"))?;
    }

    Ok(backup_dir)
}

fn revert_moves(moves: &[(PathBuf, PathBuf)]) {
    for (from, to) in moves.iter().rev() {
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).ok();
        }
        if from.exists() {
            let _ = fs::rename(from, to);
        }
    }
}

fn generate_batch_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!(
        "{}-{:06x}",
        now.as_millis(),
        now.subsec_nanos() & 0x00ff_ffff
    )
}

fn current_unix_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

pub fn delete_threads_to_trash(
    codex_home: &Path,
    thread_ids: &[String],
    create_backup: bool,
) -> Result<DeleteToTrashReport> {
    let ids = normalize_ids(thread_ids);
    if ids.is_empty() {
        bail!("no thread ids selected");
    }

    let paths = CodexHomePaths::resolve(codex_home);
    let conn = open_connection(&paths.state_db)?;
    let mut manifest_items = Vec::new();

    for thread_id in &ids {
        let thread = fetch_thread(&conn, thread_id)?
            .ok_or_else(|| anyhow!("thread not found: {thread_id}"))?;
        let rollout_path = PathBuf::from(&thread.rollout_path);
        let relative_rollout_path = relative_payload_path(codex_home, &rollout_path)
            .ok_or_else(|| anyhow!("thread has invalid rollout path: {thread_id}"))?;
        manifest_items.push(TrashManifestItem {
            thread,
            relative_rollout_path: relative_rollout_path.to_string_lossy().replace('\\', "/"),
            payload_present: rollout_path.exists(),
        });
    }

    let backup_path = if create_backup {
        Some(create_manage_backup(&paths, "manage-delete")?)
    } else {
        None
    };

    let batch_id = generate_batch_id();
    let batch_dir = paths.trash_dir.join(&batch_id);
    fs::create_dir_all(batch_dir.join("payloads"))?;

    let manifest = TrashManifest {
        format_version: 1,
        batch_id: batch_id.clone(),
        deleted_at: current_unix_timestamp(),
        items: manifest_items,
    };
    write_trash_manifest(&batch_dir.join("manifest.json"), &manifest)?;

    let original_index = read_session_index_file(&paths.session_index)?;
    let mut remaining_index = original_index.clone();
    let mut deleted_index = SessionIndexFile::default();
    for thread_id in &ids {
        if let Some(line) = remaining_index.entries.remove(thread_id) {
            deleted_index.entries.insert(thread_id.clone(), line);
        }
    }
    write_session_index_file(&batch_dir.join("session_index.jsonl"), &deleted_index)?;

    let mut moved = Vec::new();
    let mut conn = open_connection(&paths.state_db)?;
    let tx = conn.transaction()?;

    let result: Result<()> = (|| {
        for item in &manifest.items {
            if item.payload_present {
                let source = PathBuf::from(&item.thread.rollout_path);
                let target = batch_dir.join("payloads").join(&item.relative_rollout_path);
                if let Some(parent) = target.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&source, &target)?;
                moved.push((target, source));
            }
        }

        for thread_id in &ids {
            tx.execute("DELETE FROM threads WHERE id = ?1", params![thread_id])?;
        }

        write_session_index_file(&paths.session_index, &remaining_index)?;
        tx.commit()?;
        Ok(())
    })();

    if let Err(error) = result {
        write_session_index_file(&paths.session_index, &original_index).ok();
        revert_moves(&moved);
        return Err(error);
    }

    Ok(DeleteToTrashReport {
        batch_id,
        trash_dir: batch_dir,
        deleted_count: ids.len(),
        backup_path,
    })
}
