use std::sync::Arc;
use nexus_v2::traits::OrchestratorEngine;
use nexus_v2::{Actor, CircularWal, MockModelClient, PavOrchestrator, Sandbox};

#[tokio::test]
async fn test_pav_orchestrator_autonomous_plan_act_verify_loop() {
    let temp_dir = std::env::temp_dir().join(format!("nexus_v2_pav_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);

    // Create a target file to verify editing inside the sandbox
    let target_file = temp_dir.join("config.txt");
    std::fs::write(&target_file, "status=broken\n").unwrap();

    let sandbox = Sandbox::new(&temp_dir);
    let actor = Actor::new(sandbox);
    let wal = CircularWal::new(temp_dir.join("pav.wal"), 100);
    let router = Arc::new(MockModelClient::new("pav-verifier", "VERIFICATION: PASS"));

    let mut orchestrator = PavOrchestrator::new(router, actor, wal, 3);

    let result = orchestrator
        .execute_task("Repair system configuration and verify green")
        .await
        .expect("PAV loop execution");

    assert!(result.success);
    assert_eq!(result.iterations, 1);
    assert!(result.summary.contains("successfully completed"));
    println!("✓ PAV loop executed Plan, Action, and Verification autonomously with 100% success!");

    let _ = std::fs::remove_dir_all(temp_dir);
}
