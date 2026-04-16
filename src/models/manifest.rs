use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageCounts {
    pub thread_count: usize,
    pub session_file_count: usize,
    pub archived_file_count: usize,
    pub missing_file_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    pub tool_version: String,
    pub exported_at: String,
    pub source_codex_home: String,
    pub source_root_prefix: String,
    pub counts: PackageCounts,
}
