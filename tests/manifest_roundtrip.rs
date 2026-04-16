use std::fs;

use codex_history_migrator::core::checksum::compute_sha256_hex;
use codex_history_migrator::models::manifest::{Manifest, PackageCounts};
use tempfile::tempdir;

fn sample_manifest() -> Manifest {
    Manifest {
        format_version: 1,
        tool_version: "0.1.0".to_string(),
        exported_at: "2026-04-16T17:00:00Z".to_string(),
        source_codex_home: r"C:\Users\Admin\.codex".to_string(),
        source_root_prefix: r"C:\Users\Admin\.codex".to_string(),
        counts: PackageCounts {
            thread_count: 12,
            session_file_count: 10,
            archived_file_count: 2,
            missing_file_count: 1,
        },
    }
}

#[test]
fn manifest_round_trips_as_json() {
    let manifest = sample_manifest();

    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: Manifest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.format_version, manifest.format_version);
    assert_eq!(parsed.counts.thread_count, manifest.counts.thread_count);
    assert_eq!(parsed.source_root_prefix, manifest.source_root_prefix);
}

#[test]
fn computes_stable_sha256_for_file_contents() {
    let temp = tempdir().unwrap();
    let file_path = temp.path().join("sample.txt");
    fs::write(&file_path, b"codex-history").unwrap();

    let hash = compute_sha256_hex(&file_path).unwrap();

    assert_eq!(
        hash,
        "30b2cb09ef2b86778e31487cf718576af69ff774d1043bbe825d43859b69e63d"
    );
}
