use nexus_v2::{NexusV2Error, PatchEngine, Sandbox};
use nexus_v2::traits::SafetyFilter;

#[test]
fn test_ambiguity_guard_rejects_duplicate_matches() {
    let original = "fn test() {\n    let x = 1;\n    let x = 1;\n}\n";
    let target = "    let x = 1;\n";
    let replacement = "    let x = 2;\n";

    let result = PatchEngine::apply_patch(original, target, replacement);
    assert!(result.is_err(), "PatchEngine must reject multiple matching lines");

    match result.unwrap_err() {
        NexusV2Error::AmbiguousPatch { count } => {
            assert_eq!(count, 2, "Must accurately report 2 ambiguous occurrences");
            println!("✓ Ambiguity guard intercepted 2 duplicate targets: file corruption prevented!");
        }
        other => panic!("Expected AmbiguousPatch error, got {:?}", other),
    }
}

#[test]
fn test_exact_patch_succeeds_uniquely() {
    let original = "fn main() {\n    println!(\"old\");\n}\n";
    let target = "    println!(\"old\");\n";
    let replacement = "    println!(\"new\");\n";

    let result = PatchEngine::apply_patch(original, target, replacement).expect("Unique match succeeds");
    assert!(result.contains("\"new\""));
    assert!(!result.contains("\"old\""));
    println!("✓ Unambiguous patch applied cleanly at Tier 1 byte-level");
}

#[test]
fn test_sandbox_path_containment_blocks_traversal() {
    let temp_dir = std::env::temp_dir().join(format!("nexus_v2_sb_{}", uuid::Uuid::new_v4()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let sandbox = Sandbox::new(&temp_dir);

    // Attack vector 1: ../ traversal
    let res1 = sandbox.resolve_path("../../etc/passwd");
    assert!(res1.is_err());
    match res1.unwrap_err() {
        NexusV2Error::PathEscape(p) => assert!(p.contains("..")),
        other => panic!("Expected PathEscape, got {:?}", other),
    }

    // Attack vector 2: Absolute path escape
    let res2 = sandbox.resolve_path("/etc/shadow");
    assert!(res2.is_err());
    match res2.unwrap_err() {
        NexusV2Error::PathEscape(p) => assert!(p.starts_with('/')),
        other => panic!("Expected PathEscape, got {:?}", other),
    }

    let _ = std::fs::remove_dir_all(temp_dir);
    println!("✓ Sandbox strictly confined operations within workspace root");
}

#[test]
fn test_sandbox_blocks_destructive_commands() {
    let temp_dir = std::env::temp_dir().join(format!("nexus_v2_cmd_{}", uuid::Uuid::new_v4()));
    let sandbox = Sandbox::new(&temp_dir);

    let res1 = sandbox.validate_command("rm -rf /");
    assert!(res1.is_err(), "Must block destructive root wipe");

    let res2 = sandbox.validate_command(":(){ :|:& };:");
    assert!(res2.is_err(), "Must block forkbomb");

    let res3 = sandbox.validate_command("cargo test");
    assert!(res3.is_ok(), "Safe command must be permitted");

    println!("✓ Command quarantine successfully intercepted all destructive shell vectors");
}
