use crate::models::thread_record::ThreadRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeDecision {
    InsertImported,
    UpdateExisting,
    KeepExisting,
}

pub fn merge_thread(local: &ThreadRecord, imported: &ThreadRecord) -> MergeDecision {
    if local.id != imported.id {
        return MergeDecision::InsertImported;
    }

    if imported.updated_at > local.updated_at {
        MergeDecision::UpdateExisting
    } else {
        MergeDecision::KeepExisting
    }
}
