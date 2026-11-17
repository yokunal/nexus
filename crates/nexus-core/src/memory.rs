use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use thiserror::Error;
use uuid::Uuid;

#[derive(Error, Debug)]
pub enum MemoryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DistilledMemory {
    pub id: String,
    pub session_id: String,
    pub topic: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnRecord {
    pub id: i64,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySearchResult {
    pub source: String, // "distilled" or "session_turn"
    pub session_id: String,
    pub topic_or_role: String,
    pub snippet: String,
    pub score: f32,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkReport {
    pub total_queries: usize,
    pub total_duration_micros: u128,
    pub avg_latency_micros: f64,
    pub p99_latency_micros: f64,
}

#[derive(Clone)]
pub struct MemoryStore {
    conn: Arc<Mutex<Connection>>,
}

impl MemoryStore {
    /// Opens or creates the SQLite memory database with WAL journaling.
    pub fn open(db_path: &Path) -> Result<Self, MemoryError> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA cache_size = -256;
             PRAGMA mmap_size = 0;
             
             CREATE TABLE IF NOT EXISTS distilled_memories (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 topic TEXT NOT NULL,
                 content TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_distilled_topic ON distilled_memories(topic);
             CREATE INDEX IF NOT EXISTS idx_distilled_session ON distilled_memories(session_id);

             CREATE TABLE IF NOT EXISTS session_turns (
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL,
                 role TEXT NOT NULL,
                 content TEXT NOT NULL,
                 timestamp TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_turns_session ON session_turns(session_id);",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Stores a distilled memory card (key learnings, architecture decisions, bug patterns).
    pub fn save_distillation(
        &self,
        session_id: &str,
        topic: &str,
        content: &str,
    ) -> Result<DistilledMemory, MemoryError> {
        let memory = DistilledMemory {
            id: Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            topic: topic.trim().to_string(),
            content: content.trim().to_string(),
            created_at: Utc::now(),
        };

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO distilled_memories (id, session_id, topic, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                memory.id,
                memory.session_id,
                memory.topic,
                memory.content,
                memory.created_at.to_rfc3339()
            ],
        )?;

        Ok(memory)
    }

    /// Indexes a conversation turn into memory.
    pub fn index_turn(
        &self,
        session_id: &str,
        role: &str,
        content: &str,
        timestamp: DateTime<Utc>,
    ) -> Result<(), MemoryError> {
        if content.trim().is_empty() {
            return Ok(());
        }

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO session_turns (session_id, role, content, timestamp)
             VALUES (?1, ?2, ?3, ?4)",
            params![session_id, role, content, timestamp.to_rfc3339()],
        )?;

        Ok(())
    }

    /// Shrinks SQLite internal pager memory and flushes unneeded pages back to system.
    pub fn shrink(&self) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute_batch("PRAGMA shrink_memory;");
        }
    }

    /// Searches across all distilled notes and conversation turns using BM25 relevance scoring.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>, MemoryError> {
        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();

        // 1. Search Distilled Memories
        let mut stmt = conn.prepare(
            "SELECT session_id, topic, content, created_at FROM distilled_memories ORDER BY created_at DESC LIMIT 200",
        )?;
        let distilled_rows = stmt.query_map([], |row| {
            let session_id: String = row.get(0)?;
            let topic: String = row.get(1)?;
            let content: String = row.get(2)?;
            let date_str: String = row.get(3)?;
            let ts = DateTime::parse_from_rfc3339(&date_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok((session_id, topic, content, ts))
        })?;

        for row in distilled_rows {
            let (session_id, topic, content, ts) = row?;
            let text = format!("{} {}", topic, content);
            let score = calculate_bm25_score(&query_tokens, &text) * 1.5; // 1.5x boost for distilled memories
            if score > 0.05 {
                results.push(MemorySearchResult {
                    source: "distilled".to_string(),
                    session_id,
                    topic_or_role: topic,
                    snippet: truncate_snippet(&content, 200),
                    score,
                    timestamp: ts,
                });
            }
        }

        // 2. Search Session Turns
        let mut turn_stmt = conn.prepare(
            "SELECT session_id, role, content, timestamp FROM session_turns ORDER BY id DESC LIMIT 500",
        )?;
        let turn_rows = turn_stmt.query_map([], |row| {
            let session_id: String = row.get(0)?;
            let role: String = row.get(1)?;
            let content: String = row.get(2)?;
            let date_str: String = row.get(3)?;
            let ts = DateTime::parse_from_rfc3339(&date_str)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now());
            Ok((session_id, role, content, ts))
        })?;

        for row in turn_rows {
            let (session_id, role, content, ts) = row?;
            let score = calculate_bm25_score(&query_tokens, &content);
            if score > 0.05 {
                results.push(MemorySearchResult {
                    source: "turn".to_string(),
                    session_id,
                    topic_or_role: role,
                    snippet: truncate_snippet(&content, 200),
                    score,
                    timestamp: ts,
                });
            }
        }

        // Sort by relevance score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);
        Ok(results)
    }

