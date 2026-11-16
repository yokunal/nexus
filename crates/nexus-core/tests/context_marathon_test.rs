use chrono::Utc;
use nexus_core::context::{ContextBudget, InterruptedOutputManager, MacroCompactor};
use nexus_core::session::{Message, SessionStore};
use std::fs;
use uuid::Uuid;

#[test]
fn test_100k_token_context_marathon_compaction_and_durability() {
    println!("\n\x1b[1;36m=== TRIAL 4: 100k+ Token Context Marathon & Compaction Resilience ===\x1b[0m");

    let temp_dir = std::env::temp_dir().join(format!("nexus_marathon_test_{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("marathon_sessions.db");

    let store = SessionStore::new(&db_path).expect("SessionStore init");
    let session = store.create_session("100k Token Marathon").expect("Create session");

    // 1. Build an extended 30-turn software engineering marathon
    let mut history = Vec::new();
    let mut estimated_tokens = 0;

    for turn in 1..=30 {
        let is_user = turn % 2 == 1;
        let role = if is_user { "user" } else { "assistant" };

        // Generate massive tool outputs & diagnostic logs (approx 3,500 tokens per turn)
        let mut large_content = format!(
            "Turn #{}: Executing phase of the compiler and architecture pipeline.\n",
            turn
        );
        for i in 1..=75 {
            large_content.push_str(&format!(
                "  [Diagnostic Frame #{:03}] Component module_{:02} AST inspection: verifying invariants, lifetime bounds, and memory layout.\n\
                 pub fn pipeline_stage_{}_{}() -> Result<Vec<u8>, PipelineError> {{\n\
                     let mut buffer = Vec::with_capacity(1024);\n\
                     buffer.extend_from_slice(b\"frame_data_block_{}_{}\");\n\
                     Ok(buffer)\n\
                 }}\n",
                i, i % 10, turn, i, turn, i
            ));
        }

        estimated_tokens += large_content.len() / 4; // ~4 chars per token

        let msg = Message {
            role: role.to_string(),
            content: large_content,
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        };

        store.save_message(&session.id, &msg).expect("Save message");
        history.push(msg);
    }

    println!("✓ Generated 30-turn continuous engineering history: ~{} tokens", estimated_tokens);
    assert!(
        estimated_tokens >= 100_000,
        "Marathon history must exceed 100,000 tokens, got {}",
        estimated_tokens
    );

    // 2. ContextBudget threshold evaluation
    let budget = ContextBudget::new(128_000, 0.75);
    let threshold = budget.threshold_tokens();
    assert_eq!(threshold, 96_000);
    assert!(
        budget.is_above_threshold(estimated_tokens as u32),
        "100k+ tokens must cross 75% budget threshold"
    );

    let pill = budget.format_short_pill(estimated_tokens as u32);
    println!("✓ Context budget status pill: {}", pill);
    assert!(pill.starts_with("🗜️"));
    assert!(pill.contains("/128k"));

    // 3. MacroCompactor: Compress deep history into structured <context_summary>
    let (compacted_history, summary) = MacroCompactor::compact_history(
        &history,
        &temp_dir,
        Some("High-Throughput Compiler Architecture Pipeline"),
        4, // Preserve the latest 4 turns
    );

    println!("\x1b[1;32m[Compaction Performance Metrics]\x1b[0m");
    println!("  Compacted Turns: {}", summary.compacted_turns_count);
    println!("  Pre-Compact Tokens: {}", summary.pre_compact_tokens);
    println!("  Post-Compact Tokens: {}", summary.post_compact_tokens);
    println!("  Token Reduction: {:.1}%", summary.reduction_percentage());

    assert_eq!(compacted_history.len(), 5); // 1 summary turn + 4 preserved turns
    assert!(compacted_history[0].content.contains("<context_summary>"));
    assert!(compacted_history[0].content.contains("High-Throughput Compiler Architecture Pipeline"));
    assert!(
        summary.reduction_percentage() > 65.0,
        "Compaction must reduce token volume by at least 65%"
    );

    // 4. Atomic SQLite WAL Rewrite
    store
        .rewrite_session_messages(&session.id, &compacted_history)
        .expect("Atomic session rewrite");

    let loaded = store.load_session(&session.id).expect("Load session");
    assert_eq!(loaded.messages.len(), 5);
    assert!(loaded.messages[0].content.contains("<context_summary>"));
    println!("✓ Compacted session atomically committed and verified in SQLite WAL.");

    // 5. Interrupted Output Continuation Stitching
    let truncated_code = "pub struct EngineRouter {\n    routes: Arc<RwLock<HashMap<String, RouteHandler>>>,\n    active: AtomicBool,";
    let wrapped = InterruptedOutputManager::wrap_interrupted_output(truncated_code);
    assert!(wrapped.contains("<interrupted-output>"));

    let extracted = InterruptedOutputManager::extract_interrupted_output(&wrapped);
    assert_eq!(extracted, Some(truncated_code.to_string()));
    println!("✓ Interrupted code continuation stream extracted cleanly.");

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
    println!("\n\x1b[1;32m=== TRIAL 4 PASSED: 100k+ Marathon handled with >65% compaction resilience! ===\x1b[0m\n");
}
