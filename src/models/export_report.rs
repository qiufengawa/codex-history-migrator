#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    pub thread_count: usize,
    pub session_file_count: usize,
    pub archived_file_count: usize,
    pub missing_file_count: usize,
}
