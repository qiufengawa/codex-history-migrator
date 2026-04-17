mod support;

use std::fs::File;
use std::io::{Read, Write};

use codex_history_migrator::core::manage::{
    delete_threads_to_trash, export_selected_threads, list_trash_batches, load_manage_rows,
    load_preview_entries, purge_all_trash, purge_trash_batch, rename_thread, restore_trash_batch,
    set_threads_archived,
};
use codex_history_migrator::models::manage::{
    ArchivedFilter, HealthFilter, ManageFilter, ManageHealth,
};
use rusqlite::Connection;
use tempfile::tempdir;
use zip::ZipArchive;

use self::support::create_manage_codex_home;

#[test]
fn manage_rows_sort_and_filter_metadata_and_health() {
    let fixture = create_manage_codex_home();

    let rows = load_manage_rows(fixture.codex_home(), &ManageFilter::default()).unwrap();
    let ids = rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            "thread-g", "thread-b", "thread-c", "thread-d", "thread-a", "thread-e", "thread-f",
        ]
    );

    assert_eq!(
        rows.iter().find(|row| row.id == "thread-d").unwrap().health,
        ManageHealth::MissingPayload
    );
    assert_eq!(
        rows.iter().find(|row| row.id == "thread-e").unwrap().health,
        ManageHealth::ArchiveStateMismatch
    );
    assert_eq!(
        rows.iter().find(|row| row.id == "thread-f").unwrap().health,
        ManageHealth::InvalidPath
    );

    let filtered = load_manage_rows(
        fixture.codex_home(),
        &ManageFilter {
            keyword: "beta".to_string(),
            archived: ArchivedFilter::ActiveOnly,
            provider: Some("openai".to_string()),
            health: HealthFilter::HealthyOnly,
        },
    )
    .unwrap();
    assert_eq!(
        filtered
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-b"]
    );

    let needs_attention = load_manage_rows(
        fixture.codex_home(),
        &ManageFilter {
            keyword: String::new(),
            archived: ArchivedFilter::ArchivedOnly,
            provider: Some("anthropic".to_string()),
            health: HealthFilter::NeedsAttentionOnly,
        },
    )
    .unwrap();
    assert_eq!(
        needs_attention
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["thread-e"]
    );

    let structured = rows.iter().find(|row| row.id == "thread-g").unwrap();
    assert_eq!(structured.title_display, "pencil");
    assert!(
        structured
            .title_detail
            .as_deref()
            .is_some_and(|detail: &str| {
                detail.contains("MCP") && detail.contains("mcp-server-windows-x64.exe")
            })
    );
    assert_eq!(structured.first_user_message_display, "pencil");
    assert_eq!(structured.cwd_display, r"C:\Users\Admin\Desktop\Task");
}

#[test]
fn preview_parses_recent_entries_and_tolerates_unknown_and_bad_lines() {
    let fixture = create_manage_codex_home();

    let preview = load_preview_entries(fixture.codex_home(), "thread-a", 8).unwrap();

    assert_eq!(preview.len(), 5);
    assert!(
        preview.iter().any(|entry| {
            entry.entry_type == "user_message" && entry.text.contains("hello alpha")
        })
    );
    assert!(preview.iter().any(|entry| {
        entry.entry_type == "assistant_message" && entry.text.contains("alpha reply")
    }));
    assert!(preview.iter().any(|entry| {
        entry.entry_type == "invalid_json"
            && entry.is_fallback
            && entry.text.contains("not-json-at-all")
    }));
    assert!(
        preview
            .iter()
            .any(|entry| { entry.entry_type == "tool_result" && entry.text.contains("42") })
    );
    assert!(
        preview
            .iter()
            .any(|entry| { entry.text.contains("mystery: shape") })
    );
}

#[test]
fn structured_titles_and_event_messages_are_summarized_for_display() {
    let fixture = create_manage_codex_home();

    let preview = load_preview_entries(fixture.codex_home(), "thread-g", 16).unwrap();

    assert!(preview.iter().any(|entry| {
        entry.entry_type == "event_msg"
            && entry.display_type == "系统事件"
            && entry.text.contains("MCP 服务")
            && entry.text.contains("pencil")
    }));
    assert!(preview.iter().any(|entry| {
        entry.entry_type == "event_msg"
            && entry.text.contains("请求失败")
            && !entry.text.contains("{\"codex_error_info\"")
    }));
    assert!(
        preview.iter().any(|entry| {
            entry.entry_type == "event_msg" && entry.text.contains("任务完成")
        })
    );
    assert!(preview.iter().any(|entry| {
        entry.entry_type == "event_msg" && entry.text.contains("会话已回滚")
    }));
    assert!(
        preview.iter().any(|entry| {
            entry.entry_type == "event_msg" && entry.text.contains("Token 使用")
        })
    );
}

