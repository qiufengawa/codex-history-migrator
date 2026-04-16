use std::path::Path;

use anyhow::Result;
use rusqlite::params;

use crate::db::sqlite::open_connection;
use crate::models::provider_count::ProviderCount;
use crate::models::thread_record::ThreadRecord;

pub fn load_threads(db_path: &Path) -> Result<Vec<ThreadRecord>> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT
            id, rollout_path, created_at, updated_at, source, model_provider, cwd, title,
            sandbox_policy, approval_mode, tokens_used, has_user_event, archived, archived_at,
            git_sha, git_branch, git_origin_url, cli_version, first_user_message, agent_nickname,
            agent_role, memory_mode, model, reasoning_effort, agent_path
        FROM threads
        ORDER BY updated_at DESC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(ThreadRecord {
            id: row.get(0)?,
            rollout_path: row.get(1)?,
            created_at: row.get(2)?,
            updated_at: row.get(3)?,
            source: row.get(4)?,
            model_provider: row.get(5)?,
            cwd: row.get(6)?,
            title: row.get(7)?,
            sandbox_policy: row.get(8)?,
            approval_mode: row.get(9)?,
            tokens_used: row.get(10)?,
            has_user_event: row.get::<_, i64>(11)? != 0,
            archived: row.get::<_, i64>(12)? != 0,
            archived_at: row.get(13)?,
            git_sha: row.get(14)?,
            git_branch: row.get(15)?,
            git_origin_url: row.get(16)?,
            cli_version: row.get(17)?,
            first_user_message: row.get(18)?,
            agent_nickname: row.get(19)?,
            agent_role: row.get(20)?,
            memory_mode: row.get(21)?,
            model: row.get(22)?,
            reasoning_effort: row.get(23)?,
            agent_path: row.get(24)?,
        })
    })?;

    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn load_provider_counts(db_path: &Path) -> Result<Vec<ProviderCount>> {
    let conn = open_connection(db_path)?;
    let mut stmt = conn.prepare(
        r#"
        SELECT model_provider, COUNT(*)
        FROM threads
        GROUP BY model_provider
        ORDER BY COUNT(*) DESC, model_provider ASC
        "#,
    )?;

    let rows = stmt.query_map([], |row| {
        Ok(ProviderCount {
            provider: row.get(0)?,
            count: row.get::<_, i64>(1)? as usize,
        })
    })?;

    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn update_all_thread_providers(db_path: &Path, current_provider: &str) -> Result<usize> {
    let conn = open_connection(db_path)?;
    let updated = conn.execute(
        "UPDATE threads SET model_provider = ?1 WHERE model_provider <> ?1",
        params![current_provider],
    )?;
    Ok(updated)
}
