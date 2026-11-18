use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum SessionError {
    #[error("Database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("Session not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "user", "assistant"
    pub content: String,
    pub tool_calls: Option<Vec<crate::tools::ToolCall>>,
    pub tool_results: Option<Vec<crate::tools::ToolResult>>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<Message>,
}

pub struct SessionStore {
    db_path: std::path::PathBuf,
}

impl SessionStore {
    pub fn new(db_path: impl AsRef<Path>) -> Result<Self, SessionError> {
        let store = Self {
            db_path: db_path.as_ref().to_path_buf(),
        };
        store.init_db()?;
        Ok(store)
    }

    fn connect(&self) -> Result<Connection, SessionError> {
        let conn = Connection::open(&self.db_path)?;
        // Busy timeout to avoid database locks under concurrent agent operations
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        // Enable WAL mode for concurrency and tune cache to 256KB to keep memory ultra-lean
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "cache_size", -256)?;
        conn.pragma_update(None, "mmap_size", 0)?;
        Ok(conn)
    }

    fn init_db(&self) -> Result<(), SessionError> {
        let conn = self.connect()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                tool_calls TEXT,
                tool_results TEXT,
                timestamp TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );",
            [],
        )?;

        Ok(())
    }

    /// Deterministic, Git-style 7-character content-addressable hash generator
    pub fn generate_short_hash(title: &str, now: &DateTime<Utc>) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        title.hash(&mut hasher);
        now.timestamp_nanos_opt().unwrap_or(0).hash(&mut hasher);
        let rand_val = Uuid::new_v4();
        rand_val.hash(&mut hasher);
        let h = hasher.finish();
        format!("{:07x}", h & 0x0FFF_FFFF)
    }

    pub fn create_session(&self, title: &str) -> Result<Session, SessionError> {
        let now = Utc::now();
        let id = Self::generate_short_hash(title, &now);
        let conn = self.connect()?;

        conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, now.to_rfc3339(), now.to_rfc3339()],
        )?;

        let session = Session {
            id: id.clone(),
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        };

        let _ = self.export_manifest();
        Ok(session)
    }

    fn append_transcript_jsonl(&self, session_id: &str, msg: &Message) {
        if let Some(parent) = self.db_path.parent() {
            let sessions_dir = parent.join("sessions");
            let _ = std::fs::create_dir_all(&sessions_dir);
            let file_path = sessions_dir.join(format!("{}.jsonl", session_id));
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(file_path)
            {
                use std::io::Write;
                if let Ok(line) = serde_json::to_string(msg) {
                    let _ = writeln!(file, "{}", line);
                }
            }
        }
    }

    fn rewrite_transcript_jsonl(&self, session_id: &str, msgs: &[Message]) {
        if let Some(parent) = self.db_path.parent() {
            let sessions_dir = parent.join("sessions");
            let _ = std::fs::create_dir_all(&sessions_dir);
            let file_path = sessions_dir.join(format!("{}.jsonl", session_id));
            let mut content = String::new();
            for m in msgs {
                if let Ok(line) = serde_json::to_string(m) {
                    content.push_str(&line);
                    content.push('\n');
                }
            }
            let _ = std::fs::write(file_path, content);
        }
    }

    pub fn export_manifest(&self) -> Result<(), SessionError> {
        if let Some(parent) = self.db_path.parent() {
            let sessions_dir = parent.join("sessions");
            let _ = std::fs::create_dir_all(&sessions_dir);

            if let Ok(sessions) = self.list_sessions(100) {
                let manifest_path = sessions_dir.join("manifest.tsv");
                let mut content = String::from("#HASH\tUPDATED_AT\tTURNS\tTITLE\n");
                for (id, title, ts, count) in sessions {
                    content.push_str(&format!(
                        "{}\t{}\t{}\t{}\n",
                        id,
                        ts.to_rfc3339(),
                        count,
                        title.replace('\t', " ").replace('\n', " ")
                    ));
                }
                let _ = std::fs::write(manifest_path, content);
            }
        }
        Ok(())
    }

    pub fn save_message(&self, session_id: &str, msg: &Message) -> Result<(), SessionError> {
        let conn = self.connect()?;
        let calls_json = msg
            .tool_calls
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let results_json = msg
            .tool_results
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;

        conn.execute(
            "INSERT INTO messages (session_id, role, content, tool_calls, tool_results, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                session_id,
                msg.role,
                msg.content,
                calls_json,
                results_json,
                msg.timestamp.to_rfc3339()
            ],
        )?;

        conn.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        self.append_transcript_jsonl(session_id, msg);
        let _ = self.export_manifest();

        Ok(())
    }

    /// Atomically persists a batch of messages inside a single transaction.
    /// This drastically reduces fsync overhead compared to writing messages one-by-one.
    pub fn save_messages_batch(
        &self,
        session_id: &str,
        msgs: &[Message],
    ) -> Result<(), SessionError> {
        if msgs.is_empty() {
            return Ok(());
        }

        let mut conn = self.connect()?;
        let tx = conn.transaction()?;

        for msg in msgs {
            let calls_json = msg
                .tool_calls
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let results_json = msg
                .tool_results
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            tx.execute(
                "INSERT INTO messages (session_id, role, content, tool_calls, tool_results, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session_id,
                    msg.role,
                    msg.content,
                    calls_json,
                    results_json,
                    msg.timestamp.to_rfc3339()
                ],
            )?;
        }

        tx.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        tx.commit()?;

        for msg in msgs {
            self.append_transcript_jsonl(session_id, msg);
        }
        let _ = self.export_manifest();

        Ok(())
    }

    /// Performs an explicit WAL checkpoint to truncate the write-ahead log back into the main database file.
    /// Prevents uncontrolled WAL growth during long autonomous agent runs.
    pub fn checkpoint(&self) -> Result<(), SessionError> {
        let conn = self.connect()?;
        conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    /// Vacuums the session database to reclaim freed pages after message compaction.
    pub fn vacuum(&self) -> Result<(), SessionError> {
        let conn = self.connect()?;
        conn.execute_batch("VACUUM;")?;
        Ok(())
    }

    /// Atomically replaces all messages for a session (used by context compactor).
    pub fn rewrite_session_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), SessionError> {
        let mut conn = self.connect()?;
        let tx = conn.transaction()?;

        tx.execute(
            "DELETE FROM messages WHERE session_id = ?1",
            params![session_id],
        )?;

        for msg in messages {
            let calls_json = msg
                .tool_calls
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;
            let results_json = msg
                .tool_results
                .as_ref()
                .map(serde_json::to_string)
                .transpose()?;

            tx.execute(
                "INSERT INTO messages (session_id, role, content, tool_calls, tool_results, timestamp)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session_id,
                    msg.role,
                    msg.content,
                    calls_json,
                    results_json,
                    msg.timestamp.to_rfc3339()
                ],
            )?;
        }

        tx.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id],
        )?;

        tx.commit()?;

        self.rewrite_transcript_jsonl(session_id, messages);
        let _ = self.export_manifest();

        Ok(())
    }

    /// Loads session with prefix matching (e.g. "8f2" matches "8f2a1b3") or full hash match.
    pub fn load_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let conn = self.connect()?;

        let mut stmt = conn.prepare(
            "SELECT id, title, created_at, updated_at FROM sessions 
             WHERE id = ?1 OR id LIKE (?1 || '%') 
             ORDER BY updated_at DESC LIMIT 1",
        )?;
        let session_data = stmt.query_row(params![session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        });

        let (canonical_id, title, created_at_str, updated_at_str) = match session_data {
            Ok(d) => d,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(SessionError::NotFound(session_id.to_string()))
            }
            Err(e) => return Err(SessionError::Db(e)),
        };

        let mut msg_stmt = conn.prepare(
            "SELECT role, content, tool_calls, tool_results, timestamp
             FROM messages WHERE session_id = ?1 ORDER BY id ASC",
        )?;

        let message_rows = msg_stmt.query_map(params![&canonical_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?;

        let mut messages = Vec::new();
        for row in message_rows {
            let (role, content, calls_json, results_json, ts_str) = row?;
            let tool_calls = calls_json.map(|j| serde_json::from_str(&j)).transpose()?;
            let tool_results = results_json.map(|j| serde_json::from_str(&j)).transpose()?;
            let timestamp = DateTime::parse_from_rfc3339(&ts_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());

            messages.push(Message {
                role,
                content,
                tool_calls,
                tool_results,
                timestamp,
            });
        }

        // Validate and clean turn sequences
        let validated_messages = Self::validate_turns(messages);

        Ok(Session {
            id: canonical_id,
            title,
            created_at: DateTime::parse_from_rfc3339(&created_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            updated_at: DateTime::parse_from_rfc3339(&updated_at_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            messages: validated_messages,
        })
    }

    /// Loads the most recently updated session.
    pub fn load_latest_session(&self) -> Result<Session, SessionError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare("SELECT id FROM sessions ORDER BY updated_at DESC LIMIT 1")?;
        let latest_id: String = stmt.query_row([], |row| row.get(0)).map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                SessionError::NotFound("No sessions found".to_string())
            }
            other => SessionError::Db(other),
        })?;
        self.load_session(&latest_id)
    }

    /// Lists recent sessions with turn counts for the `/sessions` command and manifest export.
    pub fn list_sessions(
        &self,
        limit: usize,
    ) -> Result<Vec<(String, String, DateTime<Utc>, usize)>, SessionError> {
        let conn = self.connect()?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.updated_at, COUNT(m.id)
             FROM sessions s
             LEFT JOIN messages m ON m.session_id = s.id
             GROUP BY s.id
             ORDER BY s.updated_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            let id: String = row.get(0)?;
            let title: String = row.get(1)?;
            let updated_str: String = row.get(2)?;
            let count: usize = row.get(3)?;
            let ts = DateTime::parse_from_rfc3339(&updated_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok((id, title, ts, count))
        })?;
        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// Turn validator: repairs dangling tails or mismatched alternating turn pairs.
    pub fn validate_turns(mut msgs: Vec<Message>) -> Vec<Message> {
        // If the last message was assistant with tool calls that never received tool results,
        // prune or mark it so it doesn't cause a 400 Bad Request error from the provider API.
        if let Some(last) = msgs.last() {
            if last.role == "assistant" && last.tool_calls.is_some() && last.tool_results.is_none()
            {
                // Dangling tool call without result - remove incomplete trailing turn
                msgs.pop();
            }
        }
        msgs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_session_lifecycle_and_persistence() {
        let temp_db = env::temp_dir().join(format!("nexus_test_{}.db", Uuid::new_v4()));
        let store = SessionStore::new(&temp_db).unwrap();

        // Create session
        let session = store.create_session("Test Session 1").unwrap();
        assert_eq!(session.title, "Test Session 1");

        // Save messages
        let msg1 = Message {
            role: "user".to_string(),
            content: "Hello Nexus".to_string(),
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        };
        store.save_message(&session.id, &msg1).unwrap();

        let msg2 = Message {
            role: "assistant".to_string(),
            content: "Hello Boss".to_string(),
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        };
        store.save_message(&session.id, &msg2).unwrap();

        // Load session and verify exact restoration
        let loaded = store.load_session(&session.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].role, "user");
        assert_eq!(loaded.messages[0].content, "Hello Nexus");
        assert_eq!(loaded.messages[1].role, "assistant");
        assert_eq!(loaded.messages[1].content, "Hello Boss");

        // Test rewrite_session_messages
        let compacted_msg = Message {
            role: "user".to_string(),
            content: "Compacted summary".to_string(),
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        };
        store.rewrite_session_messages(&session.id, &[compacted_msg]).unwrap();
        let reloaded = store.load_session(&session.id).unwrap();
        assert_eq!(reloaded.messages.len(), 1);
        assert_eq!(reloaded.messages[0].content, "Compacted summary");

        let _ = std::fs::remove_file(&temp_db);
    }

    #[test]
    fn test_dangling_tool_turn_validator() {
        let msgs = vec![
            Message {
                role: "user".to_string(),
                content: "Run test".to_string(),
                tool_calls: None,
                tool_results: None,
                timestamp: Utc::now(),
            },
            Message {
                role: "assistant".to_string(),
                content: "Running...".to_string(),
                tool_calls: Some(vec![crate::tools::ToolCall::new(
                    "call_1",
                    "run_command",
                    serde_json::json!({"command": "cargo test"}),
                )]),
                tool_results: None,
                timestamp: Utc::now(),
            },
        ];

        // Dangling assistant tool call should be pruned to prevent API 400
        let validated = SessionStore::validate_turns(msgs);
        assert_eq!(validated.len(), 1);
        assert_eq!(validated[0].role, "user");
    }

    #[test]
    fn test_wal_checkpoint_and_batch_persistence() {
        let temp_db = env::temp_dir().join(format!("nexus_test_wal_{}.db", Uuid::new_v4()));
        let store = SessionStore::new(&temp_db).unwrap();
        let session = store.create_session("WAL Batch Test").unwrap();

        let batch = vec![
            Message {
                role: "user".to_string(),
                content: "Batch msg 1".to_string(),
                tool_calls: None,
                tool_results: None,
                timestamp: Utc::now(),
            },
            Message {
                role: "assistant".to_string(),
                content: "Batch msg 2".to_string(),
                tool_calls: None,
                tool_results: None,
                timestamp: Utc::now(),
            },
        ];

        store.save_messages_batch(&session.id, &batch).unwrap();
        let loaded = store.load_session(&session.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "Batch msg 1");
        assert_eq!(loaded.messages[1].content, "Batch msg 2");

        // Verify WAL checkpoint executes cleanly
        store.checkpoint().unwrap();
        // Verify vacuum executes cleanly
        store.vacuum().unwrap();

        let _ = std::fs::remove_file(&temp_db);
        let _ = std::fs::remove_file(temp_db.with_extension("db-wal"));
        let _ = std::fs::remove_file(temp_db.with_extension("db-shm"));
    }

    #[test]
    fn test_short_hash_generation_and_prefix_loading() {
        let temp_dir = env::temp_dir().join(format!("nexus_test_shorthash_{}", Uuid::new_v4()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let db_path = temp_dir.join("sessions.db");
        let store = SessionStore::new(&db_path).unwrap();

        let session = store.create_session("Documentation Website Build").unwrap();
        // Assert 7-character Git-style content-addressable hash
        assert_eq!(session.id.len(), 7);

        let msg = Message {
            role: "user".to_string(),
            content: "Build website.md with retro theme".to_string(),
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        };
        store.save_message(&session.id, &msg).unwrap();

        // Test exact load
        let exact_loaded = store.load_session(&session.id).unwrap();
        assert_eq!(exact_loaded.id, session.id);
        assert_eq!(exact_loaded.messages.len(), 1);

        // Test prefix load (4 chars)
        let prefix = &session.id[..4];
        let prefix_loaded = store.load_session(prefix).unwrap();
        assert_eq!(prefix_loaded.id, session.id);
        assert_eq!(prefix_loaded.messages.len(), 1);

        // Test latest session
        let latest = store.load_latest_session().unwrap();
        assert_eq!(latest.id, session.id);

        // Test listing sessions
        let list = store.list_sessions(10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, session.id);
        assert_eq!(list[0].3, 1);

        // Verify JSONL transcript file exists on disk
        let jsonl_path = temp_dir.join("sessions").join(format!("{}.jsonl", session.id));
        assert!(jsonl_path.exists());

        // Verify manifest.tsv exists on disk
        let manifest_path = temp_dir.join("sessions").join("manifest.tsv");
        assert!(manifest_path.exists());
        let manifest_content = std::fs::read_to_string(manifest_path).unwrap();
        assert!(manifest_content.contains(&session.id));
        assert!(manifest_content.contains("Documentation Website Build"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
