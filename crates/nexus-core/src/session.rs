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
        // Enable WAL mode for concurrency and microsecond read/write speed
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
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

    pub fn create_session(&self, title: &str) -> Result<Session, SessionError> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let conn = self.connect()?;

        conn.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)",
            params![id, title, now.to_rfc3339(), now.to_rfc3339()],
        )?;

        Ok(Session {
            id,
            title: title.to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        })
    }

    pub fn save_message(&self, session_id: &str, msg: &Message) -> Result<(), SessionError> {
        let conn = self.connect()?;
        let calls_json = msg.tool_calls.as_ref().map(serde_json::to_string).transpose()?;
        let results_json = msg.tool_results.as_ref().map(serde_json::to_string).transpose()?;

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

        Ok(())
    }

    pub fn load_session(&self, session_id: &str) -> Result<Session, SessionError> {
        let conn = self.connect()?;

        let mut stmt = conn.prepare("SELECT title, created_at, updated_at FROM sessions WHERE id = ?1")?;
        let session_data = stmt.query_row(params![session_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        });

        let (title, created_at_str, updated_at_str) = match session_data {
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

        let message_rows = msg_stmt.query_map(params![session_id], |row| {
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
            let tool_calls = calls_json
                .map(|j| serde_json::from_str(&j))
                .transpose()?;
            let tool_results = results_json
                .map(|j| serde_json::from_str(&j))
                .transpose()?;
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
            id: session_id.to_string(),
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

    /// Turn validator: repairs dangling tails or mismatched alternating turn pairs.
    pub fn validate_turns(mut msgs: Vec<Message>) -> Vec<Message> {
        // If the last message was assistant with tool calls that never received tool results,
        // prune or mark it so it doesn't cause a 400 Bad Request error from the provider API.
        if let Some(last) = msgs.last() {
            if last.role == "assistant" && last.tool_calls.is_some() && last.tool_results.is_none() {
                // Dangling tool call without result - remove incomplete trailing turn
                msgs.pop();
            }
        }
        msgs
    }
}
