#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use tempfile::TempDir;

pub struct FakeCodexHome {
    pub temp: TempDir,
    codex_home: PathBuf,
}

impl FakeCodexHome {
    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn db_path(&self) -> PathBuf {
        self.codex_home.join("state_5.sqlite")
    }

    pub fn config_path(&self) -> PathBuf {
        self.codex_home.join("config.toml")
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.codex_home.join("history_sync_backups")
    }

    pub fn thread_provider(&self, thread_id: &str) -> String {
        let conn = Connection::open(self.db_path()).unwrap();
        conn.query_row(
            "SELECT model_provider FROM threads WHERE id = ?1",
            params![thread_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    }

    pub fn provider_counts(&self) -> Vec<(String, i64)> {
        let conn = Connection::open(self.db_path()).unwrap();
        let mut stmt = conn
            .prepare(
                r#"
                SELECT model_provider, COUNT(*)
                FROM threads
                GROUP BY model_provider
                ORDER BY COUNT(*) DESC, model_provider ASC
                "#,
            )
            .unwrap();

        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap()
    }

    pub fn backup_file_count(&self) -> usize {
        if !self.backup_dir().exists() {
            return 0;
        }

        fs::read_dir(self.backup_dir()).unwrap().count()
    }

    pub fn set_thread_rollout_path(&self, thread_id: &str, rollout_path: &Path) {
        let conn = Connection::open(self.db_path()).unwrap();
        conn.execute(
            "UPDATE threads SET rollout_path = ?1 WHERE id = ?2",
            params![rollout_path.to_string_lossy().to_string(), thread_id],
        )
        .unwrap();
    }
}

pub fn create_fake_codex_home() -> FakeCodexHome {
    let temp = tempfile::tempdir().unwrap();
    let codex_home = temp.path().join(".codex");
    let sessions_dir = codex_home
        .join("sessions")
        .join("2026")
        .join("04")
        .join("16");
    let archived_dir = codex_home.join("archived_sessions");
    let session_relative = PathBuf::from("sessions/2026/04/16/rollout-a.jsonl");
    let session_path = codex_home.join(&session_relative);
    let index_path = codex_home.join("session_index.jsonl");
    let db_path = codex_home.join("state_5.sqlite");

    fs::create_dir_all(&sessions_dir).unwrap();
    fs::create_dir_all(&archived_dir).unwrap();
    fs::write(
        &session_path,
        "{\"type\":\"user_message\",\"payload\":\"hello\"}\n",
    )
    .unwrap();
    fs::write(
        codex_home.join("config.toml"),
        "model_provider = \"openai\"\nmodel = \"gpt-5.4\"\n",
    )
    .unwrap();
    fs::write(&index_path, "{\"id\":\"thread-a\",\"thread_name\":\"Example\",\"updated_at\":\"2026-04-16T17:00:00Z\"}\n").unwrap();

    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            rollout_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            source TEXT NOT NULL,
            model_provider TEXT NOT NULL,
            cwd TEXT NOT NULL,
            title TEXT NOT NULL,
            sandbox_policy TEXT NOT NULL,
            approval_mode TEXT NOT NULL,
            tokens_used INTEGER NOT NULL DEFAULT 0,
            has_user_event INTEGER NOT NULL DEFAULT 0,
            archived INTEGER NOT NULL DEFAULT 0,
            archived_at INTEGER,
            git_sha TEXT,
            git_branch TEXT,
            git_origin_url TEXT,
            cli_version TEXT NOT NULL DEFAULT '',
            first_user_message TEXT NOT NULL DEFAULT '',
            agent_nickname TEXT,
            agent_role TEXT,
            memory_mode TEXT NOT NULL DEFAULT 'enabled',
            model TEXT,
            reasoning_effort TEXT,
            agent_path TEXT
        );
        "#,
    )
    .unwrap();

    conn.execute(
        r#"
        INSERT INTO threads (
            id, rollout_path, created_at, updated_at, source, model_provider, cwd, title,
            sandbox_policy, approval_mode, tokens_used, has_user_event, archived, archived_at,
            git_sha, git_branch, git_origin_url, cli_version, first_user_message, agent_nickname,
            agent_role, memory_mode, model, reasoning_effort, agent_path
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8,
            ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19, ?20,
            ?21, ?22, ?23, ?24, ?25
        )
        "#,
        params![
            "thread-a",
            session_path.to_string_lossy().to_string(),
            100_i64,
            200_i64,
            "vscode",
            "rensu",
            codex_home.to_string_lossy().to_string(),
            "Example Thread",
            "danger-full-access",
            "never",
            0_i64,
            1_i64,
            0_i64,
            Option::<i64>::None,
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None,
            "0.119.0-alpha.28",
            "hello",
            Option::<String>::None,
            Option::<String>::None,
            "enabled",
            Some("gpt-5.4"),
            Some("high"),
            Option::<String>::None,
        ],
    )
    .unwrap();

    FakeCodexHome { temp, codex_home }
}

pub struct EmptyCodexHome {
    pub _temp: TempDir,
    codex_home: PathBuf,
}

impl EmptyCodexHome {
    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn session_file(&self, relative: &str) -> PathBuf {
        self.codex_home.join(relative)
    }

    pub fn thread_rollout_path(&self, thread_id: &str) -> String {
        let conn = Connection::open(self.codex_home.join("state_5.sqlite")).unwrap();
        conn.query_row(
            "SELECT rollout_path FROM threads WHERE id = ?1",
            params![thread_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    }

    pub fn thread_count(&self) -> i64 {
        let conn = Connection::open(self.codex_home.join("state_5.sqlite")).unwrap();
        conn.query_row("SELECT COUNT(*) FROM threads", [], |row| row.get::<_, i64>(0))
            .unwrap()
    }
}

pub fn create_empty_codex_home() -> EmptyCodexHome {
    let temp = tempfile::tempdir().unwrap();
    let codex_home = temp.path().join(".codex");
    fs::create_dir_all(codex_home.join("sessions")).unwrap();
    fs::create_dir_all(codex_home.join("archived_sessions")).unwrap();

    let conn = Connection::open(codex_home.join("state_5.sqlite")).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE threads (
            id TEXT PRIMARY KEY,
            rollout_path TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            source TEXT NOT NULL,
            model_provider TEXT NOT NULL,
            cwd TEXT NOT NULL,
            title TEXT NOT NULL,
            sandbox_policy TEXT NOT NULL,
            approval_mode TEXT NOT NULL,
            tokens_used INTEGER NOT NULL DEFAULT 0,
            has_user_event INTEGER NOT NULL DEFAULT 0,
            archived INTEGER NOT NULL DEFAULT 0,
            archived_at INTEGER,
            git_sha TEXT,
            git_branch TEXT,
            git_origin_url TEXT,
            cli_version TEXT NOT NULL DEFAULT '',
            first_user_message TEXT NOT NULL DEFAULT '',
            agent_nickname TEXT,
            agent_role TEXT,
            memory_mode TEXT NOT NULL DEFAULT 'enabled',
            model TEXT,
            reasoning_effort TEXT,
            agent_path TEXT
        );
        "#,
    )
    .unwrap();

    EmptyCodexHome {
        _temp: temp,
        codex_home,
    }
}
