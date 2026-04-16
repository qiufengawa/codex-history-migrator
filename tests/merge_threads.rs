use codex_history_migrator::core::merge::{MergeDecision, merge_thread};
use codex_history_migrator::models::thread_record::ThreadRecord;

fn sample_thread(id: &str, updated_at: i64) -> ThreadRecord {
    ThreadRecord {
        id: id.to_string(),
        rollout_path: format!(r"C:\Users\Admin\.codex\sessions\2026\04\16\{id}.jsonl"),
        created_at: updated_at - 10,
        updated_at,
        source: "vscode".to_string(),
        model_provider: "rensu".to_string(),
        cwd: r"C:\Users\Admin\Desktop\Task".to_string(),
        title: format!("Thread {id}"),
        sandbox_policy: "danger-full-access".to_string(),
        approval_mode: "never".to_string(),
        tokens_used: 0,
        has_user_event: true,
        archived: false,
        archived_at: None,
        git_sha: None,
        git_branch: None,
        git_origin_url: None,
        cli_version: "0.119.0-alpha.28".to_string(),
        first_user_message: "hello".to_string(),
        agent_nickname: None,
        agent_role: None,
        memory_mode: "enabled".to_string(),
        model: Some("gpt-5.4".to_string()),
        reasoning_effort: Some("high".to_string()),
        agent_path: None,
    }
}

#[test]
fn keeps_newer_existing_thread_when_imported_thread_is_older() {
    let local = sample_thread("thread-a", 200);
    let imported = sample_thread("thread-a", 100);

    let decision = merge_thread(&local, &imported);

    assert!(matches!(decision, MergeDecision::KeepExisting));
}

#[test]
fn updates_existing_thread_when_imported_thread_is_newer() {
    let local = sample_thread("thread-a", 100);
    let imported = sample_thread("thread-a", 200);

    let decision = merge_thread(&local, &imported);

    assert!(matches!(decision, MergeDecision::UpdateExisting));
}
