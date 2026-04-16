use codex_history_migrator::core::path_rewrite::rewrite_rollout_path;

#[test]
fn rewrites_rollout_path_from_old_root_to_new_root() {
    let old_root = r"C:\Users\Admin\.codex";
    let new_root = r"C:\Users\Bob\.codex";
    let old_path = r"C:\Users\Admin\.codex\sessions\2026\04\16\rollout-a.jsonl";

    let rewritten = rewrite_rollout_path(old_path, old_root, new_root).unwrap();

    assert_eq!(
        rewritten,
        r"C:\Users\Bob\.codex\sessions\2026\04\16\rollout-a.jsonl"
    );
}

#[test]
fn returns_none_when_path_is_outside_old_root() {
    let old_root = r"C:\Users\Admin\.codex";
    let new_root = r"C:\Users\Bob\.codex";
    let old_path = r"D:\other\rollout-a.jsonl";

    let rewritten = rewrite_rollout_path(old_path, old_root, new_root);

    assert!(rewritten.is_none());
}
