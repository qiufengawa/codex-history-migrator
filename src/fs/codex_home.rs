use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CodexHomePaths {
    pub root: PathBuf,
    pub state_db: PathBuf,
    pub config: PathBuf,
    pub backup_dir: PathBuf,
    pub session_index: PathBuf,
    pub trash_dir: PathBuf,
}

impl CodexHomePaths {
    pub fn resolve(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            state_db: root.join("state_5.sqlite"),
            config: root.join("config.toml"),
            backup_dir: root.join("history_sync_backups"),
            session_index: root.join("session_index.jsonl"),
            trash_dir: root.join("history_manager_trash"),
        }
    }
}