    /// Benchmarks search throughput on the local store.
    /// Asserts sub-millisecond execution times without external vector model overhead.
    pub fn benchmark_search_throughput(
        &self,
        queries: &[&str],
    ) -> Result<BenchmarkReport, MemoryError> {
        let mut latencies_micros = Vec::new();
        let start_total = Instant::now();

        for q in queries {
            let t0 = Instant::now();
            let _ = self.search(q, 10)?;
            latencies_micros.push(t0.elapsed().as_micros());
        }

        let total_duration = start_total.elapsed().as_micros();
        let total_queries = queries.len();
        latencies_micros.sort_unstable();

        let avg_latency = if total_queries > 0 {
            latencies_micros.iter().sum::<u128>() as f64 / total_queries as f64
        } else {
            0.0
        };

        let p99_idx = (total_queries as f32 * 0.99) as usize;
        let p99_latency = latencies_micros.get(p99_idx).copied().unwrap_or(0) as f64;

        Ok(BenchmarkReport {
            total_queries,
            total_duration_micros: total_duration,
            avg_latency_micros: avg_latency,
            p99_latency_micros: p99_latency,
        })
    }

    /// Formats search results for injection into the LLM system prompt.
    pub fn format_memory_prompt(results: &[MemorySearchResult]) -> String {
        if results.is_empty() {
            return String::new();
        }

        let mut out = String::from("## Recall: Relevant Historical Memories & Fixes\n");
        for r in results {
            out.push_str(&format!(
                "- **[{}] {}** (score: {:.2}): {}\n",
                r.source.to_uppercase(),
                r.topic_or_role,
                r.score,
                r.snippet
            ));
        }
        out
    }
}

fn tokenize(text: &str) -> HashSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| t.len() >= 2)
        .map(|s| s.to_string())
        .collect()
}

/// Computes token overlap and BM25 term frequency score between query tokens and target document.
fn calculate_bm25_score(query_tokens: &HashSet<String>, doc_text: &str) -> f32 {
    let doc_tokens: Vec<String> = doc_text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
        .filter(|t| t.len() >= 2)
        .map(|s| s.to_string())
        .collect();

    if doc_tokens.is_empty() {
        return 0.0;
    }

    let mut term_frequencies: HashMap<&str, usize> = HashMap::new();
    for token in &doc_tokens {
        *term_frequencies.entry(token.as_str()).or_insert(0) += 1;
    }

    let doc_len = doc_tokens.len() as f32;
    let avg_doc_len = 50.0;
    let k1 = 1.2;
    let b = 0.75;

    let mut score = 0.0;
    for q in query_tokens {
        if let Some(&tf) = term_frequencies.get(q.as_str()) {
            let tf = tf as f32;
            let term_score =
                (tf * (k1 + 1.0)) / (tf + k1 * (1.0 - b + b * (doc_len / avg_doc_len)));
            score += term_score;
        }
    }

    score
}

fn truncate_snippet(text: &str, max_len: usize) -> String {
    let clean = text.replace('\n', " ");
    if clean.len() <= max_len {
        clean
    } else {
        format!("{}...", &clean[..max_len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_store_distillation_and_search() {
        let temp_db = std::env::temp_dir().join(format!("nexus_mem_{}.db", Uuid::new_v4()));
        let store = MemoryStore::open(&temp_db).unwrap();

        // 1. Save distilled learnings
        store
            .save_distillation(
                "session-1",
                "borrow_checker_e0382",
                "Fixed borrow of moved value by cloning or passing reference before closure call.",
            )
            .unwrap();

        store
            .save_distillation(
                "session-2",
                "cargo_tls_configuration",
                "Configured reqwest with default-features = false and rustls-tls to avoid openssl-sys pkg-config dependency.",
            )
            .unwrap();

        // 2. Search
        let results = store.search("borrow checker moved value", 5).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].topic_or_role, "borrow_checker_e0382");

        let tls_results = store.search("rustls reqwest openssl", 5).unwrap();
        assert!(!tls_results.is_empty());
        assert_eq!(tls_results[0].topic_or_role, "cargo_tls_configuration");

        // 3. Benchmark latency
        let bench_queries = [
            "borrow checker",
            "reqwest rustls",
            "closure",
            "session",
            "compiler",
        ];
        let report = store.benchmark_search_throughput(&bench_queries).unwrap();
        assert_eq!(report.total_queries, 5);
        // Assert sub-millisecond (1,000 micros) latency
        assert!(
            report.avg_latency_micros < 1000.0,
            "Average latency was {:.2} micros",
            report.avg_latency_micros
        );
    }
}
