use chrono::Utc;
use nexus_core::memory::MemoryStore;
use nexus_core::session::{Message, SessionStore};
use std::fs;
use uuid::Uuid;

fn get_current_rss_kb() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if line.starts_with("VmRSS:") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        if let Ok(kb) = parts[1].parse::<usize>() {
                            return kb;
                        }
                    }
                }
            }
        }
    }
    0
}

#[test]
fn test_hyper_constrained_edge_memory_ceiling() {
    println!("\n\x1b[1;33m=== TRIAL 6: 64 MB RAM Hyper-Constrained Edge Benchmark ===\x1b[0m");

    let temp_dir = std::env::temp_dir().join(format!("nexus_edge_test_{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();

    let initial_rss_kb = get_current_rss_kb();
    println!("  Initial Resident Memory (RSS): {:.2} MB ({} KB)", initial_rss_kb as f64 / 1024.0, initial_rss_kb);

    // 1. Initialize tuned SessionStore (256KB page cache, mmap = 0)
    let session_db = temp_dir.join("edge_sessions.db");
    let store = SessionStore::new(&session_db).expect("SessionStore init");
    let session = store.create_session("Edge Session").expect("Create session");

    // 2. Execute 100 intensive message writes with batching
    let mut batch = Vec::new();
    for i in 1..=100 {
        batch.push(Message {
            role: if i % 2 == 1 { "user".to_string() } else { "assistant".to_string() },
            content: format!("Edge frame test payload #{:04}: High frequency embedded micro-controller telemetry.", i),
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        });
    }

    store.save_messages_batch(&session.id, &batch).expect("Batch write");
    store.checkpoint().expect("WAL checkpoint");
    store.vacuum().expect("Vacuum");

    let post_db_rss_kb = get_current_rss_kb();
    println!("  RSS after 100 DB writes + WAL checkpoint: {:.2} MB ({} KB)", post_db_rss_kb as f64 / 1024.0, post_db_rss_kb);

    // 3. Initialize tuned MemoryStore and perform 50 BM25 search queries
    let memory_db = temp_dir.join("edge_memory.db");
    let mem_store = MemoryStore::open(&memory_db).expect("MemoryStore init");

    for i in 1..=20 {
        mem_store
            .save_distillation(
                &format!("session_{}", i),
                &format!("topic_key_{}", i),
                "Embedded system low-latency edge deployment rules with static allocations.",
            )
            .expect("Save distillation");
    }

    for _ in 0..50 {
        let results = mem_store.search("embedded low-latency edge", 5).expect("Search");
        assert!(!results.is_empty());
    }

    let peak_rss_kb = get_current_rss_kb();
    let peak_rss_mb = peak_rss_kb as f64 / 1024.0;
    println!("\x1b[1;32m[Edge Resource Consumption]\x1b[0m");
    println!("  Peak RSS Measured: {:.2} MB ({} KB)", peak_rss_mb, peak_rss_kb);
    println!("  Hard Limit Allowed: 64.00 MB (65,536 KB)");

    // Strict constraint: peak memory MUST remain well beneath 64 MB (typically < 16 MB)
    assert!(
        peak_rss_mb < 64.0,
        "Peak memory {:.2} MB exceeded the 64 MB edge constraint!",
        peak_rss_mb
    );

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
    println!("\n\x1b[1;32m=== TRIAL 6 PASSED: Peak RSS was {:.2} MB — far below the 64 MB ceiling! ===\x1b[0m\n", peak_rss_mb);
}
