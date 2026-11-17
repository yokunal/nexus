use nexus_core::memory::MemoryStore;
use nexus_core::rust_specialist::{BorrowTutor, CheckMode, RustCheckRunner};
use nexus_core::sandbox::Sandbox;
use nexus_core::skills::SkillManager;
use nexus_core::tools::edit_file::{safe_replace, EditError, PatchTier};
use nexus_core::tools::run_command::{check_destructive_blacklist, get_scrubbed_env};
use std::fs;
use uuid::Uuid;

#[tokio::test]
async fn test_step1_rust_doctor_auto_tutor() {
    println!("\n\x1b[1;36m=== STEP 1: The Rust Doctor Auto-Tutor ===\x1b[0m");
    let test_proj = std::env::temp_dir().join("nexus_dogfood_rust_test_project");
    if !test_proj.join("Cargo.toml").exists() {
        fs::create_dir_all(test_proj.join("src")).unwrap();
        fs::write(
            test_proj.join("Cargo.toml"),
            "[package]\nname = \"rust_test_project\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        fs::write(
            test_proj.join("src/main.rs"),
            r#"fn main() {
    let s = String::from("hello");
    let s2 = s;
    println!("{}", s);
}
"#,
        )
        .unwrap();
    }

    let result = RustCheckRunner::run_check(&test_proj, CheckMode::Check)
        .await
        .expect("cargo check runner must execute");

    assert!(
        !result.success,
        "Rust project with E0382 must fail compilation"
    );
    assert!(
        !result.diagnostics.is_empty(),
        "Must capture structured JSON diagnostics"
    );

    let e0382_diag = result
        .diagnostics
        .iter()
        .find(|d| d.code.as_ref().map(|c| c.code.as_str()) == Some("E0382"))
        .expect("Must capture E0382 borrow of moved value diagnostic");

    println!("Captured Diagnostic: {:?}", e0382_diag.message);

    let explanation = BorrowTutor::advise(e0382_diag);
    assert!(explanation.is_some(), "BorrowTutor must explain E0382");

    let advice = explanation.unwrap();
    println!("\x1b[1;32m[Borrow Tutor Advice]\x1b[0m\n{}", advice);

    assert!(advice.contains("Use of Moved Value"));
    assert!(advice.contains("Borrow instead of moving"));
    println!("\x1b[1;32m✓ Step 1 PASSED: Rust Doctor caught E0382 and provided zero-slop repair guidance!\x1b[0m\n");
}

#[test]
fn test_step2_patcher_and_ambiguity_guard() {
    println!("\n\x1b[1;36m=== STEP 2: 3-Tier Patcher & Ambiguity Guard ===\x1b[0m");
    let temp_file = std::env::temp_dir().join(format!("nexus_patch_test_{}.txt", Uuid::new_v4()));

    // 1. Ambiguity Guard test
    let ambiguous_content = "let value = 100;\nlet value = 100;\n";
    fs::write(&temp_file, ambiguous_content).unwrap();

    let err = safe_replace(&temp_file, "let value = 100;", "let value = 200;").unwrap_err();
    assert!(
        matches!(err, EditError::AmbiguousMatch { occurrences: 2, .. }),
        "Must reject duplicate target with AmbiguousMatch"
    );
    println!("✓ Ambiguity Guard correctly blocked ambiguous patch");

    // 2. Tier 2 Whitespace-normalized test
    let indented_content = "fn compute() {\n        let factor = 2;\n        factor * 10\n}\n";
    fs::write(&temp_file, indented_content).unwrap();

    // Model provides 4 spaces instead of 8
    let unaligned_target = "    let factor = 2;\n    factor * 10";
    let replacement = "        let factor = 5;\n        factor * 10";

    let report = safe_replace(&temp_file, unaligned_target, replacement).unwrap();
    assert_eq!(report.tier, PatchTier::Tier2WhitespaceNormalized);
    let patched = fs::read_to_string(&temp_file).unwrap();
    assert!(patched.contains("let factor = 5;"));
    println!("✓ Tier 2 successfully normalized indentation and patched file");

    // 3. Tier 3 Anchor context test
    let anchor_content =
        "fn run() {\n    // start trace\n    step_one();\n    step_two();\n    // end trace\n}\n";
    fs::write(&temp_file, anchor_content).unwrap();

    let anchor_target = "fn run() {\n    step_one();\n    step_two();\n    // end trace\n}";
    let anchor_replacement =
        "fn run() {\n    step_one_v2();\n    step_two_v2();\n    // end trace\n}";

    let report3 = safe_replace(&temp_file, anchor_target, anchor_replacement).unwrap();
    assert_eq!(report3.tier, PatchTier::Tier3AnchorContext);
    let patched3 = fs::read_to_string(&temp_file).unwrap();
    assert!(patched3.contains("step_one_v2();"));
    println!("✓ Tier 3 successfully applied multi-line anchor context patch");

    println!("\x1b[1;32m✓ Step 2 PASSED: 3-Tier Patcher passes all safety and flexibility barriers!\x1b[0m\n");
}

