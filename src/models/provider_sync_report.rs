use std::path::PathBuf;

use crate::models::provider_count::ProviderCount;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderSyncReport {
    pub current_provider: String,
    pub updated_threads: usize,
    pub backup_path: Option<PathBuf>,
    pub before_counts: Vec<ProviderCount>,
    pub after_counts: Vec<ProviderCount>,
}
