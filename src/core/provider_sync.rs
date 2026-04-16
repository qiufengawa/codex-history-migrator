use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};

use crate::db::sqlite::{backup_database, restore_database};
use crate::db::threads::{load_provider_counts, update_all_thread_providers};
use crate::fs::codex_home::CodexHomePaths;
use crate::models::provider_sync_report::ProviderSyncReport;
use crate::models::provider_sync_status::ProviderSyncStatus;

const SYNC_BACKUP_LABEL: &str = "provider-sync";
const RESTORE_SAFETY_LABEL: &str = "provider-restore-safety";

pub fn read_provider_sync_status(codex_home: &Path) -> Result<ProviderSyncStatus> {
    let paths = CodexHomePaths::resolve(codex_home);
    ensure_sync_environment(&paths)?;

    let config_text = fs::read_to_string(&paths.config)?;
    let current_provider = parse_config_string(&config_text, "model_provider")
        .ok_or_else(|| anyhow::anyhow!("config.toml 中缺少 model_provider"))?;
    let current_model = parse_config_string(&config_text, "model");
    let provider_counts = load_provider_counts(&paths.state_db)?;
    let total_threads = provider_counts.iter().map(|item| item.count).sum();
    let movable_threads = provider_counts
        .iter()
        .filter(|item| item.provider != current_provider)
        .map(|item| item.count)
        .sum();
    let backups = list_backups_by_label(&paths.backup_dir, SYNC_BACKUP_LABEL)?;

    Ok(ProviderSyncStatus {
        codex_home: paths.root,
        config_path: paths.config,
        db_path: paths.state_db,
        backup_dir: paths.backup_dir,
        current_provider,
        current_model,
        total_threads,
        movable_threads,
        provider_counts,
        latest_backup_path: backups.first().cloned(),
        backup_count: backups.len(),
    })
}

pub fn sync_threads_to_current_provider(codex_home: &Path) -> Result<ProviderSyncReport> {
    sync_threads_to_current_provider_with_backup(codex_home, true)
}

pub fn sync_threads_to_current_provider_with_backup(
    codex_home: &Path,
    make_backup: bool,
) -> Result<ProviderSyncReport> {
    let status = read_provider_sync_status(codex_home)?;
    let backup_path = if make_backup {
        Some(create_backup(
            &status.backup_dir,
            &status.db_path,
            SYNC_BACKUP_LABEL,
        )?)
    } else {
        None
    };
    let updated_threads = update_all_thread_providers(&status.db_path, &status.current_provider)?;
    let after_counts = load_provider_counts(&status.db_path)?;

    Ok(ProviderSyncReport {
        current_provider: status.current_provider,
        updated_threads,
        backup_path,
        before_counts: status.provider_counts,
        after_counts,
    })
}

pub fn restore_latest_provider_backup(codex_home: &Path) -> Result<PathBuf> {
    restore_latest_provider_backup_with_safety_backup(codex_home, true)
}

pub fn restore_latest_provider_backup_with_safety_backup(
    codex_home: &Path,
    make_backup: bool,
) -> Result<PathBuf> {
    let status = read_provider_sync_status(codex_home)?;
    let backup_path = list_backups_by_label(&status.backup_dir, SYNC_BACKUP_LABEL)?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("没有可恢复的 provider 同步备份"))?;

    if make_backup {
        create_backup(&status.backup_dir, &status.db_path, RESTORE_SAFETY_LABEL)?;
    }
    restore_database(&status.db_path, &backup_path)?;
    Ok(backup_path)
}

fn ensure_sync_environment(paths: &CodexHomePaths) -> Result<()> {
    if !paths.config.exists() {
        bail!("config.toml not found at {}", paths.config.display());
    }
    if !paths.state_db.exists() {
        bail!("state_5.sqlite not found at {}", paths.state_db.display());
    }
    Ok(())
}

fn parse_config_string(config_text: &str, key: &str) -> Option<String> {
    config_text.lines().find_map(|line| {
        let content = line.split('#').next()?.trim();
        let (left, right) = content.split_once('=')?;
        if left.trim() != key {
            return None;
        }

        let value = right.trim();
        value
            .strip_prefix('"')
            .and_then(|inner| inner.strip_suffix('"'))
            .map(ToOwned::to_owned)
    })
}

fn create_backup(backup_dir: &Path, source_db: &Path, label: &str) -> Result<PathBuf> {
    fs::create_dir_all(backup_dir)?;
    let backup_path = backup_dir.join(format!(
        "state_5.sqlite.{label}.{}.bak",
        unix_timestamp_string()
    ));
    backup_database(source_db, &backup_path)?;
    Ok(backup_path)
}

fn list_backups_by_label(backup_dir: &Path, label: &str) -> Result<Vec<PathBuf>> {
    if !backup_dir.exists() {
        return Ok(Vec::new());
    }

    let mut backups = Vec::new();
    let prefix = format!("state_5.sqlite.{label}.");

    for entry in fs::read_dir(backup_dir)? {
        let entry = entry?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if file_name.starts_with(&prefix) && file_name.ends_with(".bak") {
            backups.push(path);
        }
    }

    backups.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    Ok(backups)
}

fn unix_timestamp_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::parse_config_string;

    #[test]
    fn parses_basic_string_values_from_config() {
        let config = r#"
            model_provider = "openai"
            model = "gpt-5.4"
        "#;

        assert_eq!(
            parse_config_string(config, "model_provider").as_deref(),
            Some("openai")
        );
        assert_eq!(
            parse_config_string(config, "model").as_deref(),
            Some("gpt-5.4")
        );
    }
}