#[test]
fn test_step3_security_sandbox_and_quarantine() {
    println!("\n\x1b[1;36m=== STEP 3: Security Sandbox & Command Quarantine ===\x1b[0m");
    let sandbox_dir = std::env::temp_dir().join(format!("nexus_sec_test_{}", Uuid::new_v4()));
    fs::create_dir_all(&sandbox_dir).unwrap();

    let sandbox = Sandbox::new(&sandbox_dir, false).unwrap();

    // 1. Path traversal test
    let escape_attempt = sandbox.resolve_path("../../etc/shadow");
    assert!(
        escape_attempt.is_err(),
        "Path traversal escape must be hard blocked"
    );
    println!(
        "✓ Path traversal blocked: {:?}",
        escape_attempt.unwrap_err()
    );

    // 2. Destructive blacklist test
    assert!(
        check_destructive_blacklist("rm -rf /").is_err(),
        "rm -rf / must be blacklisted"
    );
    assert!(
        check_destructive_blacklist(":(){ :|:& };:").is_err(),
        "forkbomb must be blacklisted"
    );
    println!("✓ Blacklisted destructive commands successfully intercepted");

    // 3. Sensitive env scrubbing test
    std::env::set_var("GITHUB_TOKEN", "gho_super_secret_token");
    std::env::set_var("SSH_AUTH_SOCK", "/tmp/ssh_auth.sock");
    std::env::set_var("HTTP_PROXY", "http://corporate.proxy:8080");

    let scrubbed = get_scrubbed_env();
    let scrubbed_keys: Vec<String> = scrubbed.into_iter().map(|(k, _)| k).collect();
    assert!(!scrubbed_keys.contains(&"GITHUB_TOKEN".to_string()));
    assert!(!scrubbed_keys.contains(&"SSH_AUTH_SOCK".to_string()));
    assert!(!scrubbed_keys.contains(&"HTTP_PROXY".to_string()));
    println!("✓ High-value environment secrets scrubbed from subprocess environment");

    println!("\x1b[1;32m✓ Step 3 PASSED: Security containment and quarantine boundaries verified!\x1b[0m\n");
}

#[test]
fn test_step4_self_synthesizing_skills() {
    println!("\n\x1b[1;36m=== STEP 4: Self-Synthesizing Skills Sandbox ===\x1b[0m");
    let workspace = std::env::temp_dir().join(format!("nexus_skill_wf_{}", Uuid::new_v4()));
    fs::create_dir_all(&workspace).unwrap();

    // 1. Synthesize into staging quarantine
    let skill = SkillManager::synthesize_skill(
        &workspace,
        "git-sheriff",
        "Audits branch hygiene and dirty working trees",
        "1. Run git status\n2. Verify branch tracking",
    )
    .unwrap();

    assert!(skill.is_staged);
    assert!(skill.path.exists());
    println!(
        "✓ Skill synthesized into quarantine staging: {}",
        skill.path.display()
    );

    // 2. List skills
    let skills = SkillManager::load_all(&workspace).unwrap();
    assert_eq!(skills.len(), 1);
    assert!(skills[0].is_staged);

    let prompt = SkillManager::format_skills_prompt(&skills);
    assert!(prompt.contains("[STAGED TEST-DRIVE]"));
    println!("✓ Loaded staged skill into system prompt context");

    // 3. Promote to permanent storage
    SkillManager::promote_skill(&workspace, "git-sheriff").unwrap();
    let perm_skills = SkillManager::load_all(&workspace).unwrap();
    assert_eq!(perm_skills.len(), 1);
    assert!(!perm_skills[0].is_staged);
    println!("✓ Skill promoted to permanent storage (.nexus/skills/git-sheriff)");

    println!("\x1b[1;32m✓ Step 4 PASSED: Skill self-synthesis, quarantine staging, and promotion verified!\x1b[0m\n");
}

#[test]
fn test_step5_memory_recall_and_benchmark() {
    println!("\n\x1b[1;36m=== STEP 5: Cross-Session Memory & Latency Benchmark ===\x1b[0m");
    let mem_dir = std::env::temp_dir().join(format!("nexus_mem_test_{}.db", Uuid::new_v4()));
    let store = MemoryStore::open(&mem_dir).unwrap();

    // 1. Distill key learnings
    store
        .save_distillation(
            "session-alpha",
            "borrow_checker_e0382",
            "Fixed borrow of moved value by passing &str reference instead of taking ownership.",
        )
        .unwrap();

    store
        .save_distillation(
            "session-beta",
            "lsp_stdio_transport",
            "LSP framing requires exact Content-Length headers with \\r\\n\\r\\n separators.",
        )
        .unwrap();

    // 2. Search query with BM25 ranking
    let results = store.search("borrow checker moved value", 5).unwrap();
    assert!(!results.is_empty());
    assert_eq!(results[0].topic_or_role, "borrow_checker_e0382");
    println!(
        "✓ Memory search retrieved top result '{}' with BM25 score {:.2}",
        results[0].topic_or_role, results[0].score
    );

    // 3. Microsecond Latency Benchmark
    let queries = [
        "borrow checker",
        "lsp framing",
        "content length",
        "ownership",
        "transport",
    ];
    let report = store.benchmark_search_throughput(&queries).unwrap();

    println!("\x1b[1;36m[Benchmark Latency Results]\x1b[0m");
    println!("  Total Queries: {}", report.total_queries);
    println!(
        "  Average Latency: {:.2} µs ({:.4} ms)",
        report.avg_latency_micros,
        report.avg_latency_micros / 1000.0
    );
    println!("  P99 Latency: {:.2} µs", report.p99_latency_micros);

    assert!(
        report.avg_latency_micros < 1000.0,
        "Search latency must be sub-millisecond!"
    );
    println!("\x1b[1;32m✓ Step 5 PASSED: Instant sub-millisecond cross-session recall confirmed!\x1b[0m\n");
}

