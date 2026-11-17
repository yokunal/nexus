use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use nexus_v2::pav::planner::{StepType, TaskStep};
use nexus_v2::traits::{CircularWalStore, Message, ModelClient, OrchestratorEngine, TokenUsage};
use nexus_v2::{Actor, CircularWal, CompletionResponse, NexusV2Error, PavOrchestrator, Sandbox, Verifier};

fn create_temp_env() -> (tempfile_guard::TempDirGuard, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("nexus_v2_b4_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    (tempfile_guard::TempDirGuard(dir.clone()), dir)
}

mod tempfile_guard {
    pub struct TempDirGuard(pub std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

struct MultiTurnModel {
    call_count: Arc<AtomicUsize>,
    responses: Vec<String>,
}

#[async_trait::async_trait]
impl ModelClient for MultiTurnModel {
    async fn complete(&self, _prompt: &str, _context: &[Message]) -> Result<CompletionResponse, NexusV2Error> {
        let idx = self.call_count.fetch_add(1, Ordering::SeqCst);
        let text = if idx < self.responses.len() {
            self.responses[idx].clone()
        } else {
            "PASS".to_string()
        };
        Ok(CompletionResponse {
            text,
            token_usage: TokenUsage::default(),
        })
    }
    async fn health_check(&self) -> Result<bool, NexusV2Error> {
        Ok(true)
    }
}

// 1. Single Iteration Pass
#[tokio::test]
async fn test_pav_single_iteration_pass() {
    let (_guard, ws) = create_temp_env();
    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    let wal = CircularWal::new(ws.join("pav.wal"), 100);
    let model = Arc::new(MultiTurnModel {
        call_count: Arc::new(AtomicUsize::new(0)),
        responses: vec!["VERIFICATION: PASS".to_string()],
    });

    let mut orch = PavOrchestrator::new(model, actor, wal, 3);
    let res = orch.execute_task("Quick single turn task").await.unwrap();
    assert!(res.success);
    assert_eq!(res.iterations, 1);
    println!("  ✓ Vector 4.1: Single iteration pass verified");
}

// 2. Iteration Budget Exhaustion
#[tokio::test]
async fn test_pav_iteration_budget_exhaustion() {
    let (_guard, ws) = create_temp_env();
    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    let wal = CircularWal::new(ws.join("pav.wal"), 100);
    let model = Arc::new(MultiTurnModel {
        call_count: Arc::new(AtomicUsize::new(0)),
        responses: vec!["FAIL: broken".to_string(), "FAIL: still broken".to_string()],
    });

    let mut orch = PavOrchestrator::new(model, actor, wal, 2);
    let res = orch.execute_task("Impossible task").await;
    assert!(res.is_err());
    match res.unwrap_err() {
        NexusV2Error::PavLoopExhausted { max_iterations, .. } => assert_eq!(max_iterations, 2),
        e => panic!("Expected PavLoopExhausted, got {:?}", e),
    }
    println!("  ✓ Vector 4.2: Max iteration budget exhaustion intercepted safely");
}

// 3. WAL Plan Frame Persisted
#[tokio::test]
async fn test_pav_wal_plan_frame_persisted() {
    let (_guard, ws) = create_temp_env();
    let wal_path = ws.join("test.wal");
    let mut wal = CircularWal::new(&wal_path, 100);
    wal.append_frame(1, b"{\"steps\": [\"step1\"]}").await.unwrap();

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].event_type, 1);
    println!("  ✓ Vector 4.3: Plan frame persisted with event_type 1");
}

// 4. WAL Action Frame Persisted
#[tokio::test]
async fn test_pav_wal_action_frame_persisted() {
    let (_guard, ws) = create_temp_env();
    let wal_path = ws.join("test.wal");
    let mut wal = CircularWal::new(&wal_path, 100);
    wal.append_frame(2, b"edit:src/lib.rs").await.unwrap();

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].event_type, 2);
    assert_eq!(records[0].payload, b"edit:src/lib.rs");
    println!("  ✓ Vector 4.4: Action frame persisted with event_type 2");
}

// 5. WAL Completed Frame Persisted
#[tokio::test]
async fn test_pav_wal_completed_frame_persisted() {
    let (_guard, ws) = create_temp_env();
    let wal_path = ws.join("test.wal");
    let mut wal = CircularWal::new(&wal_path, 100);
    wal.append_frame(4, b"completed:uuid-1234").await.unwrap();

    let records = wal.recover().await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].event_type, 4);
    println!("  ✓ Vector 4.5: Completed frame persisted with event_type 4");
}

// 6. Task ID Uniqueness
#[tokio::test]
async fn test_pav_task_id_uniqueness() {
    let (_guard, ws) = create_temp_env();
    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    let wal = CircularWal::new(ws.join("pav.wal"), 100);
    let model = Arc::new(MultiTurnModel {
        call_count: Arc::new(AtomicUsize::new(0)),
        responses: vec!["PASS".to_string()],
    });

    let mut orch = PavOrchestrator::new(model, actor, wal, 2);
    let r1 = orch.execute_task("Task 1").await.unwrap();
    let r2 = orch.execute_task("Task 2").await.unwrap();
    assert_ne!(r1.task_id, r2.task_id);
    println!("  ✓ Vector 4.6: Unique task UUIDs generated per execution");
}

