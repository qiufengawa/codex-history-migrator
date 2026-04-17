#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, params};
use serde_json::{Value, json};
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

    pub fn thread_title(&self, thread_id: &str) -> String {
        let conn = Connection::open(self.db_path()).unwrap();
        conn.query_row(
            "SELECT title FROM threads WHERE id = ?1",
            params![thread_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    }

    pub fn thread_rollout_path(&self, thread_id: &str) -> String {
        let conn = Connection::open(self.db_path()).unwrap();
        conn.query_row(
            "SELECT rollout_path FROM threads WHERE id = ?1",
            params![thread_id],
            |row| row.get::<_, String>(0),
        )
        .unwrap()
    }

    pub fn thread_archived(&self, thread_id: &str) -> bool {
        let conn = Connection::open(self.db_path()).unwrap();
        conn.query_row(
            "SELECT archived FROM threads WHERE id = ?1",
            params![thread_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            != 0
    }

    pub fn thread_archived_at(&self, thread_id: &str) -> Option<i64> {
        let conn = Connection::open(self.db_path()).unwrap();
        conn.query_row(
            "SELECT archived_at FROM threads WHERE id = ?1",
            params![thread_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .unwrap()
    }

    pub fn thread_exists(&self, thread_id: &str) -> bool {
        let conn = Connection::open(self.db_path()).unwrap();
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM threads WHERE id = ?1)",
            params![thread_id],
            |row| row.get::<_, i64>(0),
        )
        .unwrap()
            != 0
    }

    pub fn session_index_path(&self) -> PathBuf {
        self.codex_home.join("session_index.jsonl")
    }

    pub fn session_index_contents(&self) -> String {
        fs::read_to_string(self.session_index_path()).unwrap_or_default()
    }

    pub fn session_index_has_id(&self, thread_id: &str) -> bool {
        self.session_index_entries().contains_key(thread_id)
    }

    pub fn session_index_thread_name(&self, thread_id: &str) -> Option<String> {
        self.session_index_entries()
            .get(thread_id)
            .and_then(|value| value.get("thread_name"))
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
    }

    pub fn read_payload_for_thread(&self, thread_id: &str) -> Option<String> {
        let path = PathBuf::from(self.thread_rollout_path(thread_id));
        fs::read_to_string(path).ok()
    }

    pub fn payload_path_for_thread(&self, thread_id: &str) -> PathBuf {
        PathBuf::from(self.thread_rollout_path(thread_id))
    }

    pub fn payload_exists_for_thread(&self, thread_id: &str) -> bool {
        self.payload_path_for_thread(thread_id).exists()
    }

    pub fn trash_root(&self) -> PathBuf {
        self.codex_home.join("history_manager_trash")
    }

    pub fn trash_batch_count(&self) -> usize {
        if !self.trash_root().exists() {
            return 0;
        }

        fs::read_dir(self.trash_root()).unwrap().count()
    }

    pub fn insert_simple_thread(
        &self,
        thread_id: &str,
        title: &str,
        relative_rollout: &str,
        archived: bool,
    ) {
        let absolute_rollout = self.codex_home.join(relative_rollout);
        write_payload(
            &absolute_rollout,
            "{\"type\":\"user_message\",\"payload\":\"conflict\"}\n",
        );

        let conn = Connection::open(self.db_path()).unwrap();
        insert_thread_record(
            &conn,
            ThreadSeed {
                id: thread_id.to_string(),
                rollout_path: absolute_rollout,
                created_at: 500,
                updated_at: 600,
                model_provider: "openai".to_string(),
                cwd: format!("C:/Projects/{thread_id}"),
                title: title.to_string(),
                first_user_message: "conflict".to_string(),
                archived,
                archived_at: archived.then_some(600),
                model: Some("gpt-5.4".to_string()),
            },
        );

        upsert_session_index_entry(
            &self.session_index_path(),
            &json!({
                "id": thread_id,
                "thread_name": title,
                "updated_at": "2026-04-16T18:00:00Z"
            }),
        );
    }

    fn session_index_entries(&self) -> BTreeMap<String, Value> {
        fs::read_to_string(self.session_index_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<Value>(line).ok())
            .filter_map(|value| {
                let id = value.get("id")?.as_str()?.to_string();
                Some((id, value))
            })
            .collect()
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

pub fn create_manage_codex_home() -> FakeCodexHome {
    let temp = tempfile::tempdir().unwrap();
    let codex_home = temp.path().join(".codex");
    fs::create_dir_all(codex_home.join("sessions")).unwrap();
    fs::create_dir_all(codex_home.join("archived_sessions")).unwrap();
    fs::write(
        codex_home.join("config.toml"),
        "model_provider = \"openai\"\nmodel = \"gpt-5.4\"\n",
    )
    .unwrap();

    let db_path = codex_home.join("state_5.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    create_threads_table(&conn);

    let alpha_rollout = codex_home.join("sessions/2026/04/16/rollout-a.jsonl");
    write_payload(
        &alpha_rollout,
        concat!(
            "{\"type\":\"user_message\",\"payload\":\"hello alpha\"}\n",
            "{\"type\":\"assistant_message\",\"text\":\"alpha reply\"}\n",
            "not-json-at-all\n",
            "{\"type\":\"tool_result\",\"content\":{\"answer\":\"42\"}}\n",
            "{\"mystery\":\"shape\"}\n"
        ),
    );
    insert_thread_record(
        &conn,
        ThreadSeed {
            id: "thread-a".to_string(),
            rollout_path: alpha_rollout,
            created_at: 100,
            updated_at: 200,
            model_provider: "rensu".to_string(),
            cwd: "C:/Projects/alpha".to_string(),
            title: "Alpha Thread".to_string(),
            first_user_message: "hello alpha".to_string(),
            archived: false,
            archived_at: None,
            model: Some("gpt-5.4".to_string()),
        },
    );

    let beta_rollout = codex_home.join("sessions/2026/04/15/rollout-b.jsonl");
    write_payload(
        &beta_rollout,
        concat!(
            "{\"type\":\"user_message\",\"payload\":\"beta search\"}\n",
            "{\"type\":\"assistant_message\",\"text\":\"beta answer\"}\n"
        ),
    );
    insert_thread_record(
        &conn,
        ThreadSeed {
            id: "thread-b".to_string(),
            rollout_path: beta_rollout,
            created_at: 110,
            updated_at: 500,
            model_provider: "openai".to_string(),
            cwd: "C:/Projects/beta".to_string(),
            title: "Beta Search".to_string(),
            first_user_message: "searchable beta".to_string(),
            archived: false,
            archived_at: None,
            model: Some("gpt-5.3".to_string()),
        },
    );

    let archived_rollout = codex_home.join("archived_sessions/2026/04/14/rollout-c.jsonl");
    write_payload(
        &archived_rollout,
        concat!(
            "{\"type\":\"user_message\",\"payload\":\"archived hello\"}\n",
            "{\"type\":\"assistant_message\",\"text\":\"archived answer\"}\n"
        ),
    );
    insert_thread_record(
        &conn,
        ThreadSeed {
            id: "thread-c".to_string(),
            rollout_path: archived_rollout,
            created_at: 120,
            updated_at: 350,
            model_provider: "openai".to_string(),
            cwd: "C:/Projects/archive".to_string(),
            title: "Archived Thread".to_string(),
            first_user_message: "archived hello".to_string(),
            archived: true,
            archived_at: Some(360),
            model: Some("gpt-5.4".to_string()),
        },
    );

    let missing_rollout = codex_home.join("sessions/2026/04/13/missing-d.jsonl");
    insert_thread_record(
        &conn,
        ThreadSeed {
            id: "thread-d".to_string(),
            rollout_path: missing_rollout,
            created_at: 130,
            updated_at: 250,
            model_provider: "openai".to_string(),
            cwd: "C:/Projects/missing".to_string(),
            title: "Broken Thread".to_string(),
            first_user_message: "missing payload".to_string(),
            archived: false,
            archived_at: None,
            model: Some("gpt-5.4".to_string()),
        },
    );

    let mismatch_rollout = codex_home.join("sessions/2026/04/12/rollout-e.jsonl");
    write_payload(
        &mismatch_rollout,
        "{\"type\":\"user_message\",\"payload\":\"mismatch payload\"}\n",
    );
    insert_thread_record(
        &conn,
        ThreadSeed {
            id: "thread-e".to_string(),
            rollout_path: mismatch_rollout,
            created_at: 140,
            updated_at: 150,
            model_provider: "anthropic".to_string(),
            cwd: "C:/Projects/mismatch".to_string(),
            title: "Mismatch Thread".to_string(),
            first_user_message: "mismatch payload".to_string(),
            archived: true,
            archived_at: Some(151),
            model: Some("claude-sonnet".to_string()),
        },
    );

    let outside_rollout = temp.path().join("external").join("rollout-f.jsonl");
    write_payload(
        &outside_rollout,
        "{\"type\":\"user_message\",\"payload\":\"outside payload\"}\n",
    );
    insert_thread_record(
        &conn,
        ThreadSeed {
            id: "thread-f".to_string(),
            rollout_path: outside_rollout,
            created_at: 150,
            updated_at: 100,
            model_provider: "openai".to_string(),
            cwd: "C:/Projects/outside".to_string(),
            title: "Outside Thread".to_string(),
            first_user_message: "outside payload".to_string(),
            archived: false,
            archived_at: None,
            model: Some("gpt-5.4".to_string()),
        },
    );

    let structured_rollout = codex_home.join("sessions/2026/04/16/rollout-g.jsonl");
    write_payload(
        &structured_rollout,
        concat!(
            "{\"type\":\"user_message\",\"payload\":\"帮我安装这个 mcp\"}\n",
            "{\"type\":\"event_msg\",\"text\":{\"name\":\"pencil\",\"transport\":\"stdio\",\"command\":\"D:\\\\Tools\\\\Pencil\\\\resources\\\\app.asar.unpacked\\\\out\\\\mcp-server-windows-x64.exe\"}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"codex_error_info\":{\"response_too_many_failed_attempts\":true,\"http_status_code\":429}}}\n",
            "{\"type\":\"event_msg\",\"content\":{\"completed_at\":1776324834,\"duration_ms\":2384}}\n",
            "{\"type\":\"event_msg\",\"message\":{\"type\":\"thread_rolled_back\",\"num_turns\":1}}\n",
            "{\"type\":\"event_msg\",\"body\":{\"info\":{\"last_token_usage\":{\"cached_input_tokens\":0,\"input_tokens\":12,\"output_tokens\":34}}}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"mcp\":{\"name\":\"filesystem\",\"transport\":\"stdio\",\"command\":\"C:\\\\Program Files\\\\MCP\\\\fs-server.exe\"}}}\n",
            "{\"type\":\"tool_result\",\"tool_name\":\"read_file\",\"content\":[{\"type\":\"text\",\"text\":\"C:\\\\Users\\\\Admin\\\\Desktop\\\\Task\\\\README.md\"},{\"type\":\"text\",\"text\":\"line 1: hello\"}]}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"alpha\":{\"nested\":1},\"beta\":[1,2],\"gamma\":true,\"delta\":\"/tmp/workspace/output.json\"}}\n"
        ),
    );
    insert_thread_record(
        &conn,
        ThreadSeed {
            id: "thread-g".to_string(),
            rollout_path: structured_rollout,
            created_at: 160,
            updated_at: 550,
            model_provider: "qiuapi".to_string(),
            cwd: r"\\?\C:\Users\Admin\Desktop\Task".to_string(),
            title: "{\"event_msg\":{\"name\":\"pencil\",\"transport\":\"stdio\",\"command\":\"D:\\\\Tools\\\\Pencil\\\\resources\\\\app.asar.unpacked\\\\out\\\\mcp-server-windows-x64.exe\"}}".to_string(),
            first_user_message: "{\"input\":{\"name\":\"pencil\",\"transport\":\"stdio\",\"command\":\"D:\\\\Tools\\\\Pencil\\\\resources\\\\app.asar.unpacked\\\\out\\\\mcp-server-windows-x64.exe\",\"args\":[\"--app\",\"desktop\"]}}".to_string(),
            archived: false,
            archived_at: None,
            model: Some("gpt-5.4".to_string()),
        },
    );

    let index_path = codex_home.join("session_index.jsonl");
    fs::write(
        &index_path,
        [
            json!({"id":"thread-a","thread_name":"Alpha Thread","updated_at":"2026-04-16T17:00:00Z"}),
            json!({"id":"thread-b","thread_name":"Beta Search","updated_at":"2026-04-16T18:00:00Z"}),
            json!({"id":"thread-c","thread_name":"Archived Thread","updated_at":"2026-04-16T16:30:00Z"}),
            json!({"id":"thread-d","thread_name":"Broken Thread","updated_at":"2026-04-16T15:00:00Z"}),
            json!({"id":"thread-e","thread_name":"Mismatch Thread","updated_at":"2026-04-16T14:00:00Z"}),
            json!({"id":"thread-f","thread_name":"Outside Thread","updated_at":"2026-04-16T13:00:00Z"}),
            json!({"id":"thread-g","thread_name":"{\"event_msg\":{\"name\":\"pencil\",\"transport\":\"stdio\",\"command\":\"D:\\\\Tools\\\\Pencil\\\\resources\\\\app.asar.unpacked\\\\out\\\\mcp-server-windows-x64.exe\"}}","updated_at":"2026-04-16T19:00:00Z"}),
        ]
        .into_iter()
        .map(|value| serde_json::to_string(&value).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
            + "\n",
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
        conn.query_row("SELECT COUNT(*) FROM threads", [], |row| {
            row.get::<_, i64>(0)
        })
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

#[derive(Debug)]
struct ThreadSeed {
    id: String,
    rollout_path: PathBuf,
    created_at: i64,
    updated_at: i64,
    model_provider: String,
    cwd: String,
    title: String,
    first_user_message: String,
    archived: bool,
    archived_at: Option<i64>,
    model: Option<String>,
}

fn create_threads_table(conn: &Connection) {
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
}

fn insert_thread_record(conn: &Connection, thread: ThreadSeed) {
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
            thread.id,
            thread.rollout_path.to_string_lossy().to_string(),
            thread.created_at,
            thread.updated_at,
            "vscode",
            thread.model_provider,
            thread.cwd,
            thread.title,
            "danger-full-access",
            "never",
            0_i64,
            1_i64,
            if thread.archived { 1_i64 } else { 0_i64 },
            thread.archived_at,
            Option::<String>::None,
            Option::<String>::None,
            Option::<String>::None,
            "0.119.0-alpha.28",
            thread.first_user_message,
            Option::<String>::None,
            Option::<String>::None,
            "enabled",
            thread.model,
            Some("high".to_string()),
            Option::<String>::None,
        ],
    )
    .unwrap();
}

fn write_payload(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, body).unwrap();
}

fn upsert_session_index_entry(index_path: &Path, value: &Value) {
    let mut entries = fs::read_to_string(index_path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter_map(|entry| {
            let id = entry.get("id")?.as_str()?.to_string();
            Some((id, entry))
        })
        .collect::<BTreeMap<_, _>>();

    let id = value.get("id").and_then(Value::as_str).unwrap().to_string();
    entries.insert(id, value.clone());

    let body = entries
        .values()
        .map(|entry| serde_json::to_string(entry).unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(index_path, format!("{body}\n")).unwrap();
}
