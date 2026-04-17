use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManageFilter {
    pub keyword: String,
    pub archived: ArchivedFilter,
    pub provider: Option<String>,
    pub health: HealthFilter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArchivedFilter {
    #[default]
    All,
    ActiveOnly,
    ArchivedOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HealthFilter {
    #[default]
    All,
    HealthyOnly,
    NeedsAttentionOnly,
    MissingPayloadOnly,
    InvalidPathOnly,
    ArchiveStateMismatchOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManageHealth {
    Healthy,
    MissingPayload,
    InvalidPath,
    ArchiveStateMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManageRow {
    pub id: String,
    pub title: String,
    pub title_display: String,
    pub title_detail: Option<String>,
    pub first_user_message: String,
    pub first_user_message_display: String,
    pub updated_at: i64,
    pub model_provider: String,
    pub model: Option<String>,
    pub cwd: String,
    pub cwd_display: String,
    pub archived: bool,
    pub archived_at: Option<i64>,
    pub rollout_path: PathBuf,
    pub rollout_path_display: String,
    pub relative_rollout_path: Option<PathBuf>,
    pub payload_exists: bool,
    pub preview_available: bool,
    pub can_open_payload: bool,
    pub can_toggle_archive: bool,
    pub can_delete: bool,
    pub health: ManageHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewEntry {
    pub line_number: usize,
    pub entry_type: String,
    pub display_type: String,
    pub text: String,
    pub is_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrashBatchSummary {
    pub batch_id: String,
    pub path: PathBuf,
    pub deleted_at: i64,
    pub thread_count: usize,
    pub payload_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveUpdateReport {
    pub updated_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeleteToTrashReport {
    pub batch_id: String,
    pub trash_dir: PathBuf,
    pub deleted_count: usize,
    pub backup_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreTrashReport {
    pub restored_ids: Vec<String>,
    pub conflict_ids: Vec<String>,
}
