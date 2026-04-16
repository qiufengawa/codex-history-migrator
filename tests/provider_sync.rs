mod support;

use codex_history_migrator::core::provider_sync::{
    read_provider_sync_status, restore_latest_provider_backup,
    restore_latest_provider_backup_with_safety_backup, sync_threads_to_current_provider,
    sync_threads_to_current_provider_with_backup,
};

use self::support::create_fake_codex_home;

#[test]
fn status_reads_current_provider_and_thread_distribution() {
    let fixture = create_fake_codex_home();

    let status = read_provider_sync_status(fixture.codex_home()).unwrap();

    assert_eq!(status.current_provider, "openai");
    assert_eq!(status.current_model.as_deref(), Some("gpt-5.4"));
    assert_eq!(status.total_threads, 1);
    assert_eq!(status.movable_threads, 1);
    assert_eq!(status.provider_counts.len(), 1);
    assert_eq!(status.provider_counts[0].provider, "rensu");
    assert_eq!(status.provider_counts[0].count, 1);
    assert!(status.latest_backup_path.is_none());
}

#[test]
fn sync_moves_threads_to_current_provider_and_creates_backup() {
    let fixture = create_fake_codex_home();

    let report = sync_threads_to_current_provider(fixture.codex_home()).unwrap();

    assert_eq!(report.current_provider, "openai");
    assert_eq!(report.updated_threads, 1);
    assert!(
        report
            .backup_path
            .as_ref()
            .is_some_and(|path| path.exists())
    );
    assert_eq!(fixture.thread_provider("thread-a"), "openai");
    assert_eq!(fixture.provider_counts(), vec![("openai".to_string(), 1)]);
}

#[test]
fn sync_can_skip_backup_when_disabled() {
    let fixture = create_fake_codex_home();

    let report = sync_threads_to_current_provider_with_backup(fixture.codex_home(), false).unwrap();

    assert_eq!(report.current_provider, "openai");
    assert_eq!(report.updated_threads, 1);
    assert!(report.backup_path.is_none());
    assert_eq!(fixture.thread_provider("thread-a"), "openai");
    assert_eq!(fixture.backup_file_count(), 0);
}

#[test]
fn restore_recovers_the_latest_provider_sync_backup() {
    let fixture = create_fake_codex_home();
    let sync_report = sync_threads_to_current_provider(fixture.codex_home()).unwrap();
    assert_eq!(fixture.thread_provider("thread-a"), "openai");

    let restored_from = restore_latest_provider_backup(fixture.codex_home()).unwrap();

    assert_eq!(Some(restored_from), sync_report.backup_path);
    assert_eq!(fixture.thread_provider("thread-a"), "rensu");
    assert_eq!(fixture.provider_counts(), vec![("rensu".to_string(), 1)]);
}

#[test]
fn restore_can_skip_safety_backup_when_disabled() {
    let fixture = create_fake_codex_home();
    let sync_report = sync_threads_to_current_provider(fixture.codex_home()).unwrap();
    let backup_count_before_restore = fixture.backup_file_count();

    let restored_from =
        restore_latest_provider_backup_with_safety_backup(fixture.codex_home(), false).unwrap();

    assert_eq!(restored_from, sync_report.backup_path.unwrap());
    assert_eq!(fixture.thread_provider("thread-a"), "rensu");
    assert_eq!(fixture.backup_file_count(), backup_count_before_restore);
}
