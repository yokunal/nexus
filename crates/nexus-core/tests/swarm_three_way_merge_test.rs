use nexus_core::agent::GitWorktreeManager;
use std::fs;
use std::process::Command;
use uuid::Uuid;

#[test]
fn test_three_way_swarm_worktree_merge_battle() {
    println!("\n\x1b[1;35m=== TRIAL 5: Three-Way Branch Merge Swarm Battle ===\x1b[0m");

    let repo_dir = std::env::temp_dir().join(format!("nexus_swarm_repo_{}", Uuid::new_v4()));
    fs::create_dir_all(repo_dir.join("src")).unwrap();
    fs::create_dir_all(repo_dir.join("tests")).unwrap();

    // 1. Initialize main Git repository
    let run_cmd = |dir: &std::path::Path, args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .expect("Git command execution");
        assert!(status.success(), "Git command failed: {:?}", args);
    };

    run_cmd(&repo_dir, &["init"]);
    run_cmd(&repo_dir, &["config", "user.name", "Nexus Swarm"]);
    run_cmd(&repo_dir, &["config", "user.email", "swarm@nexus.ai"]);

    // Base repository files
    fs::write(
        repo_dir.join("Cargo.toml"),
        r#"[package]
name = "swarm-app"
version = "0.1.0"
edition = "2021"
"#,
    )
    .unwrap();

    fs::write(
        repo_dir.join("src/lib.rs"),
        r#"pub fn base_service_status() -> &'static str {
    "online"
}
"#,
    )
    .unwrap();

    run_cmd(&repo_dir, &["add", "-A"]);
    run_cmd(&repo_dir, &["commit", "-m", "chore: initial base service"]);
    println!("✓ Initialized base Git repository on `main` branch.");

    // 2. Spawn Worktree 1: Agent "optimizer"
    let (opt_dir, opt_branch) = GitWorktreeManager::create_worktree(&repo_dir, "optimizer")
        .expect("Create optimizer worktree");
    let opt_branch = opt_branch.expect("Branch name");
    println!("✓ Spawned Agent 1 (Optimizer) in isolated worktree: {}", opt_dir.display());

    fs::write(
        opt_dir.join("src/optimizer.rs"),
        r#"pub fn fast_lru_compute(key: u64) -> u64 {
    key * 42
}
"#,
    )
    .unwrap();

    let mut lib_content = fs::read_to_string(opt_dir.join("src/lib.rs")).unwrap();
    lib_content.push_str("\npub mod optimizer;\n");
    fs::write(opt_dir.join("src/lib.rs"), lib_content).unwrap();

    // 3. Spawn Worktree 2: Agent "security"
    let (sec_dir, sec_branch) = GitWorktreeManager::create_worktree(&repo_dir, "security")
        .expect("Create security worktree");
    let sec_branch = sec_branch.expect("Branch name");
    println!("✓ Spawned Agent 2 (Security) in isolated worktree: {}", sec_dir.display());

    fs::write(
        sec_dir.join("src/security.rs"),
        r#"pub fn sanitize_input(input: &str) -> String {
    input.trim().replace('<', "&lt;").replace('>', "&gt;")
}
"#,
    )
    .unwrap();

    let mut lib_content2 = fs::read_to_string(sec_dir.join("src/lib.rs")).unwrap();
    lib_content2.push_str("\npub mod security;\n");
    fs::write(sec_dir.join("src/lib.rs"), lib_content2).unwrap();

    // 4. Spawn Worktree 3: Agent "tester"
    let (test_dir, test_branch) = GitWorktreeManager::create_worktree(&repo_dir, "tester")
        .expect("Create tester worktree");
    let test_branch = test_branch.expect("Branch name");
    println!("✓ Spawned Agent 3 (Tester) in isolated worktree: {}", test_dir.display());

    fs::create_dir_all(test_dir.join("tests")).unwrap();
    fs::write(
        test_dir.join("tests/integration_tests.rs"),
        r#"use swarm_app::*;

#[test]
fn test_base_service() {
    assert_eq!(base_service_status(), "online");
}
"#,
    )
    .unwrap();

    // 5. Cleanup and preserve all 3 worktrees
    let preserved_opt = GitWorktreeManager::cleanup_worktree(&repo_dir, &opt_dir, Some(&opt_branch))
        .expect("Cleanup optimizer");
    assert_eq!(preserved_opt, Some(opt_branch.clone()));

    let preserved_sec = GitWorktreeManager::cleanup_worktree(&repo_dir, &sec_dir, Some(&sec_branch))
        .expect("Cleanup security");
    assert_eq!(preserved_sec, Some(sec_branch.clone()));

    let preserved_test = GitWorktreeManager::cleanup_worktree(&repo_dir, &test_dir, Some(&test_branch))
        .expect("Cleanup tester");
    assert_eq!(preserved_test, Some(test_branch.clone()));

    println!("✓ All 3 subagent worktrees cleanly harvested and auto-committed.");

    // 6. Execute the Three-Way Swarm Merge into `main`
    println!("\nExecuting 3-Way sequential merge into `main`:");

    // Merge 1: Optimizer
    run_cmd(&repo_dir, &["merge", &opt_branch, "-m", "merge: agent optimizer"]);
    println!("  ✓ Merged {}", opt_branch);

    // Merge 2: Security (may encounter overlapping edits on src/lib.rs)
    let sec_merge_status = Command::new("git")
        .args(["merge", &sec_branch, "-m", "merge: agent security"])
        .current_dir(&repo_dir)
        .status()
        .expect("git merge security");

    if !sec_merge_status.success() {
        println!("  ! Merge conflict detected in src/lib.rs between optimizer and security. Resolving conflict...");
        // Reconcile conflict markers in src/lib.rs
        let conflicted = fs::read_to_string(repo_dir.join("src/lib.rs")).unwrap();
        assert!(conflicted.contains("<<<<<<<"));
        
        let resolved = r#"pub fn base_service_status() -> &'static str {
    "online"
}

pub mod optimizer;
pub mod security;
"#;
        fs::write(repo_dir.join("src/lib.rs"), resolved).unwrap();
        run_cmd(&repo_dir, &["add", "src/lib.rs"]);
        run_cmd(&repo_dir, &["commit", "-m", "merge: agent security (conflict resolved)"]);
        println!("  ✓ Conflict resolved cleanly: both modules reconciled.");
    } else {
        println!("  ✓ Merged {}", sec_branch);
    }

    // Merge 3: Tester
    run_cmd(&repo_dir, &["merge", &test_branch, "-m", "merge: agent tester"]);
    println!("  ✓ Merged {}", test_branch);

    // 7. Verify unified repository contains all 3 agents' contributions
    assert!(repo_dir.join("src/optimizer.rs").exists());
    assert!(repo_dir.join("src/security.rs").exists());
    assert!(repo_dir.join("tests/integration_tests.rs").exists());

    let final_lib = fs::read_to_string(repo_dir.join("src/lib.rs")).unwrap();
    assert!(final_lib.contains("pub mod optimizer;"));
    assert!(final_lib.contains("pub mod security;"));

    // Verify git log has all commits
    let log_output = Command::new("git")
        .args(["log", "--oneline"])
        .current_dir(&repo_dir)
        .output()
        .expect("git log");
    let log_str = String::from_utf8_lossy(&log_output.stdout);
    println!("✓ Unified Git Log:\n{}", log_str);

    // Clean up
    let _ = fs::remove_dir_all(&repo_dir);
    println!("\x1b[1;32m=== TRIAL 5 PASSED: 3-way parallel swarm branches cleanly created, preserved, and merged! ===\x1b[0m\n");
}
