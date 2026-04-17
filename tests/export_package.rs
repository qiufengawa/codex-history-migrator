mod support;

use std::fs;
use std::fs::File;
use std::io::Read;

use codex_history_migrator::core::export::export_package;
use codex_history_migrator::models::manifest::Manifest;
use zip::ZipArchive;

use self::support::create_fake_codex_home;

#[test]
fn export_writes_manifest_threads_index_and_session_files() {
    let fixture = create_fake_codex_home();
    let output = fixture.temp.path().join("sample.codexhist");

    let report = export_package(fixture.codex_home(), &output).unwrap();

    assert!(output.exists());
    assert_eq!(report.thread_count, 1);
    assert_eq!(report.session_file_count, 1);

    let file = File::open(&output).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();

    let mut manifest_json = String::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut manifest_json)
        .unwrap();
    let manifest: Manifest = serde_json::from_str(&manifest_json).unwrap();
    assert_eq!(manifest.counts.thread_count, 1);

    assert!(archive.by_name("db/threads.sqlite").is_ok());
    assert!(archive.by_name("index/session_index.jsonl").is_ok());
    assert!(
        archive
            .by_name("sessions/2026/04/16/rollout-a.jsonl")
            .is_ok()
    );
}

#[test]
fn export_skips_rollout_files_outside_codex_home() {
    let fixture = create_fake_codex_home();
    let output = fixture.temp.path().join("outside-rollout.codexhist");
    let outside_rollout = fixture.temp.path().join("external-rollout.jsonl");
    fs::write(
        &outside_rollout,
        "{\"type\":\"user_message\",\"payload\":\"outside\"}\n",
    )
    .unwrap();
    fixture.set_thread_rollout_path("thread-a", &outside_rollout);

    let report = export_package(fixture.codex_home(), &output).unwrap();

    assert!(output.exists());
    assert_eq!(report.thread_count, 1);
    assert_eq!(report.session_file_count, 0);
    assert_eq!(report.missing_file_count, 1);

    let file = File::open(&output).unwrap();
    let mut archive = ZipArchive::new(file).unwrap();
    assert!(
        archive
            .by_name("sessions/2026/04/16/rollout-a.jsonl")
            .is_err()
    );
}