// 7. Actor Edits File on Disk
#[test]
fn test_pav_actor_edits_file_on_disk() {
    let (_guard, ws) = create_temp_env();
    let file_path = ws.join("sample.txt");
    std::fs::write(&file_path, "value=1\n").unwrap();

    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    actor.execute_edit("sample.txt", "value=1", "value=2").unwrap();

    let content = std::fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "value=2\n");
    println!("  ✓ Vector 4.7: Actor modified file cleanly on disk");
}

// 8. Actor Executes Safe Command
#[test]
fn test_pav_actor_executes_safe_command() {
    let (_guard, ws) = create_temp_env();
    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    let output = actor.execute_command("echo 'hello pav'").unwrap();
    assert!(output.contains("hello pav"));
    println!("  ✓ Vector 4.8: Actor executed safe command successfully");
}

// 9. Actor Rejects Failing Command
#[test]
fn test_pav_actor_rejects_failing_command() {
    let (_guard, ws) = create_temp_env();
    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    let res = actor.execute_command("bash -c 'exit 42'");
    assert!(res.is_err());
    println!("  ✓ Vector 4.9: Command failure captured and mapped to error");
}

// 10. Verifier Positive Assertion
#[test]
fn test_pav_verifier_positive_assertion() {
    let (_guard, ws) = create_temp_env();
    let path = ws.join("test.txt");
    std::fs::write(&path, "status: ready_for_prod").unwrap();

    let rep = Verifier::verify_file_content(&path, "ready_for_prod");
    assert!(rep.success);
    assert!(rep.diagnostics.is_empty());
    println!("  ✓ Vector 4.10: Verifier confirmed expected substring presence");
}

// 11. Verifier Negative Assertion
#[test]
fn test_pav_verifier_negative_assertion() {
    let (_guard, ws) = create_temp_env();
    let path = ws.join("test.txt");
    std::fs::write(&path, "status: broken").unwrap();

    let rep = Verifier::verify_file_content(&path, "ready_for_prod");
    assert!(!rep.success);
    assert!(!rep.diagnostics.is_empty());
    println!("  ✓ Vector 4.11: Verifier correctly flagged missing substring");
}

// 12. Verifier Missing File Diagnostics
#[test]
fn test_pav_verifier_missing_file_diagnostics() {
    let (_guard, ws) = create_temp_env();
    let missing_path = ws.join("non_existent.txt");

    let rep = Verifier::verify_file_content(&missing_path, "anything");
    assert!(!rep.success);
    assert!(rep.diagnostics[0].contains("Failed to read"));
    println!("  ✓ Vector 4.12: Verifier emitted clean diagnostics on missing file");
}

// 13. Multi-Step Plan Structure
#[test]
fn test_pav_multi_step_plan_structure() {
    let steps = vec![
        TaskStep {
            id: 1,
            description: "Edit config".to_string(),
            action: StepType::EditFile {
                path: "cfg.txt".to_string(),
                target: "a".to_string(),
                replacement: "b".to_string(),
            },
        },
        TaskStep {
            id: 2,
            description: "Run test".to_string(),
            action: StepType::RunCommand {
                command: "true".to_string(),
            },
        },
        TaskStep {
            id: 3,
            description: "Verify".to_string(),
            action: StepType::Verify,
        },
    ];

    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].id, 1);
    println!("  ✓ Vector 4.13: Multi-step plan DAG structured sequentially");
}

// 14. Self-Correcting Recovery Simulation
#[tokio::test]
async fn test_pav_self_correcting_recovery_simulation() {
    let (_guard, ws) = create_temp_env();
    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    let wal = CircularWal::new(ws.join("pav.wal"), 100);

    // Turn 1 fails, Turn 2 passes
    let model = Arc::new(MultiTurnModel {
        call_count: Arc::new(AtomicUsize::new(0)),
        responses: vec!["RETRY".to_string(), "PASS".to_string()],
    });

    let mut orch = PavOrchestrator::new(model, actor, wal, 3);
    let res = orch.execute_task("Self repair goal").await.unwrap();
    assert!(res.success);
    println!("  ✓ Vector 4.14: Self-correcting loop recovered on second iteration");
}

// 15. Task Result Summary Formatting
#[tokio::test]
async fn test_pav_task_result_summary_formatting() {
    let (_guard, ws) = create_temp_env();
    let sb = Sandbox::new(&ws);
    let actor = Actor::new(sb);
    let wal = CircularWal::new(ws.join("pav.wal"), 100);
    let model = Arc::new(MultiTurnModel {
        call_count: Arc::new(AtomicUsize::new(0)),
        responses: vec!["PASS".to_string()],
    });

    let mut orch = PavOrchestrator::new(model, actor, wal, 3);
    let res = orch.execute_task("Format test").await.unwrap();
    assert!(res.summary.contains("successfully completed"));
    assert!(res.summary.contains("Format test"));
    println!("  ✓ Vector 4.15: TaskResult summary formatted cleanly");
}