#[test]
fn tool_result_and_unknown_objects_have_readable_preview_text() {
    let fixture = create_manage_codex_home();

    let preview = load_preview_entries(fixture.codex_home(), "thread-g", 24).unwrap();

    assert!(preview.iter().any(|entry| {
        entry.entry_type == "event_msg"
            && entry.text.contains("filesystem")
            && entry.text.contains("fs-server.exe")
    }));
    assert!(preview.iter().any(|entry| {
        entry.entry_type == "tool_result"
            && entry.text.contains("read_file")
            && entry.text.contains("README.md")
            && entry.text.contains("line 1: hello")
    }));
    assert!(preview.iter().any(|entry| {
        entry.entry_type == "event_msg"
            && entry.is_fallback
            && entry.text.contains("alpha")
            && entry.text.contains("beta")
            && !entry.text.contains("{\"alpha\"")
    }));
}

#[test]
fn rename_updates_thread_title_and_session_index_entry() {
    let fixture = create_manage_codex_home();

    rename_thread(fixture.codex_home(), "thread-a", "Renamed Alpha").unwrap();

    assert_eq!(fixture.thread_title("thread-a"), "Renamed Alpha");
    assert_eq!(
        fixture.session_index_thread_name("thread-a").as_deref(),
        Some("Renamed Alpha")
    );
}

#[test]
fn archive_and_unarchive_move_payload_and_update_thread_metadata() {
    let fixture = create_manage_codex_home();
    let original_path = fixture.payload_path_for_thread("thread-a");
    assert!(original_path.exists());

    set_threads_archived(fixture.codex_home(), &["thread-a".to_string()], true).unwrap();

    let archived_path = fixture.payload_path_for_thread("thread-a");
    assert!(!original_path.exists());
    assert!(archived_path.exists());
    assert!(fixture.thread_archived("thread-a"));
    assert!(fixture.thread_archived_at("thread-a").is_some());
    assert!(
        archived_path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("archived_sessions/2026/04/16/rollout-a.jsonl")
    );

    set_threads_archived(fixture.codex_home(), &["thread-a".to_string()], false).unwrap();

    let restored_path = fixture.payload_path_for_thread("thread-a");
    assert!(!archived_path.exists());
    assert!(restored_path.exists());
    assert!(!fixture.thread_archived("thread-a"));
    assert_eq!(fixture.thread_archived_at("thread-a"), None);
    assert!(
        restored_path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("sessions/2026/04/16/rollout-a.jsonl")
    );
}

#[test]
fn delete_to_trash_creates_backup_batch_and_removes_live_records() {
    let fixture = create_manage_codex_home();
    let alpha_path = fixture.payload_path_for_thread("thread-a");
    let archived_path = fixture.payload_path_for_thread("thread-c");

    let report = delete_threads_to_trash(
        fixture.codex_home(),
        &["thread-a".to_string(), "thread-c".to_string()],
        true,
    )
    .unwrap();

    assert_eq!(report.deleted_count, 2);
    let backup_path = report.backup_path.clone().expect("backup path");
    assert!(backup_path.exists());
    assert_eq!(fixture.backup_file_count(), 1);
    assert!(!fixture.thread_exists("thread-a"));
    assert!(!fixture.thread_exists("thread-c"));
    assert!(!fixture.session_index_has_id("thread-a"));
    assert!(!fixture.session_index_has_id("thread-c"));
    assert!(!alpha_path.exists());
    assert!(!archived_path.exists());

    let batches = list_trash_batches(fixture.codex_home()).unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].thread_count, 2);
    assert!(batches[0].path.join("manifest.json").exists());
    assert!(batches[0].path.join("session_index.jsonl").exists());
    assert!(
        batches[0]
            .path
            .join("payloads")
            .join("sessions/2026/04/16/rollout-a.jsonl")
            .exists()
    );
    assert!(
        batches[0]
            .path
            .join("payloads")
            .join("archived_sessions/2026/04/14/rollout-c.jsonl")
            .exists()
    );
}

