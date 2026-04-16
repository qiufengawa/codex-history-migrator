#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    pub inserted_threads: usize,
    pub updated_threads: usize,
    pub skipped_threads: usize,
    pub repaired_paths: usize,
}
