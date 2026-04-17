mod support;

use std::fs;

use codex_history_migrator::core::export::export_package;
use codex_history_migrator::core::import::import_package;
use codex_history_migrator::fs::package::{unpack_zip_to_dir, write_zip_from_dir};

use self::support::{create_empty_codex_home, create_fake_codex_home};

#[test]
fn import_copies_files_merges_threads_and_repairs_paths() {
    let source = create_fake_codex_home();
    let package = source.temp.path().join("history.codexhist");
    export_package(source.codex_home(), &package).unwrap();

    let target = create_empty_codex_home();
    let report = import_package(&package, target.codex_home(), true).unwrap();

    assert_eq!(report.inserted_threads, 1);
    assert_eq!(report.updated_threads, 0);
    assert_eq!(report.skipped_threads, 0);

    let expected_session = target.session_file("sessions/2026/04/16/rollout-a.jsonl");
    assert!(expected_session.exists());
    assert_eq!(
        target.thread_rollout_path("thread-a"),
        expected_session.to_string_lossy().to_string()
    );
}

#[test]
fn import_rejects_package_when_checksums_do_not_match() {
    let source = create_fake_codex_home();
    let package = source.temp.path().join("history.codexhist");
    export_package(source.codex_home(), &package).unwrap();

    let unpacked = source.temp.path().join("tampered");
    unpack_zip_to_dir(&package, &unpacked).unwrap();
    fs::write(
        unpacked.join("checksums.json"),
        r#"{"db/threads.sqlite":"deadbeef","index/session_index.jsonl":"deadbeef"}"#,
    )
    .unwrap();

    let tampered_package = source.temp.path().join("tampered.codexhist");
    write_zip_from_dir(&unpacked, &tampered_package).unwrap();

    let target = create_empty_codex_home();
    let error = import_package(&tampered_package, target.codex_home(), true).unwrap_err();

    assert!(error.to_string().contains("checksum"));
    assert_eq!(target.thread_count(), 0);
    assert!(
        !target
            .session_file("sessions/2026/04/16/rollout-a.jsonl")
            .exists()
    );
    assert!(!target.codex_home().join(".backups").exists());
}

#[test]
fn import_rejects_package_when_session_payload_is_tampered() {
    let source = create_fake_codex_home();
    let package = source.temp.path().join("history-payload.codexhist");
    export_package(source.codex_home(), &package).unwrap();

    let unpacked = source.temp.path().join("tampered-payload");
    unpack_zip_to_dir(&package, &unpacked).unwrap();
    fs::write(
        unpacked.join("sessions/2026/04/16/rollout-a.jsonl"),
        "{\"type\":\"user_message\",\"payload\":\"tampered\"}\n",
    )
    .unwrap();

    let tampered_package = source.temp.path().join("tampered-payload.codexhist");
    write_zip_from_dir(&unpacked, &tampered_package).unwrap();

    let target = create_empty_codex_home();
    let error = import_package(&tampered_package, target.codex_home(), true).unwrap_err();

    assert!(error.to_string().contains("checksum"));
    assert_eq!(target.thread_count(), 0);
    assert!(
        !target
            .session_file("sessions/2026/04/16/rollout-a.jsonl")
            .exists()
    );
}