#[test]
fn restore_skips_conflicts_and_keeps_remaining_trash_entries() {
    let fixture = create_manage_codex_home();

    delete_threads_to_trash(
        fixture.codex_home(),
        &["thread-a".to_string(), "thread-c".to_string()],
        true,
    )
    .unwrap();
    let batch = list_trash_batches(fixture.codex_home()).unwrap().remove(0);

    fixture.insert_simple_thread(
        "thread-a",
        "Conflict Alpha",
        "sessions/conflict/thread-a.jsonl",
        false,
    );

    let report = restore_trash_batch(fixture.codex_home(), &batch.batch_id).unwrap();

    assert_eq!(report.restored_ids, vec!["thread-c".to_string()]);
    assert_eq!(report.conflict_ids, vec!["thread-a".to_string()]);
    assert!(fixture.thread_exists("thread-c"));
    assert_eq!(
        fixture.session_index_thread_name("thread-c").as_deref(),
        Some("Archived Thread")
    );
    assert_eq!(
        fixture.session_index_thread_name("thread-a").as_deref(),
        Some("Conflict Alpha")
    );

    let remaining = list_trash_batches(fixture.codex_home()).unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].thread_count, 1);
}

#[test]
fn purge_batch_and_purge_all_only_remove_trash_content() {
    let fixture = create_manage_codex_home();

    delete_threads_to_trash(fixture.codex_home(), &["thread-d".to_string()], true).unwrap();
    delete_threads_to_trash(fixture.codex_home(), &["thread-c".to_string()], true).unwrap();

    let batches = list_trash_batches(fixture.codex_home()).unwrap();
    assert_eq!(batches.len(), 2);

    purge_trash_batch(fixture.codex_home(), &batches[0].batch_id).unwrap();
    assert_eq!(list_trash_batches(fixture.codex_home()).unwrap().len(), 1);
    assert!(fixture.thread_exists("thread-b"));
    assert!(!fixture.thread_exists("thread-d"));

    purge_all_trash(fixture.codex_home()).unwrap();
    assert!(list_trash_batches(fixture.codex_home()).unwrap().is_empty());
    assert!(fixture.thread_exists("thread-b"));
    assert!(!fixture.thread_exists("thread-c"));
}

#[test]
fn export_selected_threads_only_includes_chosen_rows_index_and_payloads() {
    let fixture = create_manage_codex_home();
    let output = fixture.temp.path().join("selected.codexhist");

    let report = export_selected_threads(
        fixture.codex_home(),
        &output,
        &["thread-a".to_string(), "thread-c".to_string()],
    )
    .unwrap();

    assert_eq!(report.thread_count, 2);
    assert_eq!(report.session_file_count, 1);
    assert_eq!(report.archived_file_count, 1);
    assert!(output.exists());

    let file = File::open(&output).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();

    let mut manifest_json = String::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest_json)
        .unwrap();
    assert!(manifest_json.contains("\"thread_count\": 2"));

    let mut index_body = String::new();
    archive
        .by_name("index/session_index.jsonl")
        .unwrap()
        .read_to_string(&mut index_body)
        .unwrap();
    assert!(index_body.contains("\"thread-a\""));
    assert!(index_body.contains("\"thread-c\""));
    assert!(!index_body.contains("\"thread-b\""));

    assert!(
        archive
            .by_name("sessions/2026/04/16/rollout-a.jsonl")
            .is_ok()
    );
    assert!(
        archive
            .by_name("archived_sessions/2026/04/14/rollout-c.jsonl")
            .is_ok()
    );
    assert!(
        archive
            .by_name("sessions/2026/04/15/rollout-b.jsonl")
            .is_err()
    );

    let temp = tempdir().unwrap();
    let db_path = temp.path().join("threads.sqlite");
    let mut db_bytes = Vec::new();
    archive
        .by_name("db/threads.sqlite")
        .unwrap()
        .read_to_end(&mut db_bytes)
        .unwrap();
    File::create(&db_path)
        .unwrap()
        .write_all(&db_bytes)
        .unwrap();

    let conn = Connection::open(&db_path).unwrap();
    let mut stmt = conn
        .prepare("SELECT id FROM threads ORDER BY id ASC")
        .unwrap();
    let ids = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<rusqlite::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(ids, vec!["thread-a".to_string(), "thread-c".to_string()]);
}
