use nexus_v2::{NexusV2Error, PatchEngine, Sandbox};
use nexus_v2::traits::SafetyFilter;

fn create_temp_workspace() -> (tempfile_guard::TempDirGuard, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("nexus_v2_b1_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("create temp workspace");
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

// 1. Path Traversal: Relative Parent Blocked
#[test]
fn test_path_traversal_relative_parent_blocked() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.resolve_path("../../etc/passwd");
    assert!(res.is_err());
    match res.unwrap_err() {
        NexusV2Error::PathEscape(p) => assert!(p.contains("..")),
        e => panic!("Unexpected error: {:?}", e),
    }
    println!("  ✓ Vector 1.1: Relative parent traversal blocked");
}

// 2. Path Traversal: Nested Escape Blocked
#[test]
fn test_path_traversal_nested_escape_blocked() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.resolve_path("src/deep/nested/../../../../etc/shadow");
    assert!(res.is_err());
    match res.unwrap_err() {
        NexusV2Error::PathEscape(p) => assert!(p.contains("..")),
        e => panic!("Unexpected error: {:?}", e),
    }
    println!("  ✓ Vector 1.2: Deep nested traversal blocked");
}

// 3. Absolute Path Escape Blocked
#[test]
fn test_absolute_path_escape_blocked() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.resolve_path("/var/log/syslog");
    assert!(res.is_err());
    match res.unwrap_err() {
        NexusV2Error::PathEscape(p) => assert!(p.starts_with('/')),
        e => panic!("Unexpected error: {:?}", e),
    }
    println!("  ✓ Vector 1.3: Absolute path escape blocked");
}

// 4. Valid Relative Path Resolution
#[test]
fn test_valid_relative_path_resolution() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let resolved = sb.resolve_path("src/main.rs").expect("Valid relative path");
    assert!(resolved.starts_with(&ws));
    assert!(resolved.ends_with("src/main.rs"));
    println!("  ✓ Vector 1.4: Valid relative path resolution confirmed");
}

// 5. Deep Nested Valid Path
#[test]
fn test_deep_nested_valid_path() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let resolved = sb.resolve_path("crates/core/tests/fixtures/data.json").expect("Deep valid path");
    assert!(resolved.starts_with(&ws));
    println!("  ✓ Vector 1.5: Deep nested valid path within workspace root confirmed");
}

// 6. Command Quarantine: Blocks rm -rf /
#[test]
fn test_command_quarantine_blocks_rm_root() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.validate_command("rm -rf /");
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("Forbidden destructive"));
    println!("  ✓ Vector 1.6: rm -rf / intercepted before execution");
}

// 7. Command Quarantine: Blocks rm -rf /*
#[test]
fn test_command_quarantine_blocks_rm_wildcard() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.validate_command("sudo rm -rf /*");
    assert!(res.is_err());
    println!("  ✓ Vector 1.7: rm -rf /* wildcard intercepted");
}

// 8. Command Quarantine: Blocks Forkbomb
#[test]
fn test_command_quarantine_blocks_forkbomb() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.validate_command(":(){ :|:& };:");
    assert!(res.is_err());
    println!("  ✓ Vector 1.8: Forkbomb pattern intercepted");
}

// 9. Command Quarantine: Blocks mkfs
#[test]
fn test_command_quarantine_blocks_mkfs() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.validate_command("mkfs.ext4 /dev/sda1");
    assert!(res.is_err());
    println!("  ✓ Vector 1.9: mkfs destructive disk format intercepted");
}

// 10. Command Quarantine: Blocks dd if=/dev/zero
#[test]
fn test_command_quarantine_blocks_dd_zero() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.validate_command("dd if=/dev/zero of=/dev/sda bs=1M");
    assert!(res.is_err());
    println!("  ✓ Vector 1.10: dd disk wipe intercepted");
}

// 11. Command Quarantine: Blocks chmod -R 777 /
#[test]
fn test_command_quarantine_blocks_recursive_chmod() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    let res = sb.validate_command("chmod -R 777 /");
    assert!(res.is_err());
    println!("  ✓ Vector 1.11: Mass permission escalation intercepted");
}

// 12. Command Quarantine: Permits Safe Developer Commands
#[test]
fn test_command_quarantine_permits_safe_cargo() {
    let (_guard, ws) = create_temp_workspace();
    let sb = Sandbox::new(&ws);
    assert!(sb.validate_command("cargo check --all-targets").is_ok());
    assert!(sb.validate_command("cargo test --lib").is_ok());
    assert!(sb.validate_command("git status --short").is_ok());
    println!("  ✓ Vector 1.12: Safe developer toolchain commands permitted");
}

// 13. Patch Engine: Tier 1 Exact Match Success
#[test]
fn test_patch_tier1_exact_match_success() {
    let orig = "pub fn compute() -> i32 {\n    41\n}\n";
    let target = "    41\n";
    let replacement = "    42\n";
    let patched = PatchEngine::apply_patch(orig, target, replacement).expect("Tier 1 match");
    assert!(patched.contains("42"));
    assert!(!patched.contains("41"));
    println!("  ✓ Vector 1.13: Tier 1 exact byte-level patch applied");
}

// 14. Patch Engine: Ambiguity Guard Multiple Match Rejection
#[test]
fn test_patch_ambiguity_guard_multi_match_rejection() {
    let orig = "let x = 10;\nlet y = 20;\nlet x = 10;\n";
    let target = "let x = 10;\n";
    let replacement = "let x = 99;\n";
    let res = PatchEngine::apply_patch(orig, target, replacement);
    assert!(res.is_err());
    match res.unwrap_err() {
        NexusV2Error::AmbiguousPatch { count } => assert_eq!(count, 2),
        e => panic!("Expected AmbiguousPatch error, got {:?}", e),
    }
    println!("  ✓ Vector 1.14: Ambiguity guard rejected 2 duplicate occurrences");
}

// 15. Patch Engine: Tier 2 CRLF vs LF Normalization
#[test]
fn test_patch_tier2_crlf_lf_whitespace_normalization() {
    let orig = "fn run() {\r\n    step_one();\r\n}\r\n";
    let target = "    step_one();\n";
    let replacement = "    step_one_v2();\n";
    let patched = PatchEngine::apply_patch(orig, target, replacement).expect("Tier 2 match");
    assert!(patched.contains("step_one_v2"));
    println!("  ✓ Vector 1.15: Tier 2 CRLF/LF normalization successfully reconciled");
}
