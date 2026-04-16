use std::path::PathBuf;

use crate::models::thread_record::ThreadRecord;

#[derive(Debug, Clone)]
pub struct SessionPayload {
    pub relative_path: PathBuf,
    pub absolute_path: PathBuf,
    pub archived: bool,
}

#[derive(Debug, Clone)]
pub struct ScanReport {
    pub codex_home: PathBuf,
    pub threads: Vec<ThreadRecord>,
    pub session_payloads: Vec<SessionPayload>,
    pub missing_payloads: Vec<String>,
    pub session_index_path: Option<PathBuf>,
    pub source_root_prefix: String,
}
