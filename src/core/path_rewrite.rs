pub fn rewrite_rollout_path(path: &str, old_root: &str, new_root: &str) -> Option<String> {
    path.strip_prefix(old_root)
        .map(|suffix| format!("{new_root}{suffix}"))
}