#[test]
fn test_step6_context_engineering_compaction_and_continuation() {
    println!("\n\x1b[1;36m=== STEP 6: Native Context Engineering, Compaction & Continuation ===\x1b[0m");
    use nexus_core::context::{ContextBudget, MacroCompactor, InterruptedOutputManager};
    use nexus_core::session::{SessionStore, Message};
    use chrono::Utc;

    // 1. ContextBudget & Pill formatting
    let budget = ContextBudget::new(128_000, 0.75);
    assert_eq!(budget.threshold_tokens(), 96_000);
    assert!(!budget.is_above_threshold(95_999));
    assert!(budget.is_above_threshold(96_000));
    assert_eq!(budget.format_short_pill(50_000), "50.0k/128k");
    println!("✓ Context budget threshold and status pill formatting verified");

    // 2. MacroCompaction & Session persistence rewrite
    let db_path = std::env::temp_dir().join(format!("nexus_context_test_{}.db", Uuid::new_v4()));
    let store = SessionStore::new(&db_path).unwrap();
    let session = store.create_session("Context Compaction Session").unwrap();

    let mut history = Vec::new();
    for i in 1..=12 {
        let msg = Message {
            role: if i % 2 == 1 { "user".to_string() } else { "assistant".to_string() },
            content: format!(
                "Iteration {}: Inspecting module components and build logs.\n\
                 pub fn execute_work_{}() {{\n    \
                     let mut v = Vec::new();\n    \
                     for item in 0..100 {{\n        \
                         v.push(format!(\"data_{{}}\", item));\n    \
                     }}\n    \
                     assert_eq!(v.len(), 100);\n\
                 }}\n\
                 Detailed diagnostic log stream for turn {}:\n\
                 Compiling crate module v0.1.0\n\
                 Checking dependencies...\n\
                 Analyzing borrow invariants and lifecycle constraints...\n\
                 Target compiled successfully with 0 warnings.\n\
                 Compiler reports: zero errors, proceeding to next phase.",
                i, i, i
            ),
            tool_calls: None,
            tool_results: None,
            timestamp: Utc::now(),
        };
        store.save_message(&session.id, &msg).unwrap();
        history.push(msg);
    }

    let temp_workspace = std::env::temp_dir();
    let (compacted_history, summary) = MacroCompactor::compact_history(
        &history,
        &temp_workspace,
        Some("Refactor compilation pipeline"),
        2,
    );

    assert_eq!(compacted_history.len(), 3); // 1 summary + 2 preserved
    assert!(compacted_history[0].content.contains("<context_summary>"));
    assert!(compacted_history[0].content.contains("Refactor compilation pipeline"));
    assert_eq!(summary.compacted_turns_count, 10);
    assert!(summary.pre_compact_tokens > summary.post_compact_tokens);
    assert!(summary.reduction_percentage() > 50.0);

    store.rewrite_session_messages(&session.id, &compacted_history).unwrap();
    let reloaded = store.load_session(&session.id).unwrap();
    assert_eq!(reloaded.messages.len(), 3);
    assert!(reloaded.messages[0].content.contains("<context_summary>"));
    println!("✓ Macro-compaction token reduction ({:.1}%) and SQLite atomic rewrite verified", summary.reduction_percentage());

    // 3. Interrupted Output Continuation
    let partial_stream = "pub fn calculate_hash() -> u64 {\n    let mut hasher = DefaultHasher::new();";
    let fenced = InterruptedOutputManager::wrap_interrupted_output(partial_stream);
    assert!(fenced.contains("<interrupted-output>"));
    assert!(fenced.contains("pub fn calculate_hash()"));
    let extracted = InterruptedOutputManager::extract_interrupted_output(&fenced);
    assert_eq!(extracted, Some(partial_stream.to_string()));
    println!("✓ Interrupted output wrapping and extraction verified");

    let _ = std::fs::remove_file(&db_path);
    println!("\x1b[1;32m✓ Step 6 PASSED: End-to-end Context Engineering Engine verified!\x1b[0m\n");
}
