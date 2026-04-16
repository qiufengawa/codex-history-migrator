use std::path::PathBuf;

use crate::models::provider_count::ProviderCount;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSyncStatus {
    pub codex_home: PathBuf,
    pub config_path: PathBuf,
    pub db_path: PathBuf,
    pub backup_dir: PathBuf,
    pub current_provider: String,
    pub current_model: Option<String>,
    pub total_threads: usize,
    pub movable_threads: usize,
    pub provider_counts: Vec<ProviderCount>,
    pub latest_backup_path: Option<PathBuf>,
    pub backup_count: usize,
}
