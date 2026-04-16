mod support;

use codex_history_migrator::core::export::export_package_with_progress;
use codex_history_migrator::core::import::import_package_with_progress;
use codex_history_migrator::core::scan::scan_codex_home_with_progress;

use self::support::{create_empty_codex_home, create_fake_codex_home};

#[test]
fn scan_reports_progress_and_finishes_at_one_hundred_percent() {
    let fixture = create_fake_codex_home();
    let mut updates = Vec::new();

    let report = scan_codex_home_with_progress(fixture.codex_home(), |update| {
        updates.push(update);
    })
    .unwrap();

    assert_eq!(report.threads.len(), 1);
    assert!(!updates.is_empty());
    assert!(updates.iter().any(|item| item.message.contains("扫描")));
    assert_eq!(updates.last().unwrap().fraction, 1.0);
}

#[test]
fn export_reports_progress_and_finishes_at_one_hundred_percent() {
    let fixture = create_fake_codex_home();
    let output = fixture.temp.path().join("progress-test.codexhist");
    let mut updates = Vec::new();

    let report = export_package_with_progress(fixture.codex_home(), &output, |update| {
        updates.push(update);
    })
    .unwrap();

    assert_eq!(report.thread_count, 1);
    assert!(output.exists());
    assert!(!updates.is_empty());
    assert!(updates.iter().any(|item| item.message.contains("导出")));
    assert_eq!(updates.last().unwrap().fraction, 1.0);
}

#[test]
fn import_reports_progress_and_finishes_at_one_hundred_percent() {
    let source = create_fake_codex_home();
    let package = source.temp.path().join("progress-import.codexhist");
    export_package_with_progress(source.codex_home(), &package, |_| {}).unwrap();

    let target = create_empty_codex_home();
    let mut updates = Vec::new();

    let report = import_package_with_progress(&package, target.codex_home(), true, |update| {
        updates.push(update);
    })
    .unwrap();

    assert_eq!(report.inserted_threads, 1);
    assert!(!updates.is_empty());
    assert!(updates.iter().any(|item| item.message.contains("导入")));
    assert_eq!(updates.last().unwrap().fraction, 1.0);
}
